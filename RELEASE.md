# Release History

*****************

## Release ONDEWO T2S Rust Client 6.7.1

### New Features

* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) Offline-token start-up, as the python SDK has it: a handed-off Keycloak OFFLINE refresh token (`refresh_token`) replaces the password. The provider then starts with ONE `grant_type=refresh_token` exchange and never sends a password grant, not even when the token is refused, because Keycloak's brute-force detection counts failed password logins and N processes sharing one technical user would lock it. A Keycloak config needs `password` or `refresh_token`; with both, the refresh token wins.
* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) New module `keycloak`: `KeycloakConfig::new(url, realm, client_id, username, password).with_refresh_token(...)` and the tonic `KeycloakInterceptor`, which acquires the first token, refreshes it from a background thread 30 s before it expires and attaches `authorization: Bearer <token>` to every call; an expired token that cannot be refreshed fails the call with `Status::unauthenticated` carrying a `KeycloakAuthenticationError`. `KeycloakConfig`'s `Debug` redacts the password and the refresh token, and `verify_ssl(false)` skips the certificate check of the token endpoint only.

### Improvements

* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) A failed token refresh never ends the refreshing: every failure (an unreachable or timed-out token endpoint, a non-2xx answer, a body that is not a JSON object, a missing `access_token`, a non-numeric `expires_in`) is retried with a full-jitter backoff from 5 s doubling up to 300 s, which the background refresh and the per-call refresh share, so a Keycloak outage costs one token request per backoff step instead of one per call. A success resets the backoff.
* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) A token that is still valid keeps being served while its refresh fails; only an EXPIRED token that cannot be refreshed fails, with a Keycloak authentication error that chains the underlying cause. A malformed token response changes no provider state, and an error about a response without an `access_token` names its field names only, never their values.
* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) One provider is shared per credential set: the registry key is a SHA-256 over the Keycloak connection and credential fields (the password and the refresh token enter it only as hashes), so two configs of different tenants never share a provider and all clients of one identity log in once.
* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) The token request times out after 10 s, quoted Keycloak error bodies are cut at 500 characters, each failed attempt logs one WARNING (attempt number, retry delay, remaining token validity) and the first success afterwards one INFO; no error message and no log line carries a password or a token.
* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) The behaviour is pinned by the same test catalogue as the python SDK (`test_keycloak_refresh_resilience.py`, `test_client_config_refresh_token.py`), plus edge cases: every failure kind, the backoff ladder and its bounds, concurrent callers during an outage (one token request), malformed responses, credential-free logs and the credential-keyed registry.

*****************

## Release ONDEWO T2S Rust Client 6.7.0

### Improvements

* Tracking API Version [6.7.0](https://github.com/ondewo/ondewo-t2s-api/releases/tag/6.7.0) ( [Documentation](https://ondewo.github.io/ondewo-t2s-api/) )
* [[OND236-120]](https://ondewo.atlassian.net/browse/OND236-120) Generated with ondewo-proto-compiler 5.17.0 (protoc 35.1). New runtime floors: prost / prost-types 0.14.6, tonic / tonic-prost 0.14.9.

*****************

## Release ONDEWO T2S Rust Client 6.6.1

### New Features

* TLS, mutual TLS and plaintext channels: the new hand-written module `channel` builds the tonic
  `Endpoint` / `Channel` from a `ClientConfig` (`endpoint()`, `connect()`, `connect_lazy()`).
  `grpc_cert`, `grpc_client_cert` and `grpc_client_key` take PEM **content**, not file paths.
  Without `grpc_cert` the platform trust store is used (tonic `tls-native-roots`).
* Refused with a `ChannelError` before tonic sees the config: half a client identity
  (`IncompleteClientIdentity`), a client identity on a plaintext channel
  (`ClientIdentityWithoutTls`) and a certificate field that holds no PEM certificate
  (`NotAPemCertificate`).
* A plaintext channel logs a `tracing` warning naming `host:port`. `Debug` redacts the client key
  and renders no PEM, and error messages name only the field and `host:port`. A bare IPv6 host is
  bracketed, and `tls_domain_name` overrides the name the server certificate is checked against.
* README section "TLS, mutual TLS and certificates": the modes, a test PKI built with openssl, TLS
  security notes and the channel defaults. Of the python SDK channel options, the message size
  limit is applied per client (`MAX_MESSAGE_LENGTH`); keepalive pings are not set (TCP keepalive
  detects a dropped idle connection instead), and tonic has no reconnect backoff or retry policy.

### Tests

* `tests/tls_channel.rs` runs real handshakes against an in-process tonic server with an
  openssl-generated test PKI: plain TLS, mutual TLS, missing and foreign client identities, wrong
  CA, platform roots, CRLF PEMs, SAN override and IPv6 loopback.
* `tests/release_notes.rs` pins the RELEASE.md slice the GitHub release body is cut from.

### Build

* `ondewo-proto-compiler` is pinned to 5.15.4 (5.15.2 before this release), which pre-warms tonic
  `tls-native-roots` for the offline stub generation that `src/channel.rs` needs.
  `make generate_ondewo_protos` now also stages `README.md`, which the image's `cargo package`
  requires. The regenerated stubs under `src/api` are byte-identical.
* Tracking API Version [6.6.0](https://github.com/ondewo/ondewo-t2s-api/releases/tag/6.6.0) (unchanged)

*****************

## Release ONDEWO T2S Rust Client 6.6.0

### New Features

* Initial release of the ONDEWO T2S (Text-to-Speech) gRPC client for rust. The crate
  ships the complete client surface of the [ondewo-t2s-api](https://github.com/ondewo/ondewo-t2s-api)
  protocol buffer definitions: [prost](https://crates.io/crates/prost) message types and
  [tonic](https://crates.io/crates/tonic) service clients for every RPC, generated by the
  `ondewo-rust-proto-compiler` image of
  [ondewo-proto-compiler](https://github.com/ondewo/ondewo-proto-compiler) 5.15.1.
* The generated module tree is confined to `src/api` and is re-exported from the crate root, so a
  message declared in `package ondewo.t2s;` is reachable as
  `ondewo_t2s_client::ondewo::t2s::<Message>` and each service as
  `ondewo::t2s::<service>_client::<Service>Client`. Hand-written modules live beside it
  under `src/` and are declared in the hand-written crate barrel `src/lib.rs`, which the
  generator leaves untouched.
* `make build` runs the whole pipeline - submodule checkout, compiler image build, stub
  generation and `cargo build` - and `make check_build` asserts that a generated stub exists for
  every proto package before a release is cut.
* The generated stubs are COMMITTED under `src/api`, so the crate compiles straight from a
  checkout - no docker, no compiler image and no submodule are needed to build or test it.
  Regenerating them stays a `make build` concern.
* A hand-written `auth` module (`ondewo_t2s_client::auth::BearerTokenInterceptor`) attaches the
  Keycloak `authorization: Bearer <token>` and `cai-token` metadata to every outgoing request.
  Its `Debug` output redacts both credentials. `examples/authenticated_client.rs` shows the flow.
* An integration suite under `tests/` exercises the generated code: wire-level encode/parse
  round trips over the prost messages, pinned enum discriminants, and the generated
  `Text2SpeechServer` served over a loopback socket and driven by the generated `Text2SpeechClient`, so
  every declared RPC is really routed by its `/ondewo.t2s.Text2Speech/<Method>` path. `make coverage`
  gates the hand-written sources at 100% line coverage, as does CI.

*****************
