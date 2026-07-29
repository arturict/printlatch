# Local and self-hosted CI

These commands reproduce the normal gates without consuming GitHub-hosted
runner minutes. Run them from a clean checkout of the commit being evaluated.

## TypeScript and static assets

Use Node.js 24 and pnpm 11.7.0:

```powershell
pnpm install --frozen-lockfile
pnpm format:check
pnpm lint
pnpm typecheck
pnpm test
pnpm build
```

## Rust on a pinned Linux container

This path isolates build output from the checkout and uses Rust 1.94.1:

```powershell
$repoPath = (Get-Location).Path
docker run --rm `
  --mount "type=bind,source=$repoPath,target=/work" `
  -w /work `
  -e CARGO_TARGET_DIR=/tmp/printlatch-target `
  rust:1.94.1 `
  bash -c 'cargo fmt --all -- --check &&
    cargo test --locked --all-targets &&
    cargo clippy --locked --all-targets --all-features -- -D warnings &&
    cargo build --locked --release'
```

## Native Windows build and release smoke

The supported executable requires Rust 1.94.1, the MSVC x64 linker, and a
Windows SDK. Run in an x64 Visual Studio Developer PowerShell:

```powershell
cargo fmt --all -- --check
cargo test --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release
.\scripts\smoke-release.ps1 -ArchiveRoot .\artifacts\printlatch-v0.1.1-windows-x64
```

The smoke uses isolated temporary program and data directories. It installs the
per-user task, checks health, pairing, authenticated preview, capture hash,
invalid MIME rejection, and both uninstall modes, then removes its temporary
task and files.

## Browser operator flow

Start the release candidate with an isolated data directory, run
`printlatch dashboard`, and verify:

1. one-time pairing and target detection
2. authenticated test-PDF preview
3. explicit confirmation before capture
4. queued, printing, and terminal status updates
5. capture SHA-256 equality with the source PDF
6. retry after a transient health or status failure
7. desktop and 375 px layouts without horizontal overflow
8. no browser console errors

Do not select a physical printer during this gate.

## Cost-control state

The repository workflows are `workflow_dispatch` only through 2026-07-31.
Local or authorized self-hosted workers own build, lint, tests, browser E2E,
packaging, and smoke evidence during this window. Do not enable a paid runner
or restore automatic triggers without an explicit repository decision.
