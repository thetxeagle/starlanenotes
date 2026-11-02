import { useCallback, useEffect, useRef, useState, type RefObject } from "react";
import { X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

export interface DocumentContentsHeading {
  id: string;
  title: string;
  level: number;
  element: HTMLElement;
}

interface DocumentContentsProps {
  containerRef: RefObject<HTMLElement | null>;
  /** Change this when the displayed document or rendered mode changes. */
  contentKey: string;
  open: boolean;
  onClose: () => void;
  onHeadingsChange?: (count: number) => void;
}

function headingSlug(value: string): string {
  return value
    .normalize("NFKD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "") || "section";
}

export function collectDocumentContentsHeadings(root: HTMLElement): DocumentContentsHeading[] {
  const usedIds = new Map<string, number>();
  return Array.from(root.querySelectorAll<HTMLElement>("h1, h2, h3, h4, h5, h6"))
    .filter((element) => {
      if (!element.textContent?.trim()) return false;
      return !element.closest(
        "nav, [data-document-ui], [data-editor-toolbar], .document-contents",
      );
    })
    .map((element) => {
      const title = element.textContent?.trim() ?? "";
      const base = element.id || headingSlug(title);
      const occurrence = (usedIds.get(base) ?? 0) + 1;
      usedIds.set(base, occurrence);
      const id = occurrence === 1 ? base : `${base}-${occurrence}`;
      return {
        id,
        title,
        level: Math.min(6, Math.max(1, Number(element.tagName.slice(1)))),
        element,
      };
    });
}

const HEADING_INDENT_CLASSES = [undefined, undefined, "pl-2", "pl-4", "pl-6", "pl-8", "pl-10"];

function headingIndentClass(level: number): string | undefined {
  return HEADING_INDENT_CLASSES[Math.min(6, Math.max(1, level))];
}

function sameHeadings(
  current: DocumentContentsHeading[],
  next: DocumentContentsHeading[],
): boolean {
  return current.length === next.length && current.every((heading, index) => {
    const candidate = next[index];
    return heading.id === candidate.id && heading.title === candidate.title &&
      heading.level === candidate.level && heading.element === candidate.element;
  });
}

function findScrollContainer(element: HTMLElement): HTMLElement | null {
  let ancestor = element.parentElement;
  while (ancestor && ancestor !== document.body) {
    const { overflowY } = window.getComputedStyle(ancestor);
    if (/(auto|scroll|overlay)/.test(overflowY)) return ancestor;
    ancestor = ancestor.parentElement;
  }
  return null;
}

function stickyTopInset(scrollContainer: HTMLElement | null): number {
  if (!scrollContainer) return 0;
  const containerTop = scrollContainer.getBoundingClientRect().top;
  let bottom = containerTop;
  for (const element of Array.from(
    scrollContainer.querySelectorAll<HTMLElement>(
      "[data-document-sticky-header], [data-document-sticky-toolbar]",
    ),
  )) {
    if (window.getComputedStyle(element).position !== "sticky") continue;
    const rect = element.getBoundingClientRect();
    if (rect.height > 0 && rect.top <= containerTop + 1 && rect.bottom > bottom) {
      bottom = rect.bottom;
    }
  }
  return Math.max(0, bottom - containerTop);
}

function scrollHeadingToReadingStart(heading: HTMLElement, root: HTMLElement): void {
  const scrollContainer = findScrollContainer(root);
  const inset = stickyTopInset(scrollContainer);
  const targetTop = heading.getBoundingClientRect().top;
  if (scrollContainer) {
    const containerTop = scrollContainer.getBoundingClientRect().top;
    scrollContainer.scrollTo({
      top: scrollContainer.scrollTop + targetTop - containerTop - inset - 12,
      behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
    });
    return;
  }
  window.scrollTo({
    top: window.scrollY + targetTop - inset - 12,
    behavior: window.matchMedia("(prefers-reduced-motion: reduce)").matches ? "auto" : "smooth",
  });
}

export function getCurrentDocumentHeadingIndex(
  headings: DocumentContentsHeading[],
  topOffset: number,
): number {
  let current = 0;
  for (let index = 0; index < headings.length; index += 1) {
    if (headings[index].element.getBoundingClientRect().top <= topOffset) current = index;
    else break;
  }
  return current;
}

export function DocumentContents({
  containerRef,
  contentKey,
  open,
  onClose,
  onHeadingsChange,
}: DocumentContentsProps) {
  const [headings, setHeadings] = useState<DocumentContentsHeading[]>([]);
  const [activeId, setActiveId] = useState<string | null>(null);
  const onHeadingsChangeRef = useRef(onHeadingsChange);
  const lastReportedCountRef = useRef(-1);
  onHeadingsChangeRef.current = onHeadingsChange;

  const refreshHeadings = useCallback(() => {
    const root = containerRef.current;
    const next = root ? collectDocumentContentsHeadings(root) : [];
    setHeadings((current) => sameHeadings(current, next) ? current : next);
    setActiveId((current) => next.some((heading) => heading.id === current) ? current : next[0]?.id ?? null);
    if (lastReportedCountRef.current !== next.length) {
      lastReportedCountRef.current = next.length;
      onHeadingsChangeRef.current?.(next.length);
    }
  }, [containerRef]);

  useEffect(() => {
    refreshHeadings();
    const root = containerRef.current;
    if (!root) return;
    let frame = 0;
    const observer = new MutationObserver(() => {
      if (frame) return;
      frame = window.requestAnimationFrame(() => {
        frame = 0;
        refreshHeadings();
      });
    });
    observer.observe(root, { childList: true, subtree: true, characterData: true, attributes: true, attributeFilter: ["id"] });
    return () => {
      observer.disconnect();
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [contentKey, containerRef, refreshHeadings]);

  useEffect(() => {
    const root = containerRef.current;
    if (!root || headings.length === 0) return;
    const scrollContainer = findScrollContainer(root);
    const updateCurrent = () => {
      const inset = stickyTopInset(scrollContainer);
      setActiveId(headings[getCurrentDocumentHeadingIndex(headings, (scrollContainer?.getBoundingClientRect().top ?? 0) + inset + 20)]?.id ?? null);
    };
    scrollContainer?.addEventListener("scroll", updateCurrent, { passive: true });
    window.addEventListener("scroll", updateCurrent, { passive: true });
    window.addEventListener("resize", updateCurrent);
    updateCurrent();
    return () => {
      scrollContainer?.removeEventListener("scroll", updateCurrent);
      window.removeEventListener("scroll", updateCurrent);
      window.removeEventListener("resize", updateCurrent);
    };
  }, [containerRef, headings]);

  if (!open || headings.length < 2) return null;

  const root = containerRef.current;
  return (
    <aside className="document-contents min-w-0" data-document-ui="contents" aria-label="Document contents">
      <div className="flex items-center justify-between gap-2 px-2 pb-2">
        <h2 className="text-text text-sm font-semibold">Contents</h2>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="size-8 min-h-8 min-w-8 p-0"
          aria-label="Close contents"
          onClick={onClose}
        >
          <X size={16} aria-hidden="true" />
        </Button>
      </div>
      <nav aria-label="Document contents">
        <ol className="flex max-h-[calc(100vh-12rem)] flex-col gap-0.5 overflow-y-auto">
          {headings.map((heading) => (
            <li key={`${heading.id}-${heading.title}-${heading.level}`} className={headingIndentClass(heading.level)}>
              <button
                type="button"
                className={cn(
                  "focus-ring min-h-8 w-full rounded-button px-2 py-1 text-left text-sm leading-5 transition-colors duration-100",
                  "text-text-muted hover:bg-surface-raised hover:text-text",
                  activeId === heading.id && "bg-primary/10 text-primary font-medium",
                )}
                aria-label={`Go to ${heading.title}`}
                aria-current={activeId === heading.id ? "location" : undefined}
                title={heading.title}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  if (!root) return;
                  const latest = collectDocumentContentsHeadings(root).find((candidate) => candidate.id === heading.id);
                  if (latest) scrollHeadingToReadingStart(latest.element, root);
                }}
              >
                <span className="block truncate">{heading.title}</span>
              </button>
            </li>
          ))}
        </ol>
      </nav>
    </aside>
  );
}
