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

Add the crate to your project:

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
│   └── lib.rs                <----- hand-written crate barrel
├── examples
│   └── authenticated_client.rs   <----- the crate's usage snippet, compiled by `cargo test`
├── tests                     <----- integration tests over the generated stubs
│   ├── auth_interceptor.rs
│   ├── generated_grpc.rs
│   └── generated_messages.rs
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
branch, release tag, GitHub release and the crates.io publish.

## Support

Reach out to the ONDEWO team at [office@ondewo.com](mailto:office@ondewo.com), or open an issue in
this repository. Contributions are welcome - see [CONTRIBUTING.md](CONTRIBUTING.md).
