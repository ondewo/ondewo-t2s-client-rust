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

//! Headless Keycloak authentication: an auto-refreshing access token for every gRPC call.
//!
//! This module is HAND-WRITTEN - it is the rust counterpart of `ondewo/t2s/utils/keycloak.py` of
//! the python SDK and follows the cross-language contract every ONDEWO client SDK implements:
//!
//! 1. The provider acquires an access token ONCE at construction: with a handed-off Keycloak
//!    OFFLINE refresh token ([`KeycloakConfig::refresh_token`]) as one `grant_type=refresh_token`
//!    exchange, or else with one password grant (`grant_type=password`,
//!    `scope=offline_access`) against the PUBLIC SDK client (no client secret).
//! 2. A background thread refreshes the access token [`EXPIRY_LEEWAY_S`] before it expires, and
//!    every RPC re-checks it lazily ([`KeycloakTokenProvider::authorization_metadata`]).
//! 3. A failed refresh NEVER ends the background loop and NEVER falls back to a password grant.
//!    It is retried with a jittered exponential backoff ([`RETRY_BASE_DELAY_S`] to
//!    [`RETRY_MAX_DELAY_S`]) that the lazy path shares, so an outage costs one token request per
//!    backoff step instead of one per RPC. A token that is still valid is served throughout; only
//!    an EXPIRED token that cannot be refreshed fails the call, with
//!    [`KeycloakAuthenticationError`].
//! 4. [`get_keycloak_token_provider`] shares ONE provider per credential set (keyed by a SHA-256
//!    of the credentials, never by an address), so all clients of one identity log in once.
//!
//! The offline-token start-up exists because Keycloak's brute-force detection counts failed
//! LOGINS: N processes sending a password grant in the same instant are refused, N processes
//! refreshing one offline token are not.
//!
//! ```text
//! let config = KeycloakConfig::new(
//!     "https://keycloak.example.com/auth", "ondewo-ccai-platform", "ondewo-t2s-cai-sdk-public",
//!     "technical-user@example.com", "",
//! )
//! .with_refresh_token(std::env::var("ONDEWO_T2S_OFFLINE_TOKEN")?);
//! let interceptor = KeycloakInterceptor::new(&config)?;
//! let mut client = Text2SpeechClient::with_interceptor(channel, interceptor);
//! ```
//!
//! No error message, log line, [`fmt::Debug`] rendering or registry key ever contains a password,
//! a refresh token or an access token.

mod http_transport;

use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::error::Error;
use std::fmt;
use std::hash::{BuildHasher, Hasher};
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::{Map, Value};
use tonic::metadata::{Ascii, MetadataValue};
use tonic::service::Interceptor;
use tonic::{Request, Status};

pub use http_transport::HttpTokenTransport;

use crate::auth::AUTHORIZATION_METADATA_KEY;
use crate::channel::Redacted;

// ---------------------------------------------------------------------------------------------
// Constants - identical in every ONDEWO SDK

/// Refresh the access token this many seconds BEFORE it expires.
pub const EXPIRY_LEEWAY_S: f64 = 30.0;
/// Floor of every background wait, so an `expires_in` of 0 cannot spin the thread.
pub const MIN_REFRESH_DELAY_S: f64 = 1.0;
/// The first retry after a failed refresh - far above the floor: a Keycloak outage must not turn
/// into a 1 Hz poll per client against a realm that every call container shares.
pub const RETRY_BASE_DELAY_S: f64 = 5.0;
/// Ceiling of the failure backoff.
pub const RETRY_MAX_DELAY_S: f64 = 300.0;
/// Cap of the backoff exponent (`5 * 2^6 = 320 >= 300`), so a huge failure count cannot overflow.
pub const MAX_RETRY_EXPONENT: u32 = 6;
/// Bound of one whole token-endpoint call. It must stay below [`EXPIRY_LEEWAY_S`]: a refresh holds
/// the provider lock, and every RPC reads its token under that lock.
pub const HTTP_TIMEOUT_S: f64 = 10.0;
/// The longest slice of an error body quoted in an error message.
pub const MAX_ERROR_BODY_CHARS: usize = 500;
/// The longest [`KeycloakTokenProvider::stop`] waits for the background thread.
pub const THREAD_JOIN_TIMEOUT_S: f64 = 5.0;

/// The longest single background wait. Only bounds absurd `expires_in` values (a year).
const MAX_WAIT_S: f64 = 365.0 * 24.0 * 3600.0;

/// The tunable timings of a provider. [`Default`] is the production table above; tests shrink it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct KeycloakTimings {
    /// See [`EXPIRY_LEEWAY_S`].
    pub expiry_leeway_s: f64,
    /// See [`MIN_REFRESH_DELAY_S`].
    pub min_refresh_delay_s: f64,
    /// See [`RETRY_BASE_DELAY_S`].
    pub retry_base_delay_s: f64,
    /// See [`RETRY_MAX_DELAY_S`].
    pub retry_max_delay_s: f64,
    /// See [`MAX_RETRY_EXPONENT`].
    pub max_retry_exponent: u32,
    /// See [`HTTP_TIMEOUT_S`].
    pub http_timeout_s: f64,
    /// See [`MAX_ERROR_BODY_CHARS`].
    pub max_error_body_chars: usize,
    /// See [`THREAD_JOIN_TIMEOUT_S`].
    pub thread_join_timeout_s: f64,
}

impl Default for KeycloakTimings {
    fn default() -> Self {
        Self {
            expiry_leeway_s: EXPIRY_LEEWAY_S,
            min_refresh_delay_s: MIN_REFRESH_DELAY_S,
            retry_base_delay_s: RETRY_BASE_DELAY_S,
            retry_max_delay_s: RETRY_MAX_DELAY_S,
            max_retry_exponent: MAX_RETRY_EXPONENT,
            http_timeout_s: HTTP_TIMEOUT_S,
            max_error_body_chars: MAX_ERROR_BODY_CHARS,
            thread_join_timeout_s: THREAD_JOIN_TIMEOUT_S,
        }
    }
}

