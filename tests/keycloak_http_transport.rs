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

//! The default token-endpoint transport (`HttpTokenTransport`) against a scripted PLAINTEXT
//! loopback endpoint, and the provider, the registry and the interceptor wired to it - the path a
//! real deployment takes. TLS is covered by `tests/keycloak_https.rs`.

mod support;

use std::error::Error;
use std::net::TcpListener;
use std::time::{Duration, Instant};

use ondewo_t2s_client::keycloak::{
    get_keycloak_token_provider, HttpTokenTransport, KeycloakConfig, KeycloakInterceptor,
    KeycloakTokenProvider, TokenRequest, TokenTransport, TransportError, TransportErrorKind,
};
use support::{http_answer, json_answer, serve, Answer};
use tonic::service::Interceptor;
use tonic::Request;

const TOKEN: &str =
    r#"{"access_token":"acc-http-1","expires_in":3600,"refresh_token":"off-http-1"}"#;

fn request(url: String) -> TokenRequest {
    TokenRequest {
        url,
        form: vec![
            ("grant_type".into(), "password".into()),
            ("password".into(), "p w&=ü/+%".into()),
        ],
        timeout: Duration::from_secs(10),
        verify_ssl: true,
    }
}

fn post(url: String) -> Result<ondewo_t2s_client::keycloak::TokenHttpResponse, TransportError> {
    HttpTokenTransport.post_form(&request(url))
}

fn protocol_failure(answer: Answer) -> TransportError {
    let endpoint = serve("127.0.0.1", vec![answer]).unwrap();
    let error = post(endpoint.url("http", "127.0.0.1", "/token")).unwrap_err();
    endpoint.finish();
    assert_eq!(
        (error.kind(), error.type_name()),
        (TransportErrorKind::Other, "HttpProtocolError")
    );
    error
}

#[test]
fn a_content_length_answer_is_read_and_the_request_is_a_form_post() {
    let endpoint = serve("127.0.0.1", vec![json_answer(TOKEN)]).unwrap();
    let url = endpoint.url(
        "http",
        "127.0.0.1",
        "/auth/realms/r/protocol/openid-connect/token?x=1",
    );

    let port = endpoint.address.port();
    let response = post(url).unwrap();

    assert_eq!((response.status, response.body.as_str()), (200, TOKEN));
    let requests = endpoint.finish();
    let sent = &requests[0];
    assert!(
        sent.starts_with("POST /auth/realms/r/protocol/openid-connect/token?x=1 HTTP/1.1\r\n"),
        "{sent}"
    );
    for header in [
        "Content-Type: application/x-www-form-urlencoded\r\n",
        "Connection: close\r\n",
        "Accept: application/json\r\n",
        "Content-Length: 53\r\n",
    ] {
        assert!(sent.contains(header), "{header} missing from {sent}");
    }
    assert!(
        sent.contains(&format!("Host: 127.0.0.1:{port}\r\n")),
        "{sent}"
    );
    assert!(
        sent.ends_with("\r\n\r\ngrant_type=password&password=p+w%26%3D%C3%BC%2F%2B%25"),
        "{sent}"
    );
}

#[test]
fn a_trickled_answer_is_reassembled() {
    let body = r#"{"access_token":"acc-trickle"}"#;
    let answer = http_answer(
        "200 OK",
        &[
            &format!("content-length: {}", body.len()),
            "X-Without-Colon",
        ],
        body,
    );
    let endpoint = serve("127.0.0.1", vec![Answer::Trickle(answer)]).unwrap();

    let response = post(endpoint.url("http", "127.0.0.1", "/token")).unwrap();

    assert_eq!(response.body, body);
    endpoint.finish();
}

