import { useLayoutEffect, useRef } from "react";

/** Snapshot presentation only. Never reparent a live ProseMirror node or mount a second editor. */
export function createRenderedBlockPreview(source: HTMLElement): HTMLElement {
  const clone = source.cloneNode(true) as HTMLElement;
  const originals = [source, ...source.querySelectorAll<HTMLElement>("*")];
  const copies = [clone, ...clone.querySelectorAll<HTMLElement>("*")];
  originals.forEach((original, index) => {
    const copy = copies[index];
    if (!copy) return;
    // A body portal has none of the editor's ancestor typography/layout selectors.
    // Freeze computed presentation once at pickup, not on every pointer move.
    const style = getComputedStyle(original);
    for (let property = 0; property < style.length; property += 1) {
      const name = style.item(property);
      copy.style.setProperty(name, style.getPropertyValue(name));
    }
    for (const attribute of Array.from(copy.attributes)) {
      if (
        attribute.name === "id" ||
        attribute.name === "name" ||
        attribute.name === "href" ||
        attribute.name === "tabindex" ||
        attribute.name.startsWith("on") ||
        attribute.name.startsWith("data-starlane-")
      )
        copy.removeAttribute(attribute.name);
    }
    copy.setAttribute("contenteditable", "false");
    copy.setAttribute("draggable", "false");
    copy.style.pointerEvents = "none";
    copy.style.animation = "none";
    copy.style.transition = "none";
    if (original instanceof HTMLInputElement && copy instanceof HTMLInputElement) {
      copy.value = original.value;
      copy.checked = original.checked;
      copy.disabled = true;
    }
    if (original instanceof HTMLTextAreaElement && copy instanceof HTMLTextAreaElement) {
      copy.value = original.value;
      copy.disabled = true;
    }
    if (original instanceof HTMLSelectElement && copy instanceof HTMLSelectElement) {
      copy.value = original.value;
      copy.disabled = true;
    }
    if (original instanceof HTMLCanvasElement && copy instanceof HTMLCanvasElement) {
      try {
        copy.getContext("2d")?.drawImage(original, 0, 0);
      } catch {
        /* Unavailable canvas. */
      }
    }
  });
  // Control chrome and embedded browsing/media contexts are not dragged as live UI.
  clone
    .querySelectorAll(
      "script, style, link, iframe, object, embed, audio, video, button, [role='toolbar'], .editor-section__toolbar",
    )
    .forEach((element) => element.remove());
  clone.setAttribute("inert", "");
  clone.setAttribute("aria-hidden", "true");
  clone.style.margin = "0";
  clone.style.opacity = "1";
  clone.style.outline = "none";
  return clone;
}

export function getBlockPreviewGeometry(
  width: number,
  height: number,
  viewportWidth: number,
  viewportHeight: number,
  point: { x: number; y: number },
) {
  const scale = Math.min(
    1,
    Math.max(1, Math.min(720, viewportWidth - 32)) / Math.max(1, width),
    Math.max(1, viewportHeight - 32) / Math.max(1, height),
  );
  const previewWidth = width * scale;
  const previewHeight = height * scale;
  return {
    scale,
    width: previewWidth,
    height: previewHeight,
    left: Math.max(8, Math.min(point.x + 16, viewportWidth - previewWidth - 8)),
    top: Math.max(8, Math.min(point.y + 16, viewportHeight - previewHeight - 8)),
  };
}

export function EditorBlockDragPreview({
  source,
  rect,
  point,
}: {
  source: HTMLElement;
  rect: DOMRect;
  point: { x: number; y: number };
}) {
  const host = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    const element = host.current;
    if (!element) return;
    const clone = createRenderedBlockPreview(source);
    clone.style.width = `${rect.width}px`;
    element.replaceChildren(clone);
    return () => element.replaceChildren();
  }, [source, rect.width]);
  const geometry = getBlockPreviewGeometry(
    rect.width,
    rect.height,
    window.innerWidth,
    window.innerHeight,
    point,
  );
  return (
    <div
      data-starlane-block-drag-preview="true"
      aria-hidden="true"
      className="bg-bg text-text shadow-raised rounded-input pointer-events-none fixed z-[16] overflow-hidden"
      style={{
        left: geometry.left,
        top: geometry.top,
        width: geometry.width,
        height: geometry.height,
      }}
    >
      <div
        ref={host}
        style={{
          width: rect.width,
          transform: `scale(${geometry.scale})`,
          transformOrigin: "top left",
        }}
      />
    </div>
  );
}
