# How to become a contributor and submit your own code

## Contributor License Agreements

We'd love to accept your sample apps and patches! Before we can take them, we
have to jump a couple of legal hurdles.

Please fill out either the individual or corporate Contributor License Agreement (CLA).

- If you are an individual writing original source code and you're sure you own the intellectual
  property, then you'll need to sign an individual CLA.
- If you work for a company that wants to allow you to contribute your work, then you'll need to
  sign a corporate CLA.

Contact <office@ondewo.com> to receive the appropriate CLA and instructions for how to sign and
return it. Once we receive it, we'll be able to accept your pull requests.

## Contributing A Patch

1. Submit an issue describing your proposed change to the repo in question.
1. The repo owner will respond to your issue promptly.
1. If your proposed change is accepted, and you haven't already done so, sign a
   Contributor License Agreement (see details above).
1. Fork the desired repo, develop and test your code changes.
1. Ensure that your code adheres to the existing style in the sample to which
   you are contributing. Refer to the
   [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/) for the
   recommended coding standards for this organization.
1. Ensure that your code has an appropriate set of unit tests which all pass.
1. Submit a pull request.

## What belongs in this repository, and what does not

`src/api` is **generated**. It is rewritten in full by the
`ondewo-rust-proto-compiler` image on every `make generate_ondewo_protos`, so a change made
there is discarded by the next generation run and must never be committed on its own.

- A change to the **API surface** (a message, a field, an RPC) belongs in
  [ondewo-t2s-api](https://github.com/ondewo/ondewo-t2s-api). Once it is released there, bump
  `ONDEWO_T2S_API_GIT_BRANCH` in the `Makefile` and run `make build`.
- A change to **how** the stubs are generated belongs in
  [ondewo-proto-compiler](https://github.com/ondewo/ondewo-proto-compiler), in its `rust/`
  target. Try it out here by building the image tag from your compiler working tree and running
  `make generate_ondewo_protos`; only bump `ONDEWO_PROTO_COMPILER_GIT_BRANCH` once the compiler
  change is released.
- A change to the **hand-written** surface belongs here, under `src/` beside `src/api`, and its
  module has to be declared in the crate barrel `src/lib.rs` or consumers cannot reach it.

## Before you open a pull request

Run the same gates CI runs:

```bash
make test
make cargo_fmt_check
make cargo_doc
make precommit_hooks_run_all_files
```

A change that regenerates the stubs should also pass `make check_build`, which asserts that a
generated stub exists for every proto package of the API submodule.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat: ...`,
`fix(scope): ...`, `docs: ...`) - the `conventional-pre-commit` hook enforces this on the commit
message. The `giticket` hook prepends the JIRA ticket taken from the branch name, so do **not**
write the ticket ID into the subject yourself. Branch names look like
`feature/OND211-1234-short-description`.
