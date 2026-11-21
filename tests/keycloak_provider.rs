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

//! The cross-language Keycloak test catalogue (`T01`..`T62`) against `src/keycloak.rs`.
//!
//! No network and no real sleeps (apart from T18/T26/T39/T41, which exercise real threads): a
//! scripted transport records every form and replays scripted answers (the last one repeats), a
//! fake monotonic clock starts at 1000, the backoff jitter is a fixed fraction, and a scripted stop
//! signal drives the background loop - each `wait` records its delay, advances the fake clock by it
//! and replays a queued result (`true` = stopped); once the script is exhausted it stops the loop.
//! The canonical python twin is `tests/unit/utils/test_keycloak_refresh_resilience.py`.

use std::collections::VecDeque;
use std::error::Error;
use std::fmt;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, Weak};
use std::thread;
use std::time::{Duration, Instant};

use ondewo_t2s_client::keycloak::{
    get_keycloak_token_provider_with_options, monotonic_seconds, random_fraction,
    CondvarStopSignal, KeycloakAuthenticationError, KeycloakConfig, KeycloakConfigError,
    KeycloakError, KeycloakInterceptor, KeycloakProviderOptions, KeycloakTimings,
    KeycloakTokenProvider, StopSignal, TokenHttpResponse, TokenRequest, TokenTransport,
    TransportError, TransportErrorKind, EXPIRY_LEEWAY_S, HTTP_TIMEOUT_S, MAX_ERROR_BODY_CHARS,
    MAX_RETRY_EXPONENT, MIN_REFRESH_DELAY_S, RETRY_BASE_DELAY_S, RETRY_MAX_DELAY_S,
    THREAD_JOIN_TIMEOUT_S,
};
use serde_json::{json, Value};
use tonic::service::Interceptor;
use tonic::{Code, Request};
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing::{Event, Level, Metadata, Subscriber};

const KEYCLOAK_URL: &str = "https://keycloak.example/auth";
const REALM: &str = "ondewo-test-realm";
const CLIENT_ID: &str = "ondewo-test-sdk-public";
const USERNAME: &str = "technical-user@example.com";
const PASSWORD: &str = "pw-never-logged-5d1c";
const DEAD_SESSION: &str =
    r#"{"error":"invalid_grant","error_description":"Offline user session not found"}"#;

// ---------------------------------------------------------------------------------------------
// The fakes

