import {getPageEditCapability} from "@/lib/pageEditCapability";
import {createAutoSaveController} from "@/hooks/useAutoSave";
import {isDraftSaveHeldError} from "@/lib/dataClient";
import { loadPageViewerContext } from "@/lib/pageViewerLoad";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import {
  assertDocumentPageEditable,
  getDocumentAttributionNames,
  buildDocumentSpaceBreadcrumbHref,
  buildDocumentFolderPath,
  canEditDocumentMetadata,
  DOCUMENT_OVERFLOW_VIEW_MODES,
  DOCUMENT_TITLE_ROW_CLASS,
  DOCUMENT_TITLE_ICON_GUTTER_CLASS,
  DOCUMENT_TITLE_EMPTY_ICON_GUTTER_CLASS,
  DocumentDraftStatus,
  DocumentSourceTextarea,
  DocumentTitleTextarea,
  getDocumentActionLabels,
  getDocumentPageBottomPaddingClass,
  getDocumentChromeActionTone,
  getDesktopDocumentViewModes,
  getDocumentSaveStatusClass,
  getVisibleDocumentViewModes,
  isDocumentMetadataEditable,
  isDocumentEditingMode,
  MOBILE_DOCUMENT_CONTROL_RAIL_CLASS,
  parseDocumentContent,
  bodyOffsetForDocumentOffset,
  documentOffsetForBodyOffset,
  serializeRecoveryCheckpoint,
  recoveryCompositionHoldReason,
  recoveryCheckpointMatchesSaved,
  retainedDraftDiscussionAnchor,
  serializeDocumentContent,
  shouldBlockDocumentRenderForDraft,
  stripLeadingMatchingH1,
} from "./DocumentPage";
import {
  DOCUMENT_PAGE_WIDTH_OPTIONS,
  getDocumentPageWidthClass,
  normalizeDocumentPageWidth,
} from "@/components/editor/DocumentWidthPicker";

describe("document list invalidation", () => {
  it("emits a structural change after successfully duplicating a document", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const duplicateHandler = source
      .split("const duplicateDocument = async () => {")[1]
      ?.split("const archiveDocument = async () => {")[0];

    expect(duplicateHandler).toContain('emitGraphDirty(spaceId, "document-created")');
    const publishHandler = source
      .split("const publishActiveDraft = async () => {")[1]
      ?.split("const toggleStarred = () => {")[0];
    expect(publishHandler).toContain('emitGraphDirty(spaceId, "document-metadata-updated")');
  });
});

