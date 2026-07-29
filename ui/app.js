import { activeJobIds, canRetry, formatState, jobDiagnosis } from "/app/model.js";

const SESSION_TOKEN_KEY = "printlatch.dashboard.token";
const ACTIVE_JOB_KEY = "printlatch.dashboard.activeJob";
const VIEW_TITLES = {
  overview: ["LOCAL OPERATOR", "Overview"],
  jobs: ["LOCAL QUEUE", "Jobs"],
  integrate: ["FOR DEVELOPERS", "Integrate"],
  about: ["LOCAL AGENT", "About"],
};

const appState = {
  token: sessionStorage.getItem(SESSION_TOKEN_KEY),
  health: null,
  printers: [],
  jobs: [],
  previewJob: null,
  previewUrl: null,
  selectedTarget: null,
  connected: false,
  refreshing: false,
  pairingInProgress: false,
  pollingTimers: new Map(),
};

const elements = {
  overlay: document.querySelector("#connection-overlay"),
  connectionKicker: document.querySelector("#connection-kicker"),
  connectionHeading: document.querySelector("#connection-heading"),
  connectionMessage: document.querySelector("#connection-message"),
  connectionPill: document.querySelector("#connection-pill"),
  connectionLabel: document.querySelector("#connection-label"),
  pairingForm: document.querySelector("#pairing-form"),
  pairingCode: document.querySelector("#pairing-code"),
  pairingError: document.querySelector("#pairing-error"),
  printerList: document.querySelector("#printer-list"),
  recentJobs: document.querySelector("#recent-jobs"),
  jobsList: document.querySelector("#jobs-list"),
  jobsFilter: document.querySelector("#job-filter"),
  jobsSearch: document.querySelector("#job-search"),
  jobCount: document.querySelector("#job-nav-count"),
  createPreview: document.querySelector("#create-preview"),
  testStage: document.querySelector("#test-stage"),
  previewStage: document.querySelector("#preview-stage"),
  previewFrame: document.querySelector("#preview-frame"),
  previewMeta: document.querySelector("#preview-meta"),
  openPreview: document.querySelector("#open-preview"),
  confirmCapture: document.querySelector("#confirm-capture"),
  confirmDialog: document.querySelector("#confirm-dialog"),
  confirmTitle: document.querySelector("#confirm-title"),
  confirmCopy: document.querySelector("#confirm-copy"),
  confirmTarget: document.querySelector("#confirm-target"),
  runConfirmedJob: document.querySelector("#run-confirmed-job"),
  liveStatus: document.querySelector("#live-status"),
  toastRegion: document.querySelector("#toast-region"),
  refreshButton: document.querySelector("#refresh-button"),
  mobileMenu: document.querySelector("#mobile-menu"),
  sidebar: document.querySelector(".sidebar"),
};

function announce(message) {
  elements.liveStatus.textContent = "";
  window.setTimeout(() => {
    elements.liveStatus.textContent = message;
  }, 20);
}

function toast(message, kind = "info") {
  const item = document.createElement("div");
  item.className = "toast";
  item.dataset.kind = kind;
  item.textContent = message;
  elements.toastRegion.append(item);
  window.setTimeout(() => item.remove(), 4600);
}

function setConnection(state, label) {
  elements.connectionPill.dataset.state = state;
  elements.connectionLabel.textContent = label;
}

function setButtonLoading(button, loading, label) {
  if (!button.dataset.defaultLabel) {
    button.dataset.defaultLabel = button.textContent.trim();
  }
  button.disabled = loading;
  button.textContent = loading ? label : button.dataset.defaultLabel;
  button.setAttribute("aria-busy", String(loading));
}

function showConnectionOverlay(kind) {
  elements.overlay.hidden = false;
  if (kind === "offline") {
    elements.connectionKicker.textContent = "AGENT UNAVAILABLE";
    elements.connectionHeading.textContent = "The local service is not responding";
    elements.connectionMessage.textContent =
      "Start PrintLatch on this machine, then check again. Existing jobs are kept in the local queue.";
    setConnection("offline", "Agent offline");
  } else {
    elements.connectionKicker.textContent = "LOCAL CONNECTION";
    elements.connectionHeading.textContent = "Connect this dashboard";
    elements.connectionMessage.textContent =
      "Open a fresh, one-time dashboard link from PowerShell. No print endpoint becomes public.";
  }
}

