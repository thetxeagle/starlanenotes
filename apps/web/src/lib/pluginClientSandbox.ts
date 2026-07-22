import {
  guardPluginRuntimeAccess,
  type PluginInstallRecord,
  type PluginRuntimeAccessDecision,
} from "./pluginInstallRecords";

export interface PluginClientSandboxStartRequest {
  record: PluginInstallRecord;
  spaceId?: string;
  /** Verified plugin source supplied by a future installer, never a remote URL. */
  source: string;
}

export type PluginClientSandboxStartResult =
  | { started: true; iframe: HTMLIFrameElement; stop: () => void }
  | { started: false; decision: PluginRuntimeAccessDecision };

/**
 * Starts untrusted client code in an opaque-origin iframe. The iframe receives
 * no same-origin, form, popup, navigation, or top-level escape privileges.
 * Plugin code gets only a frozen message API; product APIs stay in the host.
 */
export function startPluginClientSandbox(
  request: PluginClientSandboxStartRequest,
  documentRef: Document = document,
): PluginClientSandboxStartResult {
  const decision = guardPluginRuntimeAccess(
    request.record,
    { kind: "client", permission: "editor_commands" },
    request.spaceId,
    { runtimeAvailable: true },
  );
  if (!decision.allowed) return { started: false, decision };

  const iframe = documentRef.createElement("iframe");
  iframe.setAttribute("sandbox", "allow-scripts");
  iframe.setAttribute("referrerpolicy", "no-referrer");
  iframe.setAttribute("aria-hidden", "true");
  iframe.style.display = "none";
  const bootstrap = `<!doctype html><meta charset="utf-8"><script>
    "use strict";
    const api=Object.freeze({post:(message)=>parent.postMessage({type:"starlane-plugin",message},"*")});
    window.addEventListener("message",(event)=>{if(event.data&&event.data.type==="starlane-plugin-start"){try{new Function("api",event.data.source)(api)}catch(error){api.post({type:"error",message:String(error)})}}});
  <\/script>`;
  iframe.srcdoc = bootstrap;
  documentRef.body.append(iframe);
  const onLoad = () => iframe.contentWindow?.postMessage(
    { type: "starlane-plugin-start", source: request.source },
    "*",
  );
  iframe.addEventListener("load", onLoad, { once: true });
  return { started: true, iframe, stop: () => iframe.remove() };
}
