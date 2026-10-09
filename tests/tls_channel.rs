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

//! TLS and mutual TLS through `channel::ClientConfig`, with REAL handshakes.
//!
//! A throw-away PKI (two CAs, a server certificate for localhost / 127.0.0.1 / ::1, a client
//! certificate from each CA) is generated with the `openssl` CLI into a temporary directory at test
//! time - no key is committed. Each handshake test starts an in-process tonic server on a loopback
//! port that answers every call with UNIMPLEMENTED: a call that comes back UNIMPLEMENTED has
//! crossed the handshake and reached the server.

use std::fmt;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use ondewo_t2s_client::channel::{
    ChannelError, ClientConfig, MAX_MESSAGE_LENGTH, REDACTED, TCP_KEEPALIVE,
    TCP_KEEPALIVE_INTERVAL, TCP_KEEPALIVE_RETRIES,
};
use tokio::net::TcpListener;
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::service::Routes;
use tonic::transport::{Certificate, Channel, Identity, Server, ServerTlsConfig};
use tonic::{Code, Request, Status};

// ---------------------------------------------------------------------------------------------
// The test PKI

/// PEM content of every certificate and key the tests use.
struct Pki {
    ca: String,
    server_cert: String,
    server_key: String,
    client_cert: String,
    client_key: String,
    other_ca: String,
    other_client_cert: String,
    other_client_key: String,
}

