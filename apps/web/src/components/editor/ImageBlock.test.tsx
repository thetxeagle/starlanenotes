import { Schema } from "prosemirror-model";
import { EditorState } from "prosemirror-state";
import type { ReactNodeViewProps } from "@tiptap/react";
import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  captureImageAttachmentHistoryTarget,
  getImageAlignmentActionLabels,
  getImageAlignmentClass,
  getImageAlignmentMarkdownTitle,
  getImageBlockActionLabels,
  normalizeImageAlignment,
  normalizeImageCaption,
} from "./ImageBlock";

describe("ImageBlock", () => {
  it("stores a concise portable caption for the Markdown alt field", () => {
    expect(normalizeImageCaption("  Launch\n  checklist   preview  ")).toBe(
      "Launch checklist preview",
    );
    expect(normalizeImageCaption("x".repeat(281))).toHaveLength(280);
  });

  it("keeps named open/download controls in both modes and limits replace/delete to editing", () => {
    expect(getImageBlockActionLabels(false)).toEqual(["Open image", "Download image"]);
    expect(getImageBlockActionLabels(true)).toEqual([
      "Open image",
      "Download image",
      "Align image left",
      "Align image center",
      "Align image right",
      "Add to gallery",
      "Add to carousel",
      "Replace image",
      "Delete image",
    ]);
  });

  it("moves image actions into one anchored menu and shows caption editing after selection", () => {
    const imageSource = readFileSync(
      resolve(process.cwd(), "src/components/editor/ImageBlock.tsx"),
      "utf8",
    );
    const menuSource = readFileSync(
      resolve(process.cwd(), "src/components/editor/EditorMediaActionMenu.tsx"),
      "utf8",
    );

    expect(imageSource).toMatch(/<EditorMediaActionMenu\s+label="Image actions"/);
    expect(imageSource).toContain("editor.isEditable && selected");
    expect(imageSource).not.toContain('aria-label="Image alignment"');
    expect(menuSource).toContain('role="menu"');
    expect(menuSource).toContain('role="menuitem"');
    expect(menuSource).toContain("action.destructive");
  });

  it("uses a portable marker for alignment and keeps the control set touch-safe", () => {
    expect(normalizeImageAlignment("center")).toBe("center");
    expect(normalizeImageAlignment("sideways")).toBe("left");
    expect(getImageAlignmentClass("right")).toBe("ml-auto");
    expect(getImageAlignmentMarkdownTitle("left")).toBe("");
    expect(getImageAlignmentMarkdownTitle("center")).toBe(' "starlane:align=center"');
    expect(getImageAlignmentActionLabels()).toEqual([
      "Align image left",
      "Align image center",
      "Align image right",
    ]);
  });
});

describe("original image attachment acknowledgement target", () => {
  function fixture() {
    const schema = new Schema({
      nodes: {
        doc: { content: "block*" },
        text: { group: "inline" },
        image: {
          group: "block",
          attrs: {
            attachmentId: {},
            src: {},
            alt: { default: "Authored caption" },
            alignment: { default: "right" },
          },
        },
      },
    });
    let state = EditorState.create({
      schema,
      doc: schema.node("doc", null, [
        schema.node("image", { attachmentId: "old-version", src: "blob:original" }),
      ]),
    });
    let position: number | undefined = 0,
      currentVersion = "old-version",
      editable = true,
      destroyed = false;
    const editor = {
      get state() {
        return state;
      },
      get isEditable() {
        return editable;
      },
      get isDestroyed() {
        return destroyed;
      },
    } as unknown as ReactNodeViewProps["editor"];
    const update = (attrs: Record<string, unknown>) => {
      const node = state.doc.nodeAt(0)!;
      state = state.apply(state.tr.setNodeMarkup(0, undefined, { ...node.attrs, ...attrs }));
    };
    const target = captureImageAttachmentHistoryTarget(
      editor,
      () => position,
      () => currentVersion,
      update,
    );
    return {
      target,
      editor,
      move: () => {
        position = 1;
      },
      rerender: () => {
        currentVersion = "other-version";
      },
      replaceNode: () => update({ attachmentId: "other-version" }),
      lock: () => {
        editable = false;
      },
      destroy: () => {
        destroyed = true;
      },
    };
  }
  it("updates only the captured image pin/preview and preserves authored caption/alignment", () => {
    const f = fixture();
    f.target.replace("blob:new-proof", "new-version");
    expect(f.editor.state.doc.nodeAt(0)!.attrs).toMatchObject({
      attachmentId: "new-version",
      src: "blob:new-proof",
      alt: "Authored caption",
      alignment: "right",
    });
    expect(() => f.target.replace("blob:again", "third-version")).toThrow(
      "Original image block changed",
    );
  });
  it.each(["move", "rerender", "replaceNode", "destroy"] as const)(
    "refuses delayed adoption after %s even with the same original callback",
    (kind) => {
      const f = fixture();
      f[kind]();
      expect(() => f.target.replace("blob:new", "new-version")).toThrow(
        "Original image block changed",
      );
      expect(f.editor.state.doc.nodeAt(0)!.attrs.src).toBe("blob:original");
    },
  );
  it("allows original read-only history proof but refuses mutation after edit permission changes", () => {
    const f = fixture();
    f.lock();
    expect(() => f.target.verify()).not.toThrow();
    expect(() => f.target.replace("blob:new", "new-version")).toThrow("no longer editable");
    expect(f.editor.state.doc.nodeAt(0)!.attrs.attachmentId).toBe("old-version");
  });
});
