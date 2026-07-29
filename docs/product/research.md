# Product research

Research date: 2026-07-29

## Decision

PrintLatch v0.1 is a secure local PDF print agent for developers and small teams
running a Windows 11 workstation with one or a few printers already installed.
The first job is invoices, packing slips, and ordinary business PDFs from an
HTTPS web application or a local Node.js process.

The release is intentionally not a universal print server, driver replacement,
BYOD printer-sharing product, fleet manager, raw thermal-printer toolkit, or
cloud relay.

## Repeated problems found

### Browsers do not provide safe silent printer access

The long-running Stack Overflow question about silent browser printing still
points developers to a local helper such as QZ Tray or PrintNode, because normal
browser printing cannot silently choose arbitrary local devices. Recent Odoo POS
questions show the same shape: direct USB printing without a dialog requires an
IoT box, proxy, or custom local service.

Sources:

- [Silent print from browser](https://stackoverflow.com/questions/36265503/silent-print-from-browser)
- [Odoo 17 kiosk USB printing discussion](https://www.odoo.com/forum/help-1/automatic-receipt-printing-in-odoo-17-kiosk-301960)
- [Odoo 18 intermittent validation and printing](https://www.odoo.com/forum/help-1/pos-validation-and-printing-issues-in-odoo-18-276589)

### Certificate and trust setup is a real integration burden

QZ Tray deliberately prompts for untrusted requests. Its FAQ says silent warning
suppression requires signed requests using a purchased certificate or a
self-managed root. Its print-server documentation includes certificate
generation and trust-store work across operating systems. A May 2026 support
thread describes needing manual trust setup per user or PC for a self-signed
certificate.

This is defensible for a broad raw-print product, but it is heavy for a small
PDF-only integration.

Sources:

- [QZ Tray FAQ](https://github.com/qzind/tray/wiki/FAQ)
- [QZ Tray print server setup](https://github.com/qzind/tray/wiki/print-server)
- [May 2026 QZ Tray silent-print support thread](https://groups.google.com/g/qz-print/c/P6ZWzCn90mQ)

### Cloud relays solve reachability but add account, cost, and dependency

PrintNode's current free tier covers 50 API prints per month on one computer;
the Essential plan starts at USD 9 per month for 5,000 prints. Its client sends
jobs through the PrintNode service. A recent WooCommerce report describes a
printer shown online in Windows and PrintNode that still does not print, while
another high-volume report mentions client print-engine failures.

ezeep's Personal tier includes 50 API pages monthly; paid tiers are per user and
start with a ten-user minimum. Reviews mention delayed jobs, unexpected
disconnects, and migration friction. Those products solve broader remote and
fleet-management jobs than this release.

Sources:

- [PrintNode pricing](https://www.printnode.com/en/pricing)
- [Recent PrintNode and WooCommerce failure report](https://www.reddit.com/r/woocommerce/comments/1ur09t5/rollo_printer_woo_dashboard_printnode/)
- [PrintNode at high scale](https://www.reddit.com/r/Netsuite/comments/1f1yye7)
- [ezeep pricing](https://www.ezeep.com/pricing)
- [ezeep review themes](https://www.g2.com/products/ezeep/reviews?qs=pros-and-cons)
- [ezeep migration review](https://www.trustpilot.com/review/ezeep.com)

### BYOD printing is a different job

PaperCut Mobility Print is a strong free product for sharing printers with
employees and students. Its own positioning is BYOD/native printing, not an
application-facing loopback API. Standalone Mobility Print has no user
authentication or secure release, and current requirements explicitly exclude
label printers and plotters because of non-standard sizes.

Sources:

- [Mobility Print product page](https://www.papercut.com/products/free-software/mobility-print/)
- [Mobility Print FAQ](https://www.papercut.com/help/manuals/mobility-print/faq/)
- [Mobility Print system requirements](https://www.papercut.com/help/manuals/mobility-print/set-up/system-requirements/)

### Reliability needs visible queue semantics

User reports across receipt and remote printing repeatedly describe jobs stuck
after the first print, devices appearing online but producing nothing, and
restarts used as a workaround. These are not all caused by local-agent software,
but they show why "request accepted" must not be presented as physical-output
proof.

Sources:

- [Receipt queue blocked after first job](https://www.reddit.com/r/printers/comments/1hhwfxf/seeking_help_with_strange_receipt_printer_issues/)
- [QZ printer list stale until restart](https://github.com/qzind/tray/issues/393)
- [PaperCut missing-job troubleshooting](https://www.papercut.com/help/manuals/mobility-print/troubleshooting/missing-print-jobs/)

## Evidence-weighted opportunity

| Candidate entry | User evidence | Existing solutions | Solo release risk | Decision |
| --- | --- | --- | --- | --- |
| Secure PDF bridge for Windows web apps | Repeated across Stack Overflow, POS forums, and competitor support | Existing tools are broader, hosted, or certificate-heavy | Moderate | Build |
| Raw receipt and label printing | Strong demand | QZ Tray and many device-specific libraries | High hardware and format matrix | Exclude from v0.1 |
| BYOD printer sharing | Strong demand | PaperCut Mobility Print is mature and free | High network and identity scope | Exclude |
| Cloud print relay | Proven market | PrintNode and ezeep are established | Requires hosted security, billing, and operations | Exclude |
| Universal cross-platform print service | Broad | Multiple mature incumbents | Very high compatibility risk | Exclude |

## Jobs to be done

1. When my HTTPS invoice or operations app creates a PDF, I want the user to
   approve that origin once and send the document to a known local printer
   without a browser print dialog.
2. When integrating printing, I want a real dry-run target and visible job state
   so I can test without consuming paper.
3. When a job fails or the agent restarts, I want an honest state and bounded
   diagnostics so I can retry without accidental duplicates.
4. When handling invoices or internal documents, I want them to remain on the
   local machine and avoid a third-party print cloud.

## Target user

- developer or technically capable operator
- one Windows 11 x64 workstation per print location
- one or a few ordinary Windows-installed document printers
- PDF already produced by the application
- values local document handling and explicit authorization
- does not need fleet policies, remote internet relay, raw printer languages, or
  mobile/BYOD discovery

