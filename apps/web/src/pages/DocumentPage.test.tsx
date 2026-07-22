import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  buildDocumentSpaceBreadcrumbHref,
  buildDocumentFolderPath,
  DOCUMENT_OVERFLOW_VIEW_MODES,
  DocumentSourceTextarea,
  DocumentTitleTextarea,
  getDocumentActionLabels,
  getDocumentPrimaryChromeActions,
  getDocumentSaveStatusClass,
  getVisibleDocumentViewModes,
  stripLeadingMatchingH1,
  shouldCloseDocumentActionsForKey,
} from "./DocumentPage";

describe("buildDocumentFolderPath", () => {
  it("builds an encoded space breadcrumb href for document pages", () => {
    expect(buildDocumentSpaceBreadcrumbHref("space one")).toBe("/spaces/space%20one/docs");
    expect(buildDocumentSpaceBreadcrumbHref("space/one")).toBe("/spaces/space%2Fone/docs");
    expect(buildDocumentSpaceBreadcrumbHref()).toBeUndefined();
  });

  it("builds a nested folder path for a document", () => {
    expect(
      buildDocumentFolderPath(
        [
          { id: "root", name: "Projects", parent_id: undefined },
          { id: "child", name: "Launch", parent_id: "root" },
          { id: "grandchild", name: "Notes", parent_id: "child" },
        ],
        [{ id: "doc-1", folder_id: "grandchild" }],
        "doc-1",
      ),
    ).toEqual(["Projects", "Launch", "Notes"]);
  });

  it("prefers a direct folder id from the loaded document", () => {
    expect(
      buildDocumentFolderPath(
        [
          { id: "root", name: "Projects", parent_id: undefined },
          { id: "child", name: "Launch", parent_id: "root" },
        ],
        [{ id: "doc-1", folder_id: undefined }],
        "doc-1",
        "child",
      ),
    ).toEqual(["Projects", "Launch"]);
  });

  it("stops when folder metadata is missing or cyclic", () => {
    expect(
      buildDocumentFolderPath(
        [{ id: "child", name: "Launch", parent_id: "missing" }],
        [{ id: "doc-1", folder_id: "child" }],
        "doc-1",
      ),
    ).toEqual(["Launch"]);

    expect(
      buildDocumentFolderPath(
        [
          { id: "a", name: "A", parent_id: "b" },
          { id: "b", name: "B", parent_id: "a" },
        ],
        [{ id: "doc-2", folder_id: "a" }],
        "doc-2",
      ),
    ).toEqual(["B", "A"]);
  });
});

describe("stripLeadingMatchingH1", () => {
  it("removes a leading H1 that duplicates the document title", () => {
    expect(stripLeadingMatchingH1("# Example Space\n\nBody", "Example Space")).toBe("Body");
    expect(stripLeadingMatchingH1("# Example   Space\n\nBody", "Example Space")).toBe("Body");
    expect(stripLeadingMatchingH1("# Different\n\nBody", "Example Space")).toBe(
      "# Different\n\nBody",
    );
  });
});

describe("getDocumentActionLabels", () => {
  it("keeps document view modes in the desktop action order", () => {
    expect(DOCUMENT_OVERFLOW_VIEW_MODES).toEqual(["preview", "markdown", "source"]);
  });

  it("describes the document action labels", () => {
    expect(getDocumentActionLabels(false, false)).toEqual([
      "Favorite",
      "Move page",
      "Page view",
      "Edit",
      "Source view",
      "Properties",
    ]);
  });

  it("reflects active favorite and properties state", () => {
    expect(getDocumentActionLabels(true, true)).toEqual([
      "Remove favorite",
      "Move page",
      "Page view",
      "Edit",
      "Source view",
      "Hide properties",
    ]);
  });

  it("only exposes source mode while editing or viewing source", () => {
    expect(getVisibleDocumentViewModes("preview")).toEqual(["preview", "markdown"]);
    expect(getVisibleDocumentViewModes("markdown")).toEqual(["preview", "markdown", "source"]);
    expect(getVisibleDocumentViewModes("source")).toEqual(["preview", "markdown", "source"]);
  });

  it("keeps move out of primary document chrome while preserving active favorite", () => {
    expect(getDocumentPrimaryChromeActions(false)).toEqual(["view-modes", "properties"]);
    expect(getDocumentPrimaryChromeActions(true)).toEqual([
      "favorite",
      "view-modes",
      "properties",
    ]);
    expect(getDocumentPrimaryChromeActions(true)).not.toContain("move");
  });

  it("closes the document overflow menu on Escape only", () => {
    expect(shouldCloseDocumentActionsForKey("Escape")).toBe(true);
    expect(shouldCloseDocumentActionsForKey("Enter")).toBe(false);
    expect(shouldCloseDocumentActionsForKey(" ")).toBe(false);
  });
});

describe("getDocumentSaveStatusClass", () => {
  it("uses semantic status tokens for autosave feedback", () => {
    expect(getDocumentSaveStatusClass("error")).toBe("text-danger");
    expect(getDocumentSaveStatusClass("saving")).toBe("text-accent");
    expect(getDocumentSaveStatusClass("saved")).toBe("text-text-muted");
    expect(getDocumentSaveStatusClass("idle")).toBe("text-text-muted");
    expect(getDocumentSaveStatusClass("error")).not.toMatch(/red-/);
  });
});

describe("DocumentSourceTextarea", () => {
  it("uses the shared textarea control for Markdown source editing", () => {
    const html = renderToStaticMarkup(
      <DocumentSourceTextarea content="# Source" locked={false} onChange={() => undefined} />,
    );

    expect(html).toContain('id="document-source"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("font-mono");
    expect(html).toContain("resize-y");
  });

  it("renders Markdown source editing as read-only for locked documents", () => {
    const html = renderToStaticMarkup(
      <DocumentSourceTextarea content="# Locked source" locked={true} onChange={() => undefined} />,
    );

    expect(html).toContain('id="document-source"');
    expect(html).toContain("readonly");
    expect(html).not.toContain(" disabled=");
  });
});

describe("DocumentTitleTextarea", () => {
  it("uses the shared textarea control without form-field chrome", () => {
    const html = renderToStaticMarkup(
      <DocumentTitleTextarea
        title="Launch Notes"
        locked={false}
        onTitleChange={() => undefined}
        onCommit={() => undefined}
      />,
    );

    expect(html).toContain('id="document-title"');
    expect(html).toContain("document-title-input");
    expect(html).toContain("rounded-none");
    expect(html).toContain("border-transparent");
    expect(html).toContain("!min-h-0");
    expect(html).toContain("focus:ring-0");
  });
});

describe("editor layout CSS", () => {
  it("keeps TipTap blocks shrink-safe so placeholders do not stack vertically", () => {
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");
    const blockRule = css.match(/\.tiptap-editor > \* \{[\s\S]*?\n\}/)?.[0] ?? "";

    expect(blockRule).toContain("min-width: 0;");
    expect(blockRule).toContain("max-width: 100%;");
    expect(blockRule).not.toContain("min-width: min(100%, 32rem)");
  });
});