describe("buildDocumentFolderPath", () => {
  it("does not reserve a persistent desktop formatting dock above the document", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(source).not.toContain("DESKTOP_EDITOR_TOOLBAR_DOCK_CLASS");
    expect(source).not.toContain("desktopToolbarPortalTarget");
  });

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

  it("treats an explicit root move as no folder even when the cached list is stale", () => {
    expect(
      buildDocumentFolderPath(
        [{ id: "child", name: "Launch", parent_id: undefined }],
        [{ id: "doc-1", folder_id: "child" }],
        "doc-1",
        null,
      ),
    ).toEqual([]);
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

describe("document server transition loading", () => {
  it("renders published content without waiting for optional draft metadata", () => {
    expect(shouldBlockDocumentRenderForDraft("2026-07-29T12:00:00Z", null)).toBe(false);
  });

  it("waits for draft content when a draft is requested or the page is unpublished", () => {
    expect(shouldBlockDocumentRenderForDraft("2026-07-29T12:00:00Z", "draft-1")).toBe(true);
    expect(shouldBlockDocumentRenderForDraft(undefined, null)).toBe(true);
  });

  it("does not interrupt published reading with an automatic draft chooser", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const loadStart = source.indexOf("// Load document");
    const loadEffect = source.slice(loadStart, source.indexOf("if (!title) return", loadStart));
    expect(loadEffect.includes("setDraftPromptOpen(true)")).toBe(false);
    expect(loadEffect.includes("selectDocumentDraft(drafts, requestedDraftId)")).toBe(true);
  });

  it("clears the consumed draft before reloading a confirmed publication", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const receiptAcceptance = source.slice(
      source.indexOf("const acceptPublicationReceipt"),
      source.indexOf("const dispatchPublication"),
    );

    expect(receiptAcceptance).toContain("setActiveDraft(null)");
    expect(receiptAcceptance).toContain("draft.id !== receipt.draft_id");
    expect(receiptAcceptance.indexOf("setActiveDraft(null)")).toBeLessThan(
      receiptAcceptance.indexOf("reloadPublishedPage()"),
    );

    const publishedReload = source.slice(
      source.indexOf("const reloadPublishedPage"),
      source.indexOf("const acceptPublicationReceipt"),
    );
    expect(publishedReload).toContain('const leavingDraftSelection = query.has("draft")');
    expect(publishedReload).toContain("if (!leavingDraftSelection) setPublicationReload");
  });

  it("loads published body and space before optional breadcrumb, template or draft metadata settles", async () => {
    let release!: () => void;
    const optional = new Promise<string[]>((resolve) => { release = () => resolve(["optional metadata"]); });
    const result = await loadPageViewerContext({
      metadata: async () => ({ published_at: "published" }),
      content: async () => ({ content: "canonical published body" }),
      space: async () => ({ name: "ordinary space" }),
      templates: () => optional, folders: () => optional, documents: () => optional, drafts: () => optional,
    }, () => undefined);
    expect(result.content).toEqual({ content: "canonical published body" });
    expect(result.space).toEqual({ name: "ordinary space" });
    expect(result.pageOnly).toBe(false);
    release();
    expect(await result.supplementary).toEqual([["optional metadata"], ["optional metadata"], ["optional metadata"]]);
    expect(await result.drafts).toEqual(["optional metadata"]);
  });

  it("updates moved-page location metadata without restarting the editor load", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(source).toContain("addDocumentLocationChangedListener");
    expect(source).toContain("folderPathRefreshRequestIdRef.current");
    expect(source).toContain("folder_id: detail.folderId ?? undefined");
    expect(source).toContain("parent_id: detail.parentId ?? undefined");
    expect(source).toContain("if (cancelled || requestId !== folderPathRefreshRequestIdRef.current) return;");
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

describe("document page width", () => {
  it("normalizes the three deliberate page-width modes", () => {
    expect(DOCUMENT_PAGE_WIDTH_OPTIONS.map((option) => option.id)).toEqual([
      "standard",
      "wide",
      "full",
    ]);
    expect(normalizeDocumentPageWidth("wide")).toBe("wide");
    expect(normalizeDocumentPageWidth("full")).toBe("full");
    expect(normalizeDocumentPageWidth("unknown")).toBe("standard");
    expect(isDocumentEditingMode("preview")).toBe(false);
    expect(isDocumentEditingMode("markdown")).toBe(true);
    expect(isDocumentEditingMode("source")).toBe(true);
  });

  it("reserves mobile toolbar space only for Markdown Edit", () => {
    expect(getDocumentPageBottomPaddingClass(true, "markdown")).toBe("pb-40");
    expect(getDocumentPageBottomPaddingClass(true, "preview")).toBe("");
    expect(getDocumentPageBottomPaddingClass(true, "source")).toBe("");
    expect(getDocumentPageBottomPaddingClass(false, "markdown")).toBe("");
  });

  it("uses one wider canvas for every legacy width value", () => {
    for (const width of ["standard", "wide", "full"] as const) {
      expect(getDocumentPageWidthClass(width)).toBe("max-w-[86.16375rem]");
    }
  });

  it("round-trips width metadata without adding frontmatter for the default", () => {
    expect(serializeDocumentContent("Body", "", "standard")).toBe("Body");
    const serialized = serializeDocumentContent("Body", "Summary", "wide");
    expect(serialized).toContain('synopsis: "Summary"');
    expect(serialized).toContain("page_width: wide");
    expect(parseDocumentContent(serialized)).toEqual({
      body: "Body",
      synopsis: "Summary",
      pageWidth: "wide",
      cssProvenance: {schemaVersion:1,kind:"absent"},
    });
  });

  it("maps body cursors through retained source only when the full draft round-trips exactly", () => {
    const body = "Alpha 😀 beta";
    const documentSource = serializeDocumentContent(body, "Summary", "wide");
    const bodyOffset = body.indexOf("beta");
    const documentOffset = documentOffsetForBodyOffset(body, bodyOffset, documentSource, "Summary", "wide", "", {schemaVersion:1,kind:"absent"});
    expect(documentOffset).toBe(documentSource.indexOf("beta"));
    expect(bodyOffsetForDocumentOffset(documentSource, documentOffset!, "Page", body)).toBe(bodyOffset);
    expect(documentOffsetForBodyOffset(body, body.indexOf("😀") + 1, documentSource, "Summary", "wide", "", {schemaVersion:1,kind:"absent"})).toBeNull();
    expect(bodyOffsetForDocumentOffset(documentSource, documentSource.indexOf("synopsis"), "Page", body)).toBeNull();
    expect(documentOffsetForBodyOffset(body, bodyOffset, `${documentSource}!`, "Summary", "wide", "", {schemaVersion:1,kind:"absent"})).toBeNull();
  });

  it("anchors draft discussion selections against the exact retained source", () => {
    const body = "Alpha 😀 beta";
    const documentSource = serializeDocumentContent(body, "Summary", "wide");
    const selectionStart = body.indexOf("😀");
    const anchor = retainedDraftDiscussionAnchor(
      { source: body, anchor: selectionStart, head: selectionStart + 2 },
      { source_digest: "a".repeat(64), content: documentSource },
      body,
      "Summary",
      "wide",
      "",
      { schemaVersion: 1, kind: "absent" },
    );

    expect(anchor).toEqual({
      source_digest: "a".repeat(64),
      selection_start: documentSource.indexOf("😀"),
      selection_end: documentSource.indexOf("😀") + 2,
      selection_text: "😀",
    });
    expect(
      retainedDraftDiscussionAnchor(
        { source: `${body}!`, anchor: selectionStart, head: selectionStart + 2 },
        { source_digest: "a".repeat(64), content: documentSource },
        body,
        "Summary",
        "wide",
        "",
        { schemaVersion: 1, kind: "absent" },
      ),
    ).toBeNull();
    expect(
      retainedDraftDiscussionAnchor(
        { source: body, anchor: selectionStart, head: selectionStart + 2 },
        { source_digest: "a".repeat(64), content: `${documentSource}!` },
        body,
        "Summary",
        "wide",
        "",
        { schemaVersion: 1, kind: "absent" },
      ),
    ).toBeNull();
  });

  it("applies the width contract directly to the document canvas", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(source).toContain("getDocumentPageWidthClass(pageWidth)");
    expect(source).toContain("data-document-page-width={pageWidth}");
    expect(source).not.toContain("<DocumentWidthPicker");
    expect(source).not.toContain("showPageWidthPicker");
    expect(source).toContain("pageWidthEditable: pageDetailsEditable");
    expect(source).toContain("onPageWidthChange: handlePageWidthChange");
    expect(source).not.toContain("desktopToolbarTrailingControl={");
    expect(source).toContain("pageWidthRef.current = nextWidth");
    expect(source).toContain("scheduleAutoSave(content)");
    expect(source).toContain("saveNow(content)");
    expect(source).toContain("data-document-page-width={pageWidth}");
    expect(source).toContain('data-document-editing={isMarkdownMode ? "true" : undefined}');
    expect(source).toContain("starlane-document-viewport");
    expect(source).not.toContain("starlane-document-width-scope");
  });
});

describe("getDocumentActionLabels", () => {
  it("reserves semantic tones for active, primary, and destructive document actions", () => {
    expect(getDocumentChromeActionTone()).toBe("text-text-muted");
    expect(getDocumentChromeActionTone("default", true)).toBe("text-primary");
    expect(getDocumentChromeActionTone("primary")).toBe("text-primary");
    expect(getDocumentChromeActionTone("danger")).toBe("text-danger");
  });

  it("keeps document view modes in the desktop action order", () => {
    expect(DOCUMENT_OVERFLOW_VIEW_MODES).toEqual(["preview", "markdown", "source"]);
  });

  it("describes the document action labels", () => {
    expect(getDocumentActionLabels(false, false)).toEqual([
      "Favorite",
      "Move page",
      "Page view",
      "Edit view",
      "Markdown view",
      "Page details",
      "Delete page",
    ]);
  });

  it("reflects active favorite and properties state", () => {
    expect(getDocumentActionLabels(true, true)).toEqual([
      "Remove favorite",
      "Move page",
      "Page view",
      "Edit view",
      "Markdown view",
      "Hide page details",
      "Delete page",
    ]);
  });

  it("keeps source out of the primary rail until source itself is active", () => {
    expect(getVisibleDocumentViewModes("preview")).toEqual(["preview", "markdown"]);
    expect(getVisibleDocumentViewModes("markdown")).toEqual(["preview", "markdown"]);
    expect(getVisibleDocumentViewModes("source")).toEqual(["preview", "markdown", "source"]);
  });

  it("keeps reading and editing primary while source stays in the page menu", () => {
    expect(getDesktopDocumentViewModes("preview")).toEqual(["preview", "markdown"]);
    expect(getDesktopDocumentViewModes("markdown")).toEqual(["preview", "markdown"]);
    expect(getDesktopDocumentViewModes("source")).toEqual(["preview", "markdown"]);
  });

  it("keeps cover and icon controls out of Page view", () => {
    expect(isDocumentMetadataEditable("preview")).toBe(false);
    expect(isDocumentMetadataEditable("markdown")).toBe(true);
    expect(isDocumentMetadataEditable("source")).toBe(true);
  });

  it("keeps cover and icon controls read-only for locked drafts", () => {
    for (const mode of ["markdown", "source"] as const) {
      expect(canEditDocumentMetadata(mode, true, false)).toBe(true);
      expect(canEditDocumentMetadata(mode, true, true)).toBe(false);
    }

    expect(canEditDocumentMetadata("markdown", false, false)).toBe(false);
    expect(canEditDocumentMetadata("preview", true, false)).toBe(false);
  });

  it("registers draft-safe cover management with Page Details and keeps canvas covers presentational", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(source).toContain("registerPageDetails({");
    expect(source).toContain("coverEditable: pageDetailsEditable");
    expect(source).toContain("dataClient.updateDocumentDraft");
    expect(source).toContain("editable={false}");
    expect(source).toContain("showEmptyPlaceholder={false}");
    expect(source).not.toContain("showEmptyPlaceholder={!mobileLayout && metadataEditable}");
  });

  it("hands explicit Edit and draft-choice intent to the writable TipTap canvas once", () => {
    const pageSource = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const editorSource = readFileSync(
      resolve(process.cwd(), "src/components/editor/Editor.tsx"),
      "utf8",
    );

    expect(pageSource).toContain("shouldChooseAccessibleDraft(draftChoices)");
    expect(pageSource).toContain("setDraftPromptOpen(true)");
    expect(pageSource.includes('await startDraft(false, next === "markdown")')).toBe(true);
    expect(pageSource).toContain("activateDraft(draft.id, true)");
    expect(pageSource).toContain("startDraft(true, true)");
    expect(pageSource).toContain("autoFocus={focusEditorOnMount && draftEditable && !pageEditBlocked}");
    expect(pageSource).toContain("setFocusEditorOnMount(false)");
    expect(editorSource).toContain("autofocus: autoFocus && !readOnly");
  });

  it("moves document management into one keyboard-owned overflow menu", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(source).toContain('aria-label="More page actions"');
    expect(source).toContain('aria-label="Page actions"');
    expect(source).toContain('role="menuitemcheckbox"');
    expect(source).toContain('role="menuitemradio"');
    expect(source).toContain("useMenuKeyboardNavigation(menuRef)");
    expect(source).toContain("starlane-menu-item-danger");
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

describe("DocumentDraftStatus", () => {
  it("renders ordinary draft ownership as quiet inline metadata", () => {
    const html = renderToStaticMarkup(<DocumentDraftStatus accessRole="owner" canPublish />);

    expect(html).toContain('data-document-draft-status="true"');
    expect(html).toContain("Unpublished draft · owned by you");
    expect(html).toContain("Publish when ready");
    expect(html).toContain("text-text-muted");
    expect(html).toContain("mb-2");
    expect(html).not.toContain("mt-2");
    expect(html).not.toContain("border-primary");
    expect(html).not.toContain("bg-primary");
  });

  it("keeps the owner-only publishing boundary visible to collaborators", () => {
    const html = renderToStaticMarkup(
      <DocumentDraftStatus accessRole="editor" canPublish={false} />,
    );

    expect(html).toContain("Unpublished draft · shared with you as editor");
    expect(html).toContain("Only the owner can publish");
  });

  it("places private draft ownership before the document title", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const titleMarker = source.indexOf("{/* Title */}");
    const placement = source.lastIndexOf("<DocumentDraftStatus", titleMarker);

    expect(placement).toBeGreaterThan(-1);
    expect(placement).toBeLessThan(titleMarker);
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

  it("hands focus to editable source mode without focusing locked source", () => {
    const editable = renderToStaticMarkup(
      <DocumentSourceTextarea
        content="# Source"
        locked={false}
        autoFocus
        onChange={() => undefined}
      />,
    );
    const locked = renderToStaticMarkup(
      <DocumentSourceTextarea
        content="# Locked source"
        locked
        autoFocus
        onChange={() => undefined}
      />,
    );

    expect(editable).toContain(" autofocus");
    expect(locked).not.toContain(" autofocus");
  });

  it("keeps source readable but non-editable for draft viewers", () => {
    const html = renderToStaticMarkup(
      <DocumentSourceTextarea
        content="# Viewer source"
        locked={false}
        readOnly
        autoFocus
        onChange={() => undefined}
      />,
    );

    expect(html).toContain('id="document-source"');
    expect(html).toContain("# Viewer source");
    expect(html).toMatch(/readonly(?:="")?/i);
    expect(html).not.toContain(" autofocus");
    expect(html).not.toContain(" disabled=");
  });

  it("renders Markdown source editing as read-only for locked documents", () => {
    const html = renderToStaticMarkup(
      <DocumentSourceTextarea content="# Locked source" locked={true} onChange={() => undefined} />,
    );

    expect(html).toContain('id="document-source"');
    expect(html).toMatch(/readonly(?:="")?/i);
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
    expect(html).toContain("!min-h-12");
    expect(html).toContain("py-1");
    expect(html).toContain("focus:ring-0");
  });

  it("keeps viewer draft titles selectable but non-editable", () => {
    const html = renderToStaticMarkup(
      <DocumentTitleTextarea
        title="Viewer title"
        locked={false}
        readOnly
        onTitleChange={() => undefined}
        onCommit={() => undefined}
      />,
    );

    expect(html).toContain('id="document-title"');
    expect(html).toContain("Viewer title");
    expect(html).toMatch(/readonly(?:="")?/i);
    expect(html).not.toContain(" disabled=");
  });
});

describe("document title row", () => {
  it("keeps the icon above the title until the page width has a safe outer gutter", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const css = readFileSync(resolve(process.cwd(), "src/styles/globals.css"), "utf8");

    expect(source).toContain('<div className="w-full min-w-0 flex-1">');
    expect(DOCUMENT_TITLE_ROW_CLASS).toContain("document-title-row");
    expect(DOCUMENT_TITLE_ROW_CLASS).toContain("relative");
    expect(DOCUMENT_TITLE_ROW_CLASS).toContain("min-w-0");
    expect(DOCUMENT_TITLE_ROW_CLASS).toContain("flex");
    expect(DOCUMENT_TITLE_ROW_CLASS).toContain("flex-col");
    expect(DOCUMENT_TITLE_ROW_CLASS).toContain("gap-1");
    expect(DOCUMENT_TITLE_ROW_CLASS).toContain("mb-3");
    expect(DOCUMENT_TITLE_ICON_GUTTER_CLASS).toContain("document-title-icon-lane");
    expect(DOCUMENT_TITLE_ICON_GUTTER_CLASS).not.toContain("sm:absolute");
    expect(DOCUMENT_TITLE_ICON_GUTTER_CLASS).toContain("shrink-0");
    expect(DOCUMENT_TITLE_EMPTY_ICON_GUTTER_CLASS).toContain("document-title-empty-icon-lane");
    expect(DOCUMENT_TITLE_EMPTY_ICON_GUTTER_CLASS).toContain("size-12");
    expect(css).toContain("@media (min-width: 92rem)");
    expect(css).toContain("@media (min-width: 117.3rem)");
    expect(css).toContain(
      '.starlane-document-scope[data-document-page-width="standard"] .document-title-icon-lane',
    );
    expect(css).toContain(
      '.starlane-document-scope[data-document-page-width="wide"] .document-title-icon-lane',
    );
    expect(css).toContain(
      '.starlane-document-scope[data-document-page-width="standard"] .document-title-empty-icon-lane',
    );
    expect(css).toContain(
      '.starlane-document-scope[data-document-page-width="wide"] .document-title-empty-icon-lane',
    );
    expect(css).not.toContain(
      '.starlane-document-scope[data-document-page-width="full"] .document-title-icon-lane',
    );
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

describe("document control rails", () => {
  it("keeps breadcrumb and real page controls in the document header", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(source).not.toContain("useWorkspaceTopBarSlots()");
    expect(source).toContain("{desktopDocumentControls}");
    expect(source).toContain("workspace-document-header");
    expect(source).toContain("desktopToolbarHost={!mobileLayout ? desktopToolbarHost : null}");
    expect(source).not.toContain("topBarSlots.breadcrumb");
    expect(source).not.toContain("showSpace={false}");
    expect(source.indexOf("<Breadcrumb")).toBeLessThan(source.indexOf("<DocumentDraftStatus"));
    expect(source).not.toContain('className="hidden xl:inline"');
    expect(source).toContain("flex h-8 w-8 items-center justify-center");
  });

  it("exposes separate inline and page comment controls on desktop", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(source).toContain('"Hide inline comments" : "Show inline comments"');
    expect(source).toContain('activeDraft?.source_digest ? "draft discussion" : "page comments"');
    expect(source).toContain('`${commentsPanelOpen ? "Hide" : "Show"} ${discussionActionLabel}`');
    expect(source).toContain("toggleCommentsPanel");
    expect(source).toContain('rightPanel.toggleTab("properties")');
    expect(source).toContain("rightPanel.openComment(commentId)");
    expect(source).toContain("<SlidersHorizontal size={14}");
    expect(source).not.toContain("<PanelRight");
    expect(source).toContain("inlineCommentsVisible={!activeDraft && !mobileLayout && inlineCommentsVisible}");
    expect(source).toContain("onDraftCommentRequested={activeDraft?.source_digest ? requestDraftComment : undefined}");
    expect(source).toContain("onToggleInlineComments={activeDraft ? undefined : toggleInlineCommentsPanel}");
    expect(source).toContain("rightPanel.openComment();");
  });

  it("replaces the mobile overflow menu with an in-flow horizontal toolbar", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");

    expect(MOBILE_DOCUMENT_CONTROL_RAIL_CLASS).toContain("overflow-x-auto");
    expect(MOBILE_DOCUMENT_CONTROL_RAIL_CLASS).not.toContain("sticky");
    expect(source).toContain('role="toolbar"');
    expect(source).toContain('aria-label="Document controls"');
    expect(source).toContain("<DocumentOutlineAccordion");
    expect(source).toContain('aria-label="Document controls"');
    expect(source).toContain("touchSafe");
    expect(source).toContain("<DocumentActionsMenu");
    expect(source).toContain('aria-label="More page actions"');
    expect(source).not.toContain('aria-label="Document actions"');
  });

  it("keeps mobile low-frequency page actions inside the shared More menu", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const mobileStart = source.indexOf("{mobileLayout && (");
    const mobileEnd = source.indexOf("\n            )}", mobileStart);
    const mobileRail = source.slice(mobileStart, mobileEnd);

    expect(mobileRail).toContain("<DocumentActionsMenu");
    expect(mobileRail).toContain("includeSourceAction={!isSourceMode}");
    expect(mobileRail).toContain("touchSafe");
    expect(mobileRail).not.toContain("onClick={toggleStarred}");
    expect(mobileRail).not.toContain("onClick={openMoveDialog}");
    expect(mobileRail).not.toContain("onClick={() => void duplicateDocument()}");
    expect(mobileRail).not.toContain("onClick={() => void archiveDocument()}");
    expect(mobileRail.includes("onDelete={() =>")).toBe(true);

    const menuSource = source.slice(source.indexOf("function DocumentActionsMenu"));
    expect(menuSource).toContain("Add to favorites");
    expect(menuSource).toContain("Move page");
    expect(menuSource).toContain("Duplicate page");
    expect(menuSource).toContain("Archive page");
    expect(menuSource).toContain("Delete page");
  });

  it("omits the duplicate More-menu Source action while Source owns the rail", () => {
    const source = readFileSync(resolve(process.cwd(), "src/pages/DocumentPage.tsx"), "utf8");
    const sourceActionGuards = source.match(/includeSourceAction={!isSourceMode}/g) ?? [];

    expect(sourceActionGuards).toHaveLength(1);
    expect(source.includes("includeSourceAction={false}")).toBe(false);
  });
});


describe("historical document attribution", () => {
  const viewer = { id: "viewer", email: "viewer@example.invalid", display_name: "Viewing user" };
  const creator = { id: "creator", email: "creator@example.invalid", display_name: "Original creator" };
  const editor = { id: "editor", email: "editor@example.invalid", display_name: "Last updater" };

  it("does not attribute missing historical identities to the viewer or creator", () => {
    expect(getDocumentAttributionNames({}, viewer, [])).toEqual({
      createdBy: "Unknown user", updatedBy: "Unknown user",
    });
    expect(getDocumentAttributionNames({ created_by: creator.id }, viewer, [creator])).toEqual({
      createdBy: creator.display_name, updatedBy: "Unknown user",
    });
  });

  it("retains unresolved stable identities without substituting a different viewer", () => {
    const id = "00000000-0000-4000-8000-000000000042";
    for (const currentUser of [viewer, creator, null]) {
      expect(getDocumentAttributionNames({ created_by: id, last_updated_by: id }, currentUser, [])).toEqual({
        createdBy: id, updatedBy: id,
      });
    }
  });

  it("resolves actual identities and legacy email/name attribution independently", () => {
    expect(getDocumentAttributionNames({ created_by: creator.id, last_updated_by: editor.email }, viewer, [creator, editor])).toEqual({
      createdBy: creator.display_name, updatedBy: editor.display_name,
    });
    expect(getDocumentAttributionNames({ created_by: viewer.id, last_updated_by: creator.display_name }, viewer, [creator])).toEqual({
      createdBy: viewer.display_name, updatedBy: creator.display_name,
    });
  });

  it("keeps a readable fallback when an actual directory record has an empty display name", () => {
    expect(getDocumentAttributionNames({ created_by: creator.id, last_updated_by: viewer.id }, { ...viewer, display_name: "" }, [{ ...creator, display_name: "" }])).toEqual({
      createdBy: creator.email, updatedBy: viewer.email,
    });
  });
});


describe("lock-owner edit/save boundaries", () => {
  const owner = () => getPageEditCapability({mode:"server",actorId:"owner",selectionCurrent:true,capabilityCurrent:true,page:{locked:true,locked_by:"owner",page_edit_allowed:true}});
  it("passes effective owner authority through title/source helpers while preserving draft read-only participation", () => {
    const capability = owner();
    expect(() => assertDocumentPageEditable(capability)).not.toThrow();
    expect(renderToStaticMarkup(<DocumentTitleTextarea title="Owned locked title" locked={!capability.canEditPage} readOnly={false} onTitleChange={() => {}} onCommit={() => {}} />)).not.toContain("readOnly");
    expect(renderToStaticMarkup(<DocumentSourceTextarea content="Owned locked source" locked={!capability.canEditPage} readOnly={false} onChange={() => {}} />)).not.toContain("readOnly");
    expect(renderToStaticMarkup(<DocumentSourceTextarea content="Participant viewer source" locked={!capability.canEditPage} readOnly onChange={() => {}} />)).toContain("readOnly");
  });
  it("uses current capability at a serialized task boundary, not an earlier captured owner grant", async () => {
    let current = owner();
    const write = vi.fn();
    const task = Promise.resolve().then(() => {assertDocumentPageEditable(current);write();});
    current = getPageEditCapability({mode:"server",actorId:"owner",selectionCurrent:true,capabilityCurrent:true,page:{locked:true,locked_by:"other",page_edit_allowed:false}});
    await expect(task).rejects.toMatchObject({reason:"source_conflict"});
    expect(write).not.toHaveBeenCalled();
  });
  it("holds the real autosave controller after a lock rejection and does not resume old bytes on unlock", async () => {
    vi.useFakeTimers();
    try {
      let current = getPageEditCapability({mode:"server",actorId:"owner",selectionCurrent:true,capabilityCurrent:false,page:{locked:true,locked_by:"other"}});
      const writes = vi.fn();
      const controller = createAutoSaveController(async body => {assertDocumentPageEditable(current);writes(body);},()=>{},20,error => !isDraftSaveHeldError(error));
      controller.scheduleAutoSave("retained original body"); await vi.advanceTimersByTimeAsync(20);
      current = getPageEditCapability({mode:"server",actorId:"owner",selectionCurrent:true,page:{locked:false}});
      await vi.advanceTimersByTimeAsync(200); expect(writes).not.toHaveBeenCalled();
      expect(await controller.saveNow("retained original body")).toBe(false);
      controller.dispose();
    } finally {vi.useRealTimers();}
  });
});


describe("actual page serializer CSS source retention", () => {
  const cases = ["", " \t\r\n", ".note { color: red; }", ".note {content:'\0';}\r\n", "@import url('https://blocked.invalid/style.css');\r\n.note{color:red;}", "x".repeat(21_000)];
  for (const [i, css] of cases.entries()) it(`preserves opaque original${i} across ordinary body/summary/width saves`, () => {
    const block = `<!-- STARLANE:custom-css -->\r\n${css}\r\n<!-- /STARLANE:custom-css -->`;
    const parsed = parseDocumentContent(`---\r\nsynopsis: "Before"\r\n---\r\n# Page\r\n\r\nBody\r\n${block}`, "Page");
    expect(parsed.body).toBe("Body");
    for (const width of ["standard","wide","full"] as const) {
      const saved = serializeDocumentContent("Edited body", 'Summary "quoted"', width, parsed.customCss ?? "", parsed.cssProvenance);
      expect(saved).toContain(block);
      expect(saved.match(/<!-- STARLANE:custom-css -->/g)).toHaveLength(1);
      const loaded = parseDocumentContent(saved);
      expect(loaded.cssProvenance).toEqual(parsed.cssProvenance);
      expect(loaded.body).toBe("Edited body");
      expect(loaded.synopsis).toBe('Summary "quoted"');
    }
    if (css.includes("@import")) expect(parsed.customCss).toBeUndefined();
  });
  it("keeps ambiguous whole originals and refuses guessed automatic composition", () => {
    for (const raw of ["<!-- starlane:custom-css -->orphan", "<!-- /starlane:custom-css -->reversed<!-- starlane:custom-css -->", "<!-- starlane:custom-css -->a<!-- starlane:custom-css -->b<!-- /starlane:custom-css -->"]) {
      const parsed = parseDocumentContent(raw);
      expect(parsed.cssProvenance).toEqual({schemaVersion:1,kind:"ambiguous",originalSource:raw});
      expect(() => serializeDocumentContent("Local edited body", "", "wide", parsed.customCss ?? "", parsed.cssProvenance)).toThrow("CSS markers need source review");
    }
  });
  it("holds newly introduced edited-body markers instead of stripping them", () => {
    const parsed = parseDocumentContent("Body");
    expect(() => serializeDocumentContent("Edited <!-- starlane:custom-css -->", "", "standard", "", parsed.cssProvenance)).toThrow("CSS markers need source review");
  });
  it.each(["", "\r\n@import url(x);\r\n", ".notice {font-weight:700}"])("holds legacy CSS intent even when its rendering agrees (%#)", rawCss => {
    const raw=`Body\n<!-- starlane:custom-css -->${rawCss}<!-- /starlane:custom-css -->`;
    const checkpoint={snapshotId:"kept",capturedAt:"now",baseDraftUpdatedAt:"then",title:"Page",content:"Edited",synopsis:"",pageWidth:"wide" as const,customCss:rawCss.includes("font-weight") ? rawCss : "",baseDraftSource:{draftId:"draft",title:"Page",content:raw,icon:null,coverImage:null}};
    const retained=JSON.stringify(checkpoint);
    expect(recoveryCompositionHoldReason(checkpoint)).toContain("no verified CSS intent");
    expect(() => serializeRecoveryCheckpoint(checkpoint)).toThrow("no verified CSS intent");
    expect(recoveryCheckpointMatchesSaved(checkpoint,{title:"Page",content:"Edited"} as never)).toBe(false);
    expect(JSON.stringify(checkpoint)).toBe(retained);
  });
  it("holds ambiguous composition for mutation while keeping the whole checkpoint available for read-only review", () => {
    const originalSource="Body<!-- starlane:custom-css -->orphan";
    const checkpoint={snapshotId:"kept",capturedAt:"now",baseDraftUpdatedAt:"then",title:"Page",content:"Edited body",synopsis:"",pageWidth:"standard" as const,customCss:"",cssProvenance:{schemaVersion:1 as const,kind:"ambiguous" as const,originalSource}};
    expect(recoveryCompositionHoldReason(checkpoint)).toContain("CSS markers need source review");
    expect(JSON.parse(JSON.stringify(checkpoint)).cssProvenance.originalSource).toBe(originalSource);
    expect(checkpoint.content).toBe("Edited body");
  });
  it("preserves existing legacy no-base and marker-free-base recovery behavior", () => {
    const checkpoint={snapshotId:"kept",capturedAt:"now",baseDraftUpdatedAt:"then",title:"Page",content:"Edited",synopsis:"",pageWidth:"standard" as const,customCss:""};
    expect(recoveryCompositionHoldReason(checkpoint)).toBeUndefined();
    expect(serializeRecoveryCheckpoint(checkpoint)).toBe("Edited");
    const markerFree={...checkpoint,baseDraftSource:{draftId:"draft",title:"Page",content:"Original",icon:null,coverImage:null}};
    expect(recoveryCompositionHoldReason(markerFree)).toBeUndefined();
    expect(serializeRecoveryCheckpoint(markerFree)).toBe("Edited");
  });
  it("composes validated new exact provenance without inferring legacy intent", () => {
    const rawBlock="<!-- starlane:custom-css -->\r\n@import url(x);\0<!-- /starlane:custom-css -->";
    const checkpoint={snapshotId:"kept",capturedAt:"now",baseDraftUpdatedAt:"then",title:"Page",content:"Edited",synopsis:"",pageWidth:"standard" as const,customCss:"",cssProvenance:{schemaVersion:1 as const,kind:"single" as const,rawBlock}};
    expect(recoveryCompositionHoldReason(checkpoint)).toBeUndefined();
    expect(serializeRecoveryCheckpoint(checkpoint)).toContain(rawBlock);
  });
  it("does not turn an old successful ACK into clearing newer ambiguous evidence", () => {
    const checkpoint={snapshotId:"newer",capturedAt:"now",baseDraftUpdatedAt:"then",title:"Page",content:"New edited body",synopsis:"",pageWidth:"standard" as const,customCss:"",cssProvenance:{schemaVersion:1 as const,kind:"ambiguous" as const,originalSource:"<!-- starlane:custom-css -->original"}};
    expect(recoveryCheckpointMatchesSaved(checkpoint,{title:"Page",content:"Previous save"} as never)).toBe(false);
  });
});
