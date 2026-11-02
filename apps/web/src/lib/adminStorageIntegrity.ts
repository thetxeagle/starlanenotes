import { api, getApiBase } from "./api";
import { getCurrentLaunchMode } from "./launchMode";
export interface StorageIntegrityIssue { kind: string; record_type: string; record_id: string | null; path: string; detail: string }
export interface StorageIntegrityReport { healthy: boolean; documents: number; drafts: number; attachments: number; issues: StorageIntegrityIssue[] }
export interface IntegrityObservation { report: StorageIntegrityReport; observedAt: string; isCurrent: () => boolean }
const failure = () => new Error("Integrity check is unavailable. Check your connection and administrator access, then retry.");
const record = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const text = (v: unknown): v is string => typeof v === "string" && v.length <= 16384;
const count = (v: unknown) => typeof v === "number" && Number.isSafeInteger(v) && v >= 0;
export function parseStorageIntegrity(value: unknown): StorageIntegrityReport {
  if (!record(value) || typeof value.healthy !== "boolean" || !count(value.documents) || !count(value.drafts) || !count(value.attachments) || !Array.isArray(value.issues) || value.issues.length > 10000) throw failure();
  const kinds = ["unsafe_path", "misplaced_file", "unsafe_symlink", "invalid_file_type", "missing_file", "unreadable_file", "orphan_file"];
  for (const issue of value.issues) {
    if (!record(issue) || !kinds.includes(String(issue.kind)) || !["document", "draft", "attachment"].includes(String(issue.record_type)) || !(issue.record_id === null || (text(issue.record_id) && issue.record_id.length > 0)) || !text(issue.path) || !text(issue.detail)) throw failure();
  }
  if (value.healthy !== (value.issues.length === 0)) throw failure();
  return value as unknown as StorageIntegrityReport;
}
/** No cache/refresh/replay; original authority and origin must survive all waits. */
export async function readStorageIntegrity(signal: AbortSignal): Promise<IntegrityObservation> {
  const base = getApiBase(), token = api.getToken(), refresh = api.getRefreshToken();
  const same = () => !signal.aborted && getCurrentLaunchMode() === "server" && getApiBase() === base && api.getToken() === token && localStorage.getItem("access_token") === token && api.getRefreshToken() === refresh;
  if (!token || !same()) throw failure();
  const response = await fetch(`${base}/admin/storage-integrity`, { headers: { Authorization: `Bearer ${token}` }, cache: "no-store", signal });
  if (!response.ok || !same()) throw failure();
  const reader = response.body?.getReader();
  if (!reader) throw failure();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let bytes = 0, raw = "";
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (!same()) throw failure();
      if (done) break;
      bytes += value.byteLength;
      if (bytes > 4 * 1024 * 1024) throw failure();
      raw += decoder.decode(value, { stream: true });
    }
    raw += decoder.decode();
  } catch (error) { await reader.cancel().catch(() => undefined); throw error; }
  finally { reader.releaseLock(); }
  const value: unknown = JSON.parse(raw);
  if (!same()) throw failure();
  return { report: parseStorageIntegrity(value), observedAt: new Date().toISOString(), isCurrent: same };
}
