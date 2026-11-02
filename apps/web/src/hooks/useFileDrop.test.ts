import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Editor as TipTapEditor } from "@tiptap/react";
import { dataClient } from "@/lib/dataClient";
import {
  handleDroppedFile,
  handleNativeDroppedFile,
  createFileDropListenerOwner,
  runFileDropGroup,
  isNestedFileDropBoundary,
  runPendingFileDrop,
} from "./useFileDrop";
import { directUploadAcknowledgementId, markDirectUploadAdopted } from "@/lib/directUploadReview";

vi.mock("@/lib/dataClient", () => ({
  dataClient: {
    saveDocumentDataAsset: vi.fn(),
    importAssetFromPath: vi.fn(),
  },
}));

vi.mock("@/lib/directUploadReview", () => ({
  directUploadAcknowledgementId: vi.fn(() => null),
  markDirectUploadAdopted: vi.fn(),
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
  const run = vi.fn(() => true);
  const insertContent = vi.fn(() => ({ run }));
  const insertContentAt = vi.fn(() => ({ run }));
  const focus = vi.fn(() => ({ insertContent, insertContentAt }));
  const chain = vi.fn(() => ({ focus }));

  return {
    editor: {
      chain,
      isEditable: true,
      isDestroyed: false,
      state: { doc: { content: { size: 100 } }, selection: { from: 1, to: 1 } },
      on: vi.fn(),
      off: vi.fn(),
    } as unknown as TipTapEditor,
    chain,
    focus,
    insertContent,
    insertContentAt,
    run,
  };
}

describe("handleDroppedFile", () => {
  beforeEach(() => {
    vi.stubGlobal("FileReader", MockFileReader);
    vi.mocked(dataClient.saveDocumentDataAsset).mockReset();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uploads browser-dropped images and inserts the stored asset URL", async () => {
    vi.mocked(dataClient.saveDocumentDataAsset).mockResolvedValue({
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

    expect(dataClient.saveDocumentDataAsset).toHaveBeenCalledWith(
      expect.objectContaining({
        name: "diagram.png",
        mime_type: "image/png",
        data_url: "data:image/png;base64,ZmFrZQ==",
        space_id: "space-1",
        document_id: "doc-1",
      }),
    );
    expect(mock.insertContentAt).toHaveBeenCalledWith(
      1,
      expect.arrayContaining([
        expect.objectContaining({
          type: "image",
          attrs: expect.objectContaining({
            src: "starlane-asset://asset-image",
            alt: "diagram.png",
          }),
        }),
      ]),
    );
  });

  it("inserts dropped Markdown text without saving an attachment asset", async () => {
    const mock = createEditorMock();

    await handleDroppedFile(new File(["# Imported"], "import.md"), mock.editor, {});

    expect(dataClient.saveDocumentDataAsset).not.toHaveBeenCalled();
    expect(mock.insertContentAt).toHaveBeenCalledWith(1, "# import.md\n\nDropped Markdown");
  });

  it("uploads generic files and inserts an escaped attachment card", async () => {
    vi.mocked(dataClient.saveDocumentDataAsset).mockResolvedValue({
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

    expect(dataClient.saveDocumentDataAsset).toHaveBeenCalledWith(
      expect.objectContaining({
        name: 'budget & "launch".pdf',
        mime_type: "application/pdf",
        data_url: "data:application/pdf;base64,ZmFrZQ==",
      }),
    );
    expect(mock.insertContentAt).toHaveBeenCalledWith(
      1,
      expect.stringContaining("budget &amp; &quot;launch&quot;.pdf"),
    );
    expect(mock.insertContentAt).toHaveBeenCalledWith(1, expect.stringContaining("2.0 KB"));
  });

  it("keeps server attachment identity when a file is dropped on the document", async () => {
    vi.mocked(dataClient.saveDocumentDataAsset).mockResolvedValue({
      id: "attachment-pdf",
      document_id: "doc-1",
      filename: "attachment-pdf.pdf",
      original_name: "field-guide.pdf",
      mime_type: "application/pdf",
      size_bytes: 12,
      url: "/api/v1/files/spaces/test/attachments/doc-1/attachment-pdf.pdf",
      created_at: "2026-08-18T12:00:00.000Z",
    });
    const mock = createEditorMock();

    await handleDroppedFile(new File(["server bytes"], "field-guide.pdf"), mock.editor, {
      space_id: "space-1",
      document_id: "doc-1",
    });

    expect(mock.insertContentAt).toHaveBeenCalledWith(
      1,
      expect.stringContaining('data-attachment-id="attachment-pdf"'),
    );
    expect(mock.insertContentAt).toHaveBeenCalledWith(
      1,
      expect.stringContaining("field-guide.pdf"),
    );
  });
});

describe("isNestedFileDropBoundary", () => {
  it("keeps dialog-owned drops out of the document-level drop handler", () => {
    const closest = vi.fn(() => ({ dataset: { starlaneFileDropBoundary: "true" } }));
    const target = { closest } as unknown as EventTarget;

    expect(isNestedFileDropBoundary(target)).toBe(true);
    expect(closest).toHaveBeenCalledWith('[data-starlane-file-drop-boundary="true"]');
  });

  it("allows ordinary document surface drops", () => {
    const target = { closest: vi.fn(() => null) } as unknown as EventTarget;

    expect(isNestedFileDropBoundary(target)).toBe(false);
  });
});

describe("pending file-drop adoption fence", () => {
  it("stays pending through a genuine deferred asset upload and editor insertion", async () => {
    vi.stubGlobal("FileReader", MockFileReader);
    let finish!: (asset: Awaited<ReturnType<typeof dataClient.saveDocumentDataAsset>>) => void;
    vi.mocked(dataClient.saveDocumentDataAsset).mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const pending = { current: 0 };
    const mock = createEditorMock();
    const operation = runPendingFileDrop(pending, () =>
      handleDroppedFile(new File(["image"], "pending.png", { type: "image/png" }), mock.editor, {}),
    );
    await Promise.resolve();
    expect(pending.current).toBe(1);
    expect(mock.insertContentAt).not.toHaveBeenCalled();
    finish({
      id: "asset",
      name: "pending.png",
      mime_type: "image/png",
      size: 5,
      source: "browser",
      url: "starlane-asset://asset",
      created_at: "now",
    });
    await operation;
    expect(mock.insertContentAt).toHaveBeenCalled();
    expect(pending.current).toBe(0);
    vi.unstubAllGlobals();
  });

  it("keeps overlapping drops fenced until the last operation settles", async () => {
    const pending = { current: 0 };
    let finishFirst!: () => void, finishSecond!: () => void;
    const first = runPendingFileDrop(
      pending,
      () =>
        new Promise<void>((resolve) => {
          finishFirst = resolve;
        }),
    );
    const second = runPendingFileDrop(
      pending,
      () =>
        new Promise<void>((resolve) => {
          finishSecond = resolve;
        }),
    );
    expect(pending.current).toBe(2);
    finishFirst();
    await first;
    expect(pending.current).toBe(1);
    finishSecond();
    await second;
    expect(pending.current).toBe(0);
  });

  it("releases failed and synchronously thrown imports without masking the failure", async () => {
    const pending = { current: 0 };
    await expect(
      runPendingFileDrop(pending, async () => {
        throw new Error("upload refused");
      }),
    ).rejects.toThrow("upload refused");
    expect(pending.current).toBe(0);
    await expect(
      runPendingFileDrop(pending, () => {
        throw new Error("read refused");
      }),
    ).rejects.toThrow("read refused");
    expect(pending.current).toBe(0);
  });
});

function pending<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

describe("drop insertion ownership across awaits", () => {
  beforeEach(() => {
    vi.stubGlobal("FileReader", MockFileReader);
    vi.mocked(dataClient.saveDocumentDataAsset).mockReset();
    vi.mocked(dataClient.importAssetFromPath).mockReset();
    vi.mocked(directUploadAcknowledgementId).mockReset().mockReturnValue(null);
    vi.mocked(markDirectUploadAdopted).mockReset();
  });
  afterEach(() => vi.unstubAllGlobals());
  const context = { space_id: "space", document_id: "page" };
  const local = {
    id: "asset",
    space_id: "space",
    document_id: "page",
    name: "x.png",
    mime_type: "image/png",
    size: 1,
    source: "browser" as const,
    url: "starlane-asset://asset",
    created_at: "now",
  };

  it("does not upload after ownership changes during FileReader", async () => {
    let finish!: () => void;
    class DeferredReader extends MockFileReader {
      readAsDataURL() {
        this.result = "data:image/png;base64,eA==";
        finish = () => this.onload?.();
      }
    }
    vi.stubGlobal("FileReader", DeferredReader);
    const mock = createEditorMock();
    let current = true;
    const task = handleDroppedFile(new File(["x"], "x.png"), mock.editor, context, () => {
      if (!current) throw new Error("scope changed");
    });
    current = false;
    finish();
    await expect(task).rejects.toThrow("scope changed");
    expect(dataClient.saveDocumentDataAsset).not.toHaveBeenCalled();
    expect(mock.run).not.toHaveBeenCalled();
  });

  it("retains a completed original upload without insertion or adoption after scope loss", async () => {
    const upload = pending<typeof local>();
    vi.mocked(dataClient.saveDocumentDataAsset).mockReturnValue(upload.promise);
    const mock = createEditorMock();
    let current = true;
    const task = handleDroppedFile(new File(["x"], "x.png"), mock.editor, context, () => {
      if (!current) throw new Error("scope changed");
    });
    await Promise.resolve();
    expect(dataClient.saveDocumentDataAsset).toHaveBeenCalledOnce();
    current = false;
    upload.resolve(local);
    await expect(task).rejects.toThrow("scope changed");
    expect(mock.run).not.toHaveBeenCalled();
    expect(markDirectUploadAdopted).not.toHaveBeenCalled();
  });

  it("refuses a Markdown read when the editor becomes readonly", async () => {
    let finish!: () => void;
    class DeferredReader extends MockFileReader {
      readAsText() {
        this.result = "typed source";
        finish = () => this.onload?.();
      }
    }
    vi.stubGlobal("FileReader", DeferredReader);
    const mock = createEditorMock();
    const task = handleDroppedFile(new File(["x"], "x.md"), mock.editor, context);
    Object.assign(mock.editor, { isEditable: false });
    finish();
    await expect(task).rejects.toThrow("no longer editable");
    expect(mock.run).not.toHaveBeenCalled();
  });

  it("never converts failed native import ownership into image fallback", async () => {
    const imported = pending<never>();
    vi.mocked(dataClient.importAssetFromPath).mockReturnValue(imported.promise);
    const mock = createEditorMock();
    let current = true;
    const task = handleNativeDroppedFile("/owned/x.png", mock.editor, context, () => {
      if (!current) throw new Error("scope changed");
    });
    current = false;
    imported.reject(new Error("import failed"));
    await expect(task).rejects.toThrow("scope changed");
    expect(mock.run).not.toHaveBeenCalled();
  });

  it("keeps native fallback for a genuine import failure in the same scope", async () => {
    vi.mocked(dataClient.importAssetFromPath).mockRejectedValue(new Error("unsupported import"));
    const mock = createEditorMock();
    await handleNativeDroppedFile("/owned/x.png", mock.editor, context, () => {});
    expect(mock.run).toHaveBeenCalledOnce();
    expect(mock.insertContentAt).toHaveBeenCalledWith(
      1,
      expect.arrayContaining([
        expect.objectContaining({
          type: "image",
          attrs: expect.objectContaining({ src: "asset://localhost/%2Fowned%2Fx.png" }),
        }),
      ]),
    );
  });

  it("does not insert native Markdown fallback after a deferred read loses scope", async () => {
    const read = pending<string>();
    const mock = createEditorMock();
    let current = true;
    const task = handleNativeDroppedFile(
      "/owned/x.md",
      mock.editor,
      context,
      () => {
        if (!current) throw new Error("scope changed");
      },
      () => read.promise,
    );
    current = false;
    read.reject(new Error("read failed"));
    await expect(task).rejects.toThrow("scope changed");
    expect(mock.run).not.toHaveBeenCalled();
  });

  it("stops the remaining drop group after the original verifier fails", async () => {
    let current = true;
    const processed: string[] = [];
    await expect(
      runFileDropGroup(
        ["one", "two"],
        () => {
          if (!current) throw new Error("scope changed");
        },
        async (item) => {
          processed.push(item);
          current = false;
        },
      ),
    ).rejects.toThrow("scope changed");
    expect(processed).toEqual(["one"]);
  });

  it("marks successful server insertion adopted only after run, and never after failed insertion", async () => {
    const asset = {
      id: "asset",
      document_id: "page",
      filename: "x.pdf",
      original_name: "x.pdf",
      mime_type: "application/pdf",
      size_bytes: 1,
      url: "/api/v1/files/owned",
      created_at: "now",
    };
    vi.mocked(dataClient.saveDocumentDataAsset).mockResolvedValue(asset);
    vi.mocked(directUploadAcknowledgementId).mockReturnValue("original-operation");
    const mock = createEditorMock();
    vi.mocked(markDirectUploadAdopted).mockImplementation(async () => {
      expect(mock.run).toHaveBeenCalledOnce();
    });
    await handleDroppedFile(new File(["x"], "x.pdf"), mock.editor, context);
    expect(markDirectUploadAdopted).toHaveBeenCalledWith(context, "original-operation");
    vi.mocked(markDirectUploadAdopted).mockClear();
    mock.run.mockReturnValue(false);
    await expect(handleDroppedFile(new File(["x"], "x.pdf"), mock.editor, context)).rejects.toThrow(
      "could not be inserted",
    );
    expect(markDirectUploadAdopted).not.toHaveBeenCalled();
  });

  it("cleans a listener that resolves after cleanup and cleans earlier partial registrations", () => {
    const owner = createFileDropListenerOwner();
    const first = vi.fn();
    const late = vi.fn();
    owner.retain(first);
    owner.stop();
    owner.retain(late);
    owner.stop();
    expect(first).toHaveBeenCalledOnce();
    expect(late).toHaveBeenCalledOnce();
    expect(owner.current()).toBe(false);
  });
});
