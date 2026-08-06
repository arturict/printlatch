const allowedCtas = new Set([
  "download-release|final|releases",
  "download-release|hero|releases",
  "download-release|nav|releases",
  "read-docs|nav|docs",
  "read-threat-model|security|threat-model",
  "view-release|release-bar|releases",
  "view-source|hero|github",
]);
const allowedSections = new Set(["hero", "product", "security", "how", "scope", "sdk", "final"]);
const scrollThresholds = [25, 50, 75, 100];
const engagedThresholds = [30, 60, 120];
const utmKeys = ["utm_source", "utm_medium", "utm_campaign", "utm_term", "utm_content"];
const safeUtmValue = /^[A-Za-z0-9._~-]{1,64}$/u;

function privacySignalEnabled(navigatorLike) {
  return navigatorLike.doNotTrack === "1" || navigatorLike.globalPrivacyControl === true;
}

function sanitizeUrl(value, baseUrl) {
  try {
    const source = new URL(value, baseUrl);
    const safe = new URLSearchParams();
    for (const key of utmKeys) {
      const candidate = source.searchParams.get(key);
      if (candidate && safeUtmValue.test(candidate)) safe.set(key, candidate);
    }
    const query = safe.toString();
    return query ? `/?${query}` : "/";
  } catch {
    return "/";
  }
}

function sanitizeRecord(record, baseUrl) {
  const sanitized = { ...record };
  if (typeof sanitized.url === "string") sanitized.url = sanitizeUrl(sanitized.url, baseUrl);
  if (typeof sanitized.referrer === "string") {
    try {
      sanitized.referrer = new URL(sanitized.referrer, baseUrl).origin;
    } catch {
      sanitized.referrer = "";
    }
  }
  return sanitized;
}

export function sanitizeUmamiPayload(
  payload,
  navigatorLike = navigator,
  baseUrl = location.origin,
) {
  if (privacySignalEnabled(navigatorLike)) return false;
  const sanitized = sanitizeRecord(payload, baseUrl);
  if (
    sanitized.payload &&
    typeof sanitized.payload === "object" &&
    !Array.isArray(sanitized.payload)
  ) {
    sanitized.payload = sanitizeRecord(sanitized.payload, baseUrl);
  }
  return sanitized;
}

export function createLandingEventTracker(send) {
  const seen = new Set();
  const once = (key, name, data) => {
    if (seen.has(key)) return;
    seen.add(key);
    send(name, data);
  };

  return {
    cta(action, location, target) {
      if (allowedCtas.has(`${action}|${location}|${target}`)) {
        send("landing-cta", { action, location, target });
      }
    },
    section(section) {
      if (allowedSections.has(section)) {
        once(`section:${section}`, "landing-section-view", { section });
      }
    },
    scroll(percentage) {
      for (const depth of scrollThresholds) {
        if (percentage >= depth) {
          once(`scroll:${depth}`, "landing-scroll-depth", { depth });
        }
      }
    },
    engaged(seconds) {
      for (const threshold of engagedThresholds) {
        if (seconds >= threshold) {
          once(`engaged:${threshold}`, "landing-engaged-time", { seconds: threshold });
        }
      }
    },
  };
}

export function startLandingAnalytics({ websiteId, scriptUrl }) {
  if (!websiteId || !scriptUrl || privacySignalEnabled(navigator)) return () => undefined;

  const queued = [];
  const send = (name, data) => {
    if (privacySignalEnabled(navigator)) return;
    if (window.umami) window.umami.track(name, data);
    else if (queued.length < 32) queued.push([name, data]);
  };
  const tracker = createLandingEventTracker(send);

  let script = document.querySelector("script[data-printlatch-landing-analytics]");
  const flush = () => {
    if (!window.umami || privacySignalEnabled(navigator)) return;
    for (const [name, data] of queued.splice(0)) window.umami.track(name, data);
  };
  if (!script) {
    window.printlatchUmamiBeforeSend = (_type, payload) => sanitizeUmamiPayload(payload);
    script = document.createElement("script");
    script.defer = true;
    script.src = scriptUrl;
    script.dataset.websiteId = websiteId;
    script.dataset.doNotTrack = "true";
    script.dataset.domains = "printlatch.vercel.app";
    script.dataset.beforeSend = "printlatchUmamiBeforeSend";
    script.dataset.printlatchLandingAnalytics = "true";
    document.head.append(script);
  }
  script.addEventListener("load", flush);
  flush();

  const handleClick = (event) => {
    const element = event.target?.closest?.("[data-analytics-action]");
    if (!element) return;
    tracker.cta(
      element.dataset.analyticsAction ?? "",
      element.dataset.analyticsLocation ?? "",
      element.dataset.analyticsTarget ?? "",
    );
  };
  document.addEventListener("click", handleClick);

  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (entry.isIntersecting) {
          tracker.section(entry.target.dataset.analyticsSection ?? "");
        }
      }
    },
    { threshold: 0.35 },
  );
  document.querySelectorAll("[data-analytics-section]").forEach((section) => {
    observer.observe(section);
  });

  const handleScroll = () => {
    const available = document.documentElement.scrollHeight - window.innerHeight;
    tracker.scroll(available <= 0 ? 100 : (window.scrollY / available) * 100);
  };
  window.addEventListener("scroll", handleScroll, { passive: true });
  handleScroll();

  let engagedSeconds = 0;
  const interval = window.setInterval(() => {
    if (document.visibilityState === "visible") {
      engagedSeconds += 1;
      tracker.engaged(engagedSeconds);
    }
  }, 1000);

  return () => {
    script.removeEventListener("load", flush);
    document.removeEventListener("click", handleClick);
    window.removeEventListener("scroll", handleScroll);
    window.clearInterval(interval);
    observer.disconnect();
  };
}
