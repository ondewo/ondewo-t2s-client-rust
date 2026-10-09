<div align="center">
  <table>
    <tr>
      <td>
        <a href="https://ondewo.com">
            <img width="400px" src="https://raw.githubusercontent.com/ondewo/ondewo-logos/master/ondewo_we_automate_your_phone_calls.png"/>
        </a>
      </td>
    </tr>
    <tr>
        <td align="center">
          <a href="https://www.linkedin.com/company/ondewo"><img width="40px" src="https://cdn-icons-png.flaticon.com/512/3536/3536505.png"></a>
          <a href="https://www.facebook.com/ondewo"><img width="40px" src="https://cdn-icons-png.flaticon.com/512/733/733547.png"></a>
          <a href="https://twitter.com/ondewo"><img width="40px" src="https://cdn-icons-png.flaticon.com/512/733/733579.png"></a>
          <a href="https://www.instagram.com/ondewo.ai/"><img width="40px" src="https://cdn-icons-png.flaticon.com/512/174/174855.png"></a>
        </td>
    </tr>
  </table>
  <h1>
  ONDEWO T2S Client Rust Library
  </h1>
</div>

This library gives a rust application typed, async access to the ONDEWO T2S
(Text-to-Speech) gRPC API.

It is generated code plus a thin hand-written surface around it. The interface itself is defined
by the protocol buffer files of the [ONDEWO T2S API](https://github.com/ondewo/ondewo-t2s-api),
which can be compiled into 10+ high-level languages; the
[ONDEWO PROTO COMPILER](https://github.com/ondewo/ondewo-proto-compiler) turns them into the
[prost](https://crates.io/crates/prost) message types and [tonic](https://crates.io/crates/tonic)
service clients that this crate publishes.

## Rust Installation

The library is published to [crates.io](https://crates.io/crates/ondewo-t2s-client) as
**`ondewo-t2s-client`**, with the API documentation on
[docs.rs](https://docs.rs/ondewo-t2s-client). Nothing else is needed to consume it - the stubs are
generated before publishing and ship inside the crate, so installing it needs neither `docker`,
nor `protoc`, nor the proto definitions.

```bash
cargo add ondewo-t2s-client
```

or declare it in your `Cargo.toml`:

```toml
[dependencies]
ondewo-t2s-client = "6.6"
tonic = "0.14"
tokio = { version = "1", features = ["full"] }
```

The generated clients are `async`, so an async runtime is needed to drive them -
[tokio](https://crates.io/crates/tokio) is the one tonic is built against.

A few things worth knowing before pinning a version:

* The crate needs **rust 1.88 or newer** (`rust-version` in `Cargo.toml`).
* Its version tracks the **ONDEWO T2S API** it was generated from in major and minor - a crate
  `X.Y.*` speaks the API `X.Y.*` - so pin the minor of the server you talk to.
* `tonic` and `prost` types appear in the public API. Depend on the **same `tonic` 0.14 and
  `prost` 0.14** the crate does, or the two sets of types will not line up.
* The crate ships no default features and pulls in `tonic`'s `tls-ring`, `tls-native-roots` and
  `gzip`, so a TLS endpoint (`https://`) works out of the box and trusts the platform's
  certificate store unless you configure a CA - see
  [TLS, mutual TLS and certificates](#tls-mutual-tls-and-certificates).

To work on the library itself, clone it with its two submodules and set up the toolchain:

```bash
git clone --recurse-submodules git@github.com:ondewo/ondewo-t2s-client-rust.git
cd ondewo-t2s-client-rust
make setup_developer_environment_locally
```

## Usage

Every gRPC service in the API becomes one client type, and every message becomes one struct. The
module path mirrors the proto package: `package ondewo.t2s;` is reachable as
`ondewo_t2s_client::ondewo::t2s`. Run `cargo doc --open` to browse the exact service
and message names of the API version this crate was generated from.

```rust
use ondewo_t2s_client::ondewo::t2s::*;
use tonic::transport::Channel;
use tonic::Request;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. A channel to the T2S server. `Channel` handles TLS, reconnects and
    //    connection pooling; the generated clients accept any tonic service.
    let channel = Channel::from_static("https://grpc-t2s.ondewo.com:443")
        .connect()
        .await?;

    // 2. One client per gRPC service - replace `Example` with a service of the API, e.g. the
    //    service `Foo` generates `foo_client::FooClient`.
    let mut client = example_client::ExampleClient::new(channel);

    // 3. ONDEWO servers authenticate with a bearer token in the request metadata.
    let token = std::env::var("ONDEWO_T2S_TOKEN")?;
    let mut request = Request::new(ExampleRequest::default());
    request
        .metadata_mut()
        .insert("authorization", format!("Bearer {token}").parse()?);

    let response = client.example_rpc(request).await?;
    println!("{:?}", response.into_inner());

    Ok(())
}
```

## TLS, mutual TLS and certificates

gRPC encrypts with **TLS**. The hand-written module `channel` builds the tonic `Channel` from a
`ClientConfig`, the same contract every ONDEWO SDK follows:

| Mode | `use_secure_channel` | Config fields |
| --- | --- | --- |
| Plaintext (not for production) | `false` | none |
| TLS, platform trust store | `true` (the default) | none |
| TLS, custom CA | `true` | `grpc_cert` = PEM of the CA that signed the server certificate |
| Mutual TLS | `true` | `grpc_cert` (or the platform store) plus `grpc_client_cert` and `grpc_client_key` |

Rules the code enforces, all before tonic sees the config (`ChannelError`):

* The three fields hold **PEM content**, **not file paths**. Read the files yourself. A certificate
  field that holds no PEM certificate (typically a path) is refused with `NotAPemCertificate`.
* `grpc_client_cert` and `grpc_client_key` go together: setting only one is refused with
  `IncompleteClientIdentity`. Both empty means plain server-authenticated TLS.
* `use_secure_channel = false` with a client certificate is refused with
  `ClientIdentityWithoutTls` instead of silently dropping the identity. A plaintext channel logs a
  `tracing` warning naming `host:port`; the crate never installs a subscriber or touches your
  logging setup.
* No error message and no `Debug` rendering contains a PEM or a key: certificates are rendered as
  their length, a non-empty `grpc_client_key` as `***REDACTED***`, an empty one as `""`.
* The server certificate must carry the host you connect to in its subject alternative names
  (SAN). When you connect by IP and the certificate has no IP SAN, set `tls_domain_name` to a name
  in the SAN (the counterpart of gRPC's `grpc.ssl_target_name_override`). A bare IPv6 literal host
  (`::1`) is bracketed for you (`[::1]:50051`).

```rust
use std::fs;

use ondewo_t2s_client::channel::{ClientConfig, MAX_MESSAGE_LENGTH};
use ondewo_t2s_client::ondewo::t2s::*;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ClientConfig::new("10.0.0.5", 50051)
        .with_grpc_cert(fs::read_to_string("certs/ca.pem")?)
        // leave this out for server-authenticated TLS
        .with_client_identity(
            fs::read_to_string("certs/client.pem")?,
            fs::read_to_string("certs/client.key")?,
        )
        // only when connecting by IP to a certificate without that IP in its SAN
        .with_tls_domain_name("t2s.example.internal");

    let channel = config.connect().await?; // or config.connect_lazy()? - connects on the first call
    // replace `Example` with a service of the API, as in Usage above
    let mut client = example_client::ExampleClient::new(channel)
        .max_decoding_message_size(MAX_MESSAGE_LENGTH)
        .max_encoding_message_size(MAX_MESSAGE_LENGTH);
    let response = client.example_rpc(ExampleRequest::default()).await?;
    println!("{:?}", response.into_inner());
    Ok(())
}
```

One `Channel` serves every generated client: clone it (cheap) for each service, so all of them
share one connection and one TLS handshake. `config.endpoint()?` returns the tonic `Endpoint`
without connecting, for further tuning.

### Channel defaults

The python SDKs set gRPC core channel options; tonic exposes only some of them, so this crate
sets what it can and leaves the rest to tonic's behaviour:

| python option | here |
| --- | --- |
| `max_send/receive_message_length = 2**31-1` | per client, not per channel: pass `MAX_MESSAGE_LENGTH` to `max_decoding_message_size` / `max_encoding_message_size` (tonic's decoding default is 4 MiB) |
| `keepalive_time_ms = 30000`, `keepalive_timeout_ms` / `http2.ping_timeout_ms = 20000`, `http2.max_pings_without_data = 2` | **not set**: hyper has no cap on pings without data, and a default grpc-core server answers a client that keeps pinging a silent stream with GOAWAY `too_many_pings`. Instead, TCP keepalive (`TCP_KEEPALIVE` 30 s, `TCP_KEEPALIVE_INTERVAL` 10 s, `TCP_KEEPALIVE_RETRIES` 2) detects a silently dropped idle connection |
| `max_reconnect_backoff_ms = 5000` | not applicable: tonic reconnects on the next call, without an exponential backoff |
| retry policy (idempotent methods only) | not applicable: tonic does not retry calls |

### A test PKI with openssl

A CA, a server certificate with SANs, and a client certificate with the `clientAuth` extended key
usage. For tests only: the keys are unencrypted.

```bash
openssl req -x509 -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes -days 365 \
  -subj "/CN=Test CA" -keyout ca.key -out ca.pem

printf 'subjectAltName=DNS:localhost,IP:127.0.0.1\nextendedKeyUsage=serverAuth\n' > server.ext
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
  -subj "/CN=localhost" -keyout server.key -out server.csr
openssl x509 -req -in server.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 365 \
  -extfile server.ext -out server.pem

printf 'extendedKeyUsage=clientAuth\n' > client.ext
openssl req -newkey ec -pkeyopt ec_paramgen_curve:prime256v1 -nodes \
  -subj "/CN=my-client" -keyout client.key -out client.csr
openssl x509 -req -in client.csr -CA ca.pem -CAkey ca.key -CAcreateserial -days 365 \
  -extfile client.ext -out client.pem

chmod 600 *.key
openssl verify -CAfile ca.pem server.pem client.pem
```

The client then uses `ca.pem` / `client.pem` / `client.key`; a server that requires client
certificates uses `server.pem` / `server.key` and trusts `ca.pem` for its clients.
`tests/tls_channel.rs` builds the same PKI at test time and runs real handshakes against an
in-process tonic server.

### TLS security notes

* Keep the private key out of source control and out of images; load it from a file with mode
  `0600` or from a secret store at startup.
* `ClientConfig` has no serialization. If you persist one yourself, `grpc_client_key` is in it in
  clear text: treat that file as a secret.
* `Debug` redacts the key and renders no PEM, but still shows host, port and flags - log configs
  only when that is acceptable.

### TLS troubleshooting

A failed handshake surfaces either from `connect()` as `ChannelError::Connect`, or, with
`connect_lazy()`, as the `Status` of the first call:

* **`invalid peer certificate: UnknownIssuer`** (`UNAVAILABLE`): `grpc_cert` is not the CA that
  issued the server certificate, the server does not send its intermediate certificates, or - with
  no `grpc_cert` - the platform store does not trust the server's CA.
* **`certificate not valid for name "<host>"`** (`UNAVAILABLE`): the host you connect to is not in
  the server certificate's SAN. Connect by a name in the SAN, add the SAN, or set
  `tls_domain_name`.
* **`received fatal alert: CertificateRequired`** (`UNKNOWN`, "transport error") against a server
  that requires client certificates: no client certificate was presented. Set `grpc_client_cert`
  and `grpc_client_key`.
* **`received fatal alert: UnknownCA`** (`UNKNOWN`): the client certificate was issued by a CA
  the server does not trust for its clients.
* **`ChannelError::NotAPemCertificate`** / **`Error parsing TLS private key`**
  (`ChannelError::InvalidEndpoint`): a field holds something that is not PEM, typically a file
  path. Pass `fs::read_to_string(...)?` instead.

## Repository Structure

```text
.
├── ondewo-t2s-api                <----- submodule: the proto definitions to compile
├── ondewo-proto-compiler     <----- submodule: builds the ondewo-rust-proto-compiler image
├── src
│   ├── api                   <----- GENERATED - one <proto.package>.rs per package + mod.rs
│   │   ├── mod.rs
│   │   ├── ondewo.t2s.rs
│   │   └── ondewo.t2s.tonic.rs
│   ├── auth.rs               <----- hand-written bearer-token interceptor
│   ├── channel.rs            <----- hand-written channel builder: TLS, mutual TLS, plaintext
│   └── lib.rs                <----- hand-written crate barrel
├── examples
│   └── authenticated_client.rs   <----- the crate's usage snippet, compiled by `cargo test`
├── tests                     <----- integration tests over the generated stubs
│   ├── auth_interceptor.rs
│   ├── generated_grpc.rs
│   ├── generated_messages.rs
│   ├── release_notes.rs      <----- pins the RELEASE.md slice the GitHub release body is cut from
│   └── tls_channel.rs        <----- real TLS / mutual TLS handshakes with a test-time PKI
├── Cargo.toml                <----- crate manifest AND the generator's crate template
├── Cargo.lock
├── Makefile
└── README.md
```

Only `src/api` is generated. Everything beside it under `src/` is hand-written and is declared in
`src/lib.rs`; a module that is compiled into the crate but missing from that barrel is unreachable
for consumers of the crate.

## Regenerating the Stubs

Regeneration needs `docker`, `git` and `make` - the rust toolchain, `protoc` and the protoc
plugins all live inside the compiler image, and generation itself needs no network once that image
is built.

```bash
make build
```

is the whole pipeline:

1. `update_submodules` - `git submodule update --init --recursive`
1. `checkout_defined_submodule_versions` - checks out the pins at the top of the `Makefile`
   (`ONDEWO_T2S_API_GIT_BRANCH` and `ONDEWO_PROTO_COMPILER_GIT_BRANCH`)
1. `build_compiler` - builds `ondewo-rust-proto-compiler:latest` from the submodule
1. `update_cargo_version` - writes `ONDEWO_T2S_VERSION` into `Cargo.toml`
1. `generate_ondewo_protos` - runs the image over `ondewo-t2s-api/ondewo` and writes `src/api`,
   `Cargo.toml`, `Cargo.lock` and `crate-dist/` back into this repository
1. `check_build` - asserts that a generated stub exists for every proto package
1. `cargo_build` - compiles the crate

The image tag is the only contract between this repository and the compiler, so a compiler change
can be tried out without touching the submodule pin: build the tag from a compiler working tree
(`docker build -t ondewo-rust-proto-compiler:latest rust` in that repository) and run
`make generate_ondewo_protos` here.

`make help` lists every documented target.

## Testing and Linting

```bash
make test              # cargo test --all-targets
make coverage          # hand-written line coverage, gated at 100%
make cargo_fmt_check   # rustfmt over the HAND-WRITTEN sources only
make cargo_doc         # cargo doc --no-deps
make precommit_hooks_run_all_files
```

The suite under `tests/` exercises the **generated** stubs the way a broken generator would be
noticed: messages are serialized and re-parsed field by field, enum discriminants are pinned, and
the generated `Text2SpeechServer` is served over a loopback socket and driven by the generated
`Text2SpeechClient`, so every declared RPC really is encoded, routed by its
`/ondewo.t2s.Text2Speech/<Method>` path, answered and decoded again. No ONDEWO server is involved.

`tests/tls_channel.rs` runs real TLS and mutual TLS handshakes against an in-process tonic
server, with a throw-away PKI that the `openssl` CLI generates at test time - it has to be on
`PATH` (it is on GitHub's ubuntu runners). `tests/release_notes.rs` pins the `RELEASE.md` slice
that the GitHub release body is cut from.

`make coverage` measures the **hand-written** sources only - `src/api`, `tests/` and `examples/`
are excluded, because generated code is machine output rather than authored logic - and fails
below 100%. The same gate runs in CI. The generated stubs are deliberately not held to a coverage
number; they are covered by the behavioural tests above.

`src/api` is outside the rustfmt gate on purpose: it is written by the generator on every run, so
a formatter that rewrote it would only produce a diff that the next generation discards. Doctests
are off crate-wide (`doctest = false`): the protos document their RPCs with indented proto and
HTTP snippets that prost copies into doc comments and rustdoc then tries to compile as rust. The
hand-written usage snippet therefore lives in `examples/`, where `cargo test` still compiles it.

## Release

Releases are cut from the `Makefile`. Bump `ONDEWO_T2S_VERSION`, add the matching
entry to `RELEASE.md`, then:

```bash
make ondewo_release
```

which checks that the release branch and tag do not exist yet (`spc`), pulls the credentials from
the `ondewo-devops-accounts` repository and runs `make release` with them: build, commit, release
branch, release tag and the GitHub release.

### Publishing to crates.io

The upload is **not** done by `make release`. Pushing the release tag starts
[`.github/workflows/release.yml`](.github/workflows/release.yml), which asserts that the tag
matches the version in `Cargo.toml` and that the generated stubs are committed, builds and tests
the tagged commit, re-runs the packaging dry run and only then runs `make publish_crate` with the
`CARGO_REGISTRY_TOKEN` repository secret. Keeping the single upload there is deliberate: a second
publisher inside `make release` would race the workflow, and the loser would die on *crate version
already uploaded*.

The workflow refuses to start work when the secret is unset, rather than reaching the upload with
an empty token, so a missing credential is a red run that names it and not a green run that
published nothing. Two operator tasks are therefore one-time prerequisites:

* an API token from <https://crates.io/settings/tokens> with the **publish-new** and
  **publish-update** scopes, stored as the repository secret `CARGO_REGISTRY_TOKEN` under
  *Settings → Secrets and variables → Actions*,
* the same token in `account_cargo.env` of the `ondewo-devops-accounts` repository, which is what
  the manual fallback reads.

That fallback is:

```bash
make ondewo_publish_crate   # clones devops-accounts, runs publish_crate with the token from it
```

Everything about the release except the upload itself is exercised without any credential, both
locally and on every CI push:

```bash
make check_crate_metadata     # the manifest fields crates.io requires
make publish_crate_dry_run    # + the file list, a full package/verify build, the 10 MiB limit
```

## Support

Reach out to the ONDEWO team at [office@ondewo.com](mailto:office@ondewo.com), or open an issue in
this repository. Contributions are welcome - see [CONTRIBUTING.md](CONTRIBUTING.md).

## License

Apache License 2.0 — see [LICENSE](LICENSE).
