# Security model

## Pairing

`printlatch pair --origin https://app.example` writes a SHA-256 digest of a
random 128-bit code to the local database. The code expires after five minutes
and is bound to the normalized origin before it is shown.

The browser sends the code to `POST /v1/pair`. Browsers set `Origin`; application
JavaScript cannot replace it. Code and origin must match in one database
transaction. A wrong origin does not consume the code. A successful request
consumes it and returns the only copy of a browser token.

## Tokens

- 256 random bits with `pl_live_` prefix
- only SHA-256 digest stored by the agent
- expiry from 1 to 90 days
- rotation invalidates the old token immediately
- revocation is explicit
- browser token requires exact `Origin`
- local-process token requires `Origin` to be absent
- no cookies and no wildcard CORS

The hash is not a password hash because tokens have full cryptographic entropy;
offline guessing is not practical.

## Loopback boundary

The server binds only to `127.0.0.1`. The `Host` header must be
`127.0.0.1:32191`, `localhost:32191`, or `[::1]:32191`. This blocks a public DNS
name that resolves to loopback from reusing its own host authority.

Private Network Access preflights receive narrowly scoped CORS headers. Actual
protected requests still require the correct token and client-type rule.

## Documents

PrintLatch never fetches URLs. It accepts one multipart file and exact known
fields. The original filename is ignored for storage.

Accepted PDF constraints:

- MIME exactly `application/pdf`
- valid PDF signature, EOF marker, and parser load
- maximum 10 MiB, 100 pages, and 20,000 objects
- maximum 100 MiB total decoded Flate streams
- maximum 100 megapixels across declared images
- no encryption, forms, JavaScript, launch action, open action, rich media, XFA,
  file specification, or embedded file

The original bytes are retained so preview and print refer to the same SHA-256
document.

## Queue

SQLite WAL stores job state. Claim is one atomic update. Cancel only succeeds
from `queued`. Retry only succeeds from `failed` or `unknown`, with a maximum of
three submissions.

If the agent starts and finds `printing`, it records `unknown`. A human or
authorized client must inspect and retry because the previous Windows call may
already have reached the spooler.

## Logging and telemetry

Normal logs contain startup state, bounded error descriptions, job IDs, and
state transitions. They do not contain tokens, pairing codes, request bodies,
filenames, or document contents.

PrintLatch makes no telemetry requests and contains no analytics SDK.

