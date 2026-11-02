import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  filterEditorWikiLinkDocuments,
  formatWikiLinkUpdatedAt,
  getWikiLinkDocumentIdLabel,
  EditorWikiLinkDialog,
} from "./EditorWikiLinkDialog";

const documents = [
  { id: "plan", title: "Project plan", folderName: "Projects" },
  { id: "notes", title: "Meeting notes", icon: "📝" },
  { id: "archive", title: "Archived project plan" },
];

describe("EditorWikiLinkDialog", () => {
  it("filters documents by a case-insensitive search", () => {
    expect(filterEditorWikiLinkDocuments(documents, "PLAN")).toEqual([
      documents[0],
      documents[2],
    ]);
  });

  it("excludes the current page and ranks prefix matches before contains matches", () => {
    expect(filterEditorWikiLinkDocuments(documents, "project", "plan")).toEqual([
      documents[2],
    ]);
    expect(filterEditorWikiLinkDocuments(documents, "project")).toEqual([
      documents[0],
      documents[2],
    ]);
  });

  it("formats useful update metadata without exposing invalid timestamps", () => {
    expect(formatWikiLinkUpdatedAt("not-a-date")).toBe("");
    expect(formatWikiLinkUpdatedAt("2026-10-03T17:04:47Z")).toMatch(/^Updated .*2026/);
  });

  it("shows stable ID prefixes only when matching pages have duplicate titles", () => {
    const duplicates = [
      { id: "abcd-111", title: "Project plan", folderName: "Projects" },
      { id: "abce-222", title: " project PLAN ", folderName: "Projects" },
      { id: "unique-333", title: "Meeting notes" },
    ];

    expect(getWikiLinkDocumentIdLabel(duplicates[0], duplicates)).toBe("Page ID abcd");
    expect(getWikiLinkDocumentIdLabel(duplicates[1], duplicates)).toBe("Page ID abce");
    expect(getWikiLinkDocumentIdLabel(duplicates[2], duplicates)).toBeNull();
  });

  it("renders the stable ID beside duplicate-title choices", () => {
    const duplicateTitleDocuments = [
      { id: "plan-a", title: "Project plan", folderName: "Projects" },
      { id: "plan-b", title: "Project plan", folderName: "Projects" },
    ];
    const html = renderToStaticMarkup(
      <EditorWikiLinkDialog
        documents={duplicateTitleDocuments}
        query=""
        loading={false}
        error={false}
        onQueryChange={() => undefined}
        onSelect={() => undefined}
        onRetry={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(html).toContain("Page ID plan-a");
    expect(html).toContain("Page ID plan-b");
  });

  it("uses the shared accessible dialog shell and document choices", () => {
    const html = renderToStaticMarkup(
      <EditorWikiLinkDialog
        documents={documents}
        currentDocumentId="notes"
        query=""
        loading={false}
        error={false}
        onQueryChange={() => undefined}
        onSelect={() => undefined}
        onRetry={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="editor-wiki-link-title"');
    expect(html).toContain('aria-describedby="editor-wiki-link-description"');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("Project plan");
    expect(html).not.toContain("Meeting notes");
    expect(html).toContain("Projects");
    expect(html).toContain("Space root");
    expect(html).toContain(">Cancel<");
  });

  it("keeps load failures recoverable inside the dialog", () => {
    const html = renderToStaticMarkup(
      <EditorWikiLinkDialog
        documents={[]}
        query=""
        loading={false}
        error
        onQueryChange={() => undefined}
        onSelect={() => undefined}
        onRetry={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(html).toContain("Unable to load pages.");
    expect(html).toContain(">Retry<");
  });

  it("owns retry generations while preserving portable wiki-link output", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/editor/Editor.tsx"), "utf8");

    expect(source).toContain("const requestId = ++wikiLinkRequestIdRef.current");
    expect(source.match(/requestId !== wikiLinkRequestIdRef\.current/g)).toHaveLength(2);
    expect(source).toContain("dataClient.listFolders(spaceId).catch(() => [])");
    expect(source).toContain("currentDocumentId={docId}");
    expect(source).toContain("onRetry={loadWikiLinkDocuments}");
    expect(source).toContain(
      ".insertContent(serializeWikiLinkMarkdown(document.title, document.id))",
    );
  });

  it("moves from the search field into results with directional keys", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorWikiLinkDialog.tsx"),
      "utf8",
    );

    expect(source.match(/onKeyDown=\{handleListboxKeyDown\}/g)).toHaveLength(2);
  });
});
