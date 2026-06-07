import { invoke } from "@tauri-apps/api/core";
import { listen, UnlistenFn } from "@tauri-apps/api/event";
import type {
  RowResult,
  ScrapeOptions,
  ScrapeProgress,
  SelfCheckResult,
  SessionStatus,
} from "./types";

export async function initSession(zipCode?: string): Promise<SessionStatus> {
  return invoke("init_session", { zipCode });
}

export async function parseSkus(text: string): Promise<[RowResult[], number]> {
  return invoke("parse_skus", { text });
}

export async function parseSkusFile(path: string): Promise<[RowResult[], number]> {
  return invoke("parse_skus_file", { path });
}

export async function startScrape(
  rows: RowResult[],
  options?: ScrapeOptions,
): Promise<RowResult[]> {
  return invoke("start_scrape", { rows, options });
}

export async function refreshOne(
  row: RowResult,
  options?: ScrapeOptions,
): Promise<RowResult> {
  return invoke("refresh_one", { row, options });
}

export async function refreshAll(options?: ScrapeOptions): Promise<RowResult[]> {
  return invoke("refresh_all", { options });
}

export async function exportCsv(rows: RowResult[]): Promise<string> {
  return invoke("export_csv", { rows });
}

export async function cancelScrape(): Promise<void> {
  return invoke("cancel_scrape");
}

export async function runSelfCheck(zipCode?: string): Promise<SelfCheckResult> {
  return invoke("run_self_check", { zipCode });
}

export function listenScrapeProgress(
  handler: (progress: ScrapeProgress) => void,
): Promise<UnlistenFn> {
  return listen<ScrapeProgress>("scrape-progress", (event) => {
    handler(event.payload);
  });
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