impl KeycloakTimings {
    /// The wait before the next attempt after `consecutive_failures` failed refreshes.
    ///
    /// Full jitter with a doubling ceiling: uniform in `[base, min(base * 2^(n-1), max)]`, where
    /// `fraction` (clamped to `[0, 1]`) picks the point. `n <= 0` behaves as `n = 1`. The jitter is
    /// the load-bearing half: containers whose refreshes failed in the same instant would otherwise
    /// retry in lockstep for the whole outage.
    pub fn backoff_delay_s(&self, consecutive_failures: i64, fraction: f64) -> f64 {
        let exponent = (consecutive_failures.max(1) - 1).min(i64::from(self.max_retry_exponent));
        let ceiling =
            (self.retry_base_delay_s * 2f64.powi(exponent as i32)).min(self.retry_max_delay_s);
        let fraction = if fraction.is_nan() {
            0.0
        } else {
            fraction.clamp(0.0, 1.0)
        };
        self.retry_base_delay_s + fraction * (ceiling - self.retry_base_delay_s)
    }
}

// ---------------------------------------------------------------------------------------------
// Configuration

/// The Keycloak credentials a client authenticates with.
///
/// All fields are public, `refresh_token` is deliberately the LAST one. Build it with
/// [`KeycloakConfig::new`] and the `with_*` methods, or with `..Default::default()`; nothing is
/// checked until [`validate`](Self::validate) (which every provider and interceptor runs first).
/// [`fmt::Debug`] renders a non-empty `password` / `refresh_token` as `***REDACTED***`.
#[derive(Clone, PartialEq, Eq)]
pub struct KeycloakConfig {
    /// Base URL of Keycloak, the part before `/realms/<realm>` (`https://host/auth`). Trailing
    /// `/` characters are stripped.
    pub keycloak_url: String,
    /// Realm, e.g. `ondewo-ccai-platform`.
    pub realm: String,
    /// The PUBLIC SDK client id, e.g. `ondewo-t2s-cai-sdk-public`. No client secret is ever sent.
    pub client_id: String,
    /// The technical user of the password grant.
    pub username: String,
    /// The technical user's password. SECRET. Never sent when `refresh_token` is set.
    pub password: String,
    /// Bounds how long auto-refresh runs, in seconds from the initial token acquisition. `None`:
    /// refresh until the offline session itself dies.
    pub token_expiration_in_s: Option<u64>,
    /// Verify Keycloak's TLS certificate (the token-endpoint call only). `false` is for a
    /// self-signed local ingress.
    pub verify_ssl: bool,
    /// A handed-off Keycloak OFFLINE refresh token used instead of the password. SECRET.
    pub refresh_token: String,
}

impl Default for KeycloakConfig {
    fn default() -> Self {
        Self {
            keycloak_url: String::new(),
            realm: String::new(),
            client_id: String::new(),
            username: String::new(),
            password: String::new(),
            token_expiration_in_s: None,
            verify_ssl: true,
            refresh_token: String::new(),
        }
    }
}

impl KeycloakConfig {
    /// A password-grant config. Pass an empty `password` and add
    /// [`with_refresh_token`](Self::with_refresh_token) for the offline-token start-up.
    pub fn new(
        keycloak_url: impl Into<String>,
        realm: impl Into<String>,
        client_id: impl Into<String>,
        username: impl Into<String>,
        password: impl Into<String>,
    ) -> Self {
        Self {
            keycloak_url: keycloak_url.into(),
            realm: realm.into(),
            client_id: client_id.into(),
            username: username.into(),
            password: password.into(),
            ..Self::default()
        }
    }

    /// Start up with this offline refresh token instead of a password grant. When it is set, no
    /// password grant is ever sent, whatever `password` holds.
    pub fn with_refresh_token(mut self, refresh_token: impl Into<String>) -> Self {
        self.refresh_token = refresh_token.into();
        self
    }

    /// Stop auto-refreshing this many seconds after the initial token acquisition.
    pub fn with_token_expiration_in_s(mut self, token_expiration_in_s: u64) -> Self {
        self.token_expiration_in_s = Some(token_expiration_in_s);
        self
    }

    /// `false` skips the TLS verification of the token endpoint (a self-signed local ingress).
    pub fn with_verify_ssl(mut self, verify_ssl: bool) -> Self {
        self.verify_ssl = verify_ssl;
        self
    }

    /// `true` when `keycloak_url`, `realm` and `client_id` are all set. Otherwise no token is
    /// acquired and no `authorization` metadata is attached.
    pub fn is_keycloak_configured(&self) -> bool {
        !self.keycloak_url.is_empty() && !self.realm.is_empty() && !self.client_id.is_empty()
    }

    /// Check the config without any network I/O.
    ///
    /// # Errors
    ///
    /// * [`KeycloakConfigError::IncompleteKeycloakFields`] - some but not all of `keycloak_url`,
    ///   `realm` and `client_id` are set.
    /// * [`KeycloakConfigError::MissingCredential`] - Keycloak is configured but neither
    ///   `password` nor `refresh_token` is set.
    pub fn validate(&self) -> Result<(), KeycloakConfigError> {
        let set = [&self.keycloak_url, &self.realm, &self.client_id]
            .iter()
            .filter(|field| !field.is_empty())
            .count();
        if set != 0 && set != 3 {
            return Err(KeycloakConfigError::IncompleteKeycloakFields);
        }
        if set == 3 && self.password.is_empty() && self.refresh_token.is_empty() {
            return Err(KeycloakConfigError::MissingCredential);
        }
        Ok(())
    }

