//! ONDEWO T2S (Text-to-Speech) gRPC client.
//!
//! Everything under [`api`] is GENERATED from the protos of the `ondewo-t2s-api` submodule by the
//! `ondewo-rust-proto-compiler` image - run `make generate_ondewo_protos` to regenerate it and
//! never edit it by hand. The proto packages appear as nested modules, so a message declared in
//! `package ondewo.t2s;` is reachable as `ondewo::t2s::<Message>` and each gRPC service
//! as `ondewo::t2s::<service>_client::<Service>Client`.
//!
//! This barrel is HAND-WRITTEN and is left untouched by the generator (`make-lib-entry-point.sh`
//! only writes a default one when `src/lib.rs` is absent). Declare every hand-written module you
//! add beside `api` here - a module that is compiled into the crate but not declared in the barrel
//! is unreachable from outside it.
//!
//! The rustdoc allows are load-bearing: the ONDEWO protos carry raw URLs and raw HTML anchors in
//! their comments and prost copies those verbatim into doc comments, which breaks `cargo doc` and
//! any consumer building with `RUSTDOCFLAGS=-D warnings`.
#![allow(clippy::all)]
#![allow(rustdoc::bare_urls)]
#![allow(rustdoc::broken_intra_doc_links)]
#![allow(rustdoc::invalid_html_tags)]
// The protos also document RPCs with INDENTED blocks (proto snippets, HTTP mappings). rustdoc
// reads an indented block as a rust code block, so it warns here and would try to compile them as
// doctests - which is why `Cargo.toml` sets `doctest = false` for this crate.
#![allow(rustdoc::invalid_rust_codeblocks)]

pub mod api;
pub mod auth;
pub use api::*;
