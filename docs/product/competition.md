# Competition matrix

Checked 2026-07-29. Pricing and product details can change.

| Product | Primary job | Local or cloud path | App-facing API | Authorization model | Current cost signal | PrintLatch difference |
| --- | --- | --- | --- | --- | --- | --- |
| QZ Tray | Broad browser-to-device integration, including raw formats | Local WebSocket agent | Yes | Request warnings; signed requests and certificate trust for silent flows | Core is free; silent deployment can use paid certificate/support or self-managed trust | PDF-only HTTP Fetch API, no local root certificate, exact-origin pairing |
| PrintNode | Remote API printing across locations | Hosted relay plus local client | Yes | PrintNode account and API credential | 50 prints/month free; Essential USD 9/month | No account, no hosted relay, documents remain local |
| PaperCut Mobility Print | BYOD and native printer sharing | Local server with optional peer-to-peer cloud negotiation | Not its main job | Standalone version lacks user auth/secure print | Free | Application job API, not BYOD discovery or fleet controls |
| ezeep | Cloud print management and remote work | Cloud plus connector/hub | Yes | Organization/user account and connector | Personal has 50 API pages; paid plans are per user with minimums | No cloud, user directory, quotas, hub, or fleet management |
| Browser `window.print()` | Interactive printing | Local browser dialog | Browser API only | User confirms each dialog | Free | Explicitly paired silent PDF submission with queue state |
| Direct IPP/raw socket code | Device-specific network printing | Direct network | Custom | Usually application-defined | Infrastructure only | Reuses Windows-installed printers and rejects user-supplied network destinations |

PrintLatch is not positioned as "better printing" in general. It is a smaller
trust boundary for a narrower job.

