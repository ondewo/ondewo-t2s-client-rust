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

//! Unit tests for the HAND-WRITTEN `auth` module - every branch of it.
//!
//! `tests/generated_grpc.rs` additionally proves the metadata really reaches a server; here the
//! interceptor is driven directly so the rejection paths are observable.

use ondewo_t2s_client::auth::{
    BearerTokenInterceptor, AUTHORIZATION_METADATA_KEY, CAI_TOKEN_METADATA_KEY,
};
use tonic::service::Interceptor;
use tonic::{Code, Request};

/// Run the interceptor over an empty request and return the metadata it attached.
fn intercept(mut interceptor: BearerTokenInterceptor) -> Request<()> {
    interceptor
        .call(Request::new(()))
        .expect("the interceptor never rejects a request")
}

fn metadata(request: &Request<()>, key: &str) -> Option<String> {
    request
        .metadata()
        .get(key)
        .map(|value| value.to_str().unwrap().to_string())
}

#[test]
fn the_metadata_keys_are_the_ones_the_ondewo_servers_expect() {
    assert_eq!(AUTHORIZATION_METADATA_KEY, "authorization");
    assert_eq!(CAI_TOKEN_METADATA_KEY, "cai-token");
}

#[test]
fn a_bearer_token_is_attached_to_every_request() {
    let mut interceptor = BearerTokenInterceptor::new("access-token-abc").unwrap();

    // Twice, because the interceptor is called once per RPC and must stay usable.
    for _ in 0..2 {
        let request = interceptor.call(Request::new(())).unwrap();
        assert_eq!(
            metadata(&request, AUTHORIZATION_METADATA_KEY).as_deref(),
            Some("Bearer access-token-abc")
        );
        assert_eq!(metadata(&request, CAI_TOKEN_METADATA_KEY), None);
    }
}

#[test]
fn surrounding_whitespace_is_trimmed_off_both_tokens() {
    let request = intercept(
        BearerTokenInterceptor::new("  access-token-abc\n")
            .unwrap()
            .with_cai_token("\tcai-token-xyz ")
            .unwrap(),
    );

    assert_eq!(
        metadata(&request, AUTHORIZATION_METADATA_KEY).as_deref(),
        Some("Bearer access-token-abc")
    );
    assert_eq!(
        metadata(&request, CAI_TOKEN_METADATA_KEY).as_deref(),
        Some("cai-token-xyz")
    );
}

#[test]
fn the_cai_token_is_attached_when_it_is_configured() {
    let request = intercept(
        BearerTokenInterceptor::new("access-token-abc")
            .unwrap()
            .with_cai_token("cai-token-xyz")
            .unwrap(),
    );

    assert_eq!(
        metadata(&request, CAI_TOKEN_METADATA_KEY).as_deref(),
        Some("cai-token-xyz")
    );
}

#[test]
fn a_clone_carries_the_same_credentials() {
    let interceptor = BearerTokenInterceptor::new("access-token-abc")
        .unwrap()
        .with_cai_token("cai-token-xyz")
        .unwrap();

    let original = intercept(interceptor.clone());
    let clone = intercept(interceptor);

    assert_eq!(
        metadata(&original, AUTHORIZATION_METADATA_KEY),
        metadata(&clone, AUTHORIZATION_METADATA_KEY)
    );
    assert_eq!(
        metadata(&original, CAI_TOKEN_METADATA_KEY),
        metadata(&clone, CAI_TOKEN_METADATA_KEY)
    );
}

/// A credential that reaches a log is a leaked credential, so `Debug` must redact both tokens.
#[test]
fn the_debug_output_redacts_the_credentials() {
    let without_cai_token = format!(
        "{:?}",
        BearerTokenInterceptor::new("access-token-abc").unwrap()
    );
    let with_cai_token = format!(
        "{:?}",
        BearerTokenInterceptor::new("access-token-abc")
            .unwrap()
            .with_cai_token("cai-token-xyz")
            .unwrap()
    );

    for rendered in [&without_cai_token, &with_cai_token] {
        assert!(
            rendered.contains("BearerTokenInterceptor"),
            "unexpected Debug output: {rendered}"
        );
        assert!(
            !rendered.contains("access-token-abc") && !rendered.contains("cai-token-xyz"),
            "Debug must not render a credential: {rendered}"
        );
    }
    assert!(without_cai_token.contains("None"), "{without_cai_token}");
    assert!(with_cai_token.contains("Some"), "{with_cai_token}");
}

#[test]
fn a_blank_access_token_is_rejected_at_construction_time() {
    for blank in ["", "   ", "\t\n"] {
        let error =
            BearerTokenInterceptor::new(blank).expect_err("a blank access token must be rejected");
        assert_eq!(error.code(), Code::InvalidArgument);
        assert_eq!(error.message(), "the access token must not be empty");
    }
}

/// An embedded control character would be a header-injection vector, so it must be refused
/// outright. The surrounding whitespace trim cannot mask it - the newline is in the middle.
#[test]
fn an_access_token_that_is_not_a_valid_header_value_is_rejected() {
    let error = BearerTokenInterceptor::new("access\ntoken")
        .expect_err("an access token with a control character must be rejected");

    assert_eq!(error.code(), Code::InvalidArgument);
    assert_eq!(
        error.message(),
        "the access token is not a valid HTTP header value"
    );
}

#[test]
fn a_blank_cai_token_is_rejected_at_construction_time() {
    let error = BearerTokenInterceptor::new("access-token-abc")
        .unwrap()
        .with_cai_token("  ")
        .expect_err("a blank cai token must be rejected");

    assert_eq!(error.code(), Code::InvalidArgument);
    assert_eq!(error.message(), "the cai token must not be empty");
}

#[test]
fn a_cai_token_that_is_not_a_valid_header_value_is_rejected() {
    let error = BearerTokenInterceptor::new("access-token-abc")
        .unwrap()
        .with_cai_token("cai\rtoken")
        .expect_err("a cai token with a control character must be rejected");

    assert_eq!(error.code(), Code::InvalidArgument);
    assert_eq!(
        error.message(),
        "the cai token is not a valid HTTP header value"
    );
}
