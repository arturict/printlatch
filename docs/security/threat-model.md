# Threat model

Version: 0.1.1

## Assets

- PDF document contents
- authorization tokens and one-time pairing codes
- origin and client allow-list
- printer selection
- queue integrity and job state
- local captures and retained input files
- availability of the Windows workstation and printer

## Trust boundaries

1. HTTPS web origin to loopback HTTP API
2. local Node.js process to loopback HTTP API
3. API process to local SQLite and job files
4. queue worker to Windows print APIs
5. Windows print subsystem to third-party driver and physical printer

## Adversaries

- an arbitrary website opened in the user's browser
- a malicious or compromised paired web origin
- a local unprivileged process running as another Windows user
- same-user malware
- a malicious PDF supplied by an otherwise authorized client
- a network attacker trying to route through localhost or DNS rebinding
- an accidental operator action, duplicate retry, or stale printer mapping
- a vulnerable or malicious Windows printer driver

## Threats and controls

| Threat | Control | Test or evidence |
| --- | --- | --- |
| Unauthenticated localhost print | All printer, job, document, cancel, and retry routes require a token | `tests/security.rs` |
| Cross-site request or token replay | No cookies; browser token requires exact paired `Origin`; local tokens reject every request with `Origin` | browser-origin tests |
| Cross-origin operator-shell read | CORS and Private Network Access response headers are emitted only on `/v1/*`; `/app` and its assets never grant cross-origin read access | shell and API preflight test |
| Pairing-code replay | 128-bit code, five-minute expiry, exact origin in the database transaction, one-time consumption | pairing replay test |
| Concurrent dashboard grant redemption | Stable credential rotation starts with an immediate SQLite write transaction, so competing rotations wait and observe the latest client state | writer-contention test |
| Local port impersonation | CLI verifies an HMAC challenge from installation-local state; dashboard grants are bound to the proven random agent session | real-agent and spoofed-listener tests |
| DNS rebinding | Fixed `127.0.0.1` bind and exact `Host` allow-list | rebinding test |
| WebSocket origin bypass | No WebSocket route; all upgrade attempts rejected | upgrade test |
| SSRF | No URL ingestion; every unknown multipart field is rejected | metadata-IP field test |
| Traversal | Generated UUID file path; supplied filename never selects storage | traversal filename test |
| ZIP or wrong MIME | Exact `application/pdf`, PDF signature, and parser required | MIME and ZIP tests |
| Oversized body | HTTP body cap and 10 MiB PDF cap | oversized-job test |
| PDF bomb or active content | page, object, decoded stream, and image-pixel caps; active actions, forms, embedded files, and encryption rejected | PDF guard tests |
| Command or argument injection | No shell or external print command; printer IDs are hashes resolved against current Windows enumeration | printer-ID test and source review |
| Queue double claim | Atomic SQLite `UPDATE ... RETURNING` state transition | concurrent claim/cancel test |
| Active job hidden by history cap | Job listings always include every queued or printing job in addition to the bounded recent-history window | active-history test |
| Restart duplicate | `printing` becomes `unknown`; no automatic replay | restart test |
| Cross-client document access | Every job lookup is scoped by authenticated client ID | isolation test |
| Dashboard re-pair history loss | Fresh grants atomically rotate the client selected by a dedicated internal dashboard marker, invalidate earlier tokens, and retain its jobs | re-pair history test |
| Application sessions impersonating dashboard labels | Only dashboard grants can select the internal dashboard marker; ordinary browser grants receive separate client IDs and histories even with the same name and origin | independent browser-client test |
| Secret leak in normal logs | Requests are not body-logged; token values are never tracing fields; errors are bounded | source review and SDK error test |
| Silent remote job | Loopback-only bind; remote server must act through an authorized browser on that machine or a local process | bind assertion and architecture |

## Residual risks

### Same-user malware

A process running as the same Windows user can read that user's local files,
interfere with the process, or extract tokens from the integrating application.
PrintLatch is not a sandbox against a compromised Windows account.

### Printer drivers and spooler

PrintLatch hands a rendered PDF to Windows. The Windows print subsystem and
vendor driver remain in the trusted computing base. A driver can crash, show UI,
misrender, or contain vulnerabilities. PrintLatch does not install or update
drivers.

The current Windows adapter also carries one compile-time unmaintained-crate
warning. It is documented with its exact dependency path in
[the dependency audit](dependency-audit.md).

### Malicious PDFs

The preflight checks materially reduce common parser and resource abuse, but no
PDF parser is risk-free. Limits are intentionally low. Highly sensitive
deployments should isolate the Windows print account and keep Windows and
drivers patched.

### Physical output is not observable

Success means the capture file was atomically written or Windows accepted the
print submission. It does not prove paper emerged, the right tray was loaded, or
the output was readable.

### Compromised paired origin

An XSS vulnerability on a paired origin can use that origin's token. Integrators
must apply a strict content security policy, avoid third-party script sprawl,
rotate tokens, and revoke a client after a compromise.

### Local HTTP

The API uses HTTP on loopback. Token secrecy and exact origin binding provide
request authorization; there is no local CA installation. A privileged local
attacker can already observe or modify the user process and is outside this
transport's protection boundary.
