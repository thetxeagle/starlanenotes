import { useEffect, useState } from "react";
import type { ReactNodeViewProps } from "@tiptap/react";
import { Button } from "@/components/ui/button";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";
import { Select } from "@/components/ui/select";
import { ChevronDown, ChevronLeft, ChevronRight, ImagePlus, Trash2, X } from "lucide-react";
import {
  isSafeCarouselImageSource,
  MAX_CAROUSEL_IMAGES,
  normalizeCarouselItems,
  serializeCarouselItems,
  type CarouselItem,
} from "./CarouselBlock";

export const DEFAULT_GALLERY_TITLE = "Gallery";
export { MAX_CAROUSEL_IMAGES } from "./CarouselBlock";
export const GALLERY_LAYOUTS = [
  "responsive-columns",
  "masonry",
  "accordion",
  "coverflow",
  "thumbnail-strip",
  "carousel",
  "timeline",
] as const;
export type GalleryLayout = (typeof GALLERY_LAYOUTS)[number];

export function normalizeGalleryLayout(value: unknown): GalleryLayout {
  return GALLERY_LAYOUTS.includes(value as GalleryLayout)
    ? (value as GalleryLayout)
    : "responsive-columns";
}

export function parseGalleryDirective(value: string): {
  title: string;
  layout: GalleryLayout;
  items: CarouselItem[];
} | null {
  const match = value.trim().match(/^```gallery\n([\s\S]*?)\n```$/i);
  if (!match) return null;
  const lines = match[1].split(/\r?\n/);
  const title = lines.find((line) => /^title\s*:/i.test(line))?.replace(/^title\s*:/i, "").trim();
  const layout = lines.find((line) => /^layout\s*:/i.test(line))?.replace(/^layout\s*:/i, "").trim();
  const items = lines
    .map((line) => line.trim().match(/^!\[([^\]]*)\]\(([^\s)]+)(?:\s+"([^"]*)")?\)$/))
    .filter((item): item is RegExpMatchArray => Boolean(item))
    .map((item, index) => ({ src: item[2], alt: item[1] || item[3] || `Image ${index + 1}` }))
    .filter((item) => isSafeCarouselImageSource(item.src));
  return {
    title: title?.slice(0, 120) || DEFAULT_GALLERY_TITLE,
    layout: normalizeGalleryLayout(layout),
    items: normalizeCarouselItems(items),
  };
}

export function serializeGalleryDirective(
  title: string,
  layout: GalleryLayout,
  items: CarouselItem[],
): string {
  const images = normalizeCarouselItems(items)
    .filter((item) => isSafeCarouselImageSource(item.src))
    .map((item) => `![${item.alt}](${item.src})`)
    .join("\n");
  return [
    "```gallery",
    `layout: ${normalizeGalleryLayout(layout)}`,
    `title: ${title.trim() || DEFAULT_GALLERY_TITLE}`,
    "---",
    images,
    "```",
  ].join("\n");
}

function GalleryLightbox({
  items,
  index,
  onClose,
  onNavigate,
}: {
  items: CarouselItem[];
  index: number;
  onClose: () => void;
  onNavigate: (index: number) => void;
}) {
  const item = items[index];
  if (!item) return null;
  return (
    <DialogShell labelledBy="gallery-lightbox-title" maxWidth="canvas" onClose={onClose}>
      <div className="bg-surface flex max-h-[calc(100dvh-2rem)] flex-col">
        <div className="border-border flex items-center justify-between gap-3 border-b px-3 py-2">
          <div className="min-w-0">
            <h2 id="gallery-lightbox-title" className="text-text truncate text-sm font-medium">
              {item.alt}
            </h2>
            <p className="text-text-muted text-xs">Image {index + 1} of {items.length}</p>
          </div>
          <Button type="button" variant="ghost" size="sm" onClick={onClose} aria-label="Close gallery lightbox">
            <X data-icon="inline-start" />
          </Button>
        </div>
        <div className="flex min-h-0 items-center gap-2 p-3">
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-label="Previous gallery image"
            onClick={() => onNavigate((index + items.length - 1) % items.length)}
          >
            <ChevronLeft data-icon="inline-start" />
          </Button>
          <img src={item.src} alt={item.alt} className="max-h-[min(72dvh,48rem)] min-w-0 flex-1 object-contain" />
          <Button
            type="button"
            variant="ghost"
            size="sm"
            aria-label="Next gallery image"
            onClick={() => onNavigate((index + 1) % items.length)}
          >
            <ChevronRight data-icon="inline-start" />
          </Button>
        </div>
      </div>
    </DialogShell>
  );
}