/// A scripted token-endpoint answer.
#[derive(Clone)]
enum Step {
    Answer(u16, String),
    Unreachable(io::ErrorKind, &'static str),
    Other(&'static str),
    PanicStr,
    PanicString,
    PanicOther,
}

fn token_with(access: &str, expires_in: Value, refresh: Option<&str>) -> Step {
    let mut body = json!({"access_token": access, "expires_in": expires_in});
    if let Some(refresh) = refresh {
        body["refresh_token"] = json!(refresh);
    }
    Step::Answer(200, body.to_string())
}

fn token(access: &str, expires_in: Value) -> Step {
    token_with(access, expires_in, Some("off-x"))
}

fn refused() -> Step {
    Step::Answer(400, DEAD_SESSION.to_string())
}

fn status(code: u16, body: &str) -> Step {
    Step::Answer(code, body.to_string())
}

/// A generic error a buggy custom transport could return.
#[derive(Debug)]
struct TransportBug(&'static str);

impl fmt::Display for TransportBug {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

impl Error for TransportBug {}

type Hook = Box<dyn Fn(&TokenRequest) + Send + Sync>;

#[derive(Default)]
struct FakeTransport {
    steps: Mutex<VecDeque<Step>>,
    calls: Mutex<Vec<TokenRequest>>,
    delay: Duration,
    before: Option<Hook>,
}

impl FakeTransport {
    fn new(steps: Vec<Step>) -> Arc<Self> {
        Arc::new(Self {
            steps: Mutex::new(steps.into()),
            ..Self::default()
        })
    }

    fn with(steps: Vec<Step>, delay: Duration, before: Option<Hook>) -> Arc<Self> {
        Arc::new(Self {
            steps: Mutex::new(steps.into()),
            calls: Mutex::default(),
            delay,
            before,
        })
    }

    fn calls(&self) -> Vec<TokenRequest> {
        self.calls.lock().unwrap().clone()
    }

    fn grants(&self) -> Vec<String> {
        self.calls()
            .iter()
            .map(|call| call.field("grant_type").unwrap_or_default().to_string())
            .collect()
    }
}

impl TokenTransport for FakeTransport {
    fn post_form(&self, request: &TokenRequest) -> Result<TokenHttpResponse, TransportError> {
        let step = {
            self.calls.lock().unwrap().push(request.clone());
            let mut steps = self.steps.lock().unwrap();
            if steps.len() > 1 {
                steps.pop_front().unwrap()
            } else {
                steps[0].clone()
            }
        };
        if let Some(before) = &self.before {
            before(request);
        }
        if !self.delay.is_zero() {
            thread::sleep(self.delay);
        }
        match step {
            Step::Answer(status, body) => Ok(TokenHttpResponse { status, body }),
            Step::Unreachable(kind, message) => {
                Err(TransportError::from_io(io::Error::new(kind, message)))
            }
            Step::Other(message) => Err(TransportError::other(TransportBug(message))),
            Step::PanicStr => panic!("a bug in a custom transport"),
            Step::PanicString => panic!("{}", String::from("a formatted bug")),
            Step::PanicOther => std::panic::panic_any(42_u32),
        }
    }
}

#[derive(Clone)]
struct FakeClock(Arc<Mutex<f64>>);

impl FakeClock {
    fn at(now: f64) -> Self {
        Self(Arc::new(Mutex::new(now)))
    }

    fn now(&self) -> f64 {
        *self.0.lock().unwrap()
    }

    fn advance(&self, seconds: f64) {
        *self.0.lock().unwrap() += seconds;
    }

    fn clock(&self) -> Arc<dyn Fn() -> f64 + Send + Sync> {
        let clock = self.clone();
        Arc::new(move || clock.now())
    }
}

struct ScriptedStop {
    clock: FakeClock,
    returns: Mutex<VecDeque<bool>>,
    advance_by: Mutex<Option<VecDeque<f64>>>,
    stopped: AtomicBool,
    waits: Mutex<Vec<f64>>,
}

impl ScriptedStop {
    fn new(returns: Vec<bool>, clock: &FakeClock) -> Arc<Self> {
        Self::advancing(returns, clock, None)
    }

    fn advancing(returns: Vec<bool>, clock: &FakeClock, advance_by: Option<Vec<f64>>) -> Arc<Self> {
        Arc::new(Self {
            clock: clock.clone(),
            returns: Mutex::new(returns.into()),
            advance_by: Mutex::new(advance_by.map(Into::into)),
            stopped: AtomicBool::new(false),
            waits: Mutex::default(),
        })
    }

    fn waits(&self) -> Vec<f64> {
        self.waits.lock().unwrap().clone()
    }
}

impl StopSignal for ScriptedStop {
    fn wait(&self, timeout_s: f64) -> bool {
        self.waits.lock().unwrap().push(timeout_s);
        match self.advance_by.lock().unwrap().as_mut() {
            Some(advance) => self.clock.advance(advance.pop_front().unwrap_or(0.0)),
            None => self.clock.advance(timeout_s),
        }
        let Some(result) = self.returns.lock().unwrap().pop_front() else {
            self.stopped.store(true, Ordering::SeqCst);
            return true;
        };
        if result {
            self.stopped.store(true, Ordering::SeqCst);
        }
        result
    }

    fn set(&self) {
        self.stopped.store(true, Ordering::SeqCst);
    }

    fn is_set(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

fn config() -> KeycloakConfig {
    KeycloakConfig::new(KEYCLOAK_URL, REALM, CLIENT_ID, USERNAME, PASSWORD)
}

fn options(
    transport: &Arc<FakeTransport>,
    clock: &FakeClock,
    stop: Option<Arc<ScriptedStop>>,
    jitter: f64,
) -> KeycloakProviderOptions {
    let mut options = KeycloakProviderOptions {
        transport: transport.clone(),
        clock: clock.clock(),
        random: Arc::new(move || jitter),
        start_background_refresh: false,
        ..KeycloakProviderOptions::default()
    };
    if let Some(stop) = stop {
        options.stop_signal = stop;
    }
    options
}

fn build_with(
    config: &KeycloakConfig,
    transport: &Arc<FakeTransport>,
    clock: &FakeClock,
    stop: Option<Arc<ScriptedStop>>,
    jitter: f64,
) -> Result<KeycloakTokenProvider, KeycloakError> {
    KeycloakTokenProvider::with_options(config, options(transport, clock, stop, jitter))
}

fn build(
    transport: &Arc<FakeTransport>,
    clock: &FakeClock,
    stop: Option<Arc<ScriptedStop>>,
    jitter: f64,
) -> KeycloakTokenProvider {
    build_with(&config(), transport, clock, stop, jitter).expect("the login succeeds")
}

fn authentication_error(
    result: Result<KeycloakTokenProvider, KeycloakError>,
) -> KeycloakAuthenticationError {
    match result {
        Err(KeycloakError::Authentication(error)) => error,
        Err(other) => panic!("expected an authentication error, got {other}"),
        Ok(_) => panic!("expected an authentication error, got a provider"),
    }
}

fn source_of(error: &dyn Error) -> String {
    error.source().map(ToString::to_string).unwrap_or_default()
}

// ---------------------------------------------------------------------------------------------
// Log capture: ONE global tracing subscriber (always interested, so no callsite interest is ever
// cached as "never" by a race between threads) that routes each event to the capture of the
// thread that emitted it, if that thread is capturing.

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<String>>>);

thread_local! {
    static SINK: std::cell::RefCell<Option<Capture>> = const { std::cell::RefCell::new(None) };
}

struct MessageVisitor(String);

impl Visit for MessageVisitor {
    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{value:?}");
        }
    }
}

struct Router;

impl Subscriber for Router {
    fn enabled(&self, _: &Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _: &Attributes<'_>) -> Id {
        Id::from_u64(1)
    }
    fn record(&self, _: &Id, _: &Record<'_>) {}
    fn record_follows_from(&self, _: &Id, _: &Id) {}
    fn event(&self, event: &Event<'_>) {
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);
        let level = *event.metadata().level();
        SINK.with(|sink| {
            if let Some(capture) = sink.borrow().as_ref() {
                capture
                    .0
                    .lock()
                    .unwrap()
                    .push(format!("{level} {}", visitor.0));
            }
        });
    }
    fn enter(&self, _: &Id) {}
    fn exit(&self, _: &Id) {}
}

impl Capture {
    fn lines(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }

    fn count(&self, level: Level, needle: &str) -> usize {
        let prefix = format!("{level} ");
        self.lines()
            .iter()
            .filter(|line| line.starts_with(&prefix) && line.contains(needle))
            .count()
    }
}

fn captured<T>(work: impl FnOnce() -> T) -> (T, Capture) {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| tracing::subscriber::set_global_default(Router).unwrap());
    let capture = Capture::default();
    SINK.with(|sink| *sink.borrow_mut() = Some(capture.clone()));
    let result = work();
    SINK.with(|sink| *sink.borrow_mut() = None);
    (result, capture)
}

// ---------------------------------------------------------------------------------------------
// A. The backoff ladder

#[test]
fn t01_the_ceiling_doubles_from_the_base_and_stops_at_the_maximum() {
    let timings = KeycloakTimings::default();
    let delays: Vec<f64> = (1..=10).map(|n| timings.backoff_delay_s(n, 1.0)).collect();
    assert_eq!(
        delays,
        [5.0, 10.0, 20.0, 40.0, 80.0, 160.0, 300.0, 300.0, 300.0, 300.0]
    );
}

#[test]
fn t02_a_zero_fraction_always_waits_the_base() {
    let timings = KeycloakTimings::default();
    assert!((1..20).all(|n| timings.backoff_delay_s(n, 0.0) == RETRY_BASE_DELAY_S));
}

#[test]
fn t03_the_delay_is_drawn_inside_the_window() {
    let timings = KeycloakTimings::default();
    assert_eq!(timings.backoff_delay_s(2, 0.5), 7.5);
    assert_eq!(timings.backoff_delay_s(3, 0.25), 8.75);
}

#[test]
fn t04_a_faulty_jitter_source_is_clamped_into_the_window() {
    let timings = KeycloakTimings::default();
    for fraction in [
        -1.0,
        -0.001,
        1.5,
        1e9,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ] {
        let delay = timings.backoff_delay_s(4, fraction);
        assert!(
            (RETRY_BASE_DELAY_S..=40.0).contains(&delay),
            "{fraction} -> {delay}"
        );
    }
    assert_eq!(timings.backoff_delay_s(4, f64::NAN), RETRY_BASE_DELAY_S);
}

#[test]
fn t05_a_non_positive_failure_count_is_treated_as_the_first_failure() {
    let timings = KeycloakTimings::default();
    for failures in [0, -3, i64::MIN] {
        assert_eq!(timings.backoff_delay_s(failures, 1.0), RETRY_BASE_DELAY_S);
    }
}

#[test]
fn t06_a_huge_failure_count_neither_overflows_nor_exceeds_the_ceiling() {
    let timings = KeycloakTimings::default();
    assert_eq!(
        timings.backoff_delay_s(1_000_000_000, 1.0),
        RETRY_MAX_DELAY_S
    );
    assert_eq!(timings.backoff_delay_s(i64::MAX, 1.0), RETRY_MAX_DELAY_S);
}

#[test]
fn t07_the_constants_keep_their_documented_relationship_and_values() {
    const { assert!(RETRY_BASE_DELAY_S > MIN_REFRESH_DELAY_S) };
    assert!(RETRY_BASE_DELAY_S * 2f64.powi(MAX_RETRY_EXPONENT as i32) >= RETRY_MAX_DELAY_S);
    const { assert!(HTTP_TIMEOUT_S < EXPIRY_LEEWAY_S) };
    assert_eq!(
        (
            EXPIRY_LEEWAY_S,
            MIN_REFRESH_DELAY_S,
            RETRY_BASE_DELAY_S,
            RETRY_MAX_DELAY_S,
            MAX_RETRY_EXPONENT,
            HTTP_TIMEOUT_S,
            MAX_ERROR_BODY_CHARS,
            THREAD_JOIN_TIMEOUT_S
        ),
        (30.0, 1.0, 5.0, 300.0, 6, 10.0, 500, 5.0)
    );
    let timings = KeycloakTimings::default();
    assert_eq!(timings.expiry_leeway_s, EXPIRY_LEEWAY_S);
    assert_eq!(timings.min_refresh_delay_s, MIN_REFRESH_DELAY_S);
    assert_eq!(timings.retry_base_delay_s, RETRY_BASE_DELAY_S);
    assert_eq!(timings.retry_max_delay_s, RETRY_MAX_DELAY_S);
    assert_eq!(timings.max_retry_exponent, MAX_RETRY_EXPONENT);
    assert_eq!(timings.http_timeout_s, HTTP_TIMEOUT_S);
    assert_eq!(timings.max_error_body_chars, MAX_ERROR_BODY_CHARS);
    assert_eq!(timings.thread_join_timeout_s, THREAD_JOIN_TIMEOUT_S);
}

// ---------------------------------------------------------------------------------------------
// B. The background loop survives every failure

#[test]
fn t08_a_refused_refresh_is_retried_and_the_provider_recovers() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        refused(),
        token("acc-3", json!(300)),
    ]);
    let stop = ScriptedStop::new(vec![false, false], &clock);
    let provider = build(&transport, &clock, Some(stop), 0.0);

    provider.run_refresh_loop();

    assert_eq!(
        transport.grants(),
        ["password", "refresh_token", "refresh_token"]
    );
    assert_eq!(provider.access_token(), "acc-3");
    assert_eq!(provider.consecutive_refresh_failures(), 0);
    assert_eq!(provider.next_refresh_attempt_at(), 0.0);
}

