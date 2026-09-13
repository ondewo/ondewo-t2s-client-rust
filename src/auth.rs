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

//! Bearer-token authentication for the generated gRPC clients.
//!
//! This module is HAND-WRITTEN - it is the rust counterpart of the `auth` surface the python and
//! typescript ONDEWO clients ship beside their generated stubs. It deliberately stops at
//! *attaching* credentials: obtaining and refreshing the Keycloak access token is the caller's
//! job (or that of an ingress), so this module pulls in no HTTP client and no async runtime.
//!
//! `examples/authenticated_client.rs` shows the whole flow. In short:
//!
//! ```text
//! let interceptor = BearerTokenInterceptor::new(access_token)?.with_cai_token(cai_token)?;
//! let mut client = Text2SpeechClient::with_interceptor(channel, interceptor);
//! ```
//!
//! Doc examples are `text` rather than rust here because doctests are disabled crate-wide - the
//! generated stubs carry proto snippets in their doc comments that rustdoc cannot compile. See
//! the `doctest = false` note in `Cargo.toml`.

use std::fmt;

use tonic::metadata::{Ascii, MetadataValue};
use tonic::service::Interceptor;
use tonic::{Request, Status};

/// Metadata key carrying the `Bearer` credentials, as expected by the ONDEWO servers.
pub const AUTHORIZATION_METADATA_KEY: &str = "authorization";

/// Metadata key carrying the ONDEWO CAI project token, as expected by the ONDEWO servers.
pub const CAI_TOKEN_METADATA_KEY: &str = "cai-token";

/// A [`tonic::service::Interceptor`] that attaches ONDEWO credentials to every outgoing request.
///
/// Construct it once and hand it to any generated `…Client::with_interceptor`; it is cheap to
/// clone, because the metadata values are validated and encoded at construction time rather than
/// on every call.
///
/// Its [`fmt::Debug`] output redacts both tokens, so logging a client or a channel that carries
/// this interceptor cannot leak credentials.
#[derive(Clone)]
pub struct BearerTokenInterceptor {
    authorization: MetadataValue<Ascii>,
    cai_token: Option<MetadataValue<Ascii>>,
}

impl BearerTokenInterceptor {
    /// Build an interceptor sending `authorization: Bearer <access_token>`.
    ///
    /// # Errors
    ///
    /// Returns [`Status::invalid_argument`] when `access_token` is blank or holds a byte that is
    /// not valid in an HTTP header value. Rejecting it here rather than on the first RPC keeps a
    /// malformed token from surfacing as an opaque transport error much later.
    pub fn new(access_token: &str) -> Result<Self, Status> {
        let access_token = access_token.trim();
        if access_token.is_empty() {
            return Err(Status::invalid_argument(
                "the access token must not be empty",
            ));
        }
        let authorization = format!("Bearer {access_token}")
            .parse::<MetadataValue<Ascii>>()
            .map_err(|_| {
                Status::invalid_argument("the access token is not a valid HTTP header value")
            })?;
        Ok(Self {
            authorization,
            cai_token: None,
        })
    }

    /// Additionally send `cai-token: <cai_token>` on every request.
    ///
    /// # Errors
    ///
    /// Returns [`Status::invalid_argument`] when `cai_token` is blank or holds a byte that is not
    /// valid in an HTTP header value.
    pub fn with_cai_token(mut self, cai_token: &str) -> Result<Self, Status> {
        let cai_token = cai_token.trim();
        if cai_token.is_empty() {
            return Err(Status::invalid_argument("the cai token must not be empty"));
        }
        self.cai_token = Some(cai_token.parse::<MetadataValue<Ascii>>().map_err(|_| {
            Status::invalid_argument("the cai token is not a valid HTTP header value")
        })?);
        Ok(self)
    }
}

impl fmt::Debug for BearerTokenInterceptor {
    /// Never renders a token: a credential that reaches a log is a leaked credential.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BearerTokenInterceptor")
            .field("authorization", &"<redacted>")
            .field("cai_token", &self.cai_token.as_ref().map(|_| "<redacted>"))
            .finish()
    }
}

impl Interceptor for BearerTokenInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let metadata = request.metadata_mut();
        metadata.insert(AUTHORIZATION_METADATA_KEY, self.authorization.clone());
        if let Some(cai_token) = &self.cai_token {
            metadata.insert(CAI_TOKEN_METADATA_KEY, cai_token.clone());
        }
        Ok(request)
    }
}
