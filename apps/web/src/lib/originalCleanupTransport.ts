import { api, captureOfflineServerContext, getApiBase } from "./api";
import { sameOfflineReplayOrigin, verifyOfflineReplayContext, type OfflineReplayContext } from "./offlineReplayOrigin";
import { acknowledgeCompletedCleanup, readCleanupAttempts, cleanupAttemptRequest, parseCleanupResult, type CleanupAttempt, type CleanupResult } from "./permanentDocumentCleanup";

/** One bounded original request. The caller keeps the durable receipt on every failure. */
export async function fetchOriginalCleanup(context: OfflineReplayContext, attempt: CleanupAttempt): Promise<CleanupResult> {
  const original = structuredClone(attempt);
  const request = cleanupAttemptRequest(original);
  const verify = () => {
    if (!sameOfflineReplayOrigin(context.origin, original.origin)) throw new Error("Original cleanup session differs; retain the receipt");
    verifyOfflineReplayContext(context, getApiBase(), api.getToken());
  };
  verify();
  const response = await fetch(`${context.origin.serverBase}${request.path}`, {
    method: "POST", headers: { Authorization: `Bearer ${context.bearer}`, "Content-Type": "application/json" }, body: JSON.stringify(request.body),
  });
  verify();
  if (response.status !== 200 || !response.body) throw new Error("Original cleanup acknowledgment unavailable; retain the receipt");
  const advertised = response.headers.get("Content-Length");
  if (advertised !== null && (!/^\d+$/.test(advertised) || Number(advertised) > 65536)) throw new Error("Original cleanup response exceeds bound; retain the receipt");
  const reader = response.body.getReader();
  const parts: Uint8Array[] = [];
  let length = 0;
  try {
    for (;;) {
      verify();
      const next = await reader.read();
      verify();
      if (next.done) break;
      length += next.value.byteLength;
      if (length > 65536) throw new Error("Original cleanup response exceeds bound; retain the receipt");
      parts.push(next.value.slice());
    }
    const bytes = new Uint8Array(length);
    let offset = 0;
    for (const part of parts) { bytes.set(part, offset); offset += part.byteLength; }
    const result = parseCleanupResult(JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)), original.operation);
    verify();
    return result;
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
    verify();
  }
}

/** An explicit selected retry; never looks up a replacement page or allocates identity. */
export async function reconcileAcknowledgedCleanup(attempt:CleanupAttempt):Promise<CleanupResult> {
 const original=structuredClone(attempt);cleanupAttemptRequest(original);
 if(!navigator.locks?.request)throw new Error("Original cleanup requires cross-window serialization");
 return navigator.locks.request(`starlane-original-delete:${original.origin.serverBase}:${original.space}:${original.document}`,async()=>{
  const context=captureOfflineServerContext();
  const verify=()=>{if(!sameOfflineReplayOrigin(context.origin,original.origin))throw new Error("Original cleanup session differs; retain the receipt");verifyOfflineReplayContext(context,getApiBase(),api.getToken());};
  verify();if(JSON.stringify(readCleanupAttempts().find(row=>row.operation===original.operation))!==JSON.stringify(original))throw new Error("Original selected cleanup receipt changed or is absent; no new request may be created");
  const result=await fetchOriginalCleanup(context,original);verify();
  if(result.outcome==="physically_completed")await acknowledgeCompletedCleanup(original,result,verify);
  verify();return result;
 });
}
