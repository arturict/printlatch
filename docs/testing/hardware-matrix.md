# Hardware and platform evidence

Last updated: 2026-07-29

| Path | Environment | Evidence | Claim |
| --- | --- | --- | --- |
| PDF capture | Windows 11 x64 host and GitHub `windows-2025` | Release candidate `114bf2b` passed install, health, pairing, preview hash, capture hash, MIME rejection, and uninstall locally and in [CI run 30468388853](https://github.com/arturict/printlatch/actions/runs/30468388853). The v0.1.1 archive passed the same smoke locally on Windows 11 x64. | Supported |
| Windows printer enumeration | Windows 11 x64 host, release candidate `114bf2b` | Microsoft Print to PDF, Adobe PDF, OneNote virtual printers, Brother PC-FAX, and Brother MFC-J5340DW discovered through Windows | Supported enumeration |
| Windows PDF submission API | GitHub `windows-2025` build and tests plus local released-binary enumeration | Native Windows PDF path compiled, tested without a physical target, and Clippy-clean. No driver submission was triggered during release testing. | Implemented, but physical and interactive virtual-printer output are not verified |
| Brother MFC-J5340DW physical paper output | Installed on local Windows host | Not printed during initial development | Not verified |
| Microsoft Print to PDF output | Installed on local Windows host | Driver uses an interactive `PORTPROMPT:` destination | Discovered, not a silent V1 guarantee |
| Windows 10 | None | Not tested | Unsupported |
| Windows ARM64 | None | Not tested | Unsupported |
| macOS | None | Not implemented | Unsupported |
| Linux/CUPS | Platform-neutral API tests only | No printer backend | Unsupported |
| Label or receipt printer | None | Custom media intentionally excluded | Unsupported |

The v0.1.1 executable was produced by the already-completed Windows PR build
for `c042190`; its Rust and UI/SDK sources are identical to the release tag.
The final archive was assembled and smoke-tested locally from the release tag.
Published SHA-256 values are authoritative. No build attestation is claimed
for v0.1.1.
