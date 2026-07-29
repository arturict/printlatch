# Test strategy

## Rust gates

- `cargo fmt --all -- --check`
- `cargo check --all-targets`
- `cargo test --all-targets`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo audit`

Linux/container tests cover platform-neutral authorization, API, PDF guard,
storage, and queue behavior. The Windows gate compiles and runs the Windows
backend on a Windows 11 x64 worker.

## TypeScript gates

- `pnpm format:check`
- `pnpm lint`
- `pnpm typecheck`
- `pnpm test`
- `pnpm build`

The root Vitest suite covers operator status labels, conservative printer-error
classification, interrupted-submission warnings, and retry eligibility.

## Abuse cases

The automated suite explicitly covers:

- unauthenticated requests
- CSRF-style browser use of a local-process token
- exact browser origin enforcement
- one-time pairing replay
- wrong-origin pairing without code consumption
- spoofed local listener and agent-session-bound dashboard grants
- dashboard re-pair token rotation with retained job history
- dedicated dashboard identity isolated from application pairings with matching names and origins
- serialized stable-dashboard rotation under a competing SQLite writer
- CORS and private-network headers restricted to authenticated API routes, never the operator shell
- DNS rebinding host
- WebSocket upgrade
- SSRF-shaped unknown field
- traversal-shaped filename
- ZIP and wrong MIME
- active PDF content
- oversized job
- printer-name command characters
- client-to-client job isolation
- authenticated access to the bundled test PDF
- independent polling selection for multiple active jobs
- bounded polling recovery after transient status-request failures
- active jobs retained outside the 100-job recent-history window
- fragment pairing resumed after a transient agent-health failure
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

## Operator browser smoke

The embedded dashboard is exercised against a real temporary agent and SQLite
queue:

1. create and consume a loopback dashboard grant
2. verify capture target detection and the no-Windows-printer empty state
3. validate and retrieve the built-in PDF preview
4. confirm a separate PDF capture job
5. observe queued, printing, and terminal updates through the authenticated API
6. compare the capture SHA-256 with the source PDF
7. check desktop and 375 px responsive layouts for horizontal overflow
8. check browser console errors and accessible live status messages

No physical printer is invoked by this smoke.

## Local and self-hosted execution

GitHub workflows are manual-only during the July 2026 cost-control window.
Use the pinned local and self-hosted commands in
[local CI](local-ci.md) before pushing. Re-enable automatic GitHub triggers only
after the cost window and an explicit repository decision.