#[test]
fn t09_no_kind_of_failure_ends_the_loop() {
    let failures = [
        (
            "network error",
            Step::Unreachable(io::ErrorKind::NetworkUnreachable, "network unreachable"),
        ),
        (
            "connection refused",
            Step::Unreachable(io::ErrorKind::ConnectionRefused, "connection refused"),
        ),
        (
            "timeout",
            Step::Unreachable(io::ErrorKind::TimedOut, "read timed out"),
        ),
        ("transport bug", Step::Other("a bug in a custom transport")),
        ("transport panic", Step::PanicStr),
        ("503", status(503, r#"{"error":"temporarily_unavailable"}"#)),
        ("502 empty", status(502, "")),
        ("200 not json", status(200, "<html>bad gateway</html>")),
        ("200 json array", status(200, r#"["not","an","object"]"#)),
        ("200 no access token", status(200, r#"{"expires_in":300}"#)),
        (
            "200 bad expires_in",
            status(200, r#"{"access_token":"acc","expires_in":"soon"}"#),
        ),
    ];
    for (name, failure) in failures {
        let clock = FakeClock::at(1000.0);
        let transport = FakeTransport::new(vec![
            token("acc-1", json!(300)),
            failure,
            token("acc-ok", json!(300)),
        ]);
        let stop = ScriptedStop::new(vec![false, false], &clock);
        let provider = build(&transport, &clock, Some(stop), 0.0);

        provider.run_refresh_loop();

        assert_eq!(
            transport.calls().len(),
            3,
            "{name}: the loop stopped at the failure"
        );
        assert_eq!(provider.access_token(), "acc-ok", "{name}");
    }
}

#[test]
fn t10_a_persistent_failure_keeps_the_loop_alive_without_repairing_the_provider() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), refused()]);
    let stop = ScriptedStop::new(vec![false; 6], &clock);
    let provider = build(&transport, &clock, Some(stop), 0.0);

    provider.run_refresh_loop();

    let mut expected = vec!["password".to_string()];
    expected.extend(vec!["refresh_token".to_string(); 6]);
    assert_eq!(transport.grants(), expected);
    assert_eq!(provider.access_token(), "acc-1");
    assert_eq!(provider.consecutive_refresh_failures(), 6);
}

#[test]
fn t11_no_failure_is_ever_answered_with_a_password_grant() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        status(401, r#"{"error":"invalid_client"}"#),
    ]);
    let stop = ScriptedStop::new(vec![false; 10], &clock);
    let provider = build(&transport, &clock, Some(stop), 0.0);

    provider.run_refresh_loop();

    assert_eq!(
        transport
            .grants()
            .iter()
            .filter(|grant| *grant == "password")
            .count(),
        1
    );
    assert_eq!(transport.calls().len(), 11);
}

// ---------------------------------------------------------------------------------------------
// C. The background loop backs off

#[test]
fn t12_the_first_retry_waits_the_base_not_the_floor() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        status(503, "{}"),
        token("acc-2", json!(300)),
    ]);
    let stop = ScriptedStop::new(vec![false, false], &clock);
    let provider = build(&transport, &clock, Some(stop.clone()), 0.0);

    provider.run_refresh_loop();

    assert_eq!(stop.waits()[0], 300.0 - EXPIRY_LEEWAY_S);
    assert_eq!(stop.waits()[1], RETRY_BASE_DELAY_S);
}

#[test]
fn t13_consecutive_failures_climb_the_ladder_to_the_ceiling() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), status(503, "{}")]);
    let stop = ScriptedStop::new(vec![false; 9], &clock);
    let provider = build(&transport, &clock, Some(stop.clone()), 1.0);

    provider.run_refresh_loop();

    assert_eq!(
        stop.waits()[1..],
        [5.0, 10.0, 20.0, 40.0, 80.0, 160.0, 300.0, 300.0, 300.0]
    );
}

#[test]
fn t14_a_success_resets_the_ladder_to_the_healthy_schedule() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        status(503, "{}"),
        status(503, "{}"),
        token("acc-4", json!(120)),
        status(503, "{}"),
    ]);
    let stop = ScriptedStop::new(vec![false; 4], &clock);
    let provider = build(&transport, &clock, Some(stop.clone()), 1.0);

    let ((), logs) = captured(|| provider.run_refresh_loop());

    assert_eq!(stop.waits(), [270.0, 5.0, 10.0, 90.0, 5.0]);
    assert_eq!(
        logs.count(Level::INFO, "recovered after 2 failed attempt(s)"),
        1
    );
    assert_eq!(
        logs.count(Level::WARN, "Keycloak token refresh failed (attempt"),
        3
    );
}

#[test]
fn t15_the_backoff_is_measured_from_when_the_failure_was_recorded() {
    let clock = FakeClock::at(1000.0);
    let hanging = clock.clone();
    let transport = FakeTransport::with(
        vec![token("acc-1", json!(300)), status(503, "{}")],
        Duration::ZERO,
        Some(Box::new(move |request: &TokenRequest| {
            if request.field("grant_type") == Some("refresh_token") {
                hanging.advance(HTTP_TIMEOUT_S);
            }
        })),
    );
    let stop = ScriptedStop::new(vec![false, false], &clock);
    let provider = build(&transport, &clock, Some(stop.clone()), 0.0);

    provider.run_refresh_loop();

    assert_eq!(stop.waits()[1], RETRY_BASE_DELAY_S);
}

#[test]
fn t16_the_login_deadline_still_ends_the_loop_while_it_backs_off() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(30)), status(503, "{}")]);
    let stop = ScriptedStop::new(vec![false; 50], &clock);
    let config = config().with_token_expiration_in_s(100);
    let provider = build_with(&config, &transport, &clock, Some(stop.clone()), 1.0).unwrap();

    provider.run_refresh_loop();

    assert!(
        clock.now() <= 1000.0 + 100.0 + MIN_REFRESH_DELAY_S,
        "{}",
        clock.now()
    );
    assert!(
        stop.is_set() && provider.is_stopped(),
        "the loop did not stop at the deadline"
    );
    assert!(stop.waits().iter().all(|delay| *delay <= 100.0));
    assert!(stop.waits().len() < 50);
}

#[test]
fn t17_stop_during_a_backoff_wait_ends_the_loop_without_another_request() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), status(503, "{}")]);
    let stop = ScriptedStop::advancing(vec![false, true], &clock, Some(vec![270.0, 1.0]));
    let provider = build(&transport, &clock, Some(stop), 0.0);

    provider.run_refresh_loop();

    assert_eq!(transport.grants(), ["password", "refresh_token"]);
}

#[test]
fn t17b_a_stopped_provider_runs_no_loop_at_all() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(0))]);
    let stop = ScriptedStop::new(vec![false; 5], &clock);
    let provider = build(&transport, &clock, Some(stop.clone()), 0.0);
    provider.stop();

    provider.run_refresh_loop();

    assert!(stop.waits().is_empty());
    assert_eq!(transport.calls().len(), 1);
}

/// A stop signal that ignores `set`, as a worker blocked elsewhere would: only the provider being
/// gone can end that loop.
struct DeafStop {
    waits: AtomicUsize,
}

impl StopSignal for DeafStop {
    fn wait(&self, _timeout_s: f64) -> bool {
        self.waits.fetch_add(1, Ordering::SeqCst);
        thread::sleep(Duration::from_millis(20));
        false
    }
    fn set(&self) {}
    fn is_set(&self) -> bool {
        false
    }
}

#[test]
fn t18_t40_a_dropped_provider_ends_the_loop_and_a_join_timeout_does_not_escape() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300))]);
    let deaf = Arc::new(DeafStop {
        waits: AtomicUsize::new(0),
    });
    let mut options = options(&transport, &clock, None, 0.0);
    options.stop_signal = deaf.clone();
    options.start_background_refresh = true;
    options.timings.thread_join_timeout_s = 0.01;
    let provider = KeycloakTokenProvider::with_options(&config(), options).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while deaf.waits.load(Ordering::SeqCst) < 3 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(provider.is_background_refresh_running());

    let started = Instant::now();
    drop(provider); // stop() is ignored by the deaf signal, and its join times out after 10 ms
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "the join did not time out"
    );

    thread::sleep(Duration::from_millis(200));
    let waits = deaf.waits.load(Ordering::SeqCst);
    thread::sleep(Duration::from_millis(200));
    assert_eq!(
        deaf.waits.load(Ordering::SeqCst),
        waits,
        "the loop outlived its provider"
    );
    assert_eq!(transport.calls().len(), 1, "a gone provider sent a request");
}

// ---------------------------------------------------------------------------------------------
// D. The lazy path shares the backoff

#[test]
fn t19_a_still_valid_token_is_served_when_its_refresh_fails() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), status(503, "{}")]);
    let provider = build(&transport, &clock, None, 0.0);
    clock.advance(300.0 - EXPIRY_LEEWAY_S + 1.0);

    let (metadata, logs) = captured(|| provider.authorization_metadata());

    assert_eq!(
        metadata.unwrap(),
        ("authorization", "Bearer acc-1".to_string())
    );
    assert_eq!(logs.count(Level::WARN, "refresh failed (attempt 1)"), 1);
    assert!(
        logs.lines()[0].contains("the current access token stays valid for 29.0s"),
        "{:?}",
        logs.lines()
    );
}

#[test]
fn t20_no_request_is_sent_while_the_backoff_runs() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), status(503, "{}")]);
    let provider = build(&transport, &clock, None, 0.0);
    clock.advance(275.0);

    for _ in 0..100 {
        provider.authorization_metadata().unwrap();
        clock.advance(0.04);
    }

    assert_eq!(
        transport.grants(),
        ["password", "refresh_token"],
        "every call hit the token endpoint"
    );
}

#[test]
fn t21_an_expired_token_raises_during_the_backoff_without_a_request() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), status(503, "{}")]);
    let provider = build(&transport, &clock, None, 1.0);
    clock.advance(299.0);
    provider.authorization_metadata().unwrap(); // fails, backs off 5 s, valid 1 s more
    clock.advance(2.0);

    let error = provider.authorization_metadata().unwrap_err();

    assert!(
        error
            .to_string()
            .contains("backing off after 1 consecutive failure"),
        "{error}"
    );
    assert!(
        error
            .to_string()
            .ends_with("the next attempt is due in 3.0s"),
        "{error}"
    );
    assert_eq!(transport.calls().len(), 2);
}

