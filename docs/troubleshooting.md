# Troubleshooting

## Agent does not start

Run:

```powershell
& "$env:LOCALAPPDATA\PrintLatch\bin\printlatch.exe" diagnose
& "$env:LOCALAPPDATA\PrintLatch\bin\printlatch.exe" serve
```

The foreground command shows bounded local diagnostics. Port 32191 must be free.
PrintLatch never falls back to a LAN address or random port.

## Browser cannot connect

- Confirm `http://127.0.0.1:32191/health` returns JSON.
- Create a new pairing code for the exact origin shown in the browser address
  bar.
- `https://app.example` and `https://www.app.example` are different origins.
- HTTP origins are rejected except `localhost`, `127.0.0.1`, and `::1` for
  development.
- A browser token cannot be used without its exact `Origin`.
- A local-process token is intentionally rejected from every browser origin.

## Printer is missing

Confirm it appears in Windows Settings and can print a Windows test page.
Restart PrintLatch after Windows printer changes. PrintLatch does not install or
repair drivers.

## Job says `succeeded`, but nothing printed

`succeeded` means Windows accepted the submission. Check:

- printer power and connection
- Windows print queue
- paper and ink or toner
- driver dialogs or errors
- correct printer mapping

Use `capture:pdf` to separate application and document problems from
driver/hardware problems.

## Job says `unknown`

The agent restarted while submitting to Windows. Inspect the Windows queue and
physical printer before retrying. Automatic replay is deliberately disabled to
avoid a duplicate invoice or packing slip.

## PDF is rejected

PrintLatch 0.1 accepts only static, unencrypted PDFs within 10 MiB and 100 pages.
Forms, JavaScript, launch actions, embedded files, XFA, rich media, and very
large decoded streams or images are rejected.

## Where are files stored?

Default:

```text
%LOCALAPPDATA%\PrintLatch\jobs
%LOCALAPPDATA%\PrintLatch\captures
```

Uninstall preserves them. Purge only with explicit `-PurgeData`.

