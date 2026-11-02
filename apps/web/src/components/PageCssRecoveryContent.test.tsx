import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  PageCssRecoveryContent,
  buildPageCssRecoveryExport,
  selectPageCssRecoverySource,
} from "./PageCssRecoveryContent";
import type { RetainedPageCssSource } from "@/lib/pageCssProvenance";
const block = (css: string) => `<!-- starlane:custom-css -->${css}<!-- /starlane:custom-css -->`;
const source = (rawBlock: string): RetainedPageCssSource => ({
  spaceId: "space",
  documentId: "page",
  draftId: "draft",
  provenance: { schemaVersion: 1, kind: "single", rawBlock },
});

describe("read-only retained page CSS", () => {
  it.each([
    "",
    " \r\n\0@import 'https://example.invalid/original';" + "x".repeat(20_001),
    "</textarea><script>unsafe()</script>",
  ])("renders selectable retained source safely, without authoring actions (%#)", (css) => {
    const value = source(block(css));
    const close = vi.fn();
    const html = renderToStaticMarkup(<PageCssRecoveryContent source={value} onClose={close} />);
    expect(html).toContain("Retained page CSS");
    expect(html).toMatch(
      /<textarea[^>]*(?=[^>]*aria-label="Retained original CSS source")(?=[^>]*readonly="")[^>]*>/i,
    );
    expect(html).not.toMatch(/<textarea[^>]* disabled=""/);
    expect(html).toContain("Download original JSON");
    expect(html).toContain("empty, blocked or oversized");
    expect(html).not.toContain("<script>");
    expect(html).not.toContain("<style");
    expect(html).not.toContain("Save CSS");
    expect(html).not.toContain("Apply");
    expect(close).not.toHaveBeenCalled();
  });

  it.each(["", " \r\n\0@import 'https://example.invalid/original';" + "x".repeat(20_001)])(
    "downloads exact raw span and source association through real JSON/Blob encoding (%#)",
    async (css) => {
      const original = source(block(css));
      const blob = new Blob([JSON.stringify(buildPageCssRecoveryExport(original), null, 2)], {
        type: "application/json",
      });
      const exported = JSON.parse(await blob.text());
      expect(exported).toEqual({
        format: "starlane-page-css-source",
        spaceId: "space",
        documentId: "page",
        draftId: "draft",
        schemaVersion: 1,
        kind: "single",
        rawBlock: block(css),
      });
      expect(exported.rawBlock).toBe(block(css));
    },
  );

  it("retains ambiguous whole source and explains the reconstruction hold", async () => {
    const originalSource = "WHOLE ORIGINAL\r\n\0<!-- starlane:custom-c";
    const value: RetainedPageCssSource = {
      spaceId: "space",
      documentId: "page",
      provenance: { schemaVersion: 1, kind: "ambiguous", originalSource },
    };
    const html = renderToStaticMarkup(<PageCssRecoveryContent source={value} onClose={() => {}} />);
    expect(html).toContain("whole original page source is retained");
    expect(html).toContain("automatic reconstruction remains paused");
    expect(html).toMatch(
      /<textarea[^>]*(?=[^>]*aria-label="Retained original page source")(?=[^>]*readonly="")[^>]*>/i,
    );
    const exported = JSON.parse(
      await new Blob([JSON.stringify(buildPageCssRecoveryExport(value))]).text(),
    );
    expect(exported.originalSource).toBe(originalSource);
    expect(exported).not.toHaveProperty("rawBlock");
    expect(exported).not.toHaveProperty("draftId");
  });

  it("rejects absent, mismatched and malformed retained source without a blank authoring surface", () => {
    const value = source(block("kept"));
    expect(selectPageCssRecoverySource(value, "other", "page")).toBeUndefined();
    expect(selectPageCssRecoverySource(value, "space", "other")).toBeUndefined();
    expect(selectPageCssRecoverySource({ ...value, draftId: "" }, "space", "page")).toBeUndefined();
    expect(
      selectPageCssRecoverySource(
        { ...value, provenance: { schemaVersion: 1, kind: "single", rawBlock: "not a block" } },
        "space",
        "page",
      ),
    ).toBeUndefined();
    const absent: RetainedPageCssSource = {
      spaceId: "space",
      documentId: "page",
      provenance: { schemaVersion: 1, kind: "absent" },
    };
    expect(
      renderToStaticMarkup(<PageCssRecoveryContent source={absent} onClose={() => {}} />),
    ).toBe("");
    expect(() => buildPageCssRecoveryExport(absent)).toThrow("unavailable");
    expect(selectPageCssRecoverySource(value, "space", "page")).toBe(value);
  });
});