#[test]
fn t22_an_expired_token_whose_refresh_fails_raises() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), refused()]);
    let provider = build(&transport, &clock, None, 0.0);
    clock.advance(400.0);

    let (result, logs) = captured(|| provider.authorization_metadata());
    let error = result.unwrap_err();

    assert!(error.to_string().contains("status 400"), "{error}");
    assert!(
        error.to_string().contains("Offline user session not found"),
        "{error}"
    );
    assert_eq!(
        logs.count(
            Level::WARN,
            "the current access token has expired (KeycloakAuthenticationError: "
        ),
        1
    );
}

#[test]
fn t23_a_non_keycloak_error_reaches_the_caller_as_a_keycloak_error() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        Step::Other("transport bug"),
    ]);
    let provider = build(&transport, &clock, None, 0.0);
    clock.advance(400.0);

    let (result, logs) = captured(|| provider.authorization_metadata());
    let error = result.unwrap_err();

    assert_eq!(
        error.to_string(),
        "Keycloak token refresh failed (TransportBug: transport bug)"
    );
    let cause = error.source().expect("the cause is kept");
    let cause = cause
        .downcast_ref::<TransportError>()
        .expect("the transport's error is the cause");
    assert_eq!(cause.kind(), TransportErrorKind::Other);
    assert_eq!(source_of(cause), "transport bug");
    assert_eq!(logs.count(Level::WARN, "(TransportBug: transport bug)"), 1);
}

#[test]
fn t23b_a_panicking_transport_is_a_failed_refresh_not_a_dead_caller() {
    for (step, cause) in [
        (Step::PanicStr, "a bug in a custom transport"),
        (Step::PanicString, "a formatted bug"),
        (Step::PanicOther, "a non-string panic payload"),
    ] {
        let clock = FakeClock::at(1000.0);
        let transport = FakeTransport::new(vec![token("acc-1", json!(300)), step]);
        let provider = build(&transport, &clock, None, 0.0);
        clock.advance(400.0);

        let error = provider.authorization_metadata().unwrap_err();

        assert_eq!(
            error.to_string(),
            format!("Keycloak token refresh failed (TransportPanicked: the token transport panicked: {cause})")
        );
    }
}

#[test]
fn t24_the_lazy_path_retries_once_the_backoff_elapses_and_recovers() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        status(503, "{}"),
        token("acc-3", json!(300)),
    ]);
    let provider = build(&transport, &clock, None, 0.0);
    clock.advance(280.0);
    provider.authorization_metadata().unwrap();
    assert_eq!(provider.consecutive_refresh_failures(), 1);
    assert_eq!(provider.next_refresh_attempt_at(), 1285.0);
    clock.advance(RETRY_BASE_DELAY_S);

    let (metadata, logs) = captured(|| provider.authorization_metadata());

    assert_eq!(metadata.unwrap().1, "Bearer acc-3");
    assert_eq!(provider.consecutive_refresh_failures(), 0);
    assert_eq!(
        logs.count(Level::INFO, "recovered after 1 failed attempt"),
        1
    );
}

#[test]
fn t25_a_lazy_failure_schedules_the_background_loops_next_wake() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        status(503, "{}"),
        token("acc-3", json!(300)),
    ]);
    let stop = ScriptedStop::new(vec![false], &clock);
    let provider = build(&transport, &clock, Some(stop.clone()), 1.0);
    clock.advance(280.0);
    provider.authorization_metadata().unwrap(); // next attempt at +5 s
    clock.advance(2.0);

    provider.run_refresh_loop();

    assert!(
        (stop.waits()[0] - 3.0).abs() < 1e-9,
        "the loop ignored the lazy path's backoff"
    );
    assert_eq!(provider.access_token(), "acc-3");
}

#[test]
fn t26_concurrent_calls_during_an_outage_send_one_request_and_never_fail() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::with(
        vec![
            token("acc-1", json!(20)),
            Step::Unreachable(io::ErrorKind::Other, "down"),
        ],
        Duration::from_millis(50),
        None,
    );
    let provider = Arc::new(build(&transport, &clock, None, 0.0));
    let threads: Vec<_> = (0..16)
        .map(|_| {
            let provider = Arc::clone(&provider);
            thread::spawn(move || {
                (0..20)
                    .map(|_| provider.authorization_metadata())
                    .collect::<Vec<_>>()
            })
        })
        .collect();

    let results: Vec<_> = threads
        .into_iter()
        .flat_map(|thread| thread.join().unwrap())
        .collect();

    assert_eq!(results.len(), 320);
    assert!(results
        .iter()
        .all(|result| matches!(result, Ok((_, value)) if value == "Bearer acc-1")));
    assert_eq!(transport.grants(), ["password", "refresh_token"]);
}

// ---------------------------------------------------------------------------------------------
// E. The token-endpoint answer is validated

#[test]
fn t27_a_network_failure_at_login_is_a_keycloak_error_with_its_cause() {
    for kind in [
        io::ErrorKind::Other,
        io::ErrorKind::ConnectionRefused,
        io::ErrorKind::TimedOut,
    ] {
        let transport = FakeTransport::new(vec![Step::Unreachable(kind, "unreachable")]);
        let error = authentication_error(build_with(
            &config(),
            &transport,
            &FakeClock::at(0.0),
            None,
            0.0,
        ));

        assert_eq!(
            error.to_string(),
            format!(
                "Keycloak token login could not reach the token endpoint ({kind:?}: unreachable)"
            )
        );
        let cause = error
            .source()
            .unwrap()
            .downcast_ref::<TransportError>()
            .unwrap();
        assert_eq!(
            (cause.kind(), cause.type_name()),
            (
                TransportErrorKind::Unreachable,
                format!("{kind:?}").as_str()
            )
        );
        assert_eq!(
            cause
                .source()
                .unwrap()
                .downcast_ref::<io::Error>()
                .unwrap()
                .kind(),
            kind
        );
    }
}

#[test]
fn t27b_any_other_failure_at_login_is_wrapped_too() {
    let transport = FakeTransport::new(vec![Step::Other("broken")]);
    let error = authentication_error(build_with(
        &config(),
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    ));
    assert_eq!(
        error.to_string(),
        "Keycloak token login failed (TransportBug: broken)"
    );
    assert!(error.source().is_some());
}

#[test]
fn t28_a_non_json_body_is_a_keycloak_error() {
    let transport = FakeTransport::new(vec![status(200, "<html>bad gateway</html>")]);
    let error = authentication_error(build_with(
        &config(),
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    ));
    assert_eq!(
        error.to_string(),
        "Keycloak token login returned status 200 with a body that is not JSON"
    );
    assert!(error
        .source()
        .unwrap()
        .downcast_ref::<serde_json::Error>()
        .is_some());
}