#[test]
fn a_chunked_answer_is_decoded_even_when_it_arrives_byte_by_byte() {
    let answer = http_answer(
        "200 OK",
        &["Transfer-Encoding: chunked"],
        "5;ext=1\r\n{\"acc\r\nC\r\ness_token\":1\r\n1\r\n}\r\n0\r\n\r\n",
    );
    let endpoint = serve(
        "127.0.0.1",
        vec![Answer::Trickle(answer.clone()), Answer::Raw(answer)],
    )
    .unwrap();
    let url = endpoint.url("http", "127.0.0.1", "/token");

    for _ in 0..2 {
        assert_eq!(post(url.clone()).unwrap().body, r#"{"access_token":1}"#);
    }
    endpoint.finish();
}

#[test]
fn an_answer_without_a_length_ends_with_the_connection() {
    let endpoint = serve(
        "127.0.0.1",
        vec![Answer::Raw(http_answer(
            "503 Service Unavailable",
            &[],
            "<html>down</html>",
        ))],
    )
    .unwrap();

    let response = post(endpoint.url("http", "127.0.0.1", "/token")).unwrap();

    assert_eq!(
        (response.status, response.body.as_str()),
        (503, "<html>down</html>")
    );
    endpoint.finish();
}

#[test]
fn malformed_answers_are_protocol_errors() {
    let cases = [
        (
            http_answer("200 OK", &["Content-Length: 50"], "{}"),
            "the token endpoint's response body is truncated",
        ),
        (
            http_answer("200 OK", &["Transfer-Encoding: chunked"], "5\r\n{\"a"),
            "the token endpoint's response body is truncated",
        ),
        (
            http_answer(
                "200 OK",
                &["Transfer-Encoding: chunked"],
                "zz\r\n{}\r\n0\r\n\r\n",
            ),
            "the token endpoint answered with an invalid chunk size",
        ),
        (
            http_answer("200 OK", &["Content-Length: many"], "{}"),
            "the token endpoint answered with an invalid Content-Length",
        ),
        (
            b"SPDY/3 200 OK\r\n\r\n{}".to_vec(),
            "the token endpoint answered without an HTTP/1.x status line",
        ),
        (
            b"HTTP/1.1 2x0 OK\r\n\r\n{}".to_vec(),
            "the token endpoint answered without an HTTP/1.x status line",
        ),
        (
            b"HTTP/1.1 200 OK\r\nContent-".to_vec(),
            "the connection closed before a complete HTTP response",
        ),
        (
            Vec::new(),
            "the connection closed before a complete HTTP response",
        ),
    ];
    for (answer, message) in cases {
        let error = protocol_failure(Answer::Raw(answer));
        assert_eq!(error.to_string(), message);
        assert!(error.source().is_some());
    }
}

#[test]
fn a_silent_endpoint_times_out_within_the_bound() {
    let endpoint = serve("127.0.0.1", vec![Answer::Silent(Duration::from_secs(1))]).unwrap();
    let mut request = request(endpoint.url("http", "127.0.0.1", "/token"));
    request.timeout = Duration::from_millis(300);

    let started = Instant::now();
    let error = HttpTokenTransport.post_form(&request).unwrap_err();

    assert!(
        started.elapsed() < Duration::from_secs(2),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(
        (error.kind(), error.type_name()),
        (TransportErrorKind::Unreachable, "TimedOut")
    );
    assert_eq!(
        error.to_string(),
        "the token endpoint did not answer within the timeout"
    );
    endpoint.finish();
}

#[test]
fn a_spent_timeout_fails_before_connecting() {
    let mut request = request("http://127.0.0.1:9/token".to_string());
    request.timeout = Duration::ZERO;
    let error = HttpTokenTransport.post_form(&request).unwrap_err();
    assert_eq!(error.type_name(), "TimedOut");
}

#[test]
fn a_closed_port_is_unreachable() {
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let error = post(format!("http://127.0.0.1:{port}/token")).unwrap_err();
    assert_eq!(
        (error.kind(), error.type_name()),
        (TransportErrorKind::Unreachable, "ConnectionRefused")
    );
}

#[test]
fn an_unresolvable_host_is_unreachable() {
    let error = post("http://ondewo-keycloak-test.invalid/token".to_string()).unwrap_err();
    assert_eq!(error.kind(), TransportErrorKind::Unreachable);
}

#[test]
fn unusable_urls_are_refused_before_any_connection() {
    for (url, type_name) in [
        ("not a url", "InvalidUri"),
        ("/realms/r/token", "HttpProtocolError"),
        ("ftp://keycloak.example/token", "HttpProtocolError"),
        (
            "https://user:secret@keycloak.example/token",
            "HttpProtocolError",
        ),
        // A host TLS could never verify is refused before any connection is made.
        ("https://keycloak..example/token", "InvalidDnsNameError"),
    ] {
        let error = post(url.to_string()).unwrap_err();
        assert_eq!(
            (error.kind(), error.type_name()),
            (TransportErrorKind::Other, type_name),
            "{url}"
        );
        assert!(!error.to_string().contains("secret"), "{error}");
    }
}

#[test]
fn an_ipv6_literal_is_bracketed_in_the_host_header() {
    let Some(endpoint) = serve("::1", vec![json_answer(TOKEN)]) else {
        eprintln!("no IPv6 loopback on this host - skipped");
        return;
    };
    let response = post(endpoint.url("http", "[::1]", "/token")).unwrap();
    assert_eq!(response.status, 200);
    let requests = endpoint.finish();
    assert!(requests[0].contains("Host: [::1]:"), "{}", requests[0]);
}

// ---------------------------------------------------------------------------------------------
// The provider, the registry and the interceptor on the default transport and options

fn config_for(url: &str, username: &str) -> KeycloakConfig {
    KeycloakConfig::new(url, "r", "ondewo-t2s-cai-sdk-public", username, "pw-http")
}

#[test]
fn a_provider_logs_in_over_http_with_the_default_options() {
    let endpoint = serve("127.0.0.1", vec![json_answer(TOKEN)]).unwrap();
    let base = endpoint.url("http", "127.0.0.1", "/auth/");

    let provider = KeycloakTokenProvider::new(&config_for(&base, "http-user-1")).unwrap();

    assert_eq!(
        provider.authorization_metadata().unwrap().1,
        "Bearer acc-http-1"
    );
    assert!(provider.is_background_refresh_running());
    drop(provider);
    let requests = endpoint.finish();
    assert!(requests[0].starts_with("POST /auth/realms/r/protocol/openid-connect/token HTTP/1.1"));
    assert!(requests[0].contains("grant_type=password&client_id=ondewo-t2s-cai-sdk-public&username=http-user-1&password=pw-http&scope=offline_access"));
}

#[test]
fn the_interceptor_of_a_config_shares_one_login() {
    let endpoint = serve("127.0.0.1", vec![json_answer(TOKEN)]).unwrap();
    let config = config_for(&endpoint.url("http", "127.0.0.1", "/auth"), "http-user-2");

    let mut first = KeycloakInterceptor::new(&config).unwrap();
    let second = KeycloakInterceptor::new(&config).unwrap();
    let shared = get_keycloak_token_provider(&config).unwrap();

    assert!(std::sync::Arc::ptr_eq(
        first.provider().unwrap(),
        second.provider().unwrap()
    ));
    assert!(std::sync::Arc::ptr_eq(first.provider().unwrap(), &shared));
    let request = first.call(Request::new(())).unwrap();
    assert_eq!(
        request.metadata().get("authorization").unwrap(),
        "Bearer acc-http-1"
    );
    assert_eq!(
        endpoint.finish().len(),
        1,
        "one login for one credential set"
    );
}

#[test]
fn an_unreachable_keycloak_fails_the_interceptor_with_the_typed_error() {
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let config = config_for(&format!("http://127.0.0.1:{port}/auth"), "http-user-3");
    let error = KeycloakInterceptor::new(&config).unwrap_err();
    assert!(
        error.to_string().starts_with(
            "Keycloak token login could not reach the token endpoint (ConnectionRefused: "
        ),
        "{error}"
    );
}
