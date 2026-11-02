/**
 * Interaction pattern adapted from slash-editor 0.7.0
 * `site/registry/components/block-handle.tsx` and `packages/react/src/use-block-drag.ts` (MIT).
 * Copyright 2025-present Nhat Bui.
 */
import type { Editor } from "@tiptap/core";
import type { EditorState } from "@tiptap/pm/state";
import type { Node as ProseMirrorNode } from "@tiptap/pm/model";
import { GripVertical, Plus } from "lucide-react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type PointerEvent as ReactPointerEvent,
} from "react";
import { createPortal } from "react-dom";
import { createBlockMoveTransaction } from "./slashEditorBlockMove";
import { EditorColumnsMenu } from "./EditorColumnsMenu";
import { setEditorSectionLayout } from "./EditorSection";
import { EditorBlockDragPreview } from "./EditorBlockDragPreview";

const DRAG_THRESHOLD_PX = 4;
const GUTTER_WIDTH_PX = 68;

export interface EditorGutterBlockTarget {
  node: ProseMirrorNode;
  from: number;
  to: number;
  index: number;
  parentStart: number;
  parentChildCount: number;
}

export interface EditorGutterBlockRect {
  target: EditorGutterBlockTarget;
  element: HTMLElement;
  blockRect: DOMRect;
  rect: DOMRect;
}

type HoveredBlock = EditorGutterBlockRect;

interface DropTarget {
  target: EditorGutterBlockTarget;
  position: number;
  top: number;
  left: number;
  width: number;
}

export function shouldShowEditorBlockGutter({
  showBlockChrome,
  mobileToolbar,
  readOnly,
}: {
  showBlockChrome: boolean;
  mobileToolbar: boolean;
  readOnly: boolean;
}) {
  return showBlockChrome && !mobileToolbar && !readOnly;
}

export function findRuntimeBlockTarget(
  state: EditorState,
  runtimeId: string,
): EditorGutterBlockTarget | null {
  let result: EditorGutterBlockTarget | null = null;

  state.doc.descendants((node, position) => {
    if (result || node.attrs.id !== runtimeId) return !result;
    const resolved = state.doc.resolve(position);
    result = {
      node,
      from: position,
      to: position + node.nodeSize,
      index: resolved.index(),
      parentStart: resolved.start(),
      parentChildCount: resolved.parent.childCount,
    };
    return false;
  });

  return result;
}

export function collectMovableBlockTargets(state: EditorState): EditorGutterBlockTarget[] {
  const targets: EditorGutterBlockTarget[] = [];

  state.doc.descendants((node, position, parent) => {
    const movable =
      parent?.type.name === "doc" ||
      parent?.type.name === "sectionColumn" ||
      node.type.name === "listItem" ||
      node.type.name === "taskItem";
    if (!movable) return;

    const resolved = state.doc.resolve(position);
    // A new lane's sole empty paragraph is a caret placeholder, not a second
    // independently movable object. Let the columns container own its gutter.
    // Authored blocks (including extra blank paragraphs) retain child controls.
    if (
      node.type.name === "paragraph" &&
      node.content.size === 0 &&
      parent?.type.name === "sectionColumn" &&
      parent.childCount === 1 &&
      resolved.depth > 0 &&
      resolved.node(resolved.depth - 1).type.name === "columns"
    ) {
      return false;
    }
    targets.push({
      node,
      from: position,
      to: position + node.nodeSize,
      index: resolved.index(),
      parentStart: resolved.start(),
      parentChildCount: resolved.parent.childCount,
    });
  });

  return targets;
}