#[test]
fn t29_a_json_body_that_is_not_an_object_is_a_keycloak_error() {
    for (body, word) in [
        (r#"["access_token"]"#, "array"),
        (r#""access_token""#, "string"),
        ("42", "number"),
        ("true", "boolean"),
        ("null", "null"),
    ] {
        let transport = FakeTransport::new(vec![status(200, body)]);
        let error = authentication_error(build_with(
            &config(),
            &transport,
            &FakeClock::at(0.0),
            None,
            0.0,
        ));
        assert_eq!(
            error.to_string(),
            format!("Keycloak token login returned a JSON {word} instead of an object")
        );
    }
}

#[test]
fn t30_a_huge_error_page_is_truncated_in_the_message() {
    let page = format!("<html>{}</html>", "x".repeat(10_000));
    let transport = FakeTransport::new(vec![status(502, &page)]);
    let error = authentication_error(build_with(
        &config(),
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    ));
    assert!(error.to_string().len() < MAX_ERROR_BODY_CHARS + 200);
    assert!(error
        .to_string()
        .starts_with("Keycloak token login failed with status 502: <html>xxx"));
    // Truncated by CHARACTERS, never inside a multi-byte one.
    let transport = FakeTransport::new(vec![status(502, &"ü".repeat(1000))]);
    let error = authentication_error(build_with(
        &config(),
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    ));
    assert!(error
        .to_string()
        .ends_with(&"ü".repeat(MAX_ERROR_BODY_CHARS)));
}

#[test]
fn t31_a_response_without_an_access_token_names_fields_never_values() {
    for body in [
        r#"{"refresh_token":"secret-offline-token-77","expires_in":300}"#,
        r#"{"access_token":"","refresh_token":"secret-offline-token-77","expires_in":300}"#,
        r#"{"access_token":7,"refresh_token":"secret-offline-token-77","expires_in":300}"#,
    ] {
        let transport = FakeTransport::new(vec![status(200, body)]);
        let error = authentication_error(build_with(
            &config(),
            &transport,
            &FakeClock::at(0.0),
            None,
            0.0,
        ));
        assert!(error.to_string().contains("'refresh_token'"), "{error}");
        assert!(
            !error.to_string().contains("secret-offline-token-77"),
            "{error}"
        );
        assert!(error.to_string().starts_with(
            "Keycloak token response did not contain an access_token; the response carried the fields ["
        ));
    }
    let transport = FakeTransport::new(vec![status(
        200,
        r#"{"z":1,"expires_in":300,"refresh_token":"s"}"#,
    )]);
    let error = authentication_error(build_with(
        &config(),
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    ));
    assert!(
        error
            .to_string()
            .contains("the fields ['expires_in', 'refresh_token', 'z'] (values withheld"),
        "{error}"
    );
}

#[test]
fn t32_a_non_numeric_expires_in_leaves_the_previous_token_untouched() {
    for bad in [
        json!("soon"),
        json!(null),
        json!(true),
        json!([300]),
        json!({"s": 1}),
        json!("inf"),
        json!("NaN"),
    ] {
        let clock = FakeClock::at(1000.0);
        let transport = FakeTransport::new(vec![
            token("acc-1", json!(300)),
            token_with("acc-bad", bad.clone(), Some("off-bad")),
        ]);
        let provider = build(&transport, &clock, None, 0.0);
        clock.advance(280.0);

        provider.authorization_metadata().unwrap();

        assert_eq!(provider.access_token(), "acc-1", "{bad}");
        assert_eq!(provider.access_token_expires_at(), 1300.0, "{bad}");
        let transport_at_login = FakeTransport::new(vec![token_with("acc-1", bad.clone(), None)]);
        let error = authentication_error(build_with(
            &config(),
            &transport_at_login,
            &clock,
            None,
            0.0,
        ));
        assert_eq!(
            error.to_string(),
            "Keycloak token response carried a non-numeric expires_in (value withheld)"
        );
    }
}

#[test]
fn t33_a_numeric_string_or_a_missing_expires_in_is_accepted() {
    let clock = FakeClock::at(1000.0);
    let provider = build(
        &FakeTransport::new(vec![token("acc-1", json!("300"))]),
        &clock,
        None,
        0.0,
    );
    assert_eq!(provider.access_token_expires_at(), 1300.0);
    let provider = build(
        &FakeTransport::new(vec![token("acc-1", json!(" 12.5 "))]),
        &clock,
        None,
        0.0,
    );
    assert_eq!(provider.access_token_expires_at(), 1012.5);
    let provider = build(
        &FakeTransport::new(vec![status(200, r#"{"access_token":"acc-1"}"#)]),
        &clock,
        None,
        0.0,
    );
    assert_eq!(provider.access_token_expires_at(), 1000.0);
}

#[test]
fn t34_a_negative_expires_in_counts_as_already_expired() {
    let clock = FakeClock::at(1000.0);
    let provider = build(
        &FakeTransport::new(vec![token("acc-1", json!(-50))]),
        &clock,
        None,
        0.0,
    );
    assert_eq!(provider.access_token_expires_at(), 1000.0);
}

#[test]
fn t35_a_refresh_without_a_new_refresh_token_keeps_the_previous_one() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token_with("acc-1", json!(300), Some("off-1")),
        token_with("acc-2", json!(300), None),
        token_with("acc-3", json!(300), Some("")),
        token("acc-4", json!(300)),
    ]);
    let provider = build(&transport, &clock, None, 0.0);
    for _ in 0..3 {
        clock.advance(280.0);
        provider.authorization_metadata().unwrap();
    }

    let sent: Vec<_> = transport.calls()[1..]
        .iter()
        .map(|call| call.field("refresh_token").map(str::to_string))
        .collect();
    assert_eq!(
        sent,
        [
            Some("off-1".into()),
            Some("off-1".into()),
            Some("off-1".into())
        ]
    );
    assert_eq!(provider.access_token(), "acc-4");
}

#[test]
fn t36_the_grant_forms_url_and_timeout_are_exact() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300))]);
    let config = KeycloakConfig::new(
        "https://keycloak.example/auth//",
        REALM,
        CLIENT_ID,
        USERNAME,
        PASSWORD,
    );
    let provider = build_with(&config, &transport, &clock, None, 0.0).unwrap();
    clock.advance(280.0);
    provider.authorization_metadata().unwrap();

    let calls = transport.calls();
    let form = |call: &TokenRequest| -> Vec<(String, String)> { call.form.clone() };
    let pairs = |pairs: &[(&str, &str)]| -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    };
    assert_eq!(
        form(&calls[0]),
        pairs(&[
            ("grant_type", "password"),
            ("client_id", CLIENT_ID),
            ("username", USERNAME),
            ("password", PASSWORD),
            ("scope", "offline_access"),
        ])
    );
    assert_eq!(
        form(&calls[1]),
        pairs(&[
            ("grant_type", "refresh_token"),
            ("client_id", CLIENT_ID),
            ("refresh_token", "off-x")
        ])
    );
    for call in &calls {
        assert_eq!(
            call.url,
            "https://keycloak.example/auth/realms/ondewo-test-realm/protocol/openid-connect/token"
        );
        assert_eq!(call.timeout, Duration::from_secs(10));
        assert!(call.verify_ssl);
    }
    assert_eq!(config.token_endpoint(), calls[0].url);
    let unverified = build_with(
        &config.clone().with_verify_ssl(false),
        &transport,
        &clock,
        None,
        0.0,
    )
    .unwrap();
    drop(unverified);
    assert!(!transport.calls().last().unwrap().verify_ssl);
}

// ---------------------------------------------------------------------------------------------
// F. No credential reaches a log or an error

#[test]
fn t37_the_failure_lines_and_errors_carry_no_credential() {
    let secrets = [PASSWORD, "acc-secret-1", "off-secret-1", "acc-secret-2"];
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token_with("acc-secret-1", json!(300), Some("off-secret-1")),
        status(503, "{}"),
        token_with("acc-secret-2", json!(300), None),
    ]);
    let stop = ScriptedStop::new(vec![false, false], &clock);
    let provider = build(&transport, &clock, Some(stop), 0.0);

    let ((), logs) = captured(|| provider.run_refresh_loop());
    let refresh_request = transport.calls()[1].clone();

    let text = logs.lines().join("\n");
    assert!(
        text.contains("refresh failed (attempt 1)") && text.contains("recovered"),
        "{text}"
    );
    for secret in secrets {
        assert!(!text.contains(secret), "{secret} reached the log");
    }
    let mut errors: Vec<String> = Vec::new();
    for step in [
        refused(),
        status(200, "x"),
        status(200, "[]"),
        status(200, r#"{"refresh_token":"off-secret-1"}"#),
        token_with("acc-secret-2", json!("soon"), Some("off-secret-1")),
        Step::Unreachable(io::ErrorKind::Other, "down"),
        Step::Other("bug"),
    ] {
        let transport = FakeTransport::new(vec![step]);
        errors.push(
            authentication_error(build_with(&config(), &transport, &clock, None, 0.0)).to_string(),
        );
    }
    for secret in secrets {
        assert!(
            errors.iter().all(|error| !error.contains(secret)),
            "{secret} reached an error: {errors:?}"
        );
    }
    let rendered = format!("{provider:?} {:?} {refresh_request:?}", config());
    for secret in [PASSWORD, "off-secret-1", "acc-secret-1", "acc-secret-2"] {
        assert!(
            !rendered.contains(secret),
            "{secret} reached a rendering: {rendered}"
        );
    }
    assert!(
        rendered.contains("form_fields: [\"grant_type\", \"client_id\", \"refresh_token\"]"),
        "{rendered}"
    );
}

// ---------------------------------------------------------------------------------------------
// G. stop / close

#[test]
fn t38_stop_is_idempotent_without_a_worker() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300))]);
    let provider = build(&transport, &clock, None, 0.0);
    assert!(!provider.is_background_refresh_running() && !provider.is_closed());

    provider.stop();
    provider.close();
    provider.stop();

    assert!(provider.is_closed() && provider.is_stopped());
    // The lazy path still works on demand after a stop.
    clock.advance(280.0);
    provider.authorization_metadata().unwrap();
    assert_eq!(transport.calls().len(), 2);
}