function hideConnectionOverlay() {
  elements.overlay.hidden = true;
}

async function readError(response) {
  try {
    const body = await response.json();
    return body?.error?.message || `Request failed with HTTP ${response.status}`;
  } catch {
    return `Request failed with HTTP ${response.status}`;
  }
}

async function api(path, options = {}, requiresAuth = true) {
  const headers = new Headers(options.headers);
  if (requiresAuth) {
    if (!appState.token) {
      throw new Error("Dashboard is not paired");
    }
    headers.set("Authorization", `Bearer ${appState.token}`);
  }
  const response = await fetch(path, {
    ...options,
    headers,
    cache: "no-store",
  });
  if (response.status === 401 || response.status === 403) {
    if (requiresAuth) {
      sessionStorage.removeItem(SESSION_TOKEN_KEY);
      appState.token = null;
      showConnectionOverlay("pair");
    }
  }
  if (!response.ok) {
    throw new Error(await readError(response));
  }
  return response;
}

async function checkHealth() {
  setConnection("connecting", "Connecting");
  try {
    const response = await api("/health", {}, false);
    appState.health = await response.json();
    appState.connected = true;
    setConnection("connected", "Agent connected");
    document.querySelector("#about-version").textContent =
      `${appState.health.product} ${appState.health.version}`;
    return true;
  } catch {
    appState.connected = false;
    showConnectionOverlay("offline");
    updateSetup();
    return false;
  }
}

function codeFromFragment() {
  const fragment = new URLSearchParams(window.location.hash.slice(1));
  const code = fragment.get("code");
  if (code) {
    history.replaceState(null, "", "/app/");
  }
  return code;
}

async function pair(code) {
  const normalized = code.trim().toUpperCase();
  if (!/^PL-(?:[A-F0-9]{8}-){3}[A-F0-9]{8}$/.test(normalized)) {
    throw new Error("Enter the complete PrintLatch pairing code.");
  }
  const response = await api(
    "/v1/pair",
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: normalized }),
    },
    false,
  );
  const issued = await response.json();
  appState.token = issued.token;
  sessionStorage.setItem(SESSION_TOKEN_KEY, issued.token);
  return issued;
}

async function pairFromFragment() {
  const fragmentCode = codeFromFragment();
  if (!fragmentCode || appState.pairingInProgress) {
    return false;
  }
  appState.pairingInProgress = true;
  try {
    await pair(fragmentCode);
    toast("Secure local dashboard connected.", "success");
    await refreshAll();
    return true;
  } catch (error) {
    showConnectionOverlay("pair");
    elements.pairingError.textContent = error.message;
    return false;
  } finally {
    appState.pairingInProgress = false;
  }
}

async function loadProtectedData() {
  const [printerResponse, jobResponse] = await Promise.all([
    api("/v1/printers"),
    api("/v1/jobs?limit=100"),
  ]);
  const printerBody = await printerResponse.json();
  const jobBody = await jobResponse.json();
  appState.printers = printerBody.printers;
  appState.jobs = jobBody.jobs;
  appState.previewJob =
    appState.jobs.find((job) => job.state === "preview_ready" && job.mode === "preview") || null;
  renderAll();
  if (appState.previewJob && !appState.previewUrl) {
    await loadPreviewDocument(appState.previewJob);
  }
  watchActiveJobs();
}

async function refreshAll({ announceResult = false } = {}) {
  if (appState.refreshing) {
    return;
  }
  appState.refreshing = true;
  elements.refreshButton.disabled = true;
  try {
    const healthy = await checkHealth();
    if (!healthy) {
      return;
    }
    if (!appState.token) {
      showConnectionOverlay("pair");
      return;
    }
    await loadProtectedData();
    hideConnectionOverlay();
    if (announceResult) {
      toast("Dashboard refreshed.", "success");
      announce("Dashboard refreshed");
    }
  } catch (error) {
    if (appState.token) {
      toast(error.message, "error");
    }
  } finally {
    appState.refreshing = false;
    elements.refreshButton.disabled = false;
  }
}

function printerName(id) {
  return appState.printers.find((printer) => printer.id === id)?.name || id;
}