    /// The token endpoint: `<keycloak_url>/realms/<realm>/protocol/openid-connect/token`.
    pub fn token_endpoint(&self) -> String {
        format!(
            "{}/realms/{}/protocol/openid-connect/token",
            self.keycloak_url.trim_end_matches('/'),
            self.realm
        )
    }

    /// The registry key of this credential set: the SHA-256 hex digest of every field the provider
    /// is built from, joined with `\0`. It never holds a credential in clear text.
    pub fn registry_key(&self) -> String {
        let expiration = self
            .token_expiration_in_s
            .map(|seconds| seconds.to_string())
            .unwrap_or_default();
        let verify_ssl = if self.verify_ssl { "True" } else { "False" };
        let fields = [
            self.keycloak_url.as_str(),
            &self.realm,
            &self.client_id,
            &self.username,
            &self.password,
            &expiration,
            verify_ssl,
            &self.refresh_token,
        ];
        let digest = ring::digest::digest(&ring::digest::SHA256, fields.join("\0").as_bytes());
        digest
            .as_ref()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }
}

impl fmt::Debug for KeycloakConfig {
    /// Never renders the password or the refresh token.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeycloakConfig")
            .field("keycloak_url", &self.keycloak_url)
            .field("realm", &self.realm)
            .field("client_id", &self.client_id)
            .field("username", &self.username)
            .field("password", &Redacted(&self.password))
            .field("token_expiration_in_s", &self.token_expiration_in_s)
            .field("verify_ssl", &self.verify_ssl)
            .field("refresh_token", &Redacted(&self.refresh_token))
            .finish()
    }
}

/// Why a [`KeycloakConfig`] was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum KeycloakConfigError {
    /// Some but not all of `keycloak_url`, `realm` and `client_id` are set.
    IncompleteKeycloakFields,
    /// Keycloak is configured, but neither `password` nor `refresh_token` is set.
    MissingCredential,
}

impl fmt::Display for KeycloakConfigError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::IncompleteKeycloakFields => {
                "The Keycloak fields `keycloak_url`, `realm`, and `client_id` must be provided \
                 together in ClientConfig."
            }
            Self::MissingCredential => {
                "Either the field `password` or the field `refresh_token` is mandatory in \
                 ClientConfig."
            }
        })
    }
}

impl Error for KeycloakConfigError {}

// ---------------------------------------------------------------------------------------------
// Errors

type BoxError = Box<dyn Error + Send + Sync + 'static>;

/// Every failure of a login, a refresh or the lazy read of an expired token.
///
/// The message never contains a credential; the underlying error is its [`Error::source`].
#[derive(Debug)]
pub struct KeycloakAuthenticationError {
    message: String,
    source: Option<BoxError>,
}

impl KeycloakAuthenticationError {
    fn new(message: String) -> Self {
        Self {
            message,
            source: None,
        }
    }

    fn caused_by(message: String, source: impl Into<BoxError>) -> Self {
        Self {
            message,
            source: Some(source.into()),
        }
    }

    /// The error message (the same as its [`fmt::Display`]).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for KeycloakAuthenticationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for KeycloakAuthenticationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn Error + 'static))
    }
}

/// What building a provider or an interceptor can fail with.
#[derive(Debug)]
#[non_exhaustive]
pub enum KeycloakError {
    /// The config was refused before any network I/O.
    Config(KeycloakConfigError),
    /// The initial token acquisition failed.
    Authentication(KeycloakAuthenticationError),
}

impl fmt::Display for KeycloakError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Config(error) => fmt::Display::fmt(error, formatter),
            Self::Authentication(error) => fmt::Display::fmt(error, formatter),
        }
    }
}

impl Error for KeycloakError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Config(error) => Some(error),
            Self::Authentication(error) => Some(error),
        }
    }
}

impl From<KeycloakConfigError> for KeycloakError {
    fn from(error: KeycloakConfigError) -> Self {
        Self::Config(error)
    }
}

impl From<KeycloakAuthenticationError> for KeycloakError {
    fn from(error: KeycloakAuthenticationError) -> Self {
        Self::Authentication(error)
    }
}

// ---------------------------------------------------------------------------------------------
// Transport

/// One form-encoded `POST` to the token endpoint.
#[derive(Clone, PartialEq, Eq)]
pub struct TokenRequest {
    /// The token endpoint URL.
    pub url: String,
    /// The `application/x-www-form-urlencoded` fields, in order.
    pub form: Vec<(String, String)>,
    /// The bound of the whole call (connect, send and receive).
    pub timeout: Duration,
    /// Verify the server's TLS certificate.
    pub verify_ssl: bool,
}

impl TokenRequest {
    /// The value of a form field, if the form carries it.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.form
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

impl fmt::Debug for TokenRequest {
    /// Renders the form's field NAMES only: the values are credentials.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let fields: Vec<&str> = self.form.iter().map(|(key, _)| key.as_str()).collect();
        formatter
            .debug_struct("TokenRequest")
            .field("url", &self.url)
            .field("form_fields", &fields)
            .field("timeout", &self.timeout)
            .field("verify_ssl", &self.verify_ssl)
            .finish()
    }
}

/// The status and body of a token-endpoint answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TokenHttpResponse {
    /// The HTTP status code.
    pub status: u16,
    /// The response body.
    pub body: String,
}

/// How a [`TokenTransport`] failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TransportErrorKind {
    /// The token endpoint could not be reached: connection refused, DNS, TLS handshake, timeout,
    /// a socket error.
    Unreachable,
    /// Anything else: an unusable URL, a malformed HTTP answer, a bug in an injected transport.
    Other,
}

