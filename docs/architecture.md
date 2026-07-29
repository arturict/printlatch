# Architecture

## Components

### `printlatch` agent and CLI

A single Rust executable owns configuration, SQLite migrations, authorization,
HTTP API, queue worker, PDF validation, Windows printer discovery, and Windows
PDF submission.

### `@printlatch/sdk`

A dependency-free TypeScript Fetch client for browsers and Node.js 20+. It
refuses a non-loopback base URL so copy-pasted configuration cannot quietly send
documents to a remote service.

### PDF capture

`capture:pdf` is always present. Print mode writes the accepted PDF atomically
to the captures directory. Preview mode retains the same accepted PDF and makes
it available through the authenticated document endpoint without entering the
print queue.

### Windows backend

Printer devices are enumerated through Windows APIs. Public API IDs are hashes
of current Windows names, not user-controlled command strings. The backend uses
the Windows PDF print path and submits copies without invoking a shell.

## Storage layout

```text
%LOCALAPPDATA%\PrintLatch\
├── bin\
│   └── printlatch.exe
├── captures\
│   └── <job-uuid>.pdf
├── jobs\
│   └── <job-uuid>.pdf
├── install.json
└── printlatch.sqlite3
```

Documents are not automatically deleted in 0.1. This is a deliberate
operator-visible retention choice. A bounded retention command is planned after
real usage evidence.

## Failure semantics

- API validation failures never create a job.
- A database insert failure removes the just-written temporary job file.
- Capture uses a generated destination.
- Printer submission failure becomes `failed`.
- Agent restart during printer submission becomes `unknown`.
- Windows acceptance becomes `succeeded`, with detail that physical output is
  not observable.

