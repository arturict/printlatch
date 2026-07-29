# Release scope

## Release thesis

A web application should be able to hand one bounded PDF to one explicitly
approved Windows print location without a cloud account or an unauthenticated
localhost endpoint.

## Supported

- Windows 11 x64
- one agent per signed-in Windows user
- loopback HTTP on fixed port 32191
- HTTPS browser origins after local one-time pairing
- local Node.js processes using rotatable local tokens
- static, unencrypted PDF, maximum 10 MiB and 100 pages
- Windows-installed A4 or Letter document printers
- one queue worker, copies 1 through 10
- preview without printing
- PDF capture target
- job list, state, cancel-before-submit, explicit retry, and document retrieval

## Rejected or deferred

- LAN or internet listening
- central remote dispatch to many sites
- cloud storage or relay
- printer fleet policy, quotas, accounting, or user directories
- automatic printer discovery beyond Windows
- driver installation, replacement, or update
- macOS, Linux, Windows ARM64, or Windows 10 release claims
- HTML, image, Office, ZIP, URL, or encrypted PDF ingestion
- active PDF actions, forms, embedded files, or JavaScript
- raw ESC/POS, ZPL, EPL, CPCL, PostScript, or PCL
- cash drawers, scales, scanners, serial devices, or WebUSB
- label, receipt, plotter, photo, duplex, tray, color, finishing, and custom-media
  controls
- WebSocket protocol
- silent automatic retry after an interrupted print submission
- physical paper-output confirmation
- telemetry

## Success criteria

1. A Windows release archive installs without an admin-only system service.
2. A local process can create a token without exposing it in source control.
3. A browser origin can pair only with a local, origin-bound, one-time code.
4. A valid PDF can move through preview and PDF capture end to end.
5. A Windows printer submission can be made without a shell command.
6. Invalid, oversized, active, cross-client, replayed, and origin-mismatched
   requests fail safely.
7. Restarting during submission never silently replays a possibly printed job.
8. Uninstallation removes the executable and startup entry while preserving data
   unless purge is explicitly requested.

