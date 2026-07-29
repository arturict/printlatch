# Dependency audit

Last reviewed: 2026-07-29

`cargo audit` reports no known vulnerabilities in the locked Rust dependency
graph. It reports one unmaintained-crate warning:

- `RUSTSEC-2024-0370` for `proc-macro-error 1.0.4`
- the path is `winprint → fmt-derive → fmt-derive-proc → proc-macro-error`
- this path is used while compiling the Windows printer adapter

This warning is not a published vulnerability and `proc-macro-error` is not a
runtime parser or network component. It is still a supply-chain maintenance
risk. V1 keeps `winprint` narrowly isolated behind `src/printers.rs`, pins the
full dependency graph in `Cargo.lock`, and builds releases in GitHub Actions.
Replacing the wrapper with a smaller maintained Windows API adapter is tracked
for a future release.

`pnpm audit --audit-level high` reports no known vulnerabilities. Gitleaks
reports no secrets in the release candidate.

