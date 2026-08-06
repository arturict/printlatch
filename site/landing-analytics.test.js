import { describe, expect, it, vi } from "vitest";

import { createLandingEventTracker, sanitizeUmamiPayload } from "./landing-analytics.js";

describe("landing analytics event contract", () => {
  it("emits only bounded CTA values", () => {
    const send = vi.fn();
    const tracker = createLandingEventTracker(send);

    tracker.cta("download-release", "hero", "releases");
    tracker.cta("arbitrary", "hero", "releases");

    expect(send).toHaveBeenCalledOnce();
    expect(send).toHaveBeenCalledWith("landing-cta", {
      action: "download-release",
      location: "hero",
      target: "releases",
    });
  });

  it("emits section, scroll, and engagement thresholds exactly once", () => {
    const send = vi.fn();
    const tracker = createLandingEventTracker(send);

    tracker.section("security");
    tracker.section("security");
    tracker.section("unknown");
    tracker.scroll(76);
    tracker.scroll(100);
    tracker.scroll(100);
    tracker.engaged(60);
    tracker.engaged(120);

    expect(send.mock.calls).toEqual([
      ["landing-section-view", { section: "security" }],
      ["landing-scroll-depth", { depth: 25 }],
      ["landing-scroll-depth", { depth: 50 }],
      ["landing-scroll-depth", { depth: 75 }],
      ["landing-scroll-depth", { depth: 100 }],
      ["landing-engaged-time", { seconds: 30 }],
      ["landing-engaged-time", { seconds: 60 }],
      ["landing-engaged-time", { seconds: 120 }],
    ]);
  });

  it("keeps only safe standard UTM values and strips referrer paths", () => {
    expect(
      sanitizeUmamiPayload(
        {
          payload: {
            url: "/?utm_source=reddit&utm_campaign=oss_launch&email=secret%40example.com&utm_term=bad value",
            referrer: "https://www.reddit.com/r/selfhosted/comments/private-thread?user=42",
          },
        },
        { doNotTrack: "0", globalPrivacyControl: false },
        "https://printlatch.vercel.app",
      ),
    ).toEqual({
      payload: {
        url: "/?utm_source=reddit&utm_campaign=oss_launch",
        referrer: "https://www.reddit.com",
      },
    });
  });

  it("fails closed for Global Privacy Control", () => {
    expect(
      sanitizeUmamiPayload(
        { url: "/?utm_source=reddit" },
        { globalPrivacyControl: true },
        "https://printlatch.vercel.app",
      ),
    ).toBe(false);
  });
});
