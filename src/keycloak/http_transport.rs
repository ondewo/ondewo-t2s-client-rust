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

//! The default token-endpoint transport: ONE blocking HTTP/1.1 `POST` over std sockets and rustls.
//!
//! HAND-WRITTEN, and deliberately small: the crate is generated fully offline from a pre-warmed
//! cargo cache, which holds tonic's dependency tree (http, rustls, ring, rustls-native-certs) but
//! no HTTP client crate. A token request is one form POST with `Connection: close`, so a full
//! client would buy nothing. Proxies (`HTTPS_PROXY`) are not honoured; inject a
//! [`TokenTransport`] of your own where one is needed.

use std::error::Error;
use std::fmt;
use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::Arc;
use std::time::Instant;

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature, CryptoProvider};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, RootCertStore, SignatureScheme,
    StreamOwned,
};

use super::{TokenHttpResponse, TokenRequest, TokenTransport, TransportError};

/// The default [`TokenTransport`]: `http://` and `https://` (rustls, the platform's trust store,
/// which honours `SSL_CERT_FILE` / `SSL_CERT_DIR`). The whole exchange is bounded by
/// [`TokenRequest::timeout`] - every socket operation gets the time that is left - except the
/// DNS lookup, which std cannot bound.
#[derive(Clone, Copy, Debug, Default)]
pub struct HttpTokenTransport;

impl TokenTransport for HttpTokenTransport {
    fn post_form(&self, request: &TokenRequest) -> Result<TokenHttpResponse, TransportError> {
        let deadline = Instant::now() + request.timeout;
        let target = Target::parse(&request.url)?;
        let body = encode_form(&request.form);
        let message = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/x-www-form-urlencoded\r\n\
             Content-Length: {}\r\nAccept: application/json\r\nConnection: close\r\n\
             User-Agent: ondewo-t2s-client-rust\r\n\r\n{body}",
            target.path,
            target.host_header,
            body.len()
        );
        // The TLS side is set up first, so a host TLS cannot verify fails before any connection.
        let tls = if target.tls {
            let server_name =
                ServerName::try_from(target.host.clone()).map_err(TransportError::other)?;
            Some((server_name, Arc::new(tls_config(request.verify_ssl)?)))
        } else {
            None
        };
        let tcp = connect(&target, deadline)?;
        let control = tcp.try_clone().map_err(TransportError::from_io)?;
        let Some((server_name, config)) = tls else {
            let mut stream = tcp;
            return exchange(&mut stream, &control, message.as_bytes(), deadline);
        };
        let connection =
            ClientConnection::new(config, server_name).map_err(TransportError::other)?;
        let mut stream = StreamOwned::new(connection, tcp);
        exchange(&mut stream, &control, message.as_bytes(), deadline)
    }
}

/// A malformed URL or HTTP answer.
#[derive(Debug)]
struct HttpProtocolError(String);

impl fmt::Display for HttpProtocolError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for HttpProtocolError {}

fn protocol_error(message: &str) -> TransportError {
    TransportError::other(HttpProtocolError(message.to_string()))
}

/// Where to send the request.
struct Target {
    tls: bool,
    /// The host without IPv6 brackets: what is resolved and what TLS verifies.
    host: String,
    port: u16,
    host_header: String,
    path: String,
}

impl Target {
    fn parse(url: &str) -> Result<Self, TransportError> {
        let uri: http::Uri = url.parse().map_err(TransportError::other)?;
        let tls = match uri.scheme_str() {
            Some("https") => true,
            Some("http") => false,
            _ => {
                return Err(protocol_error(
                    "the token endpoint URL must start with http:// or https://",
                ))
            }
        };
        let host = uri.host().unwrap_or_default();
        if host.is_empty()
            || uri
                .authority()
                .is_some_and(|authority| authority.as_str().contains('@'))
        {
            return Err(protocol_error(
                "the token endpoint URL must name a host and carry no credentials",
            ));
        }
        let host_header = match uri.port_u16() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        };
        Ok(Self {
            tls,
            host: host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string(),
            port: uri.port_u16().unwrap_or(if tls { 443 } else { 80 }),
            host_header,
            path: uri
                .path_and_query()
                .map_or("/", |path| path.as_str())
                .to_string(),
        })
    }
}

/// `application/x-www-form-urlencoded`: unreserved bytes as they are, space as `+`, the rest
/// percent-encoded.
fn encode_form(form: &[(String, String)]) -> String {
    fn encode(text: &str, encoded: &mut String) {
        for byte in text.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'*' => {
                    encoded.push(char::from(byte));
                }
                b' ' => encoded.push('+'),
                _ => encoded.push_str(&format!("%{byte:02X}")),
            }
        }
    }
    let mut encoded = String::new();
    for (index, (key, value)) in form.iter().enumerate() {
        if index > 0 {
            encoded.push('&');
        }
        encode(key, &mut encoded);
        encoded.push('=');
        encode(value, &mut encoded);
    }
    encoded
}

/// The time left until `deadline`, or a `TimedOut` error once it has passed.
fn remaining(deadline: Instant) -> Result<std::time::Duration, TransportError> {
    let left = deadline.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(timed_out());
    }
    Ok(left)
}

fn timed_out() -> TransportError {
    TransportError::from_io(io::Error::new(
        io::ErrorKind::TimedOut,
        "the token endpoint did not answer within the timeout",
    ))
}

