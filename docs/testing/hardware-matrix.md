# Hardware and platform evidence

Last updated: 2026-07-29

| Path | Environment | Evidence | Claim |
| --- | --- | --- | --- |
| PDF capture | Windows host and automated API tests | Accepted PDF retained by SHA-256 and written to generated capture path | Supported |
| Windows printer enumeration | Windows 11 x64 host | Microsoft Print to PDF, Adobe PDF, OneNote virtual printers, and Brother MFC-J5340DW discovered through Windows | Supported enumeration |
| Windows PDF submission API | GitHub `windows-latest` build plus local released binary smoke | Pending release gate | No final claim until green |
| Brother MFC-J5340DW physical paper output | Installed on local Windows host | Not printed during initial development | Not verified |
| Microsoft Print to PDF output | Installed on local Windows host | Driver uses an interactive `PORTPROMPT:` destination | Discovered, not a silent V1 guarantee |
| Windows 10 | None | Not tested | Unsupported |
| Windows ARM64 | None | Not tested | Unsupported |
| macOS | None | Not implemented | Unsupported |
| Linux/CUPS | Platform-neutral API tests only | No printer backend | Unsupported |
| Label or receipt printer | None | Custom media intentionally excluded | Unsupported |

This file must be updated with exact release, commit, and result before a
hardware or platform claim is expanded.

