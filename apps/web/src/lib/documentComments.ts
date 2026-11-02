import { api } from "@/lib/api";

export interface DocumentComment {
  id: string;
  document_id: string;
  parent_id?: string;
  user_id: string;
  user_name: string;
  content: string;
  selection_start?: number;
  selection_end?: number;
  selection_text?: string;
  resolved: boolean;
  resolved_by?: string;
  created_at: string;
  updated_at?: string;
  replies: DocumentComment[];
}

export const DOCUMENT_COMMENTS_CHANGED_EVENT = "starlane:document-comments-changed";

export function emitDocumentCommentsChanged(spaceId: string, docId: string): void {
  window.dispatchEvent(
    new CustomEvent(DOCUMENT_COMMENTS_CHANGED_EVENT, {
      detail: { spaceId, docId },
    }),
  );
}

export function subscribeToDocumentComments(
  spaceId: string,
  docId: string,
  listener: () => void,
): () => void {
  const handleChange = (event: Event) => {
    const detail = (event as CustomEvent<{ spaceId?: string; docId?: string }>).detail;
    if (detail?.spaceId === spaceId && detail.docId === docId) listener();
  };
  window.addEventListener(DOCUMENT_COMMENTS_CHANGED_EVENT, handleChange);
  return () => window.removeEventListener(DOCUMENT_COMMENTS_CHANGED_EVENT, handleChange);
}

export function listDocumentComments(
  spaceId: string,
  docId: string,
): Promise<DocumentComment[]> {
  return api.fetch<DocumentComment[]>(`/spaces/${spaceId}/documents/${docId}/comments`);
}
