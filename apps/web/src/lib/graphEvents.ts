const GRAPH_DIRTY_EVENT = "starlane:graph-dirty";
const GRAPH_DIRTY_STORAGE_KEY = "starlane:graph-dirty";

export interface GraphDirtyDetail {
  spaceId: string;
  reason:
    | "document-created"
    | "document-deleted"
    | "document-content-updated"
    | "document-metadata-updated";
  timestamp: number;
}

function getWindowSafe(): Window | null {
  return typeof window === "undefined" ? null : window;
}

export function emitGraphDirty(spaceId: string, reason: GraphDirtyDetail["reason"]) {
  const win = getWindowSafe();
  if (!win) return;

  const detail: GraphDirtyDetail = {
    spaceId,
    reason,
    timestamp: Date.now(),
  };

  win.dispatchEvent(new CustomEvent<GraphDirtyDetail>(GRAPH_DIRTY_EVENT, { detail }));

  // Cross-tab signal.
  try {
    win.localStorage.setItem(GRAPH_DIRTY_STORAGE_KEY, JSON.stringify(detail));
  } catch {
    // best effort
  }
}

export function addGraphDirtyListener(
  handler: (detail: GraphDirtyDetail) => void,
): () => void {
  const win = getWindowSafe();
  if (!win) return () => {};

  const onCustomEvent = (event: Event) => {
    const custom = event as CustomEvent<GraphDirtyDetail>;
    if (custom.detail) handler(custom.detail);
  };

  const onStorage = (event: StorageEvent) => {
    if (event.key !== GRAPH_DIRTY_STORAGE_KEY || !event.newValue) return;
    try {
      const parsed = JSON.parse(event.newValue) as GraphDirtyDetail;
      if (parsed?.spaceId) handler(parsed);
    } catch {
      // ignore malformed payload
    }
  };

  win.addEventListener(GRAPH_DIRTY_EVENT, onCustomEvent as EventListener);
  win.addEventListener("storage", onStorage);

  return () => {
    win.removeEventListener(GRAPH_DIRTY_EVENT, onCustomEvent as EventListener);
    win.removeEventListener("storage", onStorage);
  };
}
