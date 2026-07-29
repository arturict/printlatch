# Troubleshooting

## Agent does not start

Run:

```powershell
& "$env:LOCALAPPDATA\PrintLatch\bin\printlatch.exe" diagnose
& "$env:LOCALAPPDATA\PrintLatch\bin\printlatch.exe" serve
```

The foreground command shows bounded local diagnostics. Port 32191 must be free.
PrintLatch never falls back to a LAN address or random port.

## Dashboard does not open or connect

Run:

```powershell
printlatch dashboard
```

The command fails closed if the agent is not reachable. Start the agent, then
run it again. Each dashboard URL contains a one-time code in the URL fragment
and expires after five minutes. If a code is expired or already used, create a
new URL instead of reusing it.

The dashboard keeps its token in browser session storage. Closing the browser
session may require a new `printlatch dashboard` command. Setup progress itself
is reconstructed from the current printer list and queue.

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

The dashboard always shows the verified PDF capture target. If no hardware
appears, open Windows Settings > Bluetooth & devices > Printers & scanners,
confirm the printer exists for the current user, then use **Detect again**.

## Job says `succeeded`, but nothing printed

`succeeded` means Windows accepted the submission. Check:

- printer power and connection
- Windows print queue
- paper and ink or toner
- driver dialogs or errors
- correct printer mapping

Use `capture:pdf` to separate application and document problems from
driver/hardware problems.

For a failed dashboard job, use the bounded diagnostic:

- paper or media wording: correct the device condition first
- access or permission wording: check the current Windows user and printer ACL
- timeout wording: inspect the device and Windows queue
- driver or spooler wording: repair the Windows installation outside PrintLatch
- `unknown`: inspect the Windows queue and physical output before retrying

Retry requeues the same job ID and is capped at three submission attempts.
PrintLatch never automatically replays an interrupted submission.

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
