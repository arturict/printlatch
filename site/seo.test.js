import { readFile } from "node:fs/promises";
import { describe, expect, it } from "vitest";

describe("landing search metadata", () => {
  it("publishes structured product facts and agent discovery", async () => {
    const [html, robots, sitemap, llms] = await Promise.all([
      readFile(new URL("./index.html", import.meta.url), "utf8"),
      readFile(new URL("./robots.txt", import.meta.url), "utf8"),
      readFile(new URL("./sitemap.xml", import.meta.url), "utf8"),
      readFile(new URL("./llms.txt", import.meta.url), "utf8"),
    ]);

    expect(html).toContain('rel="canonical" href="https://printlatch.vercel.app/"');
    expect(html).toContain('type="application/ld+json"');
    expect(html).toContain('"@type": ["SoftwareApplication", "SoftwareSourceCode"]');
    expect(robots).toContain("User-agent: OAI-SearchBot");
    expect(sitemap).toContain("<lastmod>2026-08-06</lastmod>");
    expect(llms).toContain("local PDF print bridge for Windows web applications");
    expect(llms).toContain("https://github.com/arturict/printlatch");
  });
});
