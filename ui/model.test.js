import { describe, expect, it } from "vitest";
import {
  activeJobIds,
  canRetry,
  formatState,
  jobDiagnosis,
  pairingCodeFromHash,
  pollingRetryDelay,
} from "./model.js";

describe("operator job states", () => {
  it("uses explicit human-readable labels for every queue state", () => {
    expect(formatState("queued")).toBe("Queued");
    expect(formatState("printing")).toBe("Printing");
    expect(formatState("succeeded")).toBe("Success");
    expect(formatState("unknown")).toBe("Check before retry");
  });

  it("classifies only diagnostics actually reported by the worker", () => {
    expect(jobDiagnosis({ state: "failed", detail: "Printer is out of paper" }).kind).toBe("paper");
    expect(jobDiagnosis({ state: "failed", detail: "Access denied by Windows" }).kind).toBe(
      "permission",
    );
    expect(jobDiagnosis({ state: "failed", detail: "Spooler timed out" }).kind).toBe("timeout");
    expect(jobDiagnosis({ state: "failed", detail: "Printer driver failed" }).kind).toBe("driver");
    expect(jobDiagnosis({ state: "failed", detail: "Printer is offline" }).kind).toBe("offline");
    expect(jobDiagnosis({ state: "failed", detail: "Unclassified failure" }).kind).toBe("generic");
  });

  it("treats interrupted submission as duplicate-sensitive", () => {
    const diagnosis = jobDiagnosis({ state: "unknown", detail: null });
    expect(diagnosis.kind).toBe("interrupted");
    expect(diagnosis.help).toContain("will not replay");
  });

  it("offers the same-job retry only for eligible states below the cap", () => {
    expect(canRetry({ state: "failed", attempts: 1 })).toBe(true);
    expect(canRetry({ state: "unknown", attempts: 2 })).toBe(true);
    expect(canRetry({ state: "failed", attempts: 3 })).toBe(false);
    expect(canRetry({ state: "succeeded", attempts: 1 })).toBe(false);
  });

  it("keeps every active job in the polling set", () => {
    expect(
      activeJobIds([
        { id: "queued", state: "queued" },
        { id: "printing", state: "printing" },
        { id: "done", state: "succeeded" },
      ]),
    ).toEqual(["queued", "printing"]);
    expect(activeJobIds([{ id: "remembered", state: "unknown" }], "remembered")).toEqual([
      "remembered",
    ]);
  });

  it("backs off transient polling failures without abandoning the active job", () => {
    expect(pollingRetryDelay(1)).toBe(700);
    expect(pollingRetryDelay(2)).toBe(1400);
    expect(pollingRetryDelay(3)).toBe(2800);
    expect(pollingRetryDelay(4)).toBe(5000);
    expect(pollingRetryDelay(20)).toBe(5000);
  });

  it("preserves a fragment pairing code for a later healthy retry", () => {
    expect(pairingCodeFromHash("#code=PL-ABCD")).toBe("PL-ABCD");
    expect(pairingCodeFromHash("#overview")).toBeNull();
  });
});
