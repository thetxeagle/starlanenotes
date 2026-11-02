import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  activateEditorAssetOption,
  EditorUploadDialog,
  getEditorAssetOptionState,
  getEditorUploadChoices,
} from "./EditorUploadDialog";
import { getNextMenuItemIndex } from "@/components/ui/useMenuKeyboardNavigation";

describe("EditorUploadDialog", () => {
  it("prioritizes the requested source without omitting either choice", () => {
    expect(getEditorUploadChoices(25, true).map((choice) => choice.id)).toEqual(["image", "file"]);
    expect(getEditorUploadChoices(25, false).map((choice) => choice.id)).toEqual(["file", "image"]);
  });

  it("uses the shared listbox navigation for wrapping arrows and Home/End", () => {
    expect(getNextMenuItemIndex(2, 3, "ArrowDown")).toBe(0);
    expect(getNextMenuItemIndex(0, 3, "ArrowUp")).toBe(2);
    expect(getNextMenuItemIndex(1, 3, "Home")).toBe(0);
    expect(getNextMenuItemIndex(1, 3, "End")).toBe(2);
  });

  it("keeps the focused option as the sole tab stop without changing selection", () => {
    expect(getEditorAssetOptionState("asset-a", 0, "asset-b", "asset-a")).toEqual({
      tabIndex: -1,
      selected: true,
    });
    expect(getEditorAssetOptionState("asset-b", 1, "asset-b", "asset-a")).toEqual({
      tabIndex: 0,
      selected: false,
    });
    expect(getEditorAssetOptionState("asset-a", 0, null, null).tabIndex).toBe(0);
    expect(getEditorAssetOptionState("asset-b", 1, null, null).tabIndex).toBe(-1);
  });

  it("selects an option when its native button is activated", () => {
    const updates: string[] = [];
    activateEditorAssetOption(
      "asset-b",
      (id) => updates.push(`focused:${id}`),
      (id) => updates.push(`selected:${id}`),
    );

    expect(updates).toEqual(["focused:asset-b", "selected:asset-b"]);
  });

  it("uses the wide shared dialog shell and mockup-driven upload structure", () => {
    const html = renderToStaticMarkup(
      <EditorUploadDialog
        maxFileMb={25}
        existingAssets={[]}
        existingLoading={false}
        onFilesSelected={async () => []}
        onSelectExisting={() => undefined}
        onRefreshExisting={() => undefined}
        onCancel={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="editor-upload-title"');
    expect(html).toContain('aria-describedby="editor-upload-description"');
    expect(html).toContain('aria-label="Close upload dialog"');
    expect(html).toContain("max-w-6xl");
    expect(html).toContain("Upload an image or file");
    expect(html).toContain("choose an existing asset in this space");
    expect(html).toContain("Drop files here or click to browse");
    expect(html).toContain('aria-label="Browse for files"');
    expect(html).toContain('data-starlane-file-drop-boundary="true"');
    expect(html).toContain("Any file type");
    expect(html).toContain('role="tab"');
    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain("Library");
    expect(html).toContain("25 MB");
    expect(html).toContain(">Cancel<");
    expect(html).toContain('disabled=""');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
  });

  it("keeps successful uploads visible until the user confirms they are done", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorUploadDialog.tsx"),
      "utf8",
    );
    const successfulUploadBranch = source.slice(
      source.indexOf("if (failedResults.length === 0"),
      source.indexOf("const failedFiles ="),
    );

    expect(successfulUploadBranch).toContain("setUploadSummary");
    expect(successfulUploadBranch).not.toContain("onCancel()");
    expect(source).toContain("onClick={uploadComplete ? closeDialog : uploadFiles}");
    expect(source).toContain('? "Done"');
    expect(source).toContain('uploadComplete ? "Close" : "Cancel"');
  });

  it("wires the shared keyboard handler to native button options and their selection state", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorUploadDialog.tsx"),
      "utf8",
    );

    expect(source).toContain("useListboxKeyboardNavigation(existingAssetsListRef)");
    expect(source).toContain("ref={existingAssetsListRef}");
    expect(source).toContain("onKeyDown={handleExistingAssetsKeyDown}");
    expect(source).toContain('role="option"');
    expect(source).toContain("type=\"button\"");
    expect(source).toContain("tabIndex={optionState.tabIndex}");
    expect(source).toContain("aria-selected={optionState.selected}");
    expect(source).toContain("activateEditorAssetOption(");
  });
});
