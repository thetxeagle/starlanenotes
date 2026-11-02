import { renderToStaticMarkup } from "react-dom/server";
import { describe, it, expect } from "vitest";
import { PagePreservedMetadata, PreservedMetadataEvidence } from "./PagePreservedMetadata";
import type { ServerMovePreservedEvidence } from "@starlane-notes/shared-types";
const evidence: ServerMovePreservedEvidence = {
  space_id: "s",
  document_id: "p",
  operation_id: "op",
  kind: "immutable_metadata_evidence",
  editable: false,
  restoration_available: false,
  entries: [
    {
      family: "workflow",
      source_id: "w",
      disposition: "remove",
      original: {
        title: "<script>never execute</script>",
        history: ["Preserved historical transition"],
      },
      expected_active: null,
    },
  ],
};
describe("Preserved metadata display", () => {
  it("escapes original evidence and distinguishes expected historical values from current metadata", () => {
    const html = renderToStaticMarkup(<PreservedMetadataEvidence evidence={evidence} />);
    expect(html).toContain("Historical move metadata");
    expect(html).toContain("restoration is unavailable");
    expect(html).toContain("Expected active at move time");
    expect(html).toContain("&lt;script&gt;");
    expect(html).not.toContain("<script>");
    expect(html).not.toContain("<button");
    expect(html).not.toContain("href=");
  });
  it("keeps disclosure collapsed and no operation/body request implied before explicit opening", () => {
    const html = renderToStaticMarkup(<PagePreservedMetadata spaceId="s" documentId="p" />);
    expect(html).toContain('aria-expanded="false"');
    expect(html).not.toContain("Move operation");
    expect(html).not.toContain("Restore");
  });
  it("renders an empty bound operation without manufacturing missing metadata", () => {
    expect(
      renderToStaticMarkup(<PreservedMetadataEvidence evidence={{ ...evidence, entries: [] }} />),
    ).toContain("No preserved entries are available");
  });
});

describe("dedicated viewer grant historical projection", () => {
  const original = { grant_id: "grant", document_id: "p", space_id: "source", revision: "r1", role: "viewer", scope: "page", recipient: { available: true, display_name: "<script>recipient</script>", email: "SECRET_EMAIL" }, user_id: "SECRET_USER", created_by: "SECRET_CREATOR", body: "SECRET_BODY", token: "SECRET_TOKEN" };
  it("escapes entitled labels while excluding every sealed/private field even for direct renderer callers", () => {
    const html = renderToStaticMarkup(<PreservedMetadataEvidence evidence={{ ...evidence, entries: [{ family: "page_viewer_grant", source_id: "grant", disposition: "preserve", original, expected_active: { ...original, space_id: "s", revision: "r2" } }] }} />);
    expect(html).toContain("page viewer grant"); expect(html).toContain("&lt;script&gt;recipient&lt;/script&gt;");
    for (const secret of ["SECRET_EMAIL", "SECRET_USER", "SECRET_CREATOR", "SECRET_BODY", "SECRET_TOKEN", "<script>"]) expect(html).not.toContain(secret);
    expect(html).toContain("r1"); expect(html).toContain("r2"); expect(html).not.toContain("<button");
  });
  it("renders unavailable recipients generically and removed expected evidence as null", () => {
    const html = renderToStaticMarkup(<PreservedMetadataEvidence evidence={{ ...evidence, entries: [{ family: "page_viewer_grant", source_id: "grant", disposition: "remove", original: { ...original, recipient: { available: false, display_name: "SECRET_OLD_LABEL" } }, expected_active: null }] }} />);
    expect(html).toContain("Unavailable recipient"); expect(html).toContain("null"); expect(html).not.toContain("SECRET_OLD_LABEL");
  });
});
