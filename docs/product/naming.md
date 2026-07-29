# Naming decision

Decision date: 2026-07-29

## Final name

**PrintLatch**

The name describes the product's defining behavior: printing remains latched
until a local operator explicitly authorizes an origin or client.

## Checks performed

| Surface | Result at check time |
| --- | --- |
| General exact web search for `PrintLatch` and `Print Latch software` | No competing software product found |
| GitHub repository search | No exact or confusing software repository found |
| GitHub username | `printlatch` not registered |
| `arturict/printlatch` repository path | Not present before creation |
| npm `printlatch` | Package not found |
| PyPI `printlatch` | Package not found |
| crates.io `printlatch` | Crate not found |
| `printlatch.com` | Available |
| `printlatch.dev` | Available |
| `printlatch.io` | Available |

No domain was purchased. Search results include generic uses of the ordinary
words "print" and "latch" in mechanical and historical contexts, but no obvious
software-brand collision was found.

This is a practical availability and confusion screen, not a legal trademark
opinion or registration clearance. A formal legal search is outside the release
scope.

## Alternatives rejected

- **PrintBridge**: highly generic and already used descriptively by many print
  products and integrations.
- **SpoolPin**: package and repository surfaces were open, but the phrase is a
  common sewing-machine part and creates search ambiguity.
- **SpoolSeal**: available on software registries, but already used as an
  industrial valve product name.
- **PaperRelay**: an active research-paper project uses the name and the `.com`
  domain is unavailable.
- **LocalSpool**: descriptive but the `.com` domain is unavailable and the term
  appears in older spooler software documentation.

