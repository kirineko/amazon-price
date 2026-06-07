export type RowStatus =
  | "Pending"
  | "Success"
  | "Unavailable"
  | "NoPrice"
  | "Mismatch"
  | "FormatError"
  | "Failed";

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

export interface ScrapeOptions {
  ratePerSec: number;
  concurrency: number;
}

export interface ScrapeProgress {
  done: number;
  total: number;
  row: RowResult;
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
  message: string;
}

export const STATUS_LABELS: Record<RowStatus, string> = {
  Pending: "待处理",
  Success: "成功",
  Unavailable: "不可售",
  NoPrice: "无价",
  Mismatch: "疑似不匹配",
  FormatError: "格式错误",
  Failed: "失败",
};

export const STATUS_COLORS: Record<RowStatus, string> = {
  Pending: "default",
  Success: "success",
  Unavailable: "warning",
  NoPrice: "warning",
  Mismatch: "warning",
  FormatError: "error",
  Failed: "error",
};
