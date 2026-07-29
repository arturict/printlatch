# Test strategy

## Rust gates

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo test --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo audit`

Linux/container tests cover platform-neutral authorization, API, PDF guard,
storage, and queue behavior. Windows CI also compiles and runs the Windows
backend.

## TypeScript gates

- `pnpm format:check`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm test`
- `pnpm build`

## Abuse cases

The automated suite explicitly covers:

- unauthenticated requests
- CSRF-style browser use of a local-process token
- exact browser origin enforcement
- one-time pairing replay
- wrong-origin pairing without code consumption
- DNS rebinding host
- WebSocket upgrade
- SSRF-shaped unknown field
- traversal-shaped filename
- ZIP and wrong MIME
- active PDF content
- oversized job
- printer-name command characters
- client-to-client job isolation
- atomic queue claim and concurrent cancel race
- restart during print submission
- explicit retry cap
- SDK error paths that do not include the token

## Release smoke

The release archive is installed into a clean temporary path on Windows. The
smoke sequence checks:

1. install and health
2. local token creation
3. printer enumeration
4. preview job and authenticated document download
5. PDF capture print and output hash
6. invalid MIME failure
7. queued cancel behavior
8. agent restart recovery
9. uninstall while preserving data
10. purge uninstall in the isolated test directory
