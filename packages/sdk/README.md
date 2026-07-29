# `@printlatch/sdk`

Typed, dependency-free Fetch client for the PrintLatch loopback API.

The package is included in PrintLatch release archives for v0.1. It is not yet
published to npm. Browser clients must first receive a five-minute pairing code
created locally with `printlatch pair --origin https://your-app.example`.

```ts
import { PrintLatchClient } from "@printlatch/sdk";

const pairing = await PrintLatchClient.pair(code);
const client = new PrintLatchClient({ token: pairing.token });
const preview = await client.createJob({
  pdf: new Blob([pdfBytes], { type: "application/pdf" }),
  mode: "preview",
});
```

Do not place a local-process token in frontend code. Browser tokens are
origin-bound; local Node.js tokens are intentionally rejected when an `Origin`
header is present.

