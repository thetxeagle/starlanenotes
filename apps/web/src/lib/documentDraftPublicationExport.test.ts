import { describe, expect, it } from "vitest";
import type { DraftPublicationIntent } from "./documentDraftPublication";
import { exportOriginalSavedDraft } from "./documentDraftPublicationExport";

const intent: DraftPublicationIntent = {
  schema_version: 1,
  operation_id: "00000000-0000-4000-8000-000000000001",
  space_id: "space", document_id: "page", draft_id: "draft",
  origin: { version: 1, serverBase: "https://private-host", accountId: "private-account", sessionId: "private-session", storageMode: "server", provider: "none", cacheScope: "private-cache" },
  request: { schema_version: 1, operation_id: "00000000-0000-4000-8000-000000000001", draft_source_digest: "a".repeat(64), page_source_digest: "b".repeat(64) },
  source: {
    schema_version: 1, space_id: "space", document_id: "page", draft_id: "draft",
    draft_source_digest: "a".repeat(64), page_source_digest: "b".repeat(64),
    page_published: true, page_updated_at: "reviewed-page-stamp",
    draft: { title: "  Exact original title 🦅  ", content: '---\nsynopsis: "Portable"\n---\n\n<!-- starlane:template-guidance -->\nOriginal unsimplified body\r\n', icon: "  icon  ", cover_image: "  cover  " },
  },
  expectedPublication: { title: "Different published title", content: "Stripped published body", icon: null, cover_image: null },
};

describe("original saved draft export", () => {
  it("preserves every exact saved field and original route/request/digest without substituting the published projection", () => {
    const before = JSON.stringify(intent);
    const exported = exportOriginalSavedDraft(intent);
    expect(exported.source.draft).toEqual(intent.source.draft);
    expect(exported.source.draft).not.toBe(intent.source.draft);
    expect(exported.route).toEqual({ space_id: "space", document_id: "page", draft_id: "draft" });
    expect(exported.request).toEqual(intent.request);
    expect(exported.source.page_updated_at).toBe("reviewed-page-stamp");
    expect(exported.kind).toBe("device_retained_publication_source");
    expect(exported.notice).toContain("not a publication receipt");
    expect(JSON.stringify(intent)).toBe(before);
  });

  it("omits origin, identity, credentials, receipt and unexpected nested properties using an explicit allowlist", () => {
    const contaminated = structuredClone(intent);
    Object.assign(contaminated, { bearer: "TOP_SECRET", receipt: { disposition: "applied" } });
    Object.assign(contaminated.request, { token: "REQUEST_SECRET" });
    Object.assign(contaminated.source, { bearer: "SOURCE_SECRET" });
    Object.assign(contaminated.source.draft, { access_token: "DRAFT_SECRET" });
    const text = JSON.stringify(exportOriginalSavedDraft(contaminated));
    for (const secret of ["private-host", "private-account", "private-session", "private-cache", "TOP_SECRET", "REQUEST_SECRET", "SOURCE_SECRET", "DRAFT_SECRET"]) expect(text).not.toContain(secret);
    expect(JSON.parse(text)).not.toHaveProperty("origin");
    expect(JSON.parse(text)).not.toHaveProperty("receipt");
  });

  it("preserves explicit empty and null presentation values", () => {
    const source = structuredClone(intent);
    source.source.draft = { title: "", content: "", icon: null, cover_image: "" };
    expect(exportOriginalSavedDraft(source).source.draft).toEqual(source.source.draft);
  });
});
