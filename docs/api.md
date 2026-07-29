# HTTP API

Base URL: `http://127.0.0.1:32191`

The API is loopback-only. All `/v1` routes except pairing require
`Authorization: Bearer pl_live_...`.

## Local operator dashboard

`GET /app/` serves static dashboard assets. They contain no token and expose no
printer or queue data until paired. Run `printlatch dashboard` locally to create
a five-minute pairing grant for the exact loopback origin. The grant travels in
the URL fragment, so it is not sent in the HTTP request, server logs, or
referrer.

Same-origin dashboard GETs do not carry an `Origin` header in browsers. They are
accepted only when the stored dashboard origin exactly equals
`http://<current loopback Host>` and the browser supplies
`Sec-Fetch-Site: same-origin`. Cross-origin browser requests still require the
exact paired `Origin`.

The static dashboard shell is public, but `GET /app/test-page.pdf` is not. The
built-in PDF requires the same authenticated, origin-bound dashboard token as
preview and document retrieval.

## `GET /health`

Unauthenticated liveness only. It returns product, version, loopback bind policy,
and telemetry status. It exposes no printer, client, document, or queue data.

`GET /health/instance?challenge=<32 hex characters>` is reserved for the local
CLI. It returns the current random agent-session identifier plus an HMAC proof
derived from installation-local state. `printlatch dashboard` verifies that
proof before creating a grant and binds the grant to that agent session.

## `POST /v1/pair`

Browser only. Requires the browser-provided `Origin` header.

```json
{ "code": "PL-XXXXXXXX-XXXXXXXX-XXXXXXXX-XXXXXXXX" }
```

Returns a browser token once. The local CLI must have created the code for the
same exact origin within five minutes. Every ordinary application grant creates
an independent client and job history, even when another client has the same
name and origin. Only session-bound grants created by `printlatch dashboard`
rotate the built-in dashboard's stable operator credential.

## `GET /v1/printers`

Returns:

```json
{
  "printers": [
    {
      "id": "capture:pdf",
      "name": "PrintLatch PDF Capture",
      "kind": "capture",
      "tested": true,
      "detail": "..."
    }
  ]
}
```

`tested` is about the PrintLatch project evidence, not a promise about a
particular user's hardware.

## `POST /v1/jobs`

Multipart fields:

| Field | Required | Value |
| --- | --- | --- |
| `file` | yes | one `application/pdf` file |
| `mode` | yes | `preview` or `print` |
| `printer_id` | for print | an ID from `/v1/printers` |
| `copies` | no | integer 1 through 10, default 1 |

Unknown fields are rejected. URLs are never accepted.

Returns `202` and the created job.

## `GET /v1/jobs?limit=25`

Lists only jobs belonging to the authenticated client. Limit is clamped to 1
through 100 for recent history. Every `queued` or `printing` job is also
returned, even when it is older than that history window.

## `GET /v1/jobs/:id`

Returns one client-owned job.

## `GET /v1/jobs/:id/document`

Returns the accepted PDF with `Content-Type: application/pdf`,
`Content-Disposition: inline`, `X-Content-Type-Options: nosniff`, and
`Cache-Control: no-store, private`.

## `POST /v1/jobs/:id/cancel`

Only `queued` can transition to `canceled`.

## `POST /v1/jobs/:id/retry`

Only `failed` or `unknown` can transition to `queued`. At most three submission
attempts are allowed.

## Errors

```json
{
  "error": {
    "code": "bad_request",
    "message": "MIME type must be application/pdf"
  }
}
```

Internal errors are logged locally and returned as the generic message
`internal error`.