function hasVerifiedCapture() {
  return appState.jobs.some(
    (job) => job.mode === "print" && job.printer_id === "capture:pdf" && job.state === "succeeded",
  );
}

function updateSetup() {
  const agentDone = appState.connected && Boolean(appState.token);
  const targetsDone = agentDone && appState.printers.length > 0;
  const testDone = hasVerifiedCapture();
  const completed = [agentDone, targetsDone, testDone].filter(Boolean).length;
  document.querySelector("#progress-bar").style.width = `${(completed / 3) * 100}%`;
  document.querySelector("#progress-copy").textContent =
    completed === 3 ? "Setup complete" : `${completed} of 3 checks complete`;

  updateStep("agent", agentDone, !agentDone, agentDone ? "Connected" : "Action needed");
  document.querySelector("#step-agent-detail").textContent = appState.connected
    ? appState.token
      ? "Origin-bound dashboard session"
      : "Pair this dashboard"
    : "Local service unavailable";

  updateStep(
    "printers",
    targetsDone,
    agentDone && !targetsDone,
    targetsDone ? "Detected" : "Waiting",
  );
  const windowsCount = appState.printers.filter((printer) => printer.kind !== "capture").length;
  document.querySelector("#step-printers-detail").textContent = targetsDone
    ? `${windowsCount} Windows printer${windowsCount === 1 ? "" : "s"} plus verified capture`
    : "Waiting for connection";

  updateStep("test", testDone, targetsDone && !testDone, testDone ? "Verified" : "Not started");
  document.querySelector("#step-test-detail").textContent = testDone
    ? "Local PDF artifact written"
    : "No physical printer required";
  document.querySelector("#support-card").hidden = !testDone;
}

function updateStep(name, complete, active, label) {
  const step = document.querySelector(`[data-step="${name}"]`);
  step.dataset.complete = String(complete);
  step.dataset.active = String(active);
  document.querySelector(`#step-${name}-state`).textContent = label;
  const number = step.querySelector(".step-number");
  number.textContent = complete ? "✓" : String(["agent", "printers", "test"].indexOf(name) + 1);
}

function renderPrinters() {
  const fragment = document.createDocumentFragment();
  for (const printer of appState.printers) {
    const row = document.createElement("article");
    row.className = "printer-row";
    row.dataset.tested = String(printer.tested);

    const icon = document.createElement("span");
    icon.className = "printer-icon";
    icon.setAttribute("aria-hidden", "true");
    icon.textContent = printer.kind === "capture" ? "▣" : "▤";

    const copy = document.createElement("div");
    copy.className = "printer-copy";
    const name = document.createElement("strong");
    name.textContent = printer.name;
    const detail = document.createElement("span");
    detail.textContent =
      printer.kind === "capture"
        ? "Writes a local PDF artifact"
        : printer.kind === "windows_remote"
          ? "Windows network printer"
          : "Windows installed printer";
    copy.append(name, detail);

    const actions = document.createElement("div");
    actions.className = "printer-actions";
    const status = document.createElement("span");
    status.className = "target-status";
    status.dataset.state = printer.tested ? "verified" : "discovered";
    status.textContent = printer.tested ? "✓ Verified" : "● Discovered";
    actions.append(status);
    if (!printer.tested && hasVerifiedCapture()) {
      const testButton = document.createElement("button");
      testButton.type = "button";
      testButton.className = "button-ghost small-button";
      testButton.textContent = "Test page";
      testButton.addEventListener("click", () => preparePhysicalTest(printer));
      actions.append(testButton);
    }
    row.append(icon, copy, actions);
    fragment.append(row);
  }

  const windowsPrinters = appState.printers.filter((printer) => printer.kind !== "capture");
  if (windowsPrinters.length === 0) {
    const empty = document.createElement("div");
    empty.className = "empty-state compact-empty";
    const content = document.createElement("div");
    const title = document.createElement("h3");
    title.textContent = "No Windows printers detected";
    const text = document.createElement("p");
    text.textContent =
      "The verified PDF capture is ready. To add hardware, open Windows Settings > Bluetooth & devices > Printers & scanners, then detect again.";
    const action = document.createElement("button");
    action.type = "button";
    action.className = "button-secondary";
    action.textContent = "Detect again";
    action.addEventListener("click", () => refreshAll({ announceResult: true }));
    content.append(title, text, action);
    empty.append(content);
    fragment.append(empty);
  }
  elements.printerList.replaceChildren(fragment);
}

