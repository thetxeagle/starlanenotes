import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DocumentDraftRecoveryReview } from "./DocumentDraftRecoveryReview";
import type { PageCssProvenance } from "@/lib/pageCssProvenance";

const checkpoint = {
  snapshotId: "checkpoint",
  capturedAt: "same-time",
  baseDraftUpdatedAt: "same-time",
  title: "Device title",
  content: "<script>unsafe()</script>\nDevice body",
  synopsis: "Device synopsis",
  pageWidth: "wide" as const,
  customCss: "body { color: red }",
  icon: "",
  coverImage: null,
};
const loaded = {
  title: "Shared title",
  content: "Collaborator body",
  synopsis: "Shared synopsis",
  pageWidth: "standard",
  customCss: "",
  icon: "🌙",
  coverImage: "/current-cover",
};
const callbacks = { onClose() {}, onCopy() {}, onReplace() {}, onDownload() {} };

describe("draft recovery comparison", () => {
  it("keeps both complete copies and presentation evidence readable without executing retained source", () => {
    const html = renderToStaticMarkup(
      <DocumentDraftRecoveryReview
        checkpoint={checkpoint}
        loaded={loaded}
        provenance="changed"
        busy={false}
        error=""
        {...callbacks}
      />,
    );
    expect(html).toContain("The loaded draft has changed");
    for (const value of [
      "Shared title",
      "Device title",
      "Collaborator body",
      "Device body",
      "Shared synopsis",
      "Device synopsis",
      "/current-cover",
      "Rendered CSS · original source not captured",
    ])
      expect(html).toContain(value);
    expect(html).toContain("&lt;script&gt;unsafe()&lt;/script&gt;");
    expect(html).not.toContain("<script>");
  });
  it("labels unavailable source honestly and keeps download while recovery writes are blocked", () => {
    const html = renderToStaticMarkup(
      <DocumentDraftRecoveryReview
        checkpoint={checkpoint}
        loaded={loaded}
        provenance="legacy-unverified"
        loadedUnavailable
        busy={false}
        error="Original draft missing"
        replaceBlocked="Cannot replace missing draft"
        copyBlocked="Review an editable source"
        {...callbacks}
      />,
    );
    expect(html).toContain("older checkpoint has no verified original source");
    expect(html).toContain("Captured original · draft unavailable");
    expect(html).toContain("does not grant access");
    expect(html).toContain('role="alert"');
    expect(html.match(/disabled=""/g)).toHaveLength(2);
    expect(html).toContain("Download device copy");
  });
  it.each(["", " \r\n\0@import 'https://example.invalid/original.css';" + "x".repeat(20_001)])(
    "shows retained original markers when rendered CSS is empty (%#)",
    (rawCss) => {
      const rawBlock = `<!-- starlane:custom-css -->${rawCss}<!-- /starlane:custom-css -->`;
      const cssProvenance: PageCssProvenance = { schemaVersion: 1, kind: "single", rawBlock };
      const html = renderToStaticMarkup(
        <DocumentDraftRecoveryReview
          checkpoint={{ ...checkpoint, customCss: "", cssProvenance }}
          loaded={loaded}
          provenance="matching"
          busy={false}
          error=""
          {...callbacks}
        />,
      );
      expect(html).toContain("Retained original CSS source");
      expect(html).toContain("including empty or blocked CSS");
      expect(html).toContain("&lt;!-- starlane:custom-css --&gt;");
      expect(html).toContain("&lt;!-- /starlane:custom-css --&gt;");
      expect(html).toContain('aria-label="Retained device edits retained original CSS source"');
      expect(html).not.toContain("<style");
      expect(html).not.toContain("<details open");
      expect(html).not.toContain("original source not captured");
    },
  );

  it("shows ambiguous whole source separately from edited body without executing it", () => {
    const originalSource =
      "ORIGINAL WHOLE BODY\r\n<!-- starlane:custom-css --><script>original()</script>\0";
    const cssProvenance: PageCssProvenance = {
      schemaVersion: 1,
      kind: "ambiguous",
      originalSource,
    };
    const retained = { ...checkpoint, content: "EDITED LOCAL BODY", customCss: "", cssProvenance };
    const html = renderToStaticMarkup(
      <DocumentDraftRecoveryReview
        checkpoint={retained}
        loaded={loaded}
        provenance="changed"
        busy={false}
        error=""
        {...callbacks}
      />,
    );
    expect(html).toContain("ambiguous CSS markers");
    expect(html).toContain("automatic reconstruction is held");
    expect(html).toContain("whole original source is preserved");
    expect(html).toContain("source and your edited");
    expect(html).toContain("body in JSON");
    expect(html).toContain("ORIGINAL WHOLE BODY");
    expect(html).toContain("EDITED LOCAL BODY");
    expect(html).toContain("&lt;script&gt;original()&lt;/script&gt;");
    expect(html).not.toContain("<script>");
    // The existing download handler serializes the checkpoint; no transformation
    // in this presentation component is needed to retain these distinct fields.
    const downloaded = JSON.parse(JSON.stringify(retained));
    expect(downloaded.cssProvenance.originalSource).toBe(originalSource);
    expect(downloaded.content).toBe("EDITED LOCAL BODY");
  });

  it("does not label legacy rendering CSS as exact original source", () => {
    const html = renderToStaticMarkup(
      <DocumentDraftRecoveryReview
        checkpoint={checkpoint}
        loaded={loaded}
        provenance="legacy-unverified"
        busy={false}
        error=""
        {...callbacks}
      />,
    );
    expect(html).toContain("Rendered CSS · original source not captured");
    expect(html).toContain("not an exact original CSS source");
    expect(html).not.toContain("Retained original CSS source");
  });
});
