export type JobState =
  | "queued"
  | "printing"
  | "preview_ready"
  | "succeeded"
  | "failed"
  | "unknown"
  | "canceled";

export interface Printer {
  id: string;
  name: string;
  kind: "capture" | "windows_local" | "windows_remote";
  tested: boolean;
  detail: string;
}

export interface PrintJob {
  id: string;
  client_id: string;
  printer_id: string;
  state: JobState;
  mode: "preview" | "print";
  copies: number;
  page_count: number;
  byte_count: number;
  sha256: string;
  attempts: number;
  detail: string | null;
  created_at: number;
  updated_at: number;
}

export interface PairingResult {
  client_id: string;
  token: string;
  expires_at: number;
}

export interface PrintLatchOptions {
  token: string;
  baseUrl?: string;
  timeoutMs?: number;
  fetch?: typeof globalThis.fetch;
}

export interface CreateJobOptions {
  pdf: Blob;
  mode: "preview" | "print";
  printerId?: string;
  copies?: number;
  filename?: string;
}

export class PrintLatchError extends Error {
  readonly status: number;
  readonly code: string;

  constructor(status: number, code: string, message: string) {
    super(message);
    this.name = "PrintLatchError";
    this.status = status;
    this.code = code;
  }
}

export class PrintLatchClient {
  readonly baseUrl: string;
  readonly #token: string;
  readonly #timeoutMs: number;
  readonly #fetch: typeof globalThis.fetch;

  constructor(options: PrintLatchOptions) {
    if (!options.token.startsWith("pl_live_")) {
      throw new TypeError("PrintLatch token has an invalid format");
    }
    this.baseUrl = normalizeLoopbackUrl(options.baseUrl ?? "http://127.0.0.1:32191");
    this.#token = options.token;
    this.#timeoutMs = options.timeoutMs ?? 10_000;
    this.#fetch = options.fetch ?? globalThis.fetch;
    if (typeof this.#fetch !== "function") {
      throw new TypeError("A Fetch API implementation is required");
    }
  }

  static async pair(
    code: string,
    options: Omit<PrintLatchOptions, "token"> = {},
  ): Promise<PairingResult> {
    const baseUrl = normalizeLoopbackUrl(options.baseUrl ?? "http://127.0.0.1:32191");
    const fetchImplementation = options.fetch ?? globalThis.fetch;
    if (typeof fetchImplementation !== "function") {
      throw new TypeError("A Fetch API implementation is required");
    }
    const response = await timedFetch(
      fetchImplementation,
      `${baseUrl}/v1/pair`,
      {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ code }),
      },
      options.timeoutMs ?? 10_000,
    );
    return parseJson<PairingResult>(response);
  }

  async printers(): Promise<Printer[]> {
    const response = await this.#request("/v1/printers");
    const payload = await parseJson<{ printers: Printer[] }>(response);
    return payload.printers;
  }

  async createJob(options: CreateJobOptions): Promise<PrintJob> {
    if (options.pdf.type !== "application/pdf") {
      throw new TypeError("pdf Blob type must be application/pdf");
    }
    const form = new FormData();
    form.set("mode", options.mode);
    form.set("copies", String(options.copies ?? 1));
    if (options.printerId !== undefined) {
      form.set("printer_id", options.printerId);
    }
    form.set("file", options.pdf, options.filename ?? "document.pdf");
    const response = await this.#request("/v1/jobs", {
      method: "POST",
      body: form,
    });
    const payload = await parseJson<{ job: PrintJob }>(response);
    return payload.job;
  }

  async jobs(limit = 25): Promise<PrintJob[]> {
    const safeLimit = Math.max(1, Math.min(100, Math.trunc(limit)));
    const response = await this.#request(`/v1/jobs?limit=${safeLimit}`);
    const payload = await parseJson<{ jobs: PrintJob[] }>(response);
    return payload.jobs;
  }

  async job(id: string): Promise<PrintJob> {
    const response = await this.#request(`/v1/jobs/${encodeURIComponent(id)}`);
    const payload = await parseJson<{ job: PrintJob }>(response);
    return payload.job;
  }

  async document(id: string): Promise<Blob> {
    const response = await this.#request(`/v1/jobs/${encodeURIComponent(id)}/document`);
    return response.blob();
  }

  async cancel(id: string): Promise<PrintJob> {
    return this.#transition(id, "cancel");
  }

  async retry(id: string): Promise<PrintJob> {
    return this.#transition(id, "retry");
  }

  async #transition(id: string, transition: "cancel" | "retry"): Promise<PrintJob> {
    const response = await this.#request(`/v1/jobs/${encodeURIComponent(id)}/${transition}`, {
      method: "POST",
    });
    const payload = await parseJson<{ job: PrintJob }>(response);
    return payload.job;
  }

  async #request(path: string, init: RequestInit = {}): Promise<Response> {
    const headers = new Headers(init.headers);
    headers.set("Authorization", `Bearer ${this.#token}`);
    return timedFetch(
      this.#fetch,
      `${this.baseUrl}${path}`,
      {
        ...init,
        headers,
      },
      this.#timeoutMs,
    );
  }
}

function normalizeLoopbackUrl(value: string): string {
  const url = new URL(value);
  const loopbackHosts = new Set(["127.0.0.1", "localhost", "[::1]"]);
  if (
    url.protocol !== "http:" ||
    !loopbackHosts.has(url.hostname) ||
    url.username !== "" ||
    url.password !== "" ||
    url.pathname !== "/" ||
    url.search !== "" ||
    url.hash !== ""
  ) {
    throw new TypeError("PrintLatch baseUrl must be an HTTP loopback origin");
  }
  return url.origin;
}

async function timedFetch(
  fetchImplementation: typeof globalThis.fetch,
  input: string,
  init: RequestInit,
  timeoutMs: number,
): Promise<Response> {
  const controller = new AbortController();
  const timeout = setTimeout(() => controller.abort(), timeoutMs);
  try {
    return await fetchImplementation(input, { ...init, signal: controller.signal });
  } finally {
    clearTimeout(timeout);
  }
}

async function parseJson<T>(response: Response): Promise<T> {
  const body = (await response.json()) as unknown;
  if (!response.ok) {
    const error = readError(body);
    throw new PrintLatchError(response.status, error.code, error.message);
  }
  return body as T;
}

function readError(value: unknown): { code: string; message: string } {
  if (
    typeof value === "object" &&
    value !== null &&
    "error" in value &&
    typeof value.error === "object" &&
    value.error !== null &&
    "code" in value.error &&
    "message" in value.error &&
    typeof value.error.code === "string" &&
    typeof value.error.message === "string"
  ) {
    return { code: value.error.code, message: value.error.message };
  }
  return { code: "unexpected_response", message: "PrintLatch returned an unexpected response" };
}
