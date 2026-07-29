# Changelog

All notable changes are documented here. This project uses semantic versioning.

## [Unreleased]

## [0.1.1] - 2026-07-29

### Added

- embedded, origin-bound local operator dashboard
- resumable first-run status based on real agent, target, and queue state
- built-in PDF preview and explicitly confirmed capture flow
- distinct no-service, no-Windows-printer, no-job, and no-filter-result states
- accessible queue updates and conservative paper, driver, permission, timeout,
  offline, and interrupted-job guidance
- `printlatch dashboard` command and installer handoff

### Changed

- local dashboard GET authentication now accepts browser-proven same-origin
  requests only when the stored loopback origin matches the current Host
- dashboard launch verifies the local installation with an HMAC challenge and
  binds the grant to the current agent session
- fresh dashboard grants rotate one stable operator credential so queue history
  remains available while the previous token is invalidated
- ordinary application pairing grants remain independent even when client names
  and origins match; a dedicated internal marker identifies the dashboard
- every active dashboard job now keeps an independent polling loop
- transient job-status requests retry automatically with a bounded backoff
- the bundled test PDF now requires the paired dashboard token
- CORS and Private Network Access headers are now restricted to `/v1/*`, so the
  operator shell never grants cross-origin read access
- dashboard recovery commands include the running executable, active data
  directory, and port
- the embedded Node example uses the SDK's exported `PrintLatchClient`
- integration cards now keep long commands inside their own scrollers on narrow
  screens instead of widening the full dashboard

## [0.1.0] - 2026-07-29

### Added

- Windows 11 x64 loopback agent and CLI
- exact-origin, one-time browser pairing
- rotatable and revocable local-process tokens
- PDF-only validation and resource limits
- durable SQLite queue with atomic state transitions
- preview, authenticated document retrieval, and PDF capture
- Windows printer discovery and native PDF submission
- browser and Node.js TypeScript SDK
- per-user PowerShell installation and removal
- security, architecture, API, hardware, and troubleshooting documentation
- local, CI, release, and smoke workflows

[Unreleased]: https://github.com/arturict/printlatch/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/arturict/printlatch/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/arturict/printlatch/releases/tag/v0.1.0