function jobRow(job) {
  const row = document.createElement("article");
  row.className = "job-row";
  row.dataset.jobId = job.id;

  const primary = document.createElement("div");
  primary.className = "job-primary";
  const id = document.createElement("strong");
  id.textContent = job.mode === "preview" ? "PDF preview" : `Job ${job.id.slice(0, 8)}`;
  const meta = document.createElement("span");
  meta.textContent = `${job.page_count} page${job.page_count === 1 ? "" : "s"} · ${formatBytes(job.byte_count)} · attempt ${job.attempts}`;
  primary.append(id, meta);

  const target = document.createElement("div");
  target.className = "job-target";
  target.textContent = printerName(job.printer_id);

  const status = document.createElement("span");
  status.className = "status-badge";
  status.dataset.state = job.state;
  status.textContent = formatState(job.state);

  const actions = document.createElement("div");
  actions.className = "job-actions";
  if (job.state === "queued") {
    actions.append(jobAction("Cancel", () => cancelJob(job)));
  }
  if (canRetry(job)) {
    actions.append(jobAction("Retry same job", () => retryJob(job)));
  }
  if (job.state !== "canceled") {
    actions.append(jobAction("Open PDF", () => openJobDocument(job), "button-ghost"));
  }

  row.append(primary, target, status, actions);
  if (job.state === "failed" || job.state === "unknown") {
    const diagnosis = jobDiagnosis(job);
    const detail = document.createElement("div");
    detail.className = "job-detail";
    const symbol = document.createElement("strong");
    symbol.setAttribute("aria-hidden", "true");
    symbol.textContent = "!";
    const copy = document.createElement("div");
    const title = document.createElement("strong");
    title.textContent = diagnosis.title;
    const text = document.createElement("span");
    text.textContent = `${diagnosis.help}${job.detail ? ` Diagnostic: ${job.detail}` : ""}`;
    copy.append(title, document.createElement("br"), text);
    detail.append(symbol, copy);
    row.append(detail);
  }
  return row;
}

function jobAction(label, action, className = "button-secondary") {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `${className} small-button`;
  button.textContent = label;
  button.addEventListener("click", action);
  return button;
}

function renderRecentJobs() {
  if (appState.jobs.length === 0) {
    elements.recentJobs.replaceChildren(
      emptyState(
        "≡",
        "No jobs yet",
        "Create the built-in test preview above. Your real local state will appear here.",
        "Create test preview",
        () => elements.createPreview.click(),
      ),
    );
    return;
  }
  const list = document.createElement("div");
  list.className = "job-list";
  for (const job of appState.jobs.slice(0, 4)) {
    list.append(jobRow(job));
  }
  elements.recentJobs.replaceChildren(list);
}

function filteredJobs() {
  const query = elements.jobsSearch.value.trim().toLowerCase();
  const filter = elements.jobsFilter.value;
  return appState.jobs.filter((job) => {
    const matchesQuery =
      !query ||
      job.id.toLowerCase().includes(query) ||
      printerName(job.printer_id).toLowerCase().includes(query);
    const matchesFilter =
      filter === "all" ||
      (filter === "active" && ["queued", "printing"].includes(job.state)) ||
      (filter === "attention" && ["failed", "unknown"].includes(job.state)) ||
      job.state === filter;
    return matchesQuery && matchesFilter;
  });
}

function renderJobs() {
  if (appState.jobs.length === 0) {
    elements.jobsList.replaceChildren(
      emptyState(
        "≡",
        "No jobs yet",
        "Run the safe capture from Overview, or submit a PDF with the SDK.",
        "Go to safe capture",
        () => navigate("overview"),
      ),
    );
    return;
  }
  const jobs = filteredJobs();
  if (jobs.length === 0) {
    elements.jobsList.replaceChildren(
      emptyState(
        "⌕",
        "No jobs match these filters",
        "Clear the search and state filter to see the complete local queue.",
        "Clear filters",
        () => {
          elements.jobsSearch.value = "";
          elements.jobsFilter.value = "all";
          renderJobs();
        },
      ),
    );
    return;
  }
  const list = document.createElement("div");
  list.className = "job-list";
  for (const job of jobs) {
    list.append(jobRow(job));
  }
  elements.jobsList.replaceChildren(list);
}

