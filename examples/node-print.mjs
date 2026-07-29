import { readFile } from "node:fs/promises";
import process from "node:process";
import { PrintLatchClient } from "../packages/sdk/dist/index.js";

const [pdfPath, printerId = "capture:pdf"] = process.argv.slice(2);
const token = process.env.PRINTLATCH_TOKEN;

if (!pdfPath || !token) {
  console.error(
    "Usage: PRINTLATCH_TOKEN=<local-token> node examples/node-print.mjs <document.pdf> [printer-id]",
  );
  process.exitCode = 2;
} else {
  const bytes = await readFile(pdfPath);
  const client = new PrintLatchClient({ token });
  const job = await client.createJob({
    pdf: new Blob([bytes], { type: "application/pdf" }),
    filename: "document.pdf",
    printerId,
    mode: "print",
  });
  console.log(JSON.stringify({ id: job.id, state: job.state }, null, 2));
}
