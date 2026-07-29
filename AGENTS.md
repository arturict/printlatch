# Contributor instructions

PrintLatch is deliberately narrow. Preserve these invariants:

- The HTTP server binds to loopback only.
- Every print, preview, document, printer, queue, cancel, and retry operation is
  authenticated.
- Browser tokens are exact-origin bound. Local-process tokens reject requests
  carrying an Origin header.
- The agent never fetches job URLs and never turns printer or filename input
  into a shell command.
- V1 accepts only static, unencrypted PDFs within the documented limits.
- Interrupted Windows submissions become `unknown` and are never replayed
  automatically.
- Telemetry remains absent by default.

Run every gate in `docs/testing/strategy.md` before proposing a change.