function emptyState(icon, title, text, actionLabel, action) {
  const empty = document.createElement("div");
  empty.className = "empty-state";
  const content = document.createElement("div");
  const symbol = document.createElement("span");
  symbol.className = "empty-icon";
  symbol.setAttribute("aria-hidden", "true");
  symbol.textContent = icon;
  const heading = document.createElement("h3");
  heading.textContent = title;
  const paragraph = document.createElement("p");
  paragraph.textContent = text;
  const button = document.createElement("button");
  button.type = "button";
  button.className = "button-secondary";
  button.textContent = actionLabel;
  button.addEventListener("click", action);
  content.append(symbol, heading, paragraph, button);
  empty.append(content);
  return empty;
}

function renderAll() {
  updateSetup();
  renderPrinters();
  renderRecentJobs();
  renderJobs();
  elements.jobCount.textContent = String(appState.jobs.length);
  elements.jobCount.hidden = appState.jobs.length === 0;
}

function formatBytes(bytes) {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  return `${(bytes / 1024).toFixed(bytes > 1024 * 100 ? 0 : 1)} KiB`;
}

async function testPdf() {
  const response = await api("/app/test-page.pdf");
  return response.blob();
}

async function createJob(mode, printerId) {
  const pdf = await testPdf();
  const form = new FormData();
  form.set("file", pdf, "printlatch-test.pdf");
  form.set("mode", mode);
  form.set("printer_id", printerId);
  form.set("copies", "1");
  const response = await api("/v1/jobs", { method: "POST", body: form });
  const body = await response.json();
  return body.job;
}

async function createPreview() {
  setButtonLoading(elements.createPreview, true, "Validating PDF");
  try {
    const job = await createJob("preview", "capture:pdf");
    appState.previewJob = job;
    appState.jobs.unshift(job);
    await loadPreviewDocument(job);
    renderAll();
    toast("Preview ready. Nothing has been printed.", "success");
    announce("Test PDF preview ready. Nothing has been printed.");
  } catch (error) {
    toast(error.message, "error");
    announce(`Preview failed: ${error.message}`);
  } finally {
    setButtonLoading(elements.createPreview, false, "");
  }
}

async function loadPreviewDocument(job) {
  try {
    const response = await api(`/v1/jobs/${job.id}/document`);
    const blob = await response.blob();
    if (appState.previewUrl) {
      URL.revokeObjectURL(appState.previewUrl);
    }
    appState.previewUrl = URL.createObjectURL(blob);
    elements.previewMeta.textContent = `${job.page_count} page${job.page_count === 1 ? "" : "s"} · ${formatBytes(job.byte_count)} · validated`;
    elements.testStage.hidden = true;
    elements.previewStage.hidden = false;
  } catch (error) {
    toast(`Could not load preview: ${error.message}`, "error");
  }
}

function prepareCapture() {
  appState.selectedTarget =
    appState.printers.find((printer) => printer.id === "capture:pdf") || null;
  elements.confirmTitle.textContent = "Create the local PDF capture?";
  elements.confirmCopy.textContent =
    "This writes one copy of the built-in test page to the PrintLatch captures directory. It does not contact a physical printer.";
  elements.confirmTarget.textContent = "PrintLatch PDF Capture";
  elements.runConfirmedJob.textContent = "Create capture";
  elements.confirmDialog.showModal();
}

async function preparePhysicalTest(printer) {
  appState.selectedTarget = printer;
  if (!appState.previewJob) {
    await createPreview();
  }
  if (!appState.previewJob) {
    return;
  }
  elements.confirmTitle.textContent = "Submit the test page to Windows?";
  elements.confirmCopy.textContent =
    "This sends one copy of the built-in test page to the Windows print pipeline. PrintLatch can verify submission state, but not physical paper output.";
  elements.confirmTarget.textContent = printer.name;
  elements.runConfirmedJob.textContent = "Submit test page";
  elements.confirmDialog.showModal();
}

