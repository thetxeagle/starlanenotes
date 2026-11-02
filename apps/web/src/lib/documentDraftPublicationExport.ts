import type { DraftPublicationIntent } from "./documentDraftPublication";

/** A portable device copy, never publication success or authorization evidence. */
export function exportOriginalSavedDraft(intent: DraftPublicationIntent) {
  return {
    schema_version: 1,
    kind: "device_retained_publication_source",
    notice: "Original saved draft retained on this device. This file is not a publication receipt.",
    route: {
      space_id: intent.space_id,
      document_id: intent.document_id,
      draft_id: intent.draft_id,
    },
    request: {
      schema_version: intent.request.schema_version,
      operation_id: intent.request.operation_id,
      draft_source_digest: intent.request.draft_source_digest,
      page_source_digest: intent.request.page_source_digest,
    },
    source: {
      draft_source_digest: intent.source.draft_source_digest,
      page_source_digest: intent.source.page_source_digest,
      page_published: intent.source.page_published,
      page_updated_at: intent.source.page_updated_at,
      draft: {
        title: intent.source.draft.title,
        content: intent.source.draft.content,
        icon: intent.source.draft.icon,
        cover_image: intent.source.draft.cover_image,
      },
    },
  };
}
