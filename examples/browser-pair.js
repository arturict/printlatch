import { PrintLatchClient } from "../packages/sdk/dist/index.js";

const pairingCode = window.prompt("Enter the five-minute PrintLatch pairing code");
if (pairingCode) {
  const pairing = await PrintLatchClient.pair(pairingCode);
  const client = new PrintLatchClient({ token: pairing.token });
  const printers = await client.printers();
  console.table(printers);
  // Keep the token in memory in this minimal example. Production applications
  // should choose storage based on their XSS threat model and rotate it regularly.
}
