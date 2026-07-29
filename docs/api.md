# HTTP API

Base URL: `http://127.0.0.1:32191`

The API is loopback-only. All `/v1` routes except pairing require
`Authorization: Bearer pl_live_...`.

## `GET /health`

Unauthenticated liveness only. It returns product, version, loopback bind policy,
and telemetry status. It exposes no printer, client, document, or queue data.

## `POST /v1/pair`

Browser only. Requires the browser-provided `Origin` header.

```json
{ "code": "PL-XXXXXXXX-XXXXXXXX-XXXXXXXX-XXXXXXXX" }
```

Returns a browser token once. The local CLI must have created the code for the
same exact origin within five minutes.

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
through 100.

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