async function runConfirmedJob(event) {
  event.preventDefault();
  const target = appState.selectedTarget;
  if (!target) {
    return;
  }
  elements.confirmDialog.close();
  setButtonLoading(elements.confirmCapture, true, "Submitting");
  try {
    const job = await createJob("print", target.id);
    appState.jobs.unshift(job);
    localStorage.setItem(ACTIVE_JOB_KEY, job.id);
    renderAll();
    navigate("jobs");
    announce(`Job queued for ${target.name}`);
    toast(`Queued for ${target.name}.`, "success");
    watchJob(job.id);
  } catch (error) {
    toast(error.message, "error");
    announce(`Job submission failed: ${error.message}`);
  } finally {
    setButtonLoading(elements.confirmCapture, false, "");
  }
}

async function watchJob(id) {
  if (appState.pollingTimers.has(id)) {
    return;
  }
  appState.pollingTimers.set(id, null);
  let previousState = appState.jobs.find((job) => job.id === id)?.state;
  const poll = async () => {
    try {
      const response = await api(`/v1/jobs/${id}`);
      const body = await response.json();
      const index = appState.jobs.findIndex((job) => job.id === id);
      if (index >= 0) {
        appState.jobs[index] = body.job;
      } else {
        appState.jobs.unshift(body.job);
      }
      renderAll();
      if (body.job.state !== previousState) {
        announce(`Job ${id.slice(0, 8)} is now ${formatState(body.job.state)}`);
        previousState = body.job.state;
      }
      if (["queued", "printing"].includes(body.job.state)) {
        appState.pollingTimers.set(id, window.setTimeout(poll, 700));
      } else {
        appState.pollingTimers.delete(id);
        if (localStorage.getItem(ACTIVE_JOB_KEY) === id) {
          localStorage.removeItem(ACTIVE_JOB_KEY);
        }
        if (body.job.state === "succeeded") {
          toast(
            body.job.printer_id === "capture:pdf"
              ? "Verified PDF capture written."
              : "Windows accepted the test-page submission. Check physical output separately.",
            "success",
          );
          updateSetup();
          renderPrinters();
        } else if (["failed", "unknown"].includes(body.job.state)) {
          toast(jobDiagnosis(body.job).title, "error");
        }
        watchActiveJobs();
      }
    } catch (error) {
      appState.pollingTimers.delete(id);
      toast(`Could not refresh job: ${error.message}`, "error");
    }
  };
  await poll();
}

function watchActiveJobs() {
  for (const id of activeJobIds(appState.jobs, localStorage.getItem(ACTIVE_JOB_KEY))) {
    watchJob(id);
  }
}

async function cancelJob(job) {
  try {
    const response = await api(`/v1/jobs/${job.id}/cancel`, { method: "POST" });
    const body = await response.json();
    replaceJob(body.job);
    toast("Queued job canceled.", "success");
    announce(`Job ${job.id.slice(0, 8)} canceled`);
  } catch (error) {
    toast(error.message, "error");
  }
}

async function retryJob(job) {
  if (
    job.state === "unknown" &&
    !window.confirm(
      "PrintLatch cannot know whether Windows accepted this job before the restart. Check the Windows queue and physical output first. Retry this same job now?",
    )
  ) {
    return;
  }
  try {
    const response = await api(`/v1/jobs/${job.id}/retry`, { method: "POST" });
    const body = await response.json();
    replaceJob(body.job);
    toast("Same job queued for an explicit retry.", "success");
    announce(`Job ${job.id.slice(0, 8)} queued for retry`);
    watchJob(job.id);
  } catch (error) {
    toast(error.message, "error");
  }
}

function replaceJob(job) {
  const index = appState.jobs.findIndex((candidate) => candidate.id === job.id);
  if (index >= 0) {
    appState.jobs[index] = job;
  } else {
    appState.jobs.unshift(job);
  }
  renderAll();
}

async function openJobDocument(job) {
  try {
    const response = await api(`/v1/jobs/${job.id}/document`);
    const blob = await response.blob();
    const url = URL.createObjectURL(blob);
    window.open(url, "_blank", "noopener,noreferrer");
    window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
  } catch (error) {
    toast(error.message, "error");
  }
}