#[test]
fn t39_stop_from_within_the_worker_never_joins_itself() {
    let slot: Arc<OnceLock<Weak<KeycloakTokenProvider>>> = Arc::default();
    let hook_slot = Arc::clone(&slot);
    let transport = FakeTransport::with(
        vec![token("acc-1", json!(0)), token("acc-2", json!(3600))],
        Duration::ZERO,
        Some(Box::new(move |request: &TokenRequest| {
            if request.field("grant_type") == Some("refresh_token") {
                if let Some(provider) = hook_slot.get().and_then(Weak::upgrade) {
                    provider.stop(); // on the worker thread itself
                }
            }
        })),
    );
    let mut options = options(&transport, &FakeClock::at(1000.0), None, 0.0);
    options.clock = Arc::new(monotonic_seconds);
    options.start_background_refresh = true;
    options.timings.min_refresh_delay_s = 0.01;
    let provider = Arc::new(KeycloakTokenProvider::with_options(&config(), options).unwrap());
    slot.set(Arc::downgrade(&provider)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while !provider.is_closed() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(provider.is_closed(), "the worker never refreshed");
    while provider.access_token() != "acc-2" && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(provider.access_token(), "acc-2");
    provider.stop(); // after the worker detached itself: nothing left to join
    drop(provider); // and Drop after close
}

#[test]
fn t39b_dropping_a_provider_with_a_real_worker_stops_and_joins_it_promptly() {
    let transport = FakeTransport::new(vec![token("acc-1", json!(3600))]);
    let mut options = options(&transport, &FakeClock::at(1000.0), None, 0.0);
    options.start_background_refresh = true;
    let provider = KeycloakTokenProvider::with_options(&config(), options).unwrap();
    assert!(provider.is_background_refresh_running());

    let started = Instant::now();
    provider.stop(); // interrupts a 3570 s wait

    assert!(started.elapsed() < Duration::from_secs(2));
    assert!(!provider.is_background_refresh_running());
    assert_eq!(transport.calls().len(), 1);
}

// ---------------------------------------------------------------------------------------------
// H. The real worker, end to end (shrunk constants, real clock)

#[test]
fn t41_the_real_worker_survives_failures_and_recovers() {
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(0.1)),
        Step::Unreachable(io::ErrorKind::ConnectionRefused, "refused"),
        status(503, "{}"),
        Step::Other("generic"),
        token("acc-ok", json!(3600)),
    ]);
    let options = KeycloakProviderOptions {
        transport: transport.clone(),
        timings: KeycloakTimings {
            expiry_leeway_s: 0.05,
            min_refresh_delay_s: 0.01,
            retry_base_delay_s: 0.05,
            retry_max_delay_s: 0.05,
            ..KeycloakTimings::default()
        },
        ..KeycloakProviderOptions::default()
    };
    let provider = KeycloakTokenProvider::with_options(&config(), options).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while provider.access_token() != "acc-ok" && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }

    assert_eq!(provider.access_token(), "acc-ok");
    assert!(provider.is_background_refresh_running());
    let mut expected = vec!["password".to_string()];
    expected.extend(vec!["refresh_token".to_string(); 4]);
    assert_eq!(transport.grants(), expected);
    provider.stop();
    assert!(!provider.is_background_refresh_running());
}

// ---------------------------------------------------------------------------------------------
// I. The offline-token start-up

fn token_config(refresh_token: &str) -> KeycloakConfig {
    KeycloakConfig::new(KEYCLOAK_URL, REALM, CLIENT_ID, USERNAME, "")
        .with_refresh_token(refresh_token)
}

#[test]
fn t42_the_start_up_is_one_refresh_grant_with_the_handed_off_token() {
    let transport = FakeTransport::new(vec![token("acc-1", json!(300))]);
    let provider = build_with(
        &token_config("off-handed-0"),
        &transport,
        &FakeClock::at(1000.0),
        None,
        0.0,
    )
    .unwrap();

    let calls = transport.calls();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].field("grant_type"), Some("refresh_token"));
    assert_eq!(calls[0].field("refresh_token"), Some("off-handed-0"));
    assert_eq!(calls[0].field("password"), None);
    assert_eq!(provider.access_token(), "acc-1");
}

#[test]
fn t43_a_password_beside_the_token_is_never_sent() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        status(503, "{}"),
        token("acc-3", json!(300)),
    ]);
    let config = config().with_refresh_token("off-handed-0");
    let provider = build_with(&config, &transport, &clock, None, 0.0).unwrap();
    clock.advance(280.0);
    provider.authorization_metadata().unwrap();
    clock.advance(RETRY_BASE_DELAY_S);
    provider.authorization_metadata().unwrap();

    assert_eq!(transport.grants(), ["refresh_token"; 3]);
    assert!(transport
        .calls()
        .iter()
        .all(|call| call.field("password").is_none()));
}

#[test]
fn t44_a_refused_start_up_token_raises_and_never_falls_back_to_the_password() {
    let transport = FakeTransport::new(vec![refused(), token("acc-never", json!(300))]);
    let config = config().with_refresh_token("off-dead");

    let error = authentication_error(build_with(
        &config,
        &transport,
        &FakeClock::at(1000.0),
        None,
        0.0,
    ));

    assert!(
        error
            .to_string()
            .starts_with("Keycloak token refresh failed with status 400: "),
        "{error}"
    );
    assert_eq!(transport.grants(), ["refresh_token"]);
}

#[test]
fn t45_a_rotated_token_is_used_by_the_next_refresh() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token_with("acc-1", json!(300), Some("off-rotated")),
        token("acc-2", json!(300)),
    ]);
    let provider =
        build_with(&token_config("off-handed-0"), &transport, &clock, None, 0.0).unwrap();
    clock.advance(280.0);
    provider.authorization_metadata().unwrap();

    assert_eq!(
        transport.calls()[1].field("refresh_token"),
        Some("off-rotated")
    );
}

#[test]
fn t46_the_deadline_is_armed_on_the_token_path_and_ends_the_loop() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(60))]);
    let stop = ScriptedStop::new(vec![false; 50], &clock);
    let config = token_config("off-handed-0").with_token_expiration_in_s(200);
    let provider = build_with(&config, &transport, &clock, Some(stop.clone()), 0.0).unwrap();
    assert_eq!(provider.login_deadline(), Some(1200.0));

    provider.run_refresh_loop();

    assert!(
        clock.now() <= 1200.0 + MIN_REFRESH_DELAY_S,
        "{}",
        clock.now()
    );
    assert!(provider.is_stopped());
    // Past the deadline the lazy path no longer refreshes: the token is allowed to lapse.
    let requests = transport.calls().len();
    clock.advance(1000.0);
    assert_eq!(provider.authorization_metadata().unwrap().1, "Bearer acc-1");
    assert_eq!(transport.calls().len(), requests);
}

#[test]
fn t47_the_loop_survives_a_failing_offline_token() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![
        token("acc-1", json!(300)),
        refused(),
        refused(),
        token("acc-4", json!(300)),
    ]);
    let stop = ScriptedStop::new(vec![false; 3], &clock);
    let provider = build_with(
        &token_config("off-handed-0"),
        &transport,
        &clock,
        Some(stop),
        0.0,
    )
    .unwrap();

    provider.run_refresh_loop();

    assert_eq!(transport.grants(), ["refresh_token"; 4]);
    assert_eq!(provider.access_token(), "acc-4");
}

// ---------------------------------------------------------------------------------------------
// J. The shared registry

static UNIQUE: AtomicUsize = AtomicUsize::new(0);

/// Credentials no other test of this binary uses (the registry is process-wide).
fn unique_config(test: &str) -> KeycloakConfig {
    let n = UNIQUE.fetch_add(1, Ordering::SeqCst);
    KeycloakConfig::new(
        KEYCLOAK_URL,
        REALM,
        CLIENT_ID,
        format!("{test}-{n}@example.com"),
        PASSWORD,
    )
}

struct CountingFactory {
    transport: Arc<FakeTransport>,
    constructions: AtomicUsize,
}

impl CountingFactory {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            transport: FakeTransport::new(vec![token("acc-1", json!(300))]),
            constructions: AtomicUsize::new(0),
        })
    }

    fn get(self: &Arc<Self>, config: &KeycloakConfig) -> Arc<KeycloakTokenProvider> {
        let factory = Arc::clone(self);
        get_keycloak_token_provider_with_options(config, move || {
            factory.constructions.fetch_add(1, Ordering::SeqCst);
            options(&factory.transport, &FakeClock::at(1000.0), None, 0.0)
        })
        .unwrap()
    }

    fn constructions(&self) -> usize {
        self.constructions.load(Ordering::SeqCst)
    }
}

#[test]
fn t48_the_same_credentials_share_one_provider() {
    let factory = CountingFactory::new();
    let config = unique_config("t48");

    let first = factory.get(&config);
    let second = factory.get(&config.clone());

    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(factory.constructions(), 1);
    assert_eq!(
        factory.transport.calls().len(),
        1,
        "two logins for one credential set"
    );
}

