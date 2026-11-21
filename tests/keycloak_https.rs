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

//! The default token-endpoint transport over REAL TLS handshakes with a loopback rustls endpoint.
//!
//! `verify_ssl = true` trusts the platform's store, which honours `SSL_CERT_FILE`; this binary
//! points it at a throw-away CA. Setting an environment variable races every concurrent reader, so
//! the whole binary is ONE test that runs its cases in sequence.

mod support;

use std::time::Duration;

use ondewo_t2s_client::keycloak::{
    HttpTokenTransport, KeycloakConfig, KeycloakProviderOptions, KeycloakTokenProvider,
    TokenRequest, TokenTransport, TransportErrorKind,
};
use support::{http_answer, json_answer, make_pki, serve_tls, Answer};

const TOKEN: &str = r#"{"access_token":"acc-tls-1","expires_in":3600}"#;

fn request(url: String, verify_ssl: bool) -> TokenRequest {
    TokenRequest {
        url,
        form: vec![("grant_type".into(), "refresh_token".into())],
        timeout: Duration::from_secs(10),
        verify_ssl,
    }
}

#[test]
fn the_https_transport_verifies_or_skips_the_certificate_as_configured() {
    let directory =
        std::env::temp_dir().join(format!("ondewo-keycloak-https-pki-{}", std::process::id()));
    let pki = make_pki(&directory);
    let ca_file = directory.join("ca.pem");
    std::env::set_var("SSL_CERT_FILE", &ca_file);
    std::env::remove_var("SSL_CERT_DIR");

    // 1. verify_ssl = true against a server of the trusted CA, by name and by IP.
    let endpoint = serve_tls(
        &pki.server_cert,
        &pki.server_key,
        false,
        vec![json_answer(TOKEN), json_answer(TOKEN)],
    );
    for host in ["localhost", "127.0.0.1"] {
        let response = HttpTokenTransport
            .post_form(&request(endpoint.url("https", host, "/auth/token"), true))
            .unwrap_or_else(|error| panic!("{host}: {error}"));
        assert_eq!((response.status, response.body.as_str()), (200, TOKEN));
    }
    let requests = endpoint.finish();
    assert!(
        requests[0].starts_with("POST /auth/token HTTP/1.1\r\n"),
        "{}",
        requests[0]
    );
    assert!(requests[0].contains("Host: localhost:"));

    // 2. verify_ssl = true against a server of ANOTHER CA: the handshake is refused.
    let endpoint = serve_tls(
        &pki.other_server_cert,
        &pki.other_server_key,
        false,
        vec![json_answer(TOKEN)],
    );
    let error = HttpTokenTransport
        .post_form(&request(endpoint.url("https", "localhost", "/token"), true))
        .unwrap_err();
    assert_eq!(error.kind(), TransportErrorKind::Unreachable);
    assert!(
        error.to_string().contains("invalid peer certificate"),
        "{error}"
    );

    // 3. verify_ssl = false accepts it, over TLS 1.3 and TLS 1.2.
    for tls12_only in [false, true] {
        let endpoint = serve_tls(
            &pki.other_server_cert,
            &pki.other_server_key,
            tls12_only,
            vec![json_answer(TOKEN)],
        );
        let response = HttpTokenTransport
            .post_form(&request(
                endpoint.url("https", "localhost", "/token"),
                false,
            ))
            .unwrap_or_else(|error| panic!("tls12_only={tls12_only}: {error}"));
        assert_eq!(response.body, TOKEN);
        endpoint.finish();
    }

    // 4. A server that closes without a close_notify still delivers a length-less answer.
    let answer = Answer::RawNoCloseNotify(http_answer("200 OK", &[], TOKEN));
    let endpoint = serve_tls(&pki.server_cert, &pki.server_key, false, vec![answer]);
    let response = HttpTokenTransport
        .post_form(&request(endpoint.url("https", "localhost", "/token"), true))
        .unwrap();
    assert_eq!(response.body, TOKEN);
    endpoint.finish();

    // 5. The provider end to end over TLS with the default options, verify_ssl = false.
    let endpoint = serve_tls(
        &pki.other_server_cert,
        &pki.other_server_key,
        false,
        vec![json_answer(TOKEN)],
    );
    let config = KeycloakConfig::new(
        endpoint.url("https", "localhost", "/auth"),
        "r",
        "c",
        "u",
        "",
    )
    .with_refresh_token("off-tls-0")
    .with_verify_ssl(false);
    let options = KeycloakProviderOptions {
        start_background_refresh: false,
        ..KeycloakProviderOptions::default()
    };
    let provider = KeycloakTokenProvider::with_options(&config, options).unwrap();
    assert_eq!(provider.access_token(), "acc-tls-1");
    let requests = endpoint.finish();
    assert!(
        requests[0].ends_with("grant_type=refresh_token&client_id=c&refresh_token=off-tls-0"),
        "{}",
        requests[0]
    );

    std::fs::remove_dir_all(&directory).ok();
}