function GalleryPreview({
  items,
  layout,
  onOpen,
}: {
  items: CarouselItem[];
  layout: GalleryLayout;
  onOpen: (index: number) => void;
}) {
  const [activeIndex, setActiveIndex] = useState(0);
  const validItems = items.filter((item) => isSafeCarouselImageSource(item.src));
  useEffect(() => {
    if (activeIndex >= validItems.length) setActiveIndex(0);
  }, [activeIndex, validItems.length]);
  if (validItems.length === 0) {
    return <div className="border-border bg-surface-raised text-text-muted flex min-h-36 items-center justify-center border text-sm">Add image URLs to preview the gallery.</div>;
  }
  const image = (item: CarouselItem, index: number, className = "") => (
    <button
      key={`${item.src}-${index}`}
      type="button"
      className={`border-border bg-surface-raised block overflow-hidden border text-left ${className}`}
      onClick={() => onOpen(index)}
      aria-label={`Open ${item.alt} in lightbox`}
    >
      <img src={item.src} alt={item.alt} loading="lazy" className="h-full w-full object-cover" />
      <span className="text-text-muted block truncate px-2 py-1 text-xs">{item.alt}</span>
    </button>
  );
  if (layout === "accordion") {
    return (
      <div className="divide-border divide-y border border-border">
        {validItems.map((item, index) => (
          <div key={`${item.src}-${index}`}>
            <button type="button" className="text-text flex w-full items-center justify-between gap-2 px-3 py-2 text-left text-sm" onClick={() => setActiveIndex(index)} aria-expanded={activeIndex === index}>
              {item.alt}<ChevronDown data-icon="inline-end" />
            </button>
            {activeIndex === index ? <div className="p-2">{image(item, index, "aspect-video w-full")}</div> : null}
          </div>
        ))}
      </div>
    );
  }
  if (layout === "thumbnail-strip" || layout === "carousel") {
    return (
      <div className="flex flex-col gap-2">
        {image(validItems[activeIndex], activeIndex, "aspect-video w-full")}
        <div className="flex gap-2 overflow-x-auto">{validItems.map((item, index) => image(item, index, "h-16 w-24 shrink-0"))}</div>
      </div>
    );
  }
  if (layout === "timeline") {
    return <div className="flex flex-col gap-2">{validItems.map((item, index) => <div key={`${item.src}-${index}`} className="grid grid-cols-[2rem_1fr] items-start gap-2"><span className="text-text-muted pt-3 text-xs">{String(index + 1).padStart(2, "0")}</span>{image(item, index, "aspect-video w-full")}</div>)}</div>;
  }
  if (layout === "coverflow") {
    return <div className="flex snap-x gap-2 overflow-x-auto py-1">{validItems.map((item, index) => image(item, index, "aspect-[4/3] min-w-[70%] snap-center sm:min-w-[38%]"))}</div>;
  }
  const columns = validItems.length === 1 ? "grid-cols-1" : validItems.length <= 4 ? "grid-cols-2" : "grid-cols-2 sm:grid-cols-3";
  return <div className={`grid ${columns} gap-2`}>{validItems.map((item, index) => image(item, index, "aspect-[4/3]"))}</div>;
}

export function GalleryNodeView({ node, updateAttributes, editor }: ReactNodeViewProps) {
  const title = String(node.attrs.title || DEFAULT_GALLERY_TITLE);
  const layout = normalizeGalleryLayout(node.attrs.layout);
  const items = normalizeCarouselItems(node.attrs.items);
  const [lightboxIndex, setLightboxIndex] = useState<number | null>(null);
  const updateItem = (index: number, patch: Partial<CarouselItem>) => {
    updateAttributes({ items: items.map((item, itemIndex) => itemIndex === index ? { ...item, ...patch } : item) });
  };
  return (
    <div data-gallery-block="true" contentEditable={false} className="my-4 rounded-card border border-border bg-surface p-3">
      <div className="mb-3 flex flex-wrap items-center justify-between gap-2">
        {editor.isEditable ? <Input aria-label="Gallery title" value={title} onChange={(event) => updateAttributes({ title: event.target.value.slice(0, 120) })} className="h-8 max-w-xs bg-transparent text-sm" /> : <p className="text-text text-sm font-medium">{title}</p>}
        {editor.isEditable ? <Select aria-label="Gallery layout" value={layout} onChange={(event) => updateAttributes({ layout: normalizeGalleryLayout(event.target.value) })} className="h-8 w-auto text-xs">{GALLERY_LAYOUTS.map((option) => <option key={option} value={option}>{option.replace(/-/g, " ")}</option>)}</Select> : null}
        <span className="text-text-muted text-xs">{items.length}/{MAX_CAROUSEL_IMAGES}</span>
      </div>
      <GalleryPreview items={items} layout={layout} onOpen={setLightboxIndex} />
      {editor.isEditable ? <div className="mt-3 flex flex-col gap-2">{items.map((item, index) => <div key={index} className="flex items-center gap-2"><Input aria-label={`Gallery image ${index + 1} URL`} value={item.src} onChange={(event) => updateItem(index, { src: event.target.value })} placeholder="https://example.com/image.jpg" className="h-9 min-w-0 flex-1 text-xs" /><Input aria-label={`Gallery image ${index + 1} alt text`} value={item.alt} onChange={(event) => updateItem(index, { alt: event.target.value })} placeholder="Alt text" className="h-9 w-32 text-xs" /><Button type="button" variant="ghost" size="sm" className="size-8 shrink-0 p-0" aria-label={`Remove gallery image ${index + 1}`} onClick={() => updateAttributes({ items: items.filter((_, itemIndex) => itemIndex !== index) })}><Trash2 data-icon="inline-start" /></Button></div>)}<Button type="button" variant="secondary" size="sm" disabled={items.length >= MAX_CAROUSEL_IMAGES} onClick={() => updateAttributes({ items: [...items, { src: "", alt: `Image ${items.length + 1}` }] })}><ImagePlus data-icon="inline-start" />Add image</Button></div> : null}
      {lightboxIndex !== null ? <GalleryLightbox items={items.filter((item) => isSafeCarouselImageSource(item.src))} index={lightboxIndex} onClose={() => setLightboxIndex(null)} onNavigate={setLightboxIndex} /> : null}
    </div>
  );
}

export function serializeGalleryItems(items: CarouselItem[]): string {
  return serializeCarouselItems(items);
}
