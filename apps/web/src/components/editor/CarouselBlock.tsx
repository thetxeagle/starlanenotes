import { useEffect, useState } from "react";
import type { ReactNodeViewProps } from "@tiptap/react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ChevronLeft, ChevronRight, ImagePlus, Trash2 } from "lucide-react";

export const MAX_CAROUSEL_IMAGES = 9;
export const DEFAULT_CAROUSEL_TITLE = "Carousel";

export interface CarouselItem {
  src: string;
  alt: string;
}

export function isSafeCarouselImageSource(value: string): boolean {
  return /^(https?:|starlane:|blob:)/i.test(value.trim()) || /^data:image\//i.test(value.trim());
}

export function normalizeCarouselItems(value: unknown): CarouselItem[] {
  let items: unknown = value;
  if (typeof value === "string") {
    try {
      items = JSON.parse(value);
    } catch {
      items = [];
    }
  }
  if (!Array.isArray(items)) return [];
  return items
    .map((item, index) => {
      if (!item || typeof item !== "object") return null;
      const record = item as Record<string, unknown>;
      return {
        src: typeof record.src === "string" ? record.src.trim().slice(0, 2000) : "",
        alt:
          typeof record.alt === "string" && record.alt.trim()
            ? record.alt.trim().slice(0, 160)
            : `Image ${index + 1}`,
      };
    })
    .filter((item): item is CarouselItem => item !== null)
    .slice(0, MAX_CAROUSEL_IMAGES);
}

export function serializeCarouselItems(items: CarouselItem[]): string {
  return JSON.stringify(normalizeCarouselItems(items));
}

export function parseCarouselDirective(value: string): {
  title: string;
  items: CarouselItem[];
} | null {
  const match = value.trim().match(/^```carousel\n([\s\S]*?)\n```$/i);
  if (!match) return null;
  const lines = match[1].split(/\r?\n/);
  const titleLine = lines.find((line) => /^title\s*:/i.test(line));
  const items = lines
    .map((line) => line.trim().match(/^!\[([^\]]*)\]\(([^\s)]+)(?:\s+"([^"]*)")?\)$/))
    .filter((item): item is RegExpMatchArray => Boolean(item))
    .map((item, index) => ({ src: item[2], alt: item[1] || item[3] || `Image ${index + 1}` }))
    .filter((item) => isSafeCarouselImageSource(item.src));
  return {
    title: titleLine?.replace(/^title\s*:/i, "").trim().slice(0, 120) || DEFAULT_CAROUSEL_TITLE,
    items: normalizeCarouselItems(items),
  };
}

export function serializeCarouselDirective(title: string, items: CarouselItem[]): string {
  const images = normalizeCarouselItems(items)
    .filter((item) => isSafeCarouselImageSource(item.src))
    .map((item) => `![${item.alt}](${item.src})`)
    .join("\n");
  return ["```carousel", `title: ${title.trim() || DEFAULT_CAROUSEL_TITLE}`, "---", images, "```"].join(
    "\n",
  );
}

function CarouselDisplay({ items }: { items: CarouselItem[] }) {
  const validItems = items.filter((item) => isSafeCarouselImageSource(item.src));
  const [activeIndex, setActiveIndex] = useState(0);
  useEffect(() => {
    if (activeIndex >= validItems.length) setActiveIndex(0);
  }, [activeIndex, validItems.length]);
  const active = validItems[activeIndex];
  if (!active) {
    return (
      <div className="border-border bg-surface-raised text-text-muted flex min-h-36 items-center justify-center border text-sm">
        Add image URLs to preview the carousel.
      </div>
    );
  }
  return (
    <div className="border-border bg-surface-raised overflow-hidden border">
      <div className="relative aspect-video">
        <img src={active.src} alt={active.alt} className="h-full w-full object-cover" loading="lazy" />
        {validItems.length > 1 ? (
          <>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="absolute left-2 top-1/2 size-8 -translate-y-1/2 p-0"
              aria-label="Previous carousel image"
              onClick={() => setActiveIndex((index) => (index + validItems.length - 1) % validItems.length)}
            >
              <ChevronLeft data-icon="inline-start" />
            </Button>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              className="absolute right-2 top-1/2 size-8 -translate-y-1/2 p-0"
              aria-label="Next carousel image"
              onClick={() => setActiveIndex((index) => (index + 1) % validItems.length)}
            >
              <ChevronRight data-icon="inline-start" />
            </Button>
          </>
        ) : null}
      </div>
      <div className="flex items-center justify-between gap-3 px-3 py-2">
        <span className="text-text-muted truncate text-xs">{active.alt}</span>
        <div className="flex shrink-0 gap-1" aria-label="Carousel slides">
          {validItems.map((item, index) => (
            <button
              type="button"
              key={`${item.src}-${index}`}
              aria-label={`Show carousel image ${index + 1}`}
              aria-current={index === activeIndex}
              className={`size-1.5 rounded-full border ${index === activeIndex ? "border-primary bg-primary" : "border-border bg-bg"}`}
              onClick={() => setActiveIndex(index)}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

export function CarouselNodeView({ node, updateAttributes, editor }: ReactNodeViewProps) {
  const title = String(node.attrs.title || DEFAULT_CAROUSEL_TITLE);
  const items = normalizeCarouselItems(node.attrs.items);
  const updateItem = (index: number, patch: Partial<CarouselItem>) => {
    const next = items.map((item, itemIndex) => (itemIndex === index ? { ...item, ...patch } : item));
    updateAttributes({ items: next });
  };

  return (
    <div data-carousel-block="true" contentEditable={false} className="my-4 rounded-card border border-border bg-surface p-3">
      <div className="mb-3 flex items-center justify-between gap-3">
        {editor.isEditable ? (
          <Input
            aria-label="Carousel title"
            value={title}
            onChange={(event) => updateAttributes({ title: event.target.value.slice(0, 120) })}
            className="h-8 max-w-xs bg-transparent text-sm"
          />
        ) : (
          <p className="text-text text-sm font-medium">{title}</p>
        )}
        <span className="text-text-muted shrink-0 text-xs">{items.length}/{MAX_CAROUSEL_IMAGES}</span>
      </div>
      <CarouselDisplay items={items} />
      {editor.isEditable ? (
        <div className="mt-3 flex flex-col gap-2">
          {items.map((item, index) => (
            <div key={index} className="flex items-center gap-2">
              <Input
                aria-label={`Carousel image ${index + 1} URL`}
                value={item.src}
                onChange={(event) => updateItem(index, { src: event.target.value })}
                placeholder="https://example.com/image.jpg"
                className="h-9 min-w-0 flex-1 text-xs"
              />
              <Input
                aria-label={`Carousel image ${index + 1} alt text`}
                value={item.alt}
                onChange={(event) => updateItem(index, { alt: event.target.value })}
                placeholder="Alt text"
                className="h-9 w-32 text-xs"
              />
              <Button
                type="button"
                variant="ghost"
                size="sm"
                className="size-8 shrink-0 p-0"
                aria-label={`Remove carousel image ${index + 1}`}
                onClick={() => updateAttributes({ items: items.filter((_, itemIndex) => itemIndex !== index) })}
              >
                <Trash2 data-icon="inline-start" />
              </Button>
            </div>
          ))}
          <Button
            type="button"
            variant="secondary"
            size="sm"
            disabled={items.length >= MAX_CAROUSEL_IMAGES}
            onClick={() => updateAttributes({ items: [...items, { src: "", alt: `Image ${items.length + 1}` }] })}
          >
            <ImagePlus data-icon="inline-start" />
            Add image
          </Button>
        </div>
      ) : null}
    </div>
  );
}
