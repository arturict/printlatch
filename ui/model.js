export function formatState(state) {
  const labels = {
    preview_ready: "Preview ready",
    queued: "Queued",
    printing: "Printing",
    succeeded: "Success",
    failed: "Failed",
    unknown: "Check before retry",
    canceled: "Canceled",
  };
  return labels[state] || state;
}

export function canRetry(job) {
  return (job.state === "failed" || job.state === "unknown") && job.attempts < 3;
}

export function jobDiagnosis(job) {
  if (job.state === "unknown") {
    return {
      kind: "interrupted",
      title: "The agent restarted during submission",
      help: "PrintLatch will not replay this job automatically. Check the Windows queue and physical output before retrying.",
    };
  }
  const detail = (job.detail || "").toLowerCase();
  if (detail.includes("paper") || detail.includes("out of media")) {
    return {
      kind: "paper",
      title: "The printer reported a paper or media problem",
      help: "Check paper, media size, and the Windows queue. Retry this same job only after the device is ready.",
    };
  }
  if (
    detail.includes("access denied") ||
    detail.includes("permission") ||
    detail.includes("unauthorized")
  ) {
    return {
      kind: "permission",
      title: "Windows denied access to the printer",
      help: "Check the current Windows user and printer permissions, then retry this same job.",
    };
  }
  if (detail.includes("timed out") || detail.includes("timeout")) {
    return {
      kind: "timeout",
      title: "The printer handoff timed out",
      help: "Check the device and Windows queue. A retry keeps the same job ID and is limited to three attempts.",
    };
  }
  if (
    detail.includes("driver") ||
    detail.includes("windows rejected") ||
    detail.includes("spooler")
  ) {
    return {
      kind: "driver",
      title: "Windows or the installed driver rejected the handoff",
      help: "Check the printer driver and Windows queue. PrintLatch cannot repair or replace a driver.",
    };
  }
  if (detail.includes("no longer available") || detail.includes("offline")) {
    return {
      kind: "offline",
      title: "The selected printer is unavailable",
      help: "Reconnect the printer or choose another Windows-installed target before retrying.",
    };
  }
  return {
    kind: "generic",
    title: "The printer handoff failed",
    help: "Review the bounded diagnostic below, check Windows, then retry this same job if it is safe.",
  };
}