fn openssl(directory: &Path, args: &[&str]) {
    let output = Command::new("openssl")
        .args(args)
        .current_dir(directory)
        .output()
        .expect("the openssl CLI is needed to generate the test PKI");
    assert!(
        output.status.success(),
        "openssl {args:?} failed: {}",
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

fn make_leaf(directory: &Path, name: &str, ca: &str, extensions: &str) {
    std::fs::write(directory.join(format!("{name}.ext")), extensions).unwrap();
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

fn pki() -> &'static Pki {
    static PKI: OnceLock<Pki> = OnceLock::new();
    PKI.get_or_init(|| {
        let directory: PathBuf =
            std::env::temp_dir().join(format!("ondewo-tls-test-pki-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        make_ca(&directory, "ca");
        make_ca(&directory, "other-ca");
        make_leaf(
            &directory,
            "server",
            "ca",
            "subjectAltName=DNS:localhost,IP:127.0.0.1,IP:::1\nextendedKeyUsage=serverAuth\n",
        );
        make_leaf(&directory, "client", "ca", "extendedKeyUsage=clientAuth\n");
        make_leaf(
            &directory,
            "other-client",
            "other-ca",
            "extendedKeyUsage=clientAuth\n",
        );
        let read = |name: &str| std::fs::read_to_string(directory.join(name)).unwrap();
        let pki = Pki {
            ca: read("ca.pem"),
            server_cert: read("server.pem"),
            server_key: read("server.key"),
            client_cert: read("client.pem"),
            client_key: read("client.key"),
            other_ca: read("other-ca.pem"),
            other_client_cert: read("other-client.pem"),
            other_client_key: read("other-client.key"),
        };
        std::fs::remove_dir_all(&directory).unwrap();
        pki
    })
}

/// The PEM body of a key - what must never show up in a message or a `Debug` rendering.
fn key_body(pem: &str) -> &str {
    pem.lines().nth(1).unwrap()
}

// ---------------------------------------------------------------------------------------------
// The in-process server and the probe call

/// Serve TLS on `bind` (port 0); with `client_ca`, the server REQUIRES a client certificate
/// issued by it. Every call is answered UNIMPLEMENTED.
async fn serve_tls(bind: &str, client_ca: Option<&str>) -> Option<SocketAddr> {
    let pki = pki();
    let listener = TcpListener::bind(bind).await.ok()?;
    let address = listener.local_addr().unwrap();
    let mut tls =
        ServerTlsConfig::new().identity(Identity::from_pem(&pki.server_cert, &pki.server_key));
    if let Some(client_ca) = client_ca {
        tls = tls.client_ca_root(Certificate::from_pem(client_ca));
    }
    let router = Server::builder()
        .tls_config(tls)
        .unwrap()
        .add_routes(Routes::default());
    tokio::spawn(router.serve_with_incoming(TcpListenerStream::new(listener)));
    Some(address)
}

async fn serve_plaintext() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let router = Server::builder().add_routes(Routes::default());
    tokio::spawn(router.serve_with_incoming(TcpListenerStream::new(listener)));
    address
}

/// One unary call; `Ok(())` when it reached the server (UNIMPLEMENTED), the error otherwise.
async fn probe(channel: Channel) -> Result<(), Status> {
    let mut grpc = tonic::client::Grpc::new(channel);
    grpc.ready()
        .await
        .map_err(|error| Status::unavailable(error.to_string()))?;
    let result: Result<tonic::Response<()>, Status> = grpc
        .unary(
            Request::new(()),
            PathAndQuery::from_static("/ondewo.test.Tls/Probe"),
            tonic_prost::ProstCodec::<(), ()>::default(),
        )
        .await;
    match result {
        Err(status) if status.code() == Code::Unimplemented => Ok(()),
        Err(status) => Err(status),
        Ok(_) => panic!("the probe server answers every call UNIMPLEMENTED"),
    }
}

/// Connect lazily and probe: the handshake outcome surfaces on the call, whichever side fails it.
async fn reaches_server(config: &ClientConfig) -> Result<(), Status> {
    tokio::time::timeout(
        Duration::from_secs(10),
        probe(config.connect_lazy().unwrap()),
    )
    .await
    .expect("a handshake must fail, not hang")
}

fn config_for(address: SocketAddr) -> ClientConfig {
    ClientConfig::new(address.ip().to_string(), address.port()).with_grpc_cert(&pki().ca)
}

fn crlf(pem: &str) -> String {
    pem.replace("\r\n", "\n").replace('\n', "\r\n")
}

// ---------------------------------------------------------------------------------------------
// Real handshakes

#[tokio::test]
async fn plain_tls_reaches_the_server() {
    let address = serve_tls("127.0.0.1:0", None).await.unwrap();
    reaches_server(&config_for(address)).await.unwrap();
}

#[tokio::test]
async fn plain_tls_by_host_name_verifies_the_dns_san() {
    let address = serve_tls("127.0.0.1:0", None).await.unwrap();
    let config = ClientConfig::new("localhost", address.port()).with_grpc_cert(&pki().ca);
    reaches_server(&config).await.unwrap();
}

#[tokio::test]
async fn eager_connect_completes_the_handshake() {
    let address = serve_tls("127.0.0.1:0", None).await.unwrap();
    let channel = config_for(address).connect().await.unwrap();
    probe(channel).await.unwrap();
}

#[tokio::test]
async fn mutual_tls_reaches_a_server_that_requires_client_certificates() {
    let pki = pki();
    let address = serve_tls("127.0.0.1:0", Some(&pki.ca)).await.unwrap();
    let config = config_for(address).with_client_identity(&pki.client_cert, &pki.client_key);
    reaches_server(&config).await.unwrap();
}

#[tokio::test]
async fn a_server_that_requires_client_certificates_rejects_a_client_without_one() {
    let address = serve_tls("127.0.0.1:0", Some(&pki().ca)).await.unwrap();
    let status = reaches_server(&config_for(address)).await.unwrap_err();
    // TLS 1.3: the client finishes its side first, the server's alert ends the first call.
    assert!(
        format!("{status:?}").contains("CertificateRequired"),
        "{status:?}"
    );
}

#[tokio::test]
async fn a_client_identity_from_an_unrelated_ca_is_rejected() {
    let pki = pki();
    let address = serve_tls("127.0.0.1:0", Some(&pki.ca)).await.unwrap();
    let config =
        config_for(address).with_client_identity(&pki.other_client_cert, &pki.other_client_key);
    let status = reaches_server(&config).await.unwrap_err();
    assert!(format!("{status:?}").contains("UnknownCA"), "{status:?}");
}

#[tokio::test]
async fn a_wrong_ca_fails_the_handshake_with_an_error() {
    let address = serve_tls("127.0.0.1:0", None).await.unwrap();
    let config = config_for(address).with_grpc_cert(&pki().other_ca);
    let status = reaches_server(&config).await.unwrap_err();
    assert_eq!(status.code(), Code::Unavailable, "{status:?}");
    assert!(status.message().contains("UnknownIssuer"), "{status:?}");
    let error = config.connect().await.unwrap_err();
    assert!(matches!(error, ChannelError::Connect { .. }), "{error:?}");
    assert!(error.to_string().contains(&config.host_and_port()));
    assert!(std::error::Error::source(&error).is_some());
}

#[tokio::test]
async fn without_grpc_cert_the_platform_store_is_trusted_and_the_test_ca_is_not() {
    let address = serve_tls("127.0.0.1:0", None).await.unwrap();
    let config = ClientConfig::new("127.0.0.1", address.port());
    let status = reaches_server(&config).await.unwrap_err();
    assert_eq!(status.code(), Code::Unavailable, "{status:?}");
    assert!(status.message().contains("UnknownIssuer"), "{status:?}");
}

#[tokio::test]
async fn empty_client_cert_and_key_mean_plain_tls() {
    let address = serve_tls("127.0.0.1:0", None).await.unwrap();
    let config = config_for(address).with_client_identity("", "");
    reaches_server(&config).await.unwrap();
}

#[tokio::test]
async fn crlf_pems_work() {
    let pki = pki();
    let address = serve_tls("127.0.0.1:0", Some(&pki.ca)).await.unwrap();
    let config = config_for(address)
        .with_grpc_cert(crlf(&pki.ca))
        .with_client_identity(crlf(&pki.client_cert), crlf(&pki.client_key));
    reaches_server(&config).await.unwrap();
}

#[tokio::test]
async fn tls_domain_name_overrides_the_name_that_is_verified() {
    let address = serve_tls("127.0.0.1:0", None).await.unwrap();
    let wrong = config_for(address).with_tls_domain_name("not-in-the-san.example");
    assert_eq!(
        reaches_server(&wrong).await.unwrap_err().code(),
        Code::Unavailable
    );
    let right = config_for(address).with_tls_domain_name("localhost");
    reaches_server(&right).await.unwrap();
}

#[tokio::test]
async fn ipv6_loopback_works_when_the_environment_has_it() {
    let pki = pki();
    let Some(address) = serve_tls("[::1]:0", Some(&pki.ca)).await else {
        eprintln!("skipped: this environment cannot bind [::1]");
        return;
    };
    let config = ClientConfig::new("::1", address.port())
        .with_grpc_cert(&pki.ca)
        .with_client_identity(&pki.client_cert, &pki.client_key);
    assert_eq!(config.host_and_port(), format!("[::1]:{}", address.port()));
    reaches_server(&config).await.unwrap();
}

#[tokio::test]
async fn plaintext_reaches_a_plaintext_server() {
    let address = serve_plaintext().await;
    let config = ClientConfig::new("127.0.0.1", address.port()).with_use_secure_channel(false);
    reaches_server(&config).await.unwrap();
}

// ---------------------------------------------------------------------------------------------
// Refused before tonic sees anything

#[test]
fn half_a_client_identity_is_refused() {
    let pki = pki();
    let cert_only =
        ClientConfig::new("localhost", 50051).with_client_identity(&pki.client_cert, "");
    let key_only = ClientConfig::new("localhost", 50051).with_client_identity("", &pki.client_key);
    for (config, missing) in [
        (cert_only, "grpc_client_key"),
        (key_only, "grpc_client_cert"),
    ] {
        let error = config.endpoint().unwrap_err();
        assert!(
            matches!(error, ChannelError::IncompleteClientIdentity { missing: m, .. } if m == missing),
            "{error:?}"
        );
        let message = error.to_string();
        assert!(message.contains("localhost:50051") && message.contains(missing));
        assert!(!message.contains("BEGIN") && !message.contains(key_body(&pki.client_key)));
        assert!(std::error::Error::source(&error).is_none());
        assert!(config.connect_lazy().is_err());
    }
}

#[test]
fn half_a_client_identity_is_refused_on_a_plaintext_channel_too() {
    let config = ClientConfig::new("localhost", 50051)
        .with_use_secure_channel(false)
        .with_client_identity("", &pki().client_key);
    assert!(matches!(
        config.validate(),
        Err(ChannelError::IncompleteClientIdentity { .. })
    ));
}

#[tokio::test]
async fn a_client_identity_on_a_plaintext_channel_is_refused() {
    let pki = pki();
    let config = ClientConfig::new("localhost", 50051)
        .with_use_secure_channel(false)
        .with_client_identity(&pki.client_cert, &pki.client_key);
    let error = config.connect().await.unwrap_err();
    assert!(
        matches!(error, ChannelError::ClientIdentityWithoutTls { .. }),
        "{error:?}"
    );
    let message = error.to_string();
    assert!(message.contains("localhost:50051") && message.contains("use_secure_channel"));
    assert!(!message.contains("BEGIN"));
}

#[test]
fn a_file_path_instead_of_pem_content_is_refused() {
    let as_ca = ClientConfig::new("localhost", 50051).with_grpc_cert("certs/ca.pem");
    let as_client_cert = ClientConfig::new("localhost", 50051)
        .with_client_identity("certs/client.pem", "certs/client.key");
    for (config, field) in [(as_ca, "grpc_cert"), (as_client_cert, "grpc_client_cert")] {
        let error = config.endpoint().unwrap_err();
        assert!(
            matches!(error, ChannelError::NotAPemCertificate { field: f, .. } if f == field),
            "{error:?}"
        );
        assert!(error.to_string().starts_with(field));
        assert!(!error.to_string().contains("certs/"));
    }
}

#[tokio::test]
async fn a_key_that_is_not_pem_is_an_error_naming_no_key() {
    let pki = pki();
    let config = ClientConfig::new("localhost", 50051)
        .with_grpc_cert(&pki.ca)
        .with_client_identity(&pki.client_cert, "not a key");
    let error = config.endpoint().unwrap_err();
    assert!(
        matches!(error, ChannelError::InvalidEndpoint { .. }),
        "{error:?}"
    );
    assert!(std::error::Error::source(&error).is_some());
    let message = error.to_string();
    assert!(message.contains("localhost:50051"));
    assert!(!message.contains("not a key") && !message.contains("BEGIN"));
}

#[test]
fn an_invalid_host_is_an_error() {
    let error = ClientConfig::new("not a host", 50051)
        .endpoint()
        .unwrap_err();
    assert!(
        matches!(error, ChannelError::InvalidEndpoint { .. }),
        "{error:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Target, options, defaults

#[test]
fn host_and_port_brackets_bare_ipv6_literals_only() {
    let target = |host: &str| ClientConfig::new(host, 50051).host_and_port();
    assert_eq!(target("::1"), "[::1]:50051");
    assert_eq!(target("2001:db8::7"), "[2001:db8::7]:50051");
    assert_eq!(target("[::1]"), "[::1]:50051");
    assert_eq!(target("127.0.0.1"), "127.0.0.1:50051");
    assert_eq!(target("nlu.example.com"), "nlu.example.com:50051");
}

#[tokio::test]
async fn the_endpoint_uses_the_scheme_and_the_keepalive_defaults() {
    let secure = ClientConfig::new("::1", 443).endpoint().unwrap();
    assert_eq!(secure.uri().to_string(), "https://[::1]:443/");
    assert_eq!(secure.get_tcp_keepalive(), Some(TCP_KEEPALIVE));
    assert_eq!(
        secure.get_tcp_keepalive_interval(),
        Some(TCP_KEEPALIVE_INTERVAL)
    );
    assert_eq!(
        secure.get_tcp_keepalive_retries(),
        Some(TCP_KEEPALIVE_RETRIES)
    );
    let insecure = ClientConfig::new("localhost", 50051)
        .with_use_secure_channel(false)
        .endpoint()
        .unwrap();
    assert_eq!(insecure.uri().to_string(), "http://localhost:50051/");
    assert_eq!(MAX_MESSAGE_LENGTH, 2_147_483_647);
}

#[test]
fn new_is_secure_with_platform_roots_and_no_identity() {
    let config = ClientConfig::new("localhost", 50051);
    assert!(config.use_secure_channel);
    assert!(config.grpc_cert.is_empty());
    assert!(config.grpc_client_cert.is_empty() && config.grpc_client_key.is_empty());
    assert_eq!(config.tls_domain_name, None);
    assert!(config.validate().is_ok());
}

// ---------------------------------------------------------------------------------------------
// Redaction

#[test]
fn debug_redacts_the_key_and_renders_no_pem() {
    let pki = pki();
    let config = ClientConfig::new("localhost", 50051)
        .with_grpc_cert(&pki.ca)
        .with_client_identity(&pki.client_cert, &pki.client_key);
    let rendered = format!("{config:?}");
    assert!(rendered.contains(&format!("grpc_client_key: {REDACTED}")));
    assert!(rendered.contains(&format!("grpc_cert: <PEM, {} bytes>", pki.ca.len())));
    assert!(!rendered.contains("BEGIN") && !rendered.contains(key_body(&pki.client_key)));
    let pretty = format!("{config:#?}");
    assert!(!pretty.contains("BEGIN") && !pretty.contains(key_body(&pki.client_key)));
}

#[test]
fn debug_renders_an_empty_secret_empty() {
    let rendered = format!("{:?}", ClientConfig::new("localhost", 50051));
    assert!(rendered.contains(r#"grpc_client_key: """#), "{rendered}");
    assert!(rendered.contains(r#"grpc_cert: """#), "{rendered}");
    assert!(!rendered.contains(REDACTED));
}

// ---------------------------------------------------------------------------------------------
// The insecure-channel warning

/// A minimal `tracing` subscriber that records the messages of WARN events.
#[derive(Clone, Default)]
struct WarningRecorder {
    messages: Arc<Mutex<Vec<String>>>,
    next_span: Arc<AtomicUsize>,
}

struct MessageVisitor<'a>(&'a mut String);

impl tracing::field::Visit for MessageVisitor<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            *self.0 = format!("{value:?}");
        }
    }
}

impl tracing::Subscriber for WarningRecorder {
    fn enabled(&self, _: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(self.next_span.fetch_add(1, Ordering::Relaxed) as u64 + 1)
    }
    fn record(&self, _: &tracing::span::Id, _: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _: &tracing::span::Id, _: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if *event.metadata().level() == tracing::Level::WARN {
            let mut message = String::new();
            event.record(&mut MessageVisitor(&mut message));
            self.messages.lock().unwrap().push(message);
        }
    }
    fn enter(&self, _: &tracing::span::Id) {}
    fn exit(&self, _: &tracing::span::Id) {}
}

#[test]
fn an_insecure_channel_warns_naming_host_and_port() {
    let recorder = WarningRecorder::default();
    tracing::subscriber::with_default(recorder.clone(), || {
        ClientConfig::new("::1", 50051)
            .with_use_secure_channel(false)
            .endpoint()
            .unwrap();
        ClientConfig::new("localhost", 50051).endpoint().unwrap();
    });
    let messages = recorder.messages.lock().unwrap();
    assert_eq!(
        *messages,
        ["Using an INSECURE (plaintext) gRPC channel to [::1]:50051."]
    );
}
