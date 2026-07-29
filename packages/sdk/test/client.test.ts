import { describe, expect, it, vi } from "vitest";
import { PrintLatchClient, PrintLatchError } from "../src/index.js";

const TOKEN = `pl_live_${"x".repeat(43)}`;

describe("PrintLatchClient", () => {
  it("only accepts loopback HTTP agent URLs", () => {
    for (const baseUrl of [
      "https://127.0.0.1:32191",
      "http://printer.example",
      "http://127.0.0.1:32191/path",
      "http://user@127.0.0.1:32191",
    ]) {
      expect(() => new PrintLatchClient({ token: TOKEN, baseUrl })).toThrow("HTTP loopback origin");
    }
  });

  it("adds the bearer token without logging or returning it", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(
      Response.json({
        printers: [
          {
            id: "capture:pdf",
            name: "PrintLatch PDF Capture",
            kind: "capture",
            tested: true,
            detail: "test",
          },
        ],
      }),
    );
    const client = new PrintLatchClient({ token: TOKEN, fetch });
    const printers = await client.printers();
    expect(printers).toHaveLength(1);
    const request = fetch.mock.calls[0];
    expect(request?.[0]).toBe("http://127.0.0.1:32191/v1/printers");
    expect(new Headers(request?.[1]?.headers).get("Authorization")).toBe(`Bearer ${TOKEN}`);
    expect(JSON.stringify(printers)).not.toContain(TOKEN);
  });

  it("uses multipart PDF uploads and preserves safe defaults", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(
      Response.json(
        {
          job: {
            id: "job-1",
            client_id: "client-1",
            printer_id: "capture:pdf",
            state: "preview_ready",
            mode: "preview",
            copies: 1,
            page_count: 1,
            byte_count: 10,
            sha256: "abc",
            attempts: 0,
            detail: null,
            created_at: 1,
            updated_at: 1,
          },
        },
        { status: 202 },
      ),
    );
    const client = new PrintLatchClient({ token: TOKEN, fetch });
    const job = await client.createJob({
      pdf: new Blob(["%PDF-1.4\n%%EOF"], { type: "application/pdf" }),
      mode: "preview",
    });
    expect(job.state).toBe("preview_ready");
    const form = fetch.mock.calls[0]?.[1]?.body;
    expect(form).toBeInstanceOf(FormData);
    expect((form as FormData).get("copies")).toBe("1");
    expect((form as FormData).get("mode")).toBe("preview");
  });

  it("returns typed API errors without including credentials", async () => {
    const fetch = vi
      .fn<typeof globalThis.fetch>()
      .mockResolvedValue(
        Response.json(
          { error: { code: "forbidden", message: "this client is not allowed" } },
          { status: 403 },
        ),
      );
    const client = new PrintLatchClient({ token: TOKEN, fetch });
    const error = await client.printers().catch((caught: unknown) => caught);
    expect(error).toBeInstanceOf(PrintLatchError);
    expect((error as Error).message).not.toContain(TOKEN);
  });
});