function getBlockRowRect(element: HTMLElement, blockRect: DOMRect): DOMRect {
  const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT, {
    acceptNode: (node) => {
      const nonEditable = node.parentElement?.closest("[contenteditable='false']");
      return nonEditable && element.contains(nonEditable)
        ? NodeFilter.FILTER_REJECT
        : NodeFilter.FILTER_ACCEPT;
    },
  });
  const text = walker.nextNode();
  if (text) {
    const range = document.createRange();
    range.setStart(text, 0);
    range.setEnd(text, Math.min(1, text.textContent?.length ?? 0));
    const line = range.getBoundingClientRect();
    if (line.height > 0) {
      return new DOMRect(blockRect.left, line.top, blockRect.width, line.height);
    }
  }

  const height = Number.parseFloat(getComputedStyle(element).lineHeight);
  return new DOMRect(
    blockRect.left,
    blockRect.top,
    blockRect.width,
    Number.isFinite(height) ? Math.min(height, blockRect.height) : blockRect.height,
  );
}

export function collectEditorBlockRects(editor: Editor): EditorGutterBlockRect[] {
  const blocks: HoveredBlock[] = [];

  for (const target of collectMovableBlockTargets(editor.state)) {
    const dom = editor.view.nodeDOM(target.from);
    const element =
      dom instanceof HTMLElement ? dom : dom instanceof Node ? dom.parentElement : null;
    if (!element) continue;

    const blockRect = element.getBoundingClientRect();
    blocks.push({
      target,
      element,
      blockRect,
      rect: getBlockRowRect(element, blockRect),
    });
  }

  return blocks;
}

export function resolveBlockRectAtPoint(
  blocks: readonly EditorGutterBlockRect[],
  editorRect: Pick<DOMRect, "top" | "right" | "bottom" | "left">,
  point: { x: number; y: number },
): HoveredBlock | null {
  if (point.y < editorRect.top || point.y > editorRect.bottom || point.x > editorRect.right) {
    return null;
  }

  let match: HoveredBlock | null = null;
  for (const block of blocks) {
    if (point.y < block.blockRect.top || point.y > block.blockRect.bottom) continue;
    if (point.x < block.blockRect.left - GUTTER_WIDTH_PX || point.x > block.blockRect.right) {
      continue;
    }
    // The outside gutter belongs to the container, not its first paragraph.
    // Column child gutters live inside their own lane's reserved inset.
    if (block.target.node.type.name === "columns" && point.x < block.blockRect.left) {
      return block;
    }
    if (!match || block.blockRect.height < match.blockRect.height) {
      match = block;
    }
  }
  return match;
}

export function resolveSiblingBlockRectAtPoint(
  blocks: readonly EditorGutterBlockRect[],
  source: EditorGutterBlockTarget,
  editorRect: Pick<DOMRect, "top" | "right" | "bottom" | "left">,
  point: { x: number; y: number },
) {
  return resolveBlockRectAtPoint(
    blocks.filter((block) => block.target.parentStart === source.parentStart),
    editorRect,
    point,
  );
}

export function resolveCaretBlock(
  blocks: readonly EditorGutterBlockRect[],
  from: number,
  to: number,
) {
  return (
    blocks
      .filter((candidate) => candidate.target.from <= from && candidate.target.to >= to)
      .sort((a, b) => a.target.node.nodeSize - b.target.node.nodeSize)[0] ?? null
  );
}

