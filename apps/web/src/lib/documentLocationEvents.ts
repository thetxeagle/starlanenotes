export const DOCUMENT_LOCATION_CHANGED_EVENT = "starlane:document-location-changed";

export interface DocumentLocationChangedDetail {
  spaceId: string;
  documentId: string;
  folderId: string | null;
  parentId: string | null;
}

export function emitDocumentLocationChanged(detail: DocumentLocationChangedDetail): void {
  if (typeof window === "undefined") return;
  window.dispatchEvent(
    new CustomEvent<DocumentLocationChangedDetail>(DOCUMENT_LOCATION_CHANGED_EVENT, { detail }),
  );
}

export function addDocumentLocationChangedListener(
  handler: (detail: DocumentLocationChangedDetail) => void,
): () => void {
  if (typeof window === "undefined") return () => {};
  const onLocationChanged = (event: Event) => {
    const detail = (event as CustomEvent<DocumentLocationChangedDetail>).detail;
    if (detail) handler(detail);
  };
  window.addEventListener(DOCUMENT_LOCATION_CHANGED_EVENT, onLocationChanged as EventListener);
  return () =>
    window.removeEventListener(DOCUMENT_LOCATION_CHANGED_EVENT, onLocationChanged as EventListener);
}
