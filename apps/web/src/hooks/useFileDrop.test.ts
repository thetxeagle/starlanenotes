import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { dataClient } from "@/lib/dataClient";
import { handleDroppedFile } from "./useFileDrop";

vi.mock("@/lib/dataClient", () => ({
  dataClient: {
    saveBrowserAsset: vi.fn(),
  },
}));

class MockFileReader {
  result: string | ArrayBuffer | null = null;
  error: Error | null = null;
  onload: (() => void) | null = null;
  onerror: (() => void) | null = null;

  readAsDataURL(file: File) {
    this.result = `data:${file.type || "application/octet-stream"};base64,ZmFrZQ==`;
    this.onload?.();
  }

  readAsText(file: File) {
    this.result = `# ${file.name}\n\nDropped Markdown`;
    this.onload?.();
  }
}

function createEditorMock() {
  const run = vi.fn();
  const setImage = vi.fn(() => ({ run }));
  const insertContent = vi.fn(() => ({ run }));
  const focus = vi.fn(() => ({ setImage, insertContent }));
  const chain = vi.fn(() => ({ focus }));

  return {
    editor: { chain } as unknown as TipTapEditor,
    chain,
    focus,
    setImage,
    insertContent,
    run,
  };
}

describe("handleDroppedFile", () => {
  beforeEach(() => {
    vi.stubGlobal("FileReader", MockFileReader);
    vi.mocked(dataClient.saveBrowserAsset).mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uploads browser-dropped images and inserts the stored asset URL", async () => {
    vi.mocked(dataClient.saveBrowserAsset).mockResolvedValue({
      id: "asset-image",
      space_id: "space-1",
      document_id: "doc-1",
      name: "diagram.png",
      mime_type: "image/png",
      size: 12,
      source: "browser",
      url: "starlane-asset://asset-image",
      created_at: "2026-07-15T12:00:00.000Z",
    });
    const mock = createEditorMock();

    await handleDroppedFile(
      new File(["fake image"], "diagram.png", { type: "image/png" }),
      mock.editor,
      { space_id: "space-1", document_id: "doc-1" },
    );

    expect(dataClient.saveBrowserAsset).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "diagram.png",
        mime_type: "image/png",
        size: 10,
        url: "data:image/png;base64,ZmFrZQ==",
        space_id: "space-1",
        document_id: "doc-1",
      }),
    );
    expect(mock.setImage).toHaveBeenCalledWith({
      src: "starlane-asset://asset-image",
      alt: "diagram.png",
    });
  });

  it("inserts dropped Markdown text without saving an attachment asset", async () => {
    const mock = createEditorMock();

    await handleDroppedFile(new File(["# Imported"], "import.md"), mock.editor, {});

    expect(dataClient.saveBrowserAsset).not.toHaveBeenCalled();
    expect(mock.insertContent).toHaveBeenCalledWith("# import.md\n\nDropped Markdown");
  });

  it("uploads generic files and inserts an escaped attachment card", async () => {
    vi.mocked(dataClient.saveBrowserAsset).mockResolvedValue({
      id: "asset-pdf",
      space_id: "space-1",
      document_id: "doc-1",
      name: 'budget & "launch".pdf',
      mime_type: "application/pdf",
      size: 2048,
      source: "browser",
      url: "starlane-asset://asset-pdf",
      created_at: "2026-07-15T12:00:00.000Z",
    });
    const mock = createEditorMock();

    await handleDroppedFile(
      new File(["fake pdf"], 'budget & "launch".pdf', { type: "application/pdf" }),
      mock.editor,
      { space_id: "space-1", document_id: "doc-1" },
    );

    expect(dataClient.saveBrowserAsset).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'budget & "launch".pdf',
        mime_type: "application/pdf",
        url: "data:application/pdf;base64,ZmFrZQ==",
      }),
    );
    expect(mock.insertContent).toHaveBeenCalledWith(
      expect.stringContaining("budget &amp; &quot;launch&quot;.pdf"),
    );
    expect(mock.insertContent).toHaveBeenCalledWith(expect.stringContaining("2.0 KB"));
  });
});