function navigate(view) {
  const target = VIEW_TITLES[view] ? view : "overview";
  for (const section of document.querySelectorAll("[data-view]")) {
    section.hidden = section.dataset.view !== target;
  }
  for (const link of document.querySelectorAll("[data-nav]")) {
    if (link.dataset.nav === target) {
      link.setAttribute("aria-current", "page");
    } else {
      link.removeAttribute("aria-current");
    }
  }
  const [eyebrow, title] = VIEW_TITLES[target];
  document.querySelector("#page-eyebrow").textContent = eyebrow;
  document.querySelector("#page-title").textContent = title;
  history.replaceState(null, "", `#${target}`);
  elements.sidebar.dataset.open = "false";
  elements.mobileMenu.setAttribute("aria-expanded", "false");
  document.querySelector("#main-content").focus({ preventScroll: true });
}

async function copyCode(button) {
  const id = button.dataset.copy;
  const content = document.querySelector(`#${CSS.escape(id)}`).textContent;
  try {
    await navigator.clipboard.writeText(content);
    const previous = button.textContent;
    button.textContent = "Copied";
    toast("Copied to clipboard.", "success");
    window.setTimeout(() => {
      button.textContent = previous;
    }, 1600);
  } catch {
    toast("Clipboard access was blocked. Select the command and copy it manually.", "error");
  }
}

function bindEvents() {
  elements.createPreview.addEventListener("click", createPreview);
  elements.confirmCapture.addEventListener("click", prepareCapture);
  elements.runConfirmedJob.addEventListener("click", runConfirmedJob);
  elements.openPreview.addEventListener("click", () => {
    if (appState.previewUrl) {
      window.open(appState.previewUrl, "_blank", "noopener,noreferrer");
    }
  });
  elements.refreshButton.addEventListener("click", () => refreshAll({ announceResult: true }));
  document
    .querySelector("#detect-printers")
    .addEventListener("click", () => refreshAll({ announceResult: true }));
  document
    .querySelector("#retry-connection")
    .addEventListener("click", () => refreshAll({ announceResult: true }));
  elements.pairingForm.addEventListener("submit", async (event) => {
    event.preventDefault();
    elements.pairingError.textContent = "";
    const button = elements.pairingForm.querySelector("button");
    setButtonLoading(button, true, "Connecting");
    try {
      await pair(elements.pairingCode.value);
      elements.pairingCode.value = "";
      await refreshAll();
      hideConnectionOverlay();
      toast("Dashboard connected.", "success");
      announce("Dashboard connected to the local PrintLatch agent");
    } catch (error) {
      elements.pairingError.textContent = error.message;
    } finally {
      setButtonLoading(button, false, "");
    }
  });
  elements.jobsFilter.addEventListener("change", renderJobs);
  elements.jobsSearch.addEventListener("input", renderJobs);
  elements.mobileMenu.addEventListener("click", () => {
    const open = elements.sidebar.dataset.open !== "true";
    elements.sidebar.dataset.open = String(open);
    elements.mobileMenu.setAttribute("aria-expanded", String(open));
  });
  for (const link of document.querySelectorAll("[data-nav]")) {
    link.addEventListener("click", (event) => {
      event.preventDefault();
      navigate(link.dataset.nav);
    });
  }
  for (const button of document.querySelectorAll("[data-copy]")) {
    button.addEventListener("click", () => copyCode(button));
  }
  window.addEventListener("hashchange", async () => {
    const view = window.location.hash.slice(1);
    if (VIEW_TITLES[view]) {
      navigate(view);
    } else if (view.startsWith("code=")) {
      await pairFromFragment();
    }
  });
  window.addEventListener("beforeunload", () => {
    if (appState.previewUrl) {
      URL.revokeObjectURL(appState.previewUrl);
    }
  });
}

async function start() {
  bindEvents();
  const intendedView = window.location.hash.slice(1);
  const healthy = await checkHealth();
  if (!healthy) {
    return;
  }
  if (await pairFromFragment()) {
    navigate("overview");
    return;
  }
  navigate(VIEW_TITLES[intendedView] ? intendedView : "overview");
  if (!appState.token) {
    showConnectionOverlay("pair");
    return;
  }
  await refreshAll();
}

start().catch((error) => {
  showConnectionOverlay("offline");
  elements.connectionMessage.textContent = error.message;
});