#[test]
fn t49_different_credentials_get_different_providers() {
    let factory = CountingFactory::new();
    let base = unique_config("t49");
    let variants = [
        KeycloakConfig {
            username: format!("other-{}", base.username),
            ..base.clone()
        },
        KeycloakConfig {
            password: "other-password".into(),
            ..base.clone()
        },
        KeycloakConfig {
            realm: "other-realm".into(),
            ..base.clone()
        },
        KeycloakConfig {
            keycloak_url: "https://other.example/auth".into(),
            ..base.clone()
        },
        KeycloakConfig {
            client_id: "other-client".into(),
            ..base.clone()
        },
        base.clone().with_token_expiration_in_s(60),
        base.clone().with_verify_ssl(false),
        base.clone().with_refresh_token("off-other"),
    ];
    let first = factory.get(&base);
    for variant in &variants {
        assert!(!Arc::ptr_eq(&first, &factory.get(variant)), "{variant:?}");
    }
    assert_eq!(factory.constructions(), 1 + variants.len());
}

#[test]
fn t50_configs_created_and_dropped_in_a_loop_each_get_their_own_provider() {
    let factory = CountingFactory::new();
    let configs: Vec<KeycloakConfig> = (0..50).map(|_| unique_config("t50")).collect();
    for config in &configs {
        drop(factory.get(&config.clone()));
    }
    assert_eq!(factory.constructions(), 50);
    let usernames: Vec<String> = factory
        .transport
        .calls()
        .iter()
        .map(|call| call.field("username").unwrap().to_string())
        .collect();
    let expected: Vec<String> = configs
        .iter()
        .map(|config| config.username.clone())
        .collect();
    assert_eq!(usernames, expected);
}

#[test]
fn t51_t60_token_only_tenants_never_share_a_provider() {
    let factory = CountingFactory::new();
    let base = unique_config("t51");
    let tenant_a = KeycloakConfig {
        password: String::new(),
        ..base.clone()
    }
    .with_refresh_token("off-tenant-a");
    let tenant_b = KeycloakConfig {
        password: String::new(),
        ..base
    }
    .with_refresh_token("off-tenant-b");

    let a = factory.get(&tenant_a);
    let b = factory.get(&tenant_b);

    assert!(!Arc::ptr_eq(&a, &b));
    let calls = factory.transport.calls();
    let tokens: Vec<_> = calls
        .iter()
        .map(|call| call.field("refresh_token").unwrap().to_string())
        .collect();
    assert_eq!(tokens, ["off-tenant-a", "off-tenant-b"]);
    assert!(calls.iter().all(|call| call.field("password").is_none()
        && call.field("grant_type") == Some("refresh_token")));
}

#[test]
fn t52_t53_the_registry_key_is_a_digest_without_a_secret() {
    let config = config().with_refresh_token("off-secret-token-x");
    let key = config.registry_key();
    assert_eq!(key.len(), 64);
    assert!(key
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert!(!key.contains("off-secret-token-x") && !key.contains(PASSWORD));
    assert_eq!(
        key,
        config.clone().registry_key(),
        "the key is derived from values, not identity"
    );
    // The same digest as the python SDKs: SHA-256 over the fields joined with NUL.
    let empty = KeycloakConfig::default().registry_key();
    assert_eq!(
        empty,
        KeycloakConfig {
            verify_ssl: true,
            ..KeycloakConfig::default()
        }
        .registry_key()
    );
    let known = KeycloakConfig::new("u", "r", "c", "n", "p")
        .with_token_expiration_in_s(5)
        .registry_key();
    assert_eq!(known, sha256_hex("u\0r\0c\0n\0p\x005\0True\0"));
}

/// An independent SHA-256 of `input` (FIPS 180-4), so the key derivation is checked against a
/// second implementation rather than against itself.
fn sha256_hex(input: &str) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    let mut message = input.as_bytes().to_vec();
    let bit_length = (message.len() as u64) * 8;
    message.push(0x80);
    while message.len() % 64 != 56 {
        message.push(0);
    }
    message.extend_from_slice(&bit_length.to_be_bytes());
    for block in message.chunks(64) {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, value) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(value);
        }
    }
    h.iter().map(|word| format!("{word:08x}")).collect()
}

#[test]
fn t54_a_closed_provider_is_never_handed_to_a_new_client() {
    let factory = CountingFactory::new();
    let config = unique_config("t54");
    let first = factory.get(&config);
    first.close();

    let second = factory.get(&config);

    assert!(!Arc::ptr_eq(&first, &second));
    assert!(!second.is_closed());
    assert_eq!(factory.constructions(), 2);
    assert!(Arc::ptr_eq(&second, &factory.get(&config)));
}

#[test]
fn t54b_a_failed_login_is_not_cached() {
    let config = unique_config("t54b");
    let failing = FakeTransport::new(vec![refused()]);
    let error = get_keycloak_token_provider_with_options(&config, || {
        options(&failing, &FakeClock::at(0.0), None, 0.0)
    })
    .unwrap_err();
    assert!(matches!(error, KeycloakError::Authentication(_)));
    let factory = CountingFactory::new();
    factory.get(&config);
    assert_eq!(factory.constructions(), 1);
    let error = get_keycloak_token_provider_with_options(
        &KeycloakConfig {
            realm: String::new(),
            ..config
        },
        || panic!("a refused config never builds a provider"),
    )
    .unwrap_err();
    assert!(matches!(
        error,
        KeycloakError::Config(KeycloakConfigError::IncompleteKeycloakFields)
    ));
}

// ---------------------------------------------------------------------------------------------
// K. The config

#[test]
fn t55_refresh_token_is_the_last_field() {
    let source = include_str!("../src/keycloak.rs");
    let start = source
        .find("pub struct KeycloakConfig {")
        .expect("the struct exists");
    let body = &source[start..start + source[start..].find("\n}").unwrap()];
    let fields: Vec<&str> = body
        .lines()
        .filter_map(|line| line.trim().strip_prefix("pub "))
        .filter_map(|line| line.split(':').next())
        .filter(|name| !name.starts_with("struct"))
        .collect();
    assert_eq!(fields.last(), Some(&"refresh_token"), "{fields:?}");
    assert_eq!(fields.len(), 8, "{fields:?}");
    let rendered = format!("{:?}", config());
    assert!(
        rendered
            .trim_end_matches(" }")
            .ends_with("refresh_token: \"\""),
        "{rendered}"
    );
}

#[test]
fn t56_a_token_only_config_is_valid_and_keycloak_configured() {
    let config = token_config("off-1");
    assert_eq!(config.validate(), Ok(()));
    assert!(config.is_keycloak_configured());
    assert!(config.password.is_empty());
}

#[test]
fn t57_a_keycloak_config_without_any_credential_is_refused() {
    let config = KeycloakConfig::new(KEYCLOAK_URL, REALM, CLIENT_ID, USERNAME, "");
    let error = config.validate().unwrap_err();
    assert_eq!(error, KeycloakConfigError::MissingCredential);
    assert!(error.to_string().contains("refresh_token") && error.to_string().contains("password"));
    let error = KeycloakTokenProvider::with_options(&config, KeycloakProviderOptions::default())
        .unwrap_err();
    assert!(matches!(
        error,
        KeycloakError::Config(KeycloakConfigError::MissingCredential)
    ));
    assert_eq!(
        error.to_string(),
        "Either the field `password` or the field `refresh_token` is mandatory in ClientConfig."
    );
    assert!(error.source().is_some());
    assert!(KeycloakInterceptor::new(&config).is_err());
}

#[test]
fn t58_a_password_config_is_unchanged() {
    let config = config();
    assert_eq!(config.validate(), Ok(()));
    assert!(config.is_keycloak_configured());
    assert!(
        config.verify_ssl
            && config.token_expiration_in_s.is_none()
            && config.refresh_token.is_empty()
    );
    assert!(
        KeycloakConfig::default().verify_ssl,
        "verification is on by default"
    );
}

#[test]
fn t59_the_rendering_redacts_both_secrets_and_shows_an_empty_one_as_empty() {
    let config = config().with_refresh_token("off-render-secret");
    let rendered = format!("{config:?} {config:#?}");
    assert!(
        !rendered.contains("off-render-secret") && !rendered.contains(PASSWORD),
        "{rendered}"
    );
    assert!(
        rendered.contains("password: ***REDACTED***")
            && rendered.contains("refresh_token: ***REDACTED***")
    );
    let empty = format!(
        "{:?}",
        KeycloakConfig::new(KEYCLOAK_URL, REALM, CLIENT_ID, USERNAME, "")
    );
    assert!(
        empty.contains("password: \"\"") && empty.contains("refresh_token: \"\""),
        "{empty}"
    );
    assert!(!empty.contains("REDACTED"));
}

