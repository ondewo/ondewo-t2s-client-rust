// Copyright 2021-2026 ONDEWO GmbH
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! A scripted loopback token endpoint for the HTTP transport tests: plaintext or TLS (rustls), one
//! scripted answer per accepted connection, every request recorded as text.

#![allow(dead_code)]

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::{ServerConfig, ServerConnection, StreamOwned};

/// What the endpoint does with one connection.
#[derive(Clone)]
pub enum Answer {
    /// Read the request, write these bytes in one go, close.
    Raw(Vec<u8>),
    /// Read the request, write these bytes one at a time, close.
    Trickle(Vec<u8>),
    /// Read the request, wait, close without answering.
    Silent(Duration),
    /// Over TLS: write these bytes and drop the socket WITHOUT a close_notify.
    RawNoCloseNotify(Vec<u8>),
}

pub fn http_answer(status_line: &str, headers: &[&str], body: &str) -> Vec<u8> {
    let mut answer = format!("HTTP/1.1 {status_line}\r\n");
    for header in headers {
        answer.push_str(header);
        answer.push_str("\r\n");
    }
    answer.push_str("\r\n");
    answer.push_str(body);
    answer.into_bytes()
}

pub fn json_answer(body: &str) -> Answer {
    Answer::Raw(http_answer(
        "200 OK",
        &[
            "Content-Type: application/json",
            &format!("Content-Length: {}", body.len()),
        ],
        body,
    ))
}

pub struct Endpoint {
    pub address: SocketAddr,
    pub requests: Arc<Mutex<Vec<String>>>,
    handle: Option<JoinHandle<()>>,
}

impl Endpoint {
    pub fn requests(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }

    pub fn url(&self, scheme: &str, host: &str, path: &str) -> String {
        format!("{scheme}://{host}:{}{path}", self.address.port())
    }

    /// Wait until every scripted answer was served.
    pub fn finish(mut self) -> Vec<String> {
        self.handle.take().unwrap().join().unwrap();
        self.requests()
    }
}

fn read_request(stream: &mut impl Read) -> String {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let text = String::from_utf8_lossy(&buffer).to_string();
        if let Some(head_end) = text.find("\r\n\r\n") {
            let length = text[..head_end]
                .lines()
                .find_map(|line| line.strip_prefix("Content-Length: "))
                .map_or(0, |length| length.trim().parse::<usize>().unwrap());
            if buffer.len() >= head_end + 4 + length {
                return text;
            }
        }
        match stream.read(&mut chunk) {
            Ok(0) | Err(_) => return text,
            Ok(read) => buffer.extend_from_slice(&chunk[..read]),
        }
    }
}

fn play(stream: &mut impl Write, answer: &Answer) {
    match answer {
        Answer::Raw(bytes) | Answer::RawNoCloseNotify(bytes) => {
            let _ = stream.write_all(bytes);
        }
        Answer::Trickle(bytes) => {
            for byte in bytes {
                let _ = stream.write_all(&[*byte]);
                let _ = stream.flush();
                thread::sleep(Duration::from_micros(300));
            }
        }
        Answer::Silent(wait) => thread::sleep(*wait),
    }
    let _ = stream.flush();
}

/// A plaintext endpoint on `host` (e.g. `127.0.0.1`), serving `answers` in order.
pub fn serve(host: &str, answers: Vec<Answer>) -> Option<Endpoint> {
    let listener = TcpListener::bind((host, 0)).ok()?;
    let address = listener.local_addr().unwrap();
    let requests: Arc<Mutex<Vec<String>>> = Arc::default();
    let recorded = Arc::clone(&requests);
    let handle = thread::spawn(move || {
        for answer in answers {
            let (mut stream, _) = listener.accept().unwrap();
            recorded.lock().unwrap().push(read_request(&mut stream));
            play(&mut stream, &answer);
        }
    });
    Some(Endpoint {
        address,
        requests,
        handle: Some(handle),
    })
}