fn connect(target: &Target, deadline: Instant) -> Result<TcpStream, TransportError> {
    let addresses = (target.host.as_str(), target.port)
        .to_socket_addrs()
        .map_err(TransportError::from_io)?;
    let mut last_error = io::Error::new(io::ErrorKind::NotFound, "the host resolved to no address");
    for address in addresses {
        match TcpStream::connect_timeout(&address, remaining(deadline)?) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = error,
        }
    }
    Err(TransportError::from_io(last_error))
}

/// A plaintext or a TLS connection; one non-generic `exchange` serves both.
trait Stream: Read + Write {}

impl<T: Read + Write> Stream for T {}

/// Send `message` and read one HTTP response, every socket operation bounded by the time left.
fn exchange(
    stream: &mut dyn Stream,
    control: &TcpStream,
    message: &[u8],
    deadline: Instant,
) -> Result<TokenHttpResponse, TransportError> {
    let bound = |control: &TcpStream| -> Result<(), TransportError> {
        let left = remaining(deadline)?;
        control
            .set_read_timeout(Some(left))
            .map_err(TransportError::from_io)?;
        control
            .set_write_timeout(Some(left))
            .map_err(TransportError::from_io)
    };
    bound(control)?;
    stream
        .write_all(message)
        .and_then(|()| stream.flush())
        .map_err(io_error)?;
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        if let Some(response) = parse_response(&buffer, false)? {
            return Ok(response);
        }
        bound(control)?;
        match stream.read(&mut chunk) {
            Ok(read) if read > 0 => buffer.extend_from_slice(&chunk[..read]),
            // A peer that closes without a TLS close_notify reads as UnexpectedEof: the end too.
            Ok(_) => return end_of_stream(&buffer),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return end_of_stream(&buffer)
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(io_error(error)),
        }
    }
}

fn end_of_stream(buffer: &[u8]) -> Result<TokenHttpResponse, TransportError> {
    parse_response(buffer, true)?
        .ok_or_else(|| protocol_error("the connection closed before a complete HTTP response"))
}

/// A socket timeout reads as `WouldBlock` on unix and `TimedOut` elsewhere: one name for both.
fn io_error(error: io::Error) -> TransportError {
    if matches!(
        error.kind(),
        io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
    ) {
        return timed_out();
    }
    TransportError::from_io(error)
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// The response in `buffer`, `None` while it is incomplete and the stream has not ended.
fn parse_response(
    buffer: &[u8],
    at_end: bool,
) -> Result<Option<TokenHttpResponse>, TransportError> {
    let Some(head_end) = find(buffer, b"\r\n\r\n") else {
        return Ok(None);
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]);
    let mut lines = head.split("\r\n");
    let status = lines
        .next()
        .and_then(|line| line.strip_prefix("HTTP/1."))
        .and_then(|line| line.get(2..5))
        .and_then(|code| code.parse::<u16>().ok())
        .ok_or_else(|| {
            protocol_error("the token endpoint answered without an HTTP/1.x status line")
        })?;
    let mut content_length: Option<usize> = None;
    let mut chunked = false;
    for line in lines {
        let (name, value) = line.split_once(':').unwrap_or((line, ""));
        let value = value.trim();
        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(value.parse().map_err(|_| {
                protocol_error("the token endpoint answered with an invalid Content-Length")
            })?);
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            chunked = value.to_ascii_lowercase().contains("chunked");
        }
    }
    let raw_body = &buffer[head_end + 4..];
    let body = if chunked {
        decode_chunked(raw_body)?
    } else if let Some(length) = content_length {
        raw_body.get(..length).map(<[u8]>::to_vec)
    } else if at_end {
        Some(raw_body.to_vec())
    } else {
        None
    };
    match body {
        Some(body) => Ok(Some(TokenHttpResponse {
            status,
            body: String::from_utf8_lossy(&body).into_owned(),
        })),
        None if at_end => Err(protocol_error(
            "the token endpoint's response body is truncated",
        )),
        None => Ok(None),
    }
}

/// The decoded body of a chunked transfer encoding, `None` while it is incomplete.
fn decode_chunked(data: &[u8]) -> Result<Option<Vec<u8>>, TransportError> {
    let mut decoded = Vec::new();
    let mut position = 0;
    loop {
        let Some(line_end) = find(&data[position..], b"\r\n") else {
            return Ok(None);
        };
        let size_line = String::from_utf8_lossy(&data[position..position + line_end]);
        let size_text = size_line.split(';').next().unwrap_or_default().trim();
        let size = usize::from_str_radix(size_text, 16).map_err(|_| {
            protocol_error("the token endpoint answered with an invalid chunk size")
        })?;
        position += line_end + 2;
        if size == 0 {
            return Ok(Some(decoded));
        }
        let end = position.saturating_add(size);
        if data.len() < end.saturating_add(2) {
            return Ok(None);
        }
        decoded.extend_from_slice(&data[position..end]);
        position = end + 2;
    }
}

fn tls_config(verify_ssl: bool) -> Result<ClientConfig, TransportError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let builder = ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(TransportError::other)?;
    if !verify_ssl {
        return Ok(builder
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCertificate(provider)))
            .with_no_client_auth());
    }
    let mut roots = RootCertStore::empty();
    roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
    Ok(builder.with_root_certificates(roots).with_no_client_auth())
}

/// `verify_ssl = false`: any server certificate is accepted, but the handshake signatures are
/// still checked, so the connection is at least bound to the key the server presented.
#[derive(Debug)]
struct AcceptAnyServerCertificate(Arc<CryptoProvider>);

impl ServerCertVerifier for AcceptAnyServerCertificate {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.0.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.0.signature_verification_algorithms.supported_schemes()
    }
}