/// A failed token-endpoint exchange.
#[derive(Debug)]
pub struct TransportError {
    kind: TransportErrorKind,
    type_name: String,
    source: BoxError,
}

impl TransportError {
    /// The endpoint could not be reached.
    pub fn unreachable<E: Error + Send + Sync + 'static>(error: E) -> Self {
        Self::with_kind(
            TransportErrorKind::Unreachable,
            short_type_name::<E>(),
            error,
        )
    }

    /// Any other failure.
    pub fn other<E: Error + Send + Sync + 'static>(error: E) -> Self {
        Self::with_kind(TransportErrorKind::Other, short_type_name::<E>(), error)
    }

    /// An I/O error: always [`TransportErrorKind::Unreachable`], named after its
    /// [`std::io::ErrorKind`] (`ConnectionRefused`, `TimedOut`, ...).
    pub fn from_io(error: std::io::Error) -> Self {
        let type_name = format!("{:?}", error.kind());
        Self::with_kind(TransportErrorKind::Unreachable, type_name, error)
    }

    fn with_kind(kind: TransportErrorKind, type_name: String, error: impl Into<BoxError>) -> Self {
        Self {
            kind,
            type_name,
            source: error.into(),
        }
    }

    /// Whether the endpoint was unreachable or something else went wrong.
    pub fn kind(&self) -> TransportErrorKind {
        self.kind
    }

    /// The short name of the underlying error type, as it appears in messages.
    pub fn type_name(&self) -> &str {
        &self.type_name
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.source, formatter)
    }
}

impl Error for TransportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(self.source.as_ref())
    }
}

/// `std::io::Error` for `std::io::error::Error`, `Foo` for `a::Foo<b::Bar>`.
fn short_type_name<E>() -> String {
    let full = std::any::type_name::<E>();
    let without_generics = full.split('<').next().unwrap_or(full);
    without_generics
        .rsplit("::")
        .next()
        .unwrap_or(without_generics)
        .to_string()
}

/// The HTTP transport of the token endpoint. [`HttpTokenTransport`] is the default; tests inject
/// a fake one so no network is needed.
pub trait TokenTransport: Send + Sync {
    /// Send `request` and return the answer, whatever its status.
    ///
    /// # Errors
    ///
    /// A [`TransportError`] when no HTTP answer was obtained.
    fn post_form(&self, request: &TokenRequest) -> Result<TokenHttpResponse, TransportError>;
}

/// A transport that panicked: reported as a failed exchange instead of killing the caller (or the
/// background thread, whose death would let every token lapse).
#[derive(Debug)]
struct TransportPanicked(String);

impl fmt::Display for TransportPanicked {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "the token transport panicked: {}", self.0)
    }
}

impl Error for TransportPanicked {}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "a non-string panic payload".to_string()
    }
}

// ---------------------------------------------------------------------------------------------
// Test seams

/// The interruptible wait of the background thread, signalled by [`KeycloakTokenProvider::stop`].
///
/// The default is a condition variable; tests inject a scripted one that advances a fake clock.
pub trait StopSignal: Send + Sync {
    /// Wait up to `timeout_s` seconds; `true` when the stop was signalled (before or during it).
    fn wait(&self, timeout_s: f64) -> bool;
    /// Signal the stop, interrupting a pending [`wait`](Self::wait).
    fn set(&self);
    /// Whether the stop was signalled.
    fn is_set(&self) -> bool;
}

/// The default [`StopSignal`].
#[derive(Debug, Default)]
pub struct CondvarStopSignal {
    stopped: Mutex<bool>,
    condvar: Condvar,
}

impl StopSignal for CondvarStopSignal {
    fn wait(&self, timeout_s: f64) -> bool {
        let timeout = Duration::from_secs_f64(timeout_s.clamp(0.0, MAX_WAIT_S));
        let stopped = self.stopped.lock().unwrap_or_else(PoisonError::into_inner);
        let (stopped, _) = self
            .condvar
            .wait_timeout_while(stopped, timeout, |stopped| !*stopped)
            .unwrap_or_else(PoisonError::into_inner);
        *stopped
    }

    fn set(&self) {
        *self.stopped.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.condvar.notify_all();
    }