export function EditorBlockGutter({
  editor,
  enabled,
  onInsertBelow,
  onMoveComplete,
  onOpenMenu,
}: {
  editor: Editor;
  enabled: boolean;
  onInsertBelow: (target: EditorGutterBlockTarget) => void;
  onMoveComplete: (direction: "up" | "down") => void;
  onOpenMenu: (target: EditorGutterBlockTarget, trigger: HTMLElement) => void;
}) {
  const [hovered, setHovered] = useState<HoveredBlock | null>(null);
  const [dragging, setDragging] = useState<HoveredBlock | null>(null);
  const [drop, setDrop] = useState<DropTarget | null>(null);
  const [dragPoint, setDragPoint] = useState<{ x: number; y: number } | null>(null);
  const [columnMenu, setColumnMenu] = useState<HoveredBlock | null>(null);
  const hoveredRef = useRef<HoveredBlock | null>(null);
  const draggingRef = useRef<HoveredBlock | null>(null);
  const dropRef = useRef<DropTarget | null>(null);
  const blockRectsRef = useRef<HoveredBlock[]>([]);
  const geometryDirtyRef = useRef(true);
  const gestureCleanupRef = useRef<(() => void) | null>(null);
  const gestureCancelRef = useRef<(() => void) | null>(null);
  const openMenuRef = useRef(onOpenMenu);
  openMenuRef.current = onOpenMenu;

  const publishHover = useCallback((next: HoveredBlock | null) => {
    hoveredRef.current = next;
    setHovered(next);
  }, []);

  const publishDrop = useCallback((next: DropTarget | null) => {
    dropRef.current = next;
    setDrop(next);
  }, []);

  const readBlockRects = useCallback(() => {
    if (geometryDirtyRef.current) {
      blockRectsRef.current = collectEditorBlockRects(editor);
      geometryDirtyRef.current = false;
    }
    return blockRectsRef.current;
  }, [editor]);

  const resolveBlockAtPoint = useCallback(
    (clientX: number, clientY: number) =>
      resolveBlockRectAtPoint(readBlockRects(), editor.view.dom.getBoundingClientRect(), {
        x: clientX,
        y: clientY,
      }),
    [editor, readBlockRects],
  );

  const resolveDrop = useCallback(
    (clientX: number, clientY: number) => {
      const source = draggingRef.current;
      if (!source) {
        publishDrop(null);
        return;
      }

      // Resolve among siblings first. Descendants must not swallow a valid
      // destination for a whole columns block (or a containing list).
      const candidate = resolveSiblingBlockRectAtPoint(
        readBlockRects(),
        source.target,
        editor.view.dom.getBoundingClientRect(),
        { x: clientX, y: clientY },
      );
      if (
        !candidate ||
        candidate.target.from === source.target.from ||
        candidate.target.parentStart !== source.target.parentStart
      ) {
        publishDrop(null);
        return;
      }

      const after = clientY >= candidate.blockRect.top + candidate.blockRect.height / 2;
      const position = after ? candidate.target.to : candidate.target.from;
      if (position >= source.target.from && position <= source.target.to) {
        publishDrop(null);
        return;
      }

      publishDrop({
        target: candidate.target,
        position,
        top: after ? candidate.blockRect.bottom : candidate.blockRect.top,
        left: candidate.blockRect.left,
        width: candidate.blockRect.width,
      });
    },
    [editor, publishDrop, readBlockRects],
  );

  useEffect(() => {
    if (!enabled) {
      publishHover(null);
      publishDrop(null);
      return;
    }

    const handlePointerMove = (event: PointerEvent) => {
      if (event.pointerType === "touch") return;
      if (draggingRef.current) {
        resolveDrop(event.clientX, event.clientY);
        return;
      }
      if (
        event.target instanceof Element &&
        event.target.closest("[data-starlane-block-gutter], [data-starlane-columns-menu]")
      )
        return;
      if (document.activeElement?.closest("[data-starlane-block-gutter]")) return;
      publishHover(resolveBlockAtPoint(event.clientX, event.clientY));
    };

    const refreshPosition = () => {
      geometryDirtyRef.current = true;
      const current = hoveredRef.current;
      if (!current?.element.isConnected) {
        publishHover(null);
        return;
      }
      const currentId = current.target.node.attrs.id;
      const next = readBlockRects().find((block) =>
        typeof currentId === "string"
          ? block.target.node.attrs.id === currentId
          : block.target.from === current.target.from,
      );
      publishHover(next ?? null);
    };

    let refreshFrame = 0;
    const markGeometryDirty = () => {
      geometryDirtyRef.current = true;
      // Node views settle after the transaction notification. Refresh the
      // visible handle too, otherwise it remains at the pre-move coordinates.
      cancelAnimationFrame(refreshFrame);
      refreshFrame = requestAnimationFrame(refreshPosition);
    };
    const showCaretBlock = () => {
      if (!editor.isFocused || draggingRef.current) return;
      geometryDirtyRef.current = true;
      const { from, to } = editor.state.selection;
      publishHover(resolveCaretBlock(readBlockRects(), from, to));
    };
    const handleColumnContextMenu = (event: MouseEvent) => {
      if (!(event.target instanceof Element) || !editor.view.dom.contains(event.target)) return;
      if (!editor.state.selection.empty) return;
      const block = resolveBlockAtPoint(event.clientX, event.clientY);
      // Blank lanes/container padding have no independent child owner.
      // Authored child content retains the editor's normal text context menu.
      if (block?.target.node.type.name !== "columns") return;
      event.preventDefault();
      event.stopPropagation();
      setColumnMenu(block);
    };

    document.addEventListener("pointermove", handlePointerMove, { passive: true });
    document.addEventListener("scroll", refreshPosition, { capture: true, passive: true });
    window.addEventListener("resize", refreshPosition, { passive: true });
    editor.on("transaction", markGeometryDirty);
    editor.on("selectionUpdate", showCaretBlock);
    editor.on("focus", showCaretBlock);
    document.addEventListener("contextmenu", handleColumnContextMenu, true);
    return () => {
      document.removeEventListener("pointermove", handlePointerMove);
      document.removeEventListener("scroll", refreshPosition, true);
      window.removeEventListener("resize", refreshPosition);
      editor.off("transaction", markGeometryDirty);
      editor.off("selectionUpdate", showCaretBlock);
      editor.off("focus", showCaretBlock);
      document.removeEventListener("contextmenu", handleColumnContextMenu, true);
      cancelAnimationFrame(refreshFrame);
    };
  }, [
    editor,
    enabled,
    publishDrop,
    publishHover,
    readBlockRects,
    resolveBlockAtPoint,
    resolveDrop,
  ]);

  useEffect(() => {
    if (!dragging) return;
    const element = dragging.element;
    element.setAttribute("data-starlane-drag-source", "true");
    return () => element.removeAttribute("data-starlane-drag-source");
  }, [dragging]);

  useEffect(() => {
    if (enabled) return;
    gestureCleanupRef.current?.();
    draggingRef.current = null;
    setDragging(null);
    setDragPoint(null);
    setColumnMenu(null);
    publishHover(null);
    publishDrop(null);
  }, [enabled, publishDrop, publishHover]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (
        enabled &&
        !draggingRef.current &&
        (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) &&
        event.target instanceof HTMLElement &&
        editor.view.dom.contains(event.target)
      ) {
        const { from, to } = editor.state.selection;
        const source = resolveCaretBlock(readBlockRects(), from, to);
        if (source) {
          event.preventDefault();
          event.stopPropagation();
          if (source.target.node.type.name === "columns") setColumnMenu(source);
          else openMenuRef.current(source.target, source.element);
          return;
        }
      }
      if (event.key !== "Escape" || !gestureCancelRef.current) return;
      event.preventDefault();
      gestureCancelRef.current();
    };
    document.addEventListener("keydown", handleKeyDown, true);
    return () => {
      document.removeEventListener("keydown", handleKeyDown, true);
      gestureCleanupRef.current?.();
    };
  }, [editor, enabled, readBlockRects]);

  const startGripGesture = (event: ReactPointerEvent<HTMLButtonElement>) => {
    const source = hoveredRef.current;
    if (!source || event.pointerType === "touch" || event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    gestureCleanupRef.current?.();

    const trigger = event.currentTarget;
    const pointerId = event.pointerId;
    const startX = event.clientX;
    const startY = event.clientY;
    let didDrag = false;
    try {
      trigger.setPointerCapture(pointerId);
    } catch {
      // Document listeners still preserve the gesture when pointer capture is unavailable.
    }

    const cleanup = () => {
      document.removeEventListener("pointermove", handleMove);
      document.removeEventListener("pointerup", handleUp);
      document.removeEventListener("pointercancel", handleCancel);
      gestureCleanupRef.current = null;
      gestureCancelRef.current = null;
    };
    const finishDrag = () => {
      draggingRef.current = null;
      setDragging(null);
      setDragPoint(null);
      publishDrop(null);
    };
    const handleMove = (moveEvent: PointerEvent) => {
      if (moveEvent.pointerId !== pointerId) return;
      if (
        !didDrag &&
        Math.hypot(moveEvent.clientX - startX, moveEvent.clientY - startY) > DRAG_THRESHOLD_PX
      ) {
        didDrag = true;
        draggingRef.current = source;
        setDragging(source);
      }
      if (didDrag) {
        setDragPoint({ x: moveEvent.clientX, y: moveEvent.clientY });
        resolveDrop(moveEvent.clientX, moveEvent.clientY);
      }
    };
    const handleUp = (upEvent: PointerEvent) => {
      if (upEvent.pointerId !== pointerId) return;
      cleanup();
      if (didDrag) {
        const destination = dropRef.current;
        const result = destination
          ? createBlockMoveTransaction(editor.state, {
              from: source.target.from,
              size: source.target.node.nodeSize,
              to: destination.position,
            })
          : null;
        if (result) {
          editor.view.dispatch(result.transaction);
          geometryDirtyRef.current = true;
          onMoveComplete(destination!.position < source.target.from ? "up" : "down");
        }
        editor.view.focus();
        publishHover(null);
        finishDrag();
        return;
      }
      // Primary click does nothing. Structural actions belong to the explicit
      // right-click/keyboard context gesture, not to an aborted drag.
    };
    const handleCancel = (cancelEvent: PointerEvent) => {
      if (cancelEvent.pointerId !== pointerId) return;
      cleanup();
      finishDrag();
    };
    const cancelGesture = () => {
      cleanup();
      finishDrag();
      publishHover(null);
    };

    document.addEventListener("pointermove", handleMove);
    document.addEventListener("pointerup", handleUp);
    document.addEventListener("pointercancel", handleCancel);
    gestureCleanupRef.current = cleanup;
    gestureCancelRef.current = cancelGesture;
  };

  const openGripActions = (trigger: HTMLElement) => {
    const source = hoveredRef.current;
    if (!source || draggingRef.current) return;
    if (source.target.node.type.name === "columns") setColumnMenu(source);
    else onOpenMenu(source.target, trigger);
  };

  if (!enabled || typeof document === "undefined") return null;

  const closeColumnMenu = () => {
    setColumnMenu(null);
    editor.view.focus();
  };
  const currentColumnTarget = () => {
    if (!columnMenu) return null;
    // Resolve runtime identity again so an intervening editor transaction
    // cannot apply an action to the old absolute position's new occupant.
    const id = columnMenu.target.node.attrs.id;
    return typeof id === "string" ? findRuntimeBlockTarget(editor.state, id) : null;
  };
  const moveColumns = (direction: "up" | "down") => {
    const source = currentColumnTarget();
    if (!source) return closeColumnMenu();
    const siblings = collectMovableBlockTargets(editor.state).filter(
      (block) => block.parentStart === source.parentStart,
    );
    const destination = siblings.find(
      (block) => block.index === source.index + (direction === "up" ? -1 : 1),
    );
    const result =
      destination &&
      createBlockMoveTransaction(editor.state, {
        from: source.from,
        size: source.node.nodeSize,
        to: direction === "up" ? destination.from : destination.to,
      });
    if (result) {
      editor.view.dispatch(result.transaction);
      onMoveComplete(direction);
    }
    closeColumnMenu();
  };

  // Document chrome stays below Contents (20), inspectors (30), and dialogs (50).
  return createPortal(
    <>
      {columnMenu ? (
        <EditorColumnsMenu
          rect={
            new DOMRect(
              columnMenu.blockRect.left - GUTTER_WIDTH_PX,
              columnMenu.rect.top,
              GUTTER_WIDTH_PX,
              32,
            )
          }
          layout={columnMenu.target.node.attrs.layout}
          canMoveUp={columnMenu.target.index > 0}
          canMoveDown={columnMenu.target.index < columnMenu.target.parentChildCount - 1}
          onClose={closeColumnMenu}
          onMove={moveColumns}
          onLayout={(layout) => {
            const target = currentColumnTarget();
            if (target) setEditorSectionLayout(editor, () => target.from, layout);
            closeColumnMenu();
          }}
          onInsert={() => {
            const target = currentColumnTarget();
            if (target) onInsertBelow(target);
            setColumnMenu(null);
          }}
          onDuplicate={() => {
            const target = currentColumnTarget();
            if (target)
              editor.view.dispatch(
                editor.state.tr
                  .insert(target.to, target.node.copy(target.node.content))
                  .scrollIntoView(),
              );
            closeColumnMenu();
          }}
          onDelete={() => {
            const target = currentColumnTarget();
            if (target) editor.chain().deleteRange({ from: target.from, to: target.to }).run();
            closeColumnMenu();
          }}
        />
      ) : null}
      {hovered?.target.node.type.name === "columns" && !dragging ? (
        <div
          data-starlane-column-block-outline="true"
          className="border-primary rounded-input pointer-events-none fixed z-[14] border"
          style={{
            top: hovered.blockRect.top,
            left: hovered.blockRect.left,
            width: hovered.blockRect.width,
            height: hovered.blockRect.height,
          }}
        />
      ) : null}
      {hovered && !dragging ? (
        <div
          data-starlane-block-gutter="true"
          className="pointer-events-none fixed z-[15] flex h-8 items-center justify-end gap-0.5"
          style={{
            top: hovered.rect.top,
            left: Math.max(4, hovered.rect.left - GUTTER_WIDTH_PX),
            width: Math.min(GUTTER_WIDTH_PX, Math.max(0, hovered.rect.left - 4)),
          }}
          role="toolbar"
          aria-label="Block gutter"
        >
          <button
            type="button"
            tabIndex={-1}
            aria-label="Insert block below"
            title="Insert block below"
            onPointerDown={(event) => {
              event.preventDefault();
              event.stopPropagation();
            }}
            onClick={() => {
              onInsertBelow(hovered.target);
              publishHover(null);
            }}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text pointer-events-auto flex size-7 items-center justify-center transition-colors duration-100"
          >
            <Plus size={16} />
          </button>
          <button
            type="button"
            tabIndex={0}
            aria-label="Drag to reorder; right-click or Shift+F10 for block actions"
            title="Drag to move · Right-click for actions"
            onPointerDown={startGripGesture}
            onContextMenu={(event) => {
              event.preventDefault();
              event.stopPropagation();
              openGripActions(event.currentTarget);
            }}
            onKeyDown={(event) => {
              if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
                event.preventDefault();
                event.stopPropagation();
                openGripActions(event.currentTarget);
              }
            }}
            className="focus-ring rounded-button text-text-muted hover:bg-surface-raised hover:text-text pointer-events-auto flex size-7 cursor-grab items-center justify-center transition-colors duration-100 active:cursor-grabbing"
          >
            <GripVertical size={16} />
          </button>
        </div>
      ) : null}
      {drop && dragging ? (
        <div
          data-starlane-block-drop-indicator="true"
          className="bg-primary pointer-events-none fixed z-[16] h-0.5"
          style={{ top: drop.top - 1, left: drop.left, width: drop.width }}
        />
      ) : null}
      {dragging && dragPoint ? (
        <EditorBlockDragPreview
          source={dragging.element}
          rect={dragging.blockRect}
          point={dragPoint}
        />
      ) : null}
    </>,
    document.body,
  );
}
