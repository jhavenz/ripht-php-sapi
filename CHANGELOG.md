# Changelog

## Unreleased

- ci: Add GitHub Action to guard against reintroducing legacy names in code.
- lint: Fix Clippy warnings and enforce `-D warnings` in CI.
- api: Add `#[non_exhaustive]` to `ExecutionMessage` and `#[must_use]` on `RiphtSapi::execute*` methods.
- ergonomics: Add `From<ExecutionResult>` → `http::Response<Vec<u8>>` behind `http` feature for easy conversion.
- api: Add `prelude` re-exports for common types (`RiphtSapi`, `Executor`, `ExecutionResult`, `ExecutionMessage`, `SapiError`, `WebRequest`, `CliRequest`).
