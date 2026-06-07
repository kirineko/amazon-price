export type RowStatus =
  | "pending"
  | "success"
  | "unavailable"
  | "noPrice"
  | "mismatch"
  | "formatError"
  | "failed";

export interface RowResult {
  sku: string;
  dpCode: string;
  asin: string;
  amazonUrl: string;
  priceText?: string | null;
  priceValue?: number | null;
  currency: string;
  status: RowStatus;
  error?: string | null;
  fetchedAt?: string | null;
}

export type ProxyMode = "auto" | "manual" | "off";

export interface ProxyConfig {
  mode: ProxyMode;
  url?: string | null;
  username?: string | null;
  password?: string | null;
}

export interface ScrapeOptions {
  requestIntervalMs: number;
}

export interface ScrapeProgress {
  done: number;
  total: number;
  row: RowResult;
}

export interface ParseSkusResult {
  rows: RowResult[];
  duplicateCount: number;
  invalidCount: number;
  validCount: number;
}

export interface SessionStatus {
  initialized: boolean;
  zipCode: string;
  deliveryLocation?: string | null;
  message: string;
}

export interface SelfCheckResult {
  ok: boolean;
  asin: string;
  priceText?: string | null;
  currency?: string | null;
  message: string;
}

export const STATUS_LABELS: Record<RowStatus, string> = {
  pending: "待处理",
  success: "成功",
  unavailable: "不可售",
  noPrice: "无价",
  mismatch: "疑似不匹配",
  formatError: "格式错误",
  failed: "失败",
};

export const STATUS_COLORS: Record<RowStatus, string> = {
  pending: "default",
  success: "success",
  unavailable: "warning",
  noPrice: "warning",
  mismatch: "warning",
  formatError: "error",
  failed: "error",
};