/// A TLS endpoint with `certificate_chain` / `key` (PEM); `tls12_only` restricts it to TLS 1.2.
pub fn serve_tls(
    certificate_chain: &str,
    key: &str,
    tls12_only: bool,
    answers: Vec<Answer>,
) -> Endpoint {
    let chain: Vec<CertificateDer<'static>> =
        CertificateDer::pem_slice_iter(certificate_chain.as_bytes())
            .collect::<Result<_, _>>()
            .unwrap();
    let key = PrivateKeyDer::from_pem_slice(key.as_bytes()).unwrap();
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ServerConfig::builder_with_provider(provider);
    let builder = if tls12_only {
        builder
            .with_protocol_versions(&[&rustls::version::TLS12])
            .unwrap()
    } else {
        builder.with_safe_default_protocol_versions().unwrap()
    };
    let config = Arc::new(
        builder
            .with_no_client_auth()
            .with_single_cert(chain, key)
            .unwrap(),
    );
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let address = listener.local_addr().unwrap();
    let requests: Arc<Mutex<Vec<String>>> = Arc::default();
    let recorded = Arc::clone(&requests);
    let handle = thread::spawn(move || {
        for answer in answers {
            let (tcp, _) = listener.accept().unwrap();
            let connection = ServerConnection::new(Arc::clone(&config)).unwrap();
            let mut stream: StreamOwned<ServerConnection, TcpStream> =
                StreamOwned::new(connection, tcp);
            recorded.lock().unwrap().push(read_request(&mut stream));
            play(&mut stream, &answer);
            if !matches!(answer, Answer::RawNoCloseNotify(_)) {
                stream.conn.send_close_notify();
                let _ = stream.flush();
            }
        }
    });
    Endpoint {
        address,
        requests,
        handle: Some(handle),
    }
}

// ---------------------------------------------------------------------------------------------
// A throw-away PKI from the openssl CLI (nothing is committed).

pub struct Pki {
    pub ca: String,
    pub server_cert: String,
    pub server_key: String,
    pub other_server_cert: String,
    pub other_server_key: String,
}

fn openssl(directory: &Path, args: &[&str]) {
    let output = Command::new("openssl")
        .args(args)
        .current_dir(directory)
        .output()
        .expect("the openssl CLI is needed to generate the test PKI");
    assert!(
        output.status.success(),
        "openssl {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn make_ca(directory: &Path, name: &str) {
    openssl(
        directory,
        &[
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-nodes",
            "-days",
            "2",
            "-subj",
            &format!("/CN={name}"),
            "-keyout",
            &format!("{name}.key"),
            "-out",
            &format!("{name}.pem"),
        ],
    );
}

fn make_server(directory: &Path, name: &str, ca: &str) {
    std::fs::write(
        directory.join(format!("{name}.ext")),
        "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1\nextendedKeyUsage=serverAuth\n",
    )
    .unwrap();
    openssl(
        directory,
        &[
            "req",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:prime256v1",
            "-nodes",
            "-subj",
            &format!("/CN={name}"),
            "-keyout",
            &format!("{name}.key"),
            "-out",
            &format!("{name}.csr"),
        ],
    );
    openssl(
        directory,
        &[
            "x509",
            "-req",
            "-in",
            &format!("{name}.csr"),
            "-CA",
            &format!("{ca}.pem"),
            "-CAkey",
            &format!("{ca}.key"),
            "-CAcreateserial",
            "-days",
            "2",
            "-extfile",
            &format!("{name}.ext"),
            "-out",
            &format!("{name}.pem"),
        ],
    );
}

/// Generate the PKI into `directory`.
pub fn make_pki(directory: &Path) -> Pki {
    std::fs::create_dir_all(directory).unwrap();
    make_ca(directory, "ca");
    make_ca(directory, "other-ca");
    make_server(directory, "server", "ca");
    make_server(directory, "other-server", "other-ca");
    let read = |name: &str| std::fs::read_to_string(directory.join(name)).unwrap();
    Pki {
        ca: read("ca.pem"),
        server_cert: read("server.pem"),
        server_key: read("server.key"),
        other_server_cert: read("other-server.pem"),
        other_server_key: read("other-server.key"),
    }
}
