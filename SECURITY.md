# Security policy

## Supported versions

| Version | Supported |
| --- | --- |
| 0.1.x | Yes |
| older | No |

## Report privately

Use [GitHub private vulnerability reporting](https://github.com/arturict/printlatch/security/advisories/new).
Do not open a public issue for a suspected vulnerability or include real
documents, credentials, tokens, machine names, or personal data.

Include:

- affected PrintLatch version
- Windows version
- minimal reproduction using synthetic data
- expected and observed result
- impact assessment
- any proposed mitigation

You should receive an acknowledgment within seven days. No bounty or guaranteed
fix timeline is offered.

## Scope

The security boundary and residual risks are documented in
[the threat model](docs/security/threat-model.md). Same-user malware, unpatched
Windows printer drivers, and proof of physical paper output are outside
PrintLatch's protection boundary.