#[test]
fn t61_a_partial_keycloak_triple_is_refused_naming_the_three_fields() {
    let full = config();
    let partials = [
        KeycloakConfig {
            realm: String::new(),
            client_id: String::new(),
            ..full.clone()
        },
        KeycloakConfig {
            keycloak_url: String::new(),
            client_id: String::new(),
            ..full.clone()
        },
        KeycloakConfig {
            keycloak_url: String::new(),
            realm: String::new(),
            ..full.clone()
        },
        KeycloakConfig {
            keycloak_url: String::new(),
            ..full.clone()
        },
        KeycloakConfig {
            realm: String::new(),
            ..full.clone()
        },
        KeycloakConfig {
            client_id: String::new(),
            ..full
        },
    ];
    for partial in partials {
        let error = partial.validate().unwrap_err();
        assert_eq!(error, KeycloakConfigError::IncompleteKeycloakFields);
        for field in ["`keycloak_url`", "`realm`", "`client_id`"] {
            assert!(error.to_string().contains(field), "{error}");
        }
        assert!(!partial.is_keycloak_configured());
    }
}

#[test]
fn t62_without_keycloak_no_provider_is_built_and_nothing_is_attached() {
    // No triple, no credential either: valid, and nothing ever reaches a network.
    for config in [
        KeycloakConfig::default(),
        KeycloakConfig {
            username: USERNAME.into(),
            ..KeycloakConfig::default()
        },
    ] {
        assert_eq!(config.validate(), Ok(()));
        let mut interceptor = KeycloakInterceptor::new(&config).unwrap();
        assert!(interceptor.provider().is_none());
        let request = interceptor.call(Request::new(())).unwrap();
        assert!(request.metadata().get("authorization").is_none());
        assert_eq!(
            format!("{interceptor:?}"),
            "KeycloakInterceptor { provider: None }"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The interceptor over a provider

#[test]
fn the_interceptor_attaches_the_bearer_token_and_maps_failures_to_unauthenticated() {
    let clock = FakeClock::at(1000.0);
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), refused()]);
    let provider = Arc::new(build(&transport, &clock, None, 0.0));
    let mut interceptor = KeycloakInterceptor::from_provider(Arc::clone(&provider));
    assert!(Arc::ptr_eq(interceptor.provider().unwrap(), &provider));

    let request = interceptor.call(Request::new(())).unwrap();
    assert_eq!(
        request
            .metadata()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        "Bearer acc-1"
    );

    clock.advance(400.0);
    let status = interceptor.call(Request::new(())).unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);
    assert!(
        status.message().contains("status 400"),
        "{}",
        status.message()
    );
    let rendered = format!("{interceptor:?}");
    assert!(
        rendered.contains("KeycloakTokenProvider")
            && !rendered.contains("acc-1")
            && !rendered.contains(PASSWORD)
    );
}

#[test]
fn an_access_token_that_is_no_header_value_is_refused_without_rendering_it() {
    let transport = FakeTransport::new(vec![token("acc-\u{1}-secret", json!(300))]);
    let mut interceptor = KeycloakInterceptor::from_provider(Arc::new(build(
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    )));
    let status = interceptor.call(Request::new(())).unwrap_err();
    assert_eq!(status.code(), Code::Unauthenticated);
    assert_eq!(
        status.message(),
        "the Keycloak access token is not a valid HTTP header value"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_interceptor_refreshes_through_block_in_place_on_a_multi_threaded_runtime() {
    let clock = FakeClock::at(1000.0);
    let transport =
        FakeTransport::new(vec![token("acc-1", json!(300)), token("acc-2", json!(300))]);
    let mut interceptor =
        KeycloakInterceptor::from_provider(Arc::new(build(&transport, &clock, None, 0.0)));
    clock.advance(280.0);
    let request = interceptor.call(Request::new(())).unwrap();
    assert_eq!(
        request
            .metadata()
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap(),
        "Bearer acc-2"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn the_interceptor_works_on_a_current_thread_runtime_too() {
    let transport = FakeTransport::new(vec![token("acc-1", json!(300))]);
    let mut interceptor = KeycloakInterceptor::from_provider(Arc::new(build(
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    )));
    assert!(interceptor.call(Request::new(())).is_ok());
}

// ---------------------------------------------------------------------------------------------
// The defaults and the small types

#[test]
fn the_default_clock_is_monotonic_and_the_default_jitter_is_a_fraction() {
    let first = monotonic_seconds();
    thread::sleep(Duration::from_millis(5));
    assert!(monotonic_seconds() > first);
    let fractions: Vec<f64> = (0..1000).map(|_| random_fraction()).collect();
    assert!(fractions
        .iter()
        .all(|fraction| (0.0..1.0).contains(fraction)));
    assert!(
        fractions.iter().any(|fraction| *fraction < 0.5)
            && fractions.iter().any(|fraction| *fraction >= 0.5)
    );
}

#[test]
fn the_default_options_jitter_a_real_failure() {
    let transport = FakeTransport::new(vec![token("acc-1", json!(300)), status(503, "{}")]);
    let clock = FakeClock::at(1000.0);
    let options = KeycloakProviderOptions {
        transport: transport.clone(),
        clock: clock.clock(),
        start_background_refresh: false,
        ..KeycloakProviderOptions::default()
    };
    let provider = KeycloakTokenProvider::with_options(&config(), options).unwrap();
    clock.advance(280.0);
    provider.authorization_metadata().unwrap();
    let delay = provider.next_refresh_attempt_at() - 1280.0;
    assert!(
        (RETRY_BASE_DELAY_S..=RETRY_BASE_DELAY_S).contains(&delay),
        "{delay}"
    );
}

#[test]
fn the_condvar_stop_signal_times_out_and_is_interrupted() {
    let signal = Arc::new(CondvarStopSignal::default());
    assert!(!signal.is_set());
    let started = Instant::now();
    assert!(!signal.wait(0.02));
    assert!(started.elapsed() >= Duration::from_millis(20));
    assert!(!signal.wait(-1.0), "a negative wait returns at once");

    let waiter = Arc::clone(&signal);
    let handle = thread::spawn(move || waiter.wait(f64::INFINITY));
    thread::sleep(Duration::from_millis(20));
    signal.set();
    assert!(
        handle.join().unwrap(),
        "the stop did not interrupt the wait"
    );
    assert!(signal.is_set() && signal.wait(100.0));
}

/// A generic error type: its short name drops the module path and the generic arguments.
#[derive(Debug)]
struct Wrapped<T>(T);

impl<T: fmt::Debug> fmt::Display for Wrapped<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "wrapped {:?}", self.0)
    }
}

impl<T: fmt::Debug> Error for Wrapped<T> {}

#[test]
fn transport_errors_carry_their_kind_type_and_cause() {
    let unreachable =
        TransportError::unreachable(Wrapped(std::collections::HashMap::<u8, u8>::new()));
    assert_eq!(unreachable.kind(), TransportErrorKind::Unreachable);
    assert_eq!(unreachable.type_name(), "Wrapped");
    assert_eq!(unreachable.to_string(), "wrapped {}");
    assert_eq!(source_of(&unreachable), "wrapped {}");
    let io = TransportError::from_io(io::Error::new(io::ErrorKind::ConnectionReset, "reset"));
    assert_eq!(
        (io.kind(), io.type_name()),
        (TransportErrorKind::Unreachable, "ConnectionReset")
    );
    assert!(format!("{io:?}").contains("ConnectionReset"));
}

#[test]
fn the_error_types_render_and_chain() {
    let config_error = KeycloakError::from(KeycloakConfigError::IncompleteKeycloakFields);
    assert_eq!(
        config_error.to_string(),
        "The Keycloak fields `keycloak_url`, `realm`, and `client_id` must be provided together in ClientConfig."
    );
    assert_eq!(source_of(&config_error), config_error.to_string());
    let transport = FakeTransport::new(vec![refused()]);
    let error = authentication_error(build_with(
        &config(),
        &transport,
        &FakeClock::at(0.0),
        None,
        0.0,
    ));
    assert_eq!(error.message(), error.to_string());
    assert!(error.source().is_none());
    assert!(format!("{error:?}").contains("KeycloakAuthenticationError"));
    let wrapped = KeycloakError::from(error);
    assert!(wrapped.to_string().contains("status 400"));
    assert_eq!(source_of(&wrapped), wrapped.to_string());
    assert!(format!("{wrapped:?}").starts_with("Authentication("));
}

#[test]
fn the_token_types_render_without_values() {
    let request = TokenRequest {
        url: "https://kc/token".into(),
        form: vec![("password".into(), PASSWORD.into())],
        timeout: Duration::from_secs(10),
        verify_ssl: true,
    };
    let rendered = format!("{request:?}");
    assert!(
        !rendered.contains(PASSWORD) && rendered.contains("\"password\""),
        "{rendered}"
    );
    assert_eq!(request.field("missing"), None);
    let response = TokenHttpResponse {
        status: 200,
        body: "{}".into(),
    };
    assert_eq!(response.clone(), response);
    assert!(format!("{response:?}").contains("200"));
    let timings = KeycloakTimings::default();
    assert_eq!(timings, timings.clone());
    assert!(format!("{timings:?}").contains("expiry_leeway_s: 30.0"));
}
