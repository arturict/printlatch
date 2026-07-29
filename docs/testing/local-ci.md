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
pnpm audit --audit-level high
```

## Secret scanning

Run Gitleaks against the full local Git history:

```powershell
$repoPath = (Get-Location).Path
docker run --rm `
  --mount "type=bind,source=$repoPath,target=/repo" `
  ghcr.io/gitleaks/gitleaks:v8.30.1 `
  detect --source=/repo --redact --verbose
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
    cargo check --locked --all-targets &&
    cargo test --locked --all-targets &&
    cargo clippy --locked --all-targets --all-features -- -D warnings &&
    cargo build --locked --release &&
    cargo install cargo-audit --locked --version 0.22.1 &&
    cargo audit'
```

## Native Windows build and release smoke

The supported executable requires Rust 1.94.1, the MSVC x64 linker, and a
Windows SDK. Run in an x64 Visual Studio Developer PowerShell:

```powershell
cargo fmt --all -- --check
cargo check --locked --all-targets
cargo test --locked --all-targets
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo build --locked --release
cargo install cargo-audit --locked --version 0.22.1
cargo audit

$version = (cargo metadata --no-deps --format-version 1 |
  ConvertFrom-Json).packages[0].version
$stage = ".\artifacts\printlatch-v$version-windows-x64"
New-Item -ItemType Directory -Force `
  "$stage\docs\assets", "$stage\examples", "$stage\packages\sdk\dist" |
  Out-Null
Copy-Item .\target\release\printlatch.exe "$stage\printlatch.exe"
Copy-Item -Path @(
  ".\scripts\install.ps1"
  ".\scripts\uninstall.ps1"
  ".\scripts\smoke-release.ps1"
) -Destination $stage
Copy-Item .\docs\assets\sample.pdf "$stage\docs\assets\sample.pdf"
Copy-Item .\examples\*.js, .\examples\*.mjs "$stage\examples\"
Copy-Item .\packages\sdk\dist\* "$stage\packages\sdk\dist\"
Copy-Item -Path @(
  ".\packages\sdk\package.json"
  ".\packages\sdk\README.md"
) -Destination "$stage\packages\sdk\"
Copy-Item -Path @(
  ".\README.md"
  ".\LICENSE"
  ".\NOTICE"
  ".\SECURITY.md"
) -Destination $stage
.\scripts\smoke-release.ps1 -ArchiveRoot $stage
```

The smoke chooses an unused loopback port and uses isolated temporary program
and data directories. It installs with `-NoStartup`, checks health, pairing,
authenticated preview, capture hash, invalid MIME rejection, and both uninstall
modes, then removes its temporary files. Scheduled-task registration is not
claimed by this smoke.

## Browser operator flow

Start the release candidate with an isolated data directory and port:

```powershell
$dataDir = Join-Path $env:TEMP (
  "printlatch-browser-smoke-" + [guid]::NewGuid().ToString("N")
)
$portProbe = [System.Net.Sockets.TcpListener]::new(
  [System.Net.IPAddress]::Loopback,
  0
)
$portProbe.Start()
$port = ([System.Net.IPEndPoint]$portProbe.LocalEndpoint).Port
$portProbe.Stop()
$dataDir
$port
.\target\release\printlatch.exe serve --data-dir $dataDir --port $port
```

In a second PowerShell window, paste the exact data path printed by the first
window and its printed port:

```powershell
$dataDir = "<paste the path printed by the first window>"
$port = <paste the port printed by the first window>
.\target\release\printlatch.exe dashboard --data-dir $dataDir --port $port
```

Then verify:

1. one-time pairing and target detection
2. authenticated test-PDF preview
3. explicit confirmation before capture
4. queued, printing, and terminal status updates
5. capture SHA-256 equality with the source PDF
6. retry after a transient health or status failure
7. desktop and 375 px layouts without horizontal overflow
8. no browser console errors
9. the no-Windows-printer empty state when the isolated environment has none
10. screen-reader-visible live status announcements for queue transitions

Do not select a physical printer during this gate.
After stopping the agent, remove only the unique data directory created for
this run:

```powershell
Remove-Item -LiteralPath $dataDir -Recurse -Force
```

## Cost-control state

The repository workflows are `workflow_dispatch` only through 2026-07-31.
Local or authorized self-hosted workers own build, lint, tests, browser E2E,
packaging, and smoke evidence during this window. Do not enable a paid runner
or restore automatic triggers without an explicit repository decision.