    fn is_set(&self) -> bool {
        *self.stopped.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// A monotonic clock in seconds.
pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;
/// A source of a fraction in `[0, 1)`, consulted only to jitter the backoff after a failure.
pub type RandomFraction = Arc<dyn Fn() -> f64 + Send + Sync>;

/// Seconds since the first call, on the monotonic [`Instant`] clock.
pub fn monotonic_seconds() -> f64 {
    static EPOCH: OnceLock<Instant> = OnceLock::new();
    EPOCH.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// A uniform fraction in `[0, 1)` from a process-wide xorshift seeded by the std hasher's keys.
pub fn random_fraction() -> f64 {
    static STATE: AtomicU64 = AtomicU64::new(0);
    let mut state = STATE.load(Ordering::Relaxed);
    if state == 0 {
        state = RandomState::new().build_hasher().finish() | 1;
    }
    state ^= state << 13;
    state ^= state >> 7;
    state ^= state << 17;
    STATE.store(state, Ordering::Relaxed);
    (state >> 11) as f64 / (1u64 << 53) as f64
}

/// Everything about a provider that is not a credential: the timings and the test seams.
#[derive(Clone)]
pub struct KeycloakProviderOptions {
    /// The constants; the production table by default.
    pub timings: KeycloakTimings,
    /// The token-endpoint transport; [`HttpTokenTransport`] by default.
    pub transport: Arc<dyn TokenTransport>,
    /// The monotonic clock; [`monotonic_seconds`] by default.
    pub clock: Clock,
    /// The backoff jitter; [`random_fraction`] by default.
    pub random: RandomFraction,
    /// The background thread's stop; a [`CondvarStopSignal`] by default.
    pub stop_signal: Arc<dyn StopSignal>,
    /// Start the background refresh thread (`true` by default). With `false`, only the lazy path
    /// refreshes, unless the caller drives [`KeycloakTokenProvider::run_refresh_loop`].
    pub start_background_refresh: bool,
}

impl Default for KeycloakProviderOptions {
    fn default() -> Self {
        Self {
            timings: KeycloakTimings::default(),
            transport: Arc::new(HttpTokenTransport),
            clock: Arc::new(monotonic_seconds),
            random: Arc::new(random_fraction),
            stop_signal: Arc::new(CondvarStopSignal::default()),
            start_background_refresh: true,
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The provider

/// The mutable token state, guarded by the provider lock.
struct TokenState {
    access_token: String,
    refresh_token: String,
    expires_at: f64,
    login_deadline: Option<f64>,
    consecutive_failures: i64,
    next_attempt_at: f64,
}

/// A failed exchange: an authentication error, or anything else (wrapped only where it surfaces).
enum Failure {
    Authentication(KeycloakAuthenticationError),
    Transport(TransportError),
}

impl Failure {
    fn type_name(&self) -> &str {
        match self {
            Self::Authentication(_) => "KeycloakAuthenticationError",
            Self::Transport(error) => error.type_name(),
        }
    }

    fn describe(&self) -> String {
        let message: &dyn fmt::Display = match self {
            Self::Authentication(error) => error,
            Self::Transport(error) => error,
        };
        format!("{}: {message}", self.type_name())
    }

    /// The typed error: an authentication error unchanged, anything else wrapped.
    fn into_authentication_error(self, action: &str) -> KeycloakAuthenticationError {
        match self {
            Self::Authentication(error) => error,
            Self::Transport(error) => KeycloakAuthenticationError::caused_by(
                format!(
                    "Keycloak token {action} failed ({}: {error})",
                    error.type_name()
                ),
                error,
            ),
        }
    }
}

/// The shared core the background thread holds weakly.
struct Inner {
    config: KeycloakConfig,
    token_endpoint: String,
    timings: KeycloakTimings,
    transport: Arc<dyn TokenTransport>,
    clock: Clock,
    random: RandomFraction,
    state: Mutex<TokenState>,
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, TokenState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn now(&self) -> f64 {
        (self.clock)()
    }

    /// The decision table shared by the lazy path and the background thread; the caller holds the
    /// lock (that is what makes concurrent callers single-flight).
    fn refresh_if_within_window(
        &self,
        state: &mut TokenState,
        now: f64,
    ) -> Result<(), KeycloakAuthenticationError> {
        if state.login_deadline.is_some_and(|deadline| now >= deadline) {
            // The bounded auto-refresh window has closed: let the token lapse.
            return Ok(());
        }
        if now < state.expires_at - self.timings.expiry_leeway_s {
            return Ok(());
        }
        if now < state.next_attempt_at {
            // A previous refresh failed and its backoff has not elapsed: send nothing.
            if now < state.expires_at {
                return Ok(());
            }
            return Err(KeycloakAuthenticationError::new(format!(
                "The Keycloak access token has expired and the refresh is backing off after {} \
                 consecutive failure(s); the next attempt is due in {:.1}s",
                state.consecutive_failures,
                state.next_attempt_at - now
            )));
        }
        match self.refresh(state) {
            Ok(()) => {
                Self::record_success(state);
                Ok(())
            }
            Err(failure) => {
                self.record_failure(state, &failure);
                // Re-read the clock: the failed attempt itself may have taken the HTTP timeout.
                if self.now() < state.expires_at {
                    return Ok(());
                }
                Err(failure.into_authentication_error("refresh"))
            }
        }
    }

    /// Count a failure, schedule the jittered retry and warn once. Deliberately NOT a fallback to
    /// a password grant: a re-login at the least predictable moment is the burst the offline token
    /// exists to avoid. The backoff is measured from NOW, when the failure is recorded.
    fn record_failure(&self, state: &mut TokenState, failure: &Failure) {
        let now = self.now();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        let delay = self
            .timings
            .backoff_delay_s(state.consecutive_failures, (self.random)());
        state.next_attempt_at = now + delay;
        let remaining = state.expires_at - now;
        let validity = if remaining > 0.0 {
            format!("the current access token stays valid for {remaining:.1}s")
        } else {
            "the current access token has expired".to_string()
        };
        tracing::warn!(
            "Keycloak token refresh failed (attempt {}); retrying in {delay:.1}s, {validity} ({})",
            state.consecutive_failures,
            failure.describe()
        );
    }

    fn record_success(state: &mut TokenState) {
        if state.consecutive_failures > 0 {
            tracing::info!(
                "Keycloak token refresh recovered after {} failed attempt(s)",
                state.consecutive_failures
            );
        }
        state.consecutive_failures = 0;
        state.next_attempt_at = 0.0;
    }

    fn login(&self, state: &mut TokenState) -> Result<(), Failure> {
        let form = vec![
            ("grant_type", "password"),
            ("client_id", self.config.client_id.as_str()),
            ("username", self.config.username.as_str()),
            ("password", self.config.password.as_str()),
            ("scope", "offline_access"),
        ];
        let payload = self.post_token_request(form, "login")?;
        self.store_tokens(state, &payload)
    }

    fn refresh(&self, state: &mut TokenState) -> Result<(), Failure> {
        let refresh_token = state.refresh_token.clone();
        let form = vec![
            ("grant_type", "refresh_token"),
            ("client_id", self.config.client_id.as_str()),
            ("refresh_token", refresh_token.as_str()),
        ];
        let payload = self.post_token_request(form, "refresh")?;
        self.store_tokens(state, &payload)
    }

    fn post_token_request(
        &self,
        form: Vec<(&str, &str)>,
        action: &str,
    ) -> Result<Map<String, Value>, Failure> {
        let request = TokenRequest {
            url: self.token_endpoint.clone(),
            form: form
                .into_iter()
                .map(|(key, value)| (key.to_string(), value.to_string()))
                .collect(),
            timeout: Duration::from_secs_f64(self.timings.http_timeout_s),
            verify_ssl: self.config.verify_ssl,
        };
        let outcome = panic::catch_unwind(AssertUnwindSafe(|| self.transport.post_form(&request)))
            .unwrap_or_else(|payload| {
                Err(TransportError::other(TransportPanicked(panic_message(
                    payload.as_ref(),
                ))))
            });
        let response = match outcome {
            Ok(response) => response,
            Err(error) if error.kind() == TransportErrorKind::Unreachable => {
                return Err(Failure::Authentication(
                    KeycloakAuthenticationError::caused_by(
                        format!(
                            "Keycloak token {action} could not reach the token endpoint ({}: \
                             {error})",
                            error.type_name()
                        ),
                        error,
                    ),
                ));
            }
            Err(error) => return Err(Failure::Transport(error)),
        };
        if !(200..300).contains(&response.status) {
            let body: String = response
                .body
                .chars()
                .take(self.timings.max_error_body_chars)
                .collect();
            return Err(Failure::Authentication(KeycloakAuthenticationError::new(
                format!(
                    "Keycloak token {action} failed with status {}: {body}",
                    response.status
                ),
            )));
        }
        let body: Value = serde_json::from_str(&response.body).map_err(|error| {
            Failure::Authentication(KeycloakAuthenticationError::caused_by(
                format!(
                    "Keycloak token {action} returned status {} with a body that is not JSON",
                    response.status
                ),
                error,
            ))
        })?;
        match body {
            Value::Object(object) => Ok(object),
            other => Err(Failure::Authentication(KeycloakAuthenticationError::new(
                format!(
                    "Keycloak token {action} returned a JSON {} instead of an object",
                    json_type_name(&other)
                ),
            ))),
        }
    }

    /// Validate the whole response first, assign afterwards: a malformed one changes nothing.
    fn store_tokens(
        &self,
        state: &mut TokenState,
        payload: &Map<String, Value>,
    ) -> Result<(), Failure> {
        let Some(access_token) = payload
            .get("access_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
        else {
            // The field NAMES, never the values: a token response is credential material.
            let mut names: Vec<&str> = payload.keys().map(String::as_str).collect();
            names.sort_unstable();
            let names: Vec<String> = names.iter().map(|name| format!("'{name}'")).collect();
            return Err(Failure::Authentication(KeycloakAuthenticationError::new(
                format!(
                    "Keycloak token response did not contain an access_token; the response \
                     carried the fields [{}] (values withheld: a token response is credential \
                     material)",
                    names.join(", ")
                ),
            )));
        };
        let expires_in = match payload.get("expires_in") {
            None => Some(0.0),
            Some(Value::Number(number)) => number.as_f64(),
            Some(Value::String(text)) => text.trim().parse::<f64>().ok(),
            Some(_) => None,
        }
        .filter(|seconds| seconds.is_finite())
        .ok_or_else(|| {
            Failure::Authentication(KeycloakAuthenticationError::new(
                "Keycloak token response carried a non-numeric expires_in (value withheld)"
                    .to_string(),
            ))
        })?;
        state.access_token = access_token.to_string();
        // Keep the previous refresh token when the response carries none (rotation is optional).
        if let Some(refresh_token) = payload
            .get("refresh_token")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
        {
            state.refresh_token = refresh_token.to_string();
        }
        state.expires_at = self.now() + expires_in.max(0.0);
        Ok(())
    }
}

fn json_type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        // An object never gets here: it is the one shape the caller accepts.
        _ => "array",
    }
}

/// The background thread and the way to learn that it has finished.
struct Worker {
    handle: JoinHandle<()>,
    finished: mpsc::Receiver<()>,
}

/// An auto-refreshing Keycloak access token; see the [module documentation](self).
///
/// Dropping the provider stops its background thread. Share one per credential set through
/// [`get_keycloak_token_provider`], or build one directly with [`new`](Self::new).
pub struct KeycloakTokenProvider {
    inner: Arc<Inner>,
    stop_signal: Arc<dyn StopSignal>,
    worker: Mutex<Option<Worker>>,
    closed: AtomicBool,
}

impl KeycloakTokenProvider {
    /// Validate `config`, acquire the first token and start the background refresh.
    ///
    /// # Errors
    ///
    /// [`KeycloakError::Config`] for a refused config, [`KeycloakError::Authentication`] when the
    /// first token acquisition fails (no retry, no fallback, no background thread started).
    pub fn new(config: &KeycloakConfig) -> Result<Self, KeycloakError> {
        Self::with_options(config, KeycloakProviderOptions::default())
    }

    /// [`new`](Self::new) with custom timings and test seams.
    ///
    /// # Errors
    ///
    /// As [`new`](Self::new).
    pub fn with_options(
        config: &KeycloakConfig,
        options: KeycloakProviderOptions,
    ) -> Result<Self, KeycloakError> {
        config.validate()?;
        let inner = Arc::new(Inner {
            token_endpoint: config.token_endpoint(),
            config: config.clone(),
            timings: options.timings,
            transport: options.transport,
            clock: options.clock,
            random: options.random,
            state: Mutex::new(TokenState {
                access_token: String::new(),
                refresh_token: config.refresh_token.clone(),
                expires_at: 0.0,
                login_deadline: None,
                consecutive_failures: 0,
                next_attempt_at: 0.0,
            }),
        });
        {
            let mut state = inner.lock();
            // A handed-off offline token starts up with ONE refresh grant and never with a
            // password grant, whatever the outcome.
            let (outcome, action) = if config.refresh_token.is_empty() {
                (inner.login(&mut state), "login")
            } else {
                (inner.refresh(&mut state), "refresh")
            };
            outcome.map_err(|failure| failure.into_authentication_error(action))?;
            // Armed on both paths, after the first token, whichever credential produced it.
            state.login_deadline = config
                .token_expiration_in_s
                .map(|seconds| inner.now() + seconds as f64);
        }
        let provider = Self {
            inner,
            stop_signal: options.stop_signal,
            worker: Mutex::new(None),
            closed: AtomicBool::new(false),
        };
        if options.start_background_refresh {
            provider.start_background_refresh();
        }
        Ok(provider)
    }

    fn start_background_refresh(&self) {
        let core = Arc::clone(&self.inner);
        let stop_signal = Arc::clone(&self.stop_signal);
        let (finished_sender, finished) = mpsc::channel::<()>();
        let handle = thread::Builder::new()
            .name(format!(
                "keycloak-token-refresh-{}",
                self.inner.config.client_id
            ))
            .spawn(move || {
                // Dropped when the thread ends, however it ends: that is what stop() waits for.
                let _finished_sender = finished_sender;
                refresh_loop(core, stop_signal.as_ref());
            })
            .expect("failed to spawn the Keycloak token refresh thread");
        *self.worker.lock().unwrap_or_else(PoisonError::into_inner) =
            Some(Worker { handle, finished });
    }

    /// `("authorization", "Bearer <access token>")`, refreshed first when it is inside the leeway.
    ///
    /// While a failed refresh is backing off, no request is sent and the still-valid token is
    /// served. Concurrent callers are single-flight: one refresh, the others wait for it.
    ///
    /// # Errors
    ///
    /// [`KeycloakAuthenticationError`] when the access token has expired and could not be
    /// refreshed (now, or because the backoff of an earlier failure has not elapsed).
    pub fn authorization_metadata(
        &self,
    ) -> Result<(&'static str, String), KeycloakAuthenticationError> {
        let mut state = self.inner.lock();
        self.inner
            .refresh_if_within_window(&mut state, self.inner.now())?;
        Ok((
            AUTHORIZATION_METADATA_KEY,
            format!("Bearer {}", state.access_token),
        ))
    }

    /// The most recent access token (without refreshing it). SECRET.
    pub fn access_token(&self) -> String {
        self.inner.lock().access_token.clone()
    }

    /// When the access token expires, on the provider's clock.
    pub fn access_token_expires_at(&self) -> f64 {
        self.inner.lock().expires_at
    }

    /// When auto-refresh stops (`token_expiration_in_s` after the first token), if bounded.
    pub fn login_deadline(&self) -> Option<f64> {
        self.inner.lock().login_deadline
    }

    /// Consecutive failed refreshes since the last success.
    pub fn consecutive_refresh_failures(&self) -> i64 {
        self.inner.lock().consecutive_failures
    }

    /// When the next refresh may be attempted after a failure (0 when none is pending).
    pub fn next_refresh_attempt_at(&self) -> f64 {
        self.inner.lock().next_attempt_at
    }

    /// Whether the background loop was told to stop (by [`stop`](Self::stop) or the deadline).
    pub fn is_stopped(&self) -> bool {
        self.stop_signal.is_set()
    }

    /// Whether [`stop`](Self::stop) / [`close`](Self::close) was called.
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Whether the background thread is alive.
    pub fn is_background_refresh_running(&self) -> bool {
        self.worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
            .is_some_and(|worker| !worker.handle.is_finished())
    }

    /// Run the background refresh loop on the CALLING thread until it stops. Meant for a provider
    /// built with `start_background_refresh: false`, i.e. for tests and custom schedulers.
    pub fn run_refresh_loop(&self) {
        refresh_loop(Arc::clone(&self.inner), self.stop_signal.as_ref());
    }

    /// Stop the background thread and wait for it at most `thread_join_timeout_s`.
    ///
    /// Idempotent, safe from any state and from the background thread itself (which never waits
    /// for itself). A pending wait, including a backoff wait, is interrupted, so no further
    /// request is sent. The lazy path keeps working on demand afterwards.
    pub fn stop(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.stop_signal.set();
        let worker = self
            .worker
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        let Some(worker) = worker else {
            return;
        };
        if worker.handle.thread().id() == thread::current().id() {
            return;
        }
        let timeout = Duration::from_secs_f64(self.inner.timings.thread_join_timeout_s.max(0.0));
        if worker.finished.recv_timeout(timeout) != Err(RecvTimeoutError::Timeout) {
            let _ = worker.handle.join();
        }
    }

    /// The same as [`stop`](Self::stop).
    pub fn close(&self) {
        self.stop();
    }
}

impl Drop for KeycloakTokenProvider {
    fn drop(&mut self) {
        self.stop();
    }
}

impl fmt::Debug for KeycloakTokenProvider {
    /// Never renders a credential or a token.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeycloakTokenProvider")
            .field("token_endpoint", &self.inner.token_endpoint)
            .field("client_id", &self.inner.config.client_id)
            .field("username", &self.inner.config.username)
            .field("closed", &self.is_closed())
            .finish_non_exhaustive()
    }
}

/// The background loop. It holds the provider core only WEAKLY while it waits, so it never keeps a
/// dropped provider alive, and a failed refresh never ends it.
fn refresh_loop(core: Arc<Inner>, stop_signal: &dyn StopSignal) {
    let weak = Arc::downgrade(&core);
    let mut core = core;
    loop {
        if stop_signal.is_set() {
            return;
        }
        let Some(delay) = next_delay(&core, stop_signal) else {
            return;
        };
        // Drop the strong reference before waiting, so a long wait cannot pin the provider.
        drop(core);
        if stop_signal.wait(delay) {
            return;
        }
        let Some(alive) = weak.upgrade() else {
            return;
        };
        core = alive;
        let mut state = core.lock();
        if let Err(error) = core.refresh_if_within_window(&mut state, core.now()) {
            // Already recorded and warned by record_failure; never a second warning.
            tracing::debug!("Keycloak background refresh will be retried ({error})");
        }
    }
}

/// How long the loop waits before its next refresh, or `None` (after signalling the stop) once the
/// bounded auto-refresh window has closed.
fn next_delay(core: &Inner, stop_signal: &dyn StopSignal) -> Option<f64> {
    let now = core.now();
    let state = core.lock();
    if state.login_deadline.is_some_and(|deadline| now >= deadline) {
        stop_signal.set();
        return None;
    }
    let mut delay = state.expires_at - core.timings.expiry_leeway_s - now;
    if state.consecutive_failures > 0 {
        // A failure leaves expires_at unchanged; wait for the backoff, never the 1 s floor.
        delay = state.next_attempt_at - now;
    }
    delay = delay.max(core.timings.min_refresh_delay_s);
    if let Some(deadline) = state.login_deadline {
        delay = delay.min(deadline - now);
    }
    Some(delay)
}

// ---------------------------------------------------------------------------------------------
// The shared registry

type Registry = Mutex<HashMap<String, Weak<KeycloakTokenProvider>>>;

fn registry() -> &'static Registry {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    REGISTRY.get_or_init(Registry::default)
}

/// The shared provider of `config`'s credential set, created (one login) on first use.
///
/// Held weakly: once the last client holding it is gone, the provider stops and the next call
/// logs in again. A provider that was [`closed`](KeycloakTokenProvider::close) is never handed out.
///
/// # Errors
///
/// As [`KeycloakTokenProvider::new`].
pub fn get_keycloak_token_provider(
    config: &KeycloakConfig,
) -> Result<Arc<KeycloakTokenProvider>, KeycloakError> {
    get_keycloak_token_provider_with_options(config, KeycloakProviderOptions::default)
}

/// [`get_keycloak_token_provider`] with the options a NEW provider is built with; `options` is
/// not called when a live provider of the credential set exists.
///
/// # Errors
///
/// As [`KeycloakTokenProvider::new`].
pub fn get_keycloak_token_provider_with_options(
    config: &KeycloakConfig,
    options: impl FnOnce() -> KeycloakProviderOptions,
) -> Result<Arc<KeycloakTokenProvider>, KeycloakError> {
    config.validate()?;
    let key = config.registry_key();
    let mut providers = registry().lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(provider) = providers.get(&key).and_then(Weak::upgrade) {
        if !provider.is_closed() {
            return Ok(provider);
        }
    }
    let provider = Arc::new(KeycloakTokenProvider::with_options(config, options())?);
    providers.retain(|_, provider| provider.strong_count() > 0);
    providers.insert(key, Arc::downgrade(&provider));
    Ok(provider)
}

// ---------------------------------------------------------------------------------------------
// The interceptor

/// A [`tonic::service::Interceptor`] attaching `authorization: Bearer <token>` from a
/// [`KeycloakTokenProvider`] to every request; without Keycloak it attaches nothing.
///
/// A refresh that has to happen inside the interceptor blocks for at most the HTTP timeout; on a
/// multi-threaded tokio runtime it runs through `tokio::task::block_in_place`, so the other tasks
/// of that worker move elsewhere meanwhile.
#[derive(Clone)]
pub struct KeycloakInterceptor {
    provider: Option<Arc<KeycloakTokenProvider>>,
}

impl KeycloakInterceptor {
    /// The interceptor of `config`: the shared provider of its credential set, or a pass-through
    /// when Keycloak is not configured (no provider is built, no metadata attached).
    ///
    /// # Errors
    ///
    /// As [`get_keycloak_token_provider`].
    pub fn new(config: &KeycloakConfig) -> Result<Self, KeycloakError> {
        config.validate()?;
        if !config.is_keycloak_configured() {
            return Ok(Self { provider: None });
        }
        Ok(Self::from_provider(get_keycloak_token_provider(config)?))
    }

    /// An interceptor reading its token from `provider`.
    pub fn from_provider(provider: Arc<KeycloakTokenProvider>) -> Self {
        Self {
            provider: Some(provider),
        }
    }

    /// The provider, or `None` for a pass-through interceptor.
    pub fn provider(&self) -> Option<&Arc<KeycloakTokenProvider>> {
        self.provider.as_ref()
    }
}

impl fmt::Debug for KeycloakInterceptor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeycloakInterceptor")
            .field("provider", &self.provider)
            .finish()
    }
}

fn run_blocking<T>(work: impl FnOnce() -> T) -> T {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(work)
        }
        _ => work(),
    }
}

impl Interceptor for KeycloakInterceptor {
    fn call(&mut self, mut request: Request<()>) -> Result<Request<()>, Status> {
        let Some(provider) = &self.provider else {
            return Ok(request);
        };
        let (key, value) = run_blocking(|| provider.authorization_metadata())
            .map_err(|error| Status::unauthenticated(error.to_string()))?;
        let value = value.parse::<MetadataValue<Ascii>>().map_err(|_| {
            Status::unauthenticated("the Keycloak access token is not a valid HTTP header value")
        })?;
        request.metadata_mut().insert(key, value);
        Ok(request)
    }
}
