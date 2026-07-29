# Contributing

Thank you for helping PrintLatch remain small, secure, and honest.

## Before proposing a feature

Read [the release scope](docs/product/scope.md). Open an issue with the user job,
evidence, security impact, and a test plan. Requests for cloud relay, fleet
management, raw printer languages, or unverified platform claims are outside the
current direction.

## Development

1. Fork the repository and create a focused branch.
2. Do not use real customer documents, printer credentials, or personal data.
3. Add tests for behavior and abuse paths.
4. Run all commands in [the test strategy](docs/testing/strategy.md).
5. Update the smallest relevant documentation and changelog entry.
6. Open a pull request using the template.

## Code expectations

- no shell construction from user input
- no new network destination without an explicit threat-model update
- no wildcard CORS, cookie auth, or unauthenticated mutation
- no telemetry by default
- no hardware or platform claim without reproducible evidence
- no token, pairing code, file body, or sensitive path in logs
- preserve interrupted-job semantics that prevent silent duplicates

By contributing, you agree that your contribution is licensed under Apache
License 2.0.

