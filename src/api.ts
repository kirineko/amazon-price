import type {
  RowResult,
  ScrapeOptions,
  ScrapeProgress,
  SelfCheckResult,
  SessionStatus,
} from "./types";

class ApiError extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export class AuthError extends ApiError {
  constructor(message = "未认证，请先登录") {
    super(401, message);
  }
}

async function readError(res: Response): Promise<string> {
  try {
    const body = (await res.json()) as { error?: string };
    return body.error ?? res.statusText;
  } catch {
    return res.statusText;
  }
}

async function apiFetch<T>(
  path: string,
  init?: RequestInit,
): Promise<T> {
  const res = await fetch(`/api${path}`, {
    credentials: "include",
    headers: {
      "Content-Type": "application/json",
      ...(init?.headers ?? {}),
    },
    ...init,
  });

  if (res.status === 401) {
    throw new AuthError(await readError(res));
  }

  if (!res.ok) {
    throw new ApiError(res.status, await readError(res));
  }

  if (res.status === 204) {
    return undefined as T;
  }

  const contentType = res.headers.get("content-type") ?? "";
  if (contentType.includes("text/csv")) {
    return (await res.text()) as T;
  }

  return (await res.json()) as T;
}

export async function login(password: string): Promise<void> {
  await apiFetch<{ ok: boolean }>("/login", {
    method: "POST",
    body: JSON.stringify({ password }),
  });
}

export async function logout(): Promise<void> {
  await apiFetch<{ ok: boolean }>("/logout", { method: "POST" });
}

export async function checkAuth(): Promise<boolean> {
  const res = await fetch("/api/auth/status", { credentials: "include" });
  if (!res.ok) {
    return false;
  }
  const body = (await res.json()) as { authenticated: boolean };
  return body.authenticated;
}

export async function initSession(zipCode?: string): Promise<SessionStatus> {
  return apiFetch<SessionStatus>("/session", {
    method: "POST",
    body: JSON.stringify({ zipCode }),
  });
}

export async function parseSkus(text: string): Promise<[RowResult[], number]> {
  const body = await apiFetch<{ rows: RowResult[]; duplicateCount: number }>(
    "/skus/parse",
    {
      method: "POST",
      body: JSON.stringify({ text }),
    },
  );
  return [body.rows, body.duplicateCount];
}

export async function startScrape(
  rows: RowResult[],
  options?: ScrapeOptions,
): Promise<RowResult[]> {
  return apiFetch<RowResult[]>("/scrape", {
    method: "POST",
    body: JSON.stringify({ rows, options }),
  });
}

export async function refreshOne(
  row: RowResult,
  options?: ScrapeOptions,
): Promise<RowResult> {
  const rows = await apiFetch<RowResult[]>("/scrape/refresh", {
    method: "POST",
    body: JSON.stringify({ row, options }),
  });
  const updated = rows[0];
  if (!updated) {
    throw new Error("刷新失败");
  }
  return updated;
}

export async function refreshAll(options?: ScrapeOptions): Promise<RowResult[]> {
  return apiFetch<RowResult[]>("/scrape/refresh", {
    method: "POST",
    body: JSON.stringify({ options }),
  });
}

export async function exportCsv(rows: RowResult[]): Promise<string> {
  return apiFetch<string>("/export.csv", {
    method: "POST",
    body: JSON.stringify({ rows }),
  });
}

export async function cancelScrape(): Promise<void> {
  await apiFetch<{ ok: boolean }>("/scrape/cancel", { method: "POST" });
}

export async function runSelfCheck(zipCode?: string): Promise<SelfCheckResult> {
  const query = zipCode ? `?zipCode=${encodeURIComponent(zipCode)}` : "";
  return apiFetch<SelfCheckResult>(`/self-check${query}`);
}

export function listenScrapeProgress(
  handler: (progress: ScrapeProgress) => void,
): () => void {
  const source = new EventSource("/api/events", { withCredentials: true });

  source.onmessage = (event) => {
    try {
      handler(JSON.parse(event.data) as ScrapeProgress);
    } catch {
      // ignore malformed events
    }
  };

  return () => {
    source.close();
  };
}

export function downloadCsv(content: string, filename = "amazon-prices.csv") {
  const blob = new Blob([content], { type: "text/csv;charset=utf-8;" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  link.click();
  URL.revokeObjectURL(url);
}

export { ApiError };
