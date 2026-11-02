/**
 * Adapted from slash-editor 0.7.0 `packages/core/src/block-id.ts` (MIT).
 * Copyright 2025-present Nhat Bui.
 *
 * This narrow, source-owned fork avoids loading SlashEditor's complete root
 * barrel for runtime identity alone. Starlane block IDs are intentionally
 * ephemeral until a portable persistent representation is approved.
 */
import type { GlobalAttributes, Mark, Node, Range } from "@tiptap/core";
import {
  callOrReturn,
  combineTransactionSteps,
  Extension,
  findChildrenInRange,
  getChangedRanges,
  getExtensionField,
} from "@tiptap/core";
import type { Node as ProseMirrorNode, Schema } from "@tiptap/pm/model";
import { Plugin, PluginKey, type Transaction } from "@tiptap/pm/state";

export interface RuntimeBlockIdOptions {
  types: string[] | "auto";
  exclude: string[];
}

export const runtimeBlockIdPluginKey = new PluginKey<null>("blockId");

const ID_LENGTH = 12;
const ID_ALPHABET = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz-_";

function generateRuntimeBlockId(): string {
  const bytes = new Uint8Array(ID_LENGTH);
  crypto.getRandomValues(bytes);
  let id = "";
  for (const byte of bytes) id += ID_ALPHABET[byte % ID_ALPHABET.length];
  return id;
}

function isBlockExpression(value: unknown): boolean {
  return typeof value === "string" && /\bblock\b/.test(value);
}

function deriveBlockTypes(extensions: (Node | Mark)[], exclude: string[]): string[] {
  const types: string[] = [];

  for (const extension of extensions) {
    if (extension.type !== "node" || exclude.includes(extension.name)) continue;
    const context = {
      name: extension.name,
      options: extension.options,
      storage: extension.storage,
    };
    if (callOrReturn(getExtensionField(extension, "topNode", context))) continue;
    const group = callOrReturn(getExtensionField(extension, "group", context));
    const content = callOrReturn(getExtensionField(extension, "content", context));
    if (isBlockExpression(group) || isBlockExpression(content)) types.push(extension.name);
  }

  return types;
}

function collectIdCounts(doc: ProseMirrorNode, types: Set<string>): Map<string, number> {
  const counts = new Map<string, number>();
  doc.descendants((node) => {
    if (types.has(node.type.name) && typeof node.attrs.id === "string") {
      counts.set(node.attrs.id, (counts.get(node.attrs.id) ?? 0) + 1);
    }
  });
  return counts;
}

function blockIdTypes(schema: Schema): Set<string> {
  const types = new Set<string>();
  for (const [name, type] of Object.entries(schema.nodes)) {
    if (type.spec.attrs && "id" in type.spec.attrs) types.add(name);
  }
  return types;
}

function assignRuntimeBlockIds(
  tr: Transaction,
  ranges: readonly Range[],
  types: Set<string>,
  counts: Map<string, number>,
): boolean {
  let changed = false;

  for (const range of ranges) {
    for (const { node, pos } of findChildrenInRange(tr.doc, range, (candidate) =>
      types.has(candidate.type.name),
    )) {
      const id = node.attrs.id;
      if (typeof id === "string" && (counts.get(id) ?? 0) <= 1) continue;
      if (typeof id === "string") counts.set(id, (counts.get(id) ?? 1) - 1);
      const nextId = generateRuntimeBlockId();
      counts.set(nextId, 1);
      tr.setNodeAttribute(pos, "id", nextId);
      changed = true;
    }
  }

  return changed;
}

export const StarlaneRuntimeBlockId = Extension.create<RuntimeBlockIdOptions>({
  name: "blockId",

  addOptions() {
    return { types: "auto", exclude: [] };
  },

  addGlobalAttributes() {
    const types =
      this.options.types === "auto"
        ? deriveBlockTypes(this.extensions, this.options.exclude)
        : this.options.types;
    const attributes: GlobalAttributes = types.map((type) => ({
      types: [type],
      attributes: {
        id: {
          default: null,
          parseHTML: (element) => element.getAttribute("data-block-id"),
          renderHTML: (attrs) => ({
            "data-block-type": type,
            ...(typeof attrs.id === "string" ? { "data-block-id": attrs.id } : {}),
          }),
        },
      },
    }));
    return attributes;
  },

  onCreate() {
    const types = blockIdTypes(this.editor.schema);
    const tr = this.editor.state.tr;
    const counts = collectIdCounts(tr.doc, types);
    if (assignRuntimeBlockIds(tr, [{ from: 0, to: tr.doc.content.size }], types, counts)) {
      tr.setMeta("addToHistory", false);
      this.editor.view.dispatch(tr);
    }
  },

  addProseMirrorPlugins() {
    return [
      new Plugin({
        key: runtimeBlockIdPluginKey,
        appendTransaction: (transactions, oldState, newState) => {
          if (!transactions.some((tr) => tr.docChanged)) return null;
          const types = blockIdTypes(this.editor.schema);
          const tr = newState.tr;
          const counts = collectIdCounts(tr.doc, types);
          const ranges = getChangedRanges(
            combineTransactionSteps(oldState.doc, [...transactions]),
          ).map((change) => change.newRange);
          return assignRuntimeBlockIds(tr, ranges, types, counts) ? tr : null;
        },
      }),
    ];
  },
});

export function runtimeBlockId(options: Partial<RuntimeBlockIdOptions> = {}) {
  return StarlaneRuntimeBlockId.configure(options);
}
