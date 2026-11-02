import { normalizeBlockPlacement, type BlockPlacement } from "./blockPlacement";
import { DialogShell } from "@/components/ui/dialog-shell";
import { Button } from "@/components/ui/button";
import { highlightedCodeToHtml } from "@/components/editor/Editor";
import { cn } from "@/lib/utils";
import {
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Copy,
  Pause,
  Play,
  Volume2,
  VolumeX,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";

export type PortableStatusState = "online" | "offline" | "maintenance" | "degraded";
export type PortableGalleryLayout =
  | "carousel-7"
  | "masonry"
  | "accordion"
  | "coverflow"
  | "kenburns"
  | "thumbnail-strip"
  | "timeline"
  | "responsive-columns";

export interface PortableCodeTab {
  label: string;
  language: string;
  code: string;
}

export interface PortableStatusBlockConfig {
  state: PortableStatusState;
  label: string;
  detail?: string;
}

export interface PortableVideoBlockConfig {
  src: string;
  title?: string;
  poster?: string;
}

export interface PortableGalleryItem {
  src: string;
  alt: string;
  caption?: string;
}

export interface PortableGalleryBlockConfig {
  layout: PortableGalleryLayout;
  title?: string;
  items: PortableGalleryItem[];
}

const GALLERY_LAYOUTS: readonly PortableGalleryLayout[] = [
  "carousel-7",
  "masonry",
  "accordion",
  "coverflow",
  "kenburns",
  "thumbnail-strip",
  "timeline",
  "responsive-columns",
];

const STATUS_STYLES: Record<PortableStatusState, { label: string; className: string }> = {
  online: { label: "Online", className: "bg-success" },
  offline: { label: "Offline", className: "bg-danger" },
  maintenance: { label: "Maintenance", className: "bg-warning" },
  degraded: { label: "Degraded", className: "bg-accent" },
};

function readKeyValueLines(source: string): { values: Map<string, string>; remainder: string[] } {
  const values = new Map<string, string>();
  const remainder: string[] = [];
  for (const line of source.split("\n")) {
    const match = line.match(/^([a-z][a-z0-9-]*)\s*:\s*(.+)$/i);
    if (!match) {
      remainder.push(line);
      continue;
    }
    values.set(match[1].toLowerCase(), match[2].trim());
  }
  return { values, remainder };
}

function isSafeMediaSource(value: string | undefined): value is string {
  if (!value) return false;
  const trimmed = value.trim();
  return (
    /^(https?:|starlane:|blob:)/i.test(trimmed) ||
    /^data:image\/(png|gif|jpe?g|webp|avif);/i.test(trimmed)
  );
}

export function parsePortableStatusBlock(source: string): PortableStatusBlockConfig {
  const { values, remainder } = readKeyValueLines(source);
  const rawState = values.get("state")?.toLowerCase();
  const state: PortableStatusState =
    rawState === "online" ||
    rawState === "offline" ||
    rawState === "maintenance" ||
    rawState === "degraded"
      ? rawState
      : "online";
  const label = values.get("label") || remainder.join(" ").trim() || STATUS_STYLES[state].label;
  return { state, label: label.slice(0, 120), detail: values.get("detail")?.slice(0, 240) };
}

export function parsePortableVideoBlock(source: string): PortableVideoBlockConfig | null {
  const { values } = readKeyValueLines(source);
  const src = values.get("src") || "";
  if (!isSafeMediaSource(src)) return null;
  const poster = values.get("poster");
  return {
    src,
    title: values.get("title")?.slice(0, 120),
    poster: isSafeMediaSource(poster) ? poster : undefined,
  };
}

export function parsePortableCodeTabs(source: string): { title?: string; tabs: PortableCodeTab[] } {
  const lines = source.replace(/\r\n?/g, "\n").split("\n");
  const titleLine = lines[0]?.match(/^title:\s*(.+)$/i);
  const work = titleLine ? lines.slice(1) : lines;
  const tabs: PortableCodeTab[] = [];
  let current: PortableCodeTab | null = null;
  for (const line of work) {
    const tab = line.match(/^tab:\s*([^|\n]*?)(?:\s*\|\s*([\w-]+))?\s*$/i);
    if (tab) {
      if (current) tabs.push({ ...current, code: current.code.replace(/\n$/, "") });
      current = {
        label: tab[1].trim().slice(0, 80),
        language: (tab[2] || "text").trim(),
        code: "",
      };
      continue;
    }
    if (line.trim() === "---" && current) {
      tabs.push({ ...current, code: current.code.replace(/\n$/, "") });
      current = null;
      continue;
    }
    if (current) current.code += `${line}\n`;
  }
  if (current) tabs.push({ ...current, code: current.code.replace(/\n$/, "") });
  return {
    title: titleLine?.[1].trim().slice(0, 120),
    // Keep empty tabs visible. Optional names are allowed, so page view uses
    // the same positional fallback as the editor tab strip.
    tabs: tabs.map((tab, index) => ({ ...tab, label: tab.label || `Tab ${index + 1}` })),
  };
}

export function parsePortableGalleryBlock(source: string): PortableGalleryBlockConfig {
  const [settingsSource, ...itemSource] = source.split(/^---\s*$/m);
  const { values } = readKeyValueLines(settingsSource);
  const rawLayout = values.get("layout")?.toLowerCase();
  const layout: PortableGalleryLayout = GALLERY_LAYOUTS.includes(rawLayout as PortableGalleryLayout)
    ? (rawLayout as PortableGalleryLayout)
    : "responsive-columns";
  const items = itemSource
    .join("\n")
    .split("\n")
    .map((line) => line.trim().match(/^!\[([^\]]*)\]\(([^\s)]+)(?:\s+"([^"]*)")?\)$/))
    .filter((match): match is RegExpMatchArray => Boolean(match))
    .map((match) => ({
      src: match[2],
      alt: match[1] || "Gallery image",
      caption: match[3] || match[1] || undefined,
    }))
    .filter((item) => isSafeMediaSource(item.src))
    .slice(0, 9);
  return { layout, title: values.get("title")?.slice(0, 120), items };
}

export function PortableStatusBlock({ config }: { config: PortableStatusBlockConfig }) {
  const status = STATUS_STYLES[config.state];
  return (
    <section
      data-portable-status="true"
      aria-label={`${status.label}: ${config.label}`}
      className="border-border bg-surface my-4 border"
    >
      <div className="flex items-start gap-3 px-3 py-3">
        <span
          className={cn("mt-1.5 size-2 shrink-0 rounded-full", status.className)}
          aria-hidden="true"
        />
        <div className="min-w-0">
          <p className="text-text text-sm font-medium">{config.label}</p>
          {config.detail ? (
            <p className="text-text-muted mt-1 text-xs leading-5">{config.detail}</p>
          ) : null}
        </div>
        <span className="text-text-muted ml-auto shrink-0 text-xs">{status.label}</span>
      </div>
    </section>
  );
}

export function PortableCodeTabs({ title, tabs }: { title?: string; tabs: PortableCodeTab[] }) {
  const [activeIndex, setActiveIndex] = useState(0);
  const [copied, setCopied] = useState(false);
  const activeTab = tabs[activeIndex] || tabs[0];
  if (!activeTab) return null;
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(activeTab.code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };
  return (
    <section
      data-portable-code-tabs="true"
      className="border-border bg-surface my-4 overflow-hidden border"
    >
      <div className="border-border bg-surface-raised flex items-center justify-between gap-3 border-b px-3 py-2">
        <div className="min-w-0">
          {title ? <p className="text-text truncate text-sm font-medium">{title}</p> : null}
          <div
            role="tablist"
            aria-label={title || "Code examples"}
            className="mt-1 flex max-w-full overflow-x-auto"
          >
            {tabs.map((tab, index) => (
              <button
                key={`${tab.label}-${index}`}
                type="button"
                role="tab"
                aria-selected={index === activeIndex}
                className={cn(
                  "border-b-2 px-2 py-1 text-xs transition-colors duration-100",
                  index === activeIndex
                    ? "border-primary text-text"
                    : "text-text-muted hover:text-text border-transparent",
                )}
                onClick={() => setActiveIndex(index)}
              >
                {tab.label || `Tab ${index + 1}`}
              </button>
            ))}
          </div>
        </div>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void copy()}
          aria-label={`Copy ${activeTab.label} code`}
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
          <span className="hidden sm:inline">{copied ? "Copied" : "Copy"}</span>
        </Button>
      </div>
      <pre className="bg-surface-raised m-0 overflow-x-auto px-4 py-3">
        <code
          className={`language-${activeTab.language}`}
          dangerouslySetInnerHTML={{
            __html: highlightedCodeToHtml(activeTab.code, activeTab.language),
          }}
        />
      </pre>
    </section>
  );
}

export function PortableCodeBlock({
  code,
  language,
  title,
  placement,
}: {
  code: string;
  language: string;
  title?: string;
  placement?: BlockPlacement;
}) {
  const [copied, setCopied] = useState(false);
  const label = language || "Plain text";
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(code);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1600);
    } catch {
      setCopied(false);
    }
  };
  return (
    <section
      data-portable-code-block="true"
      data-block-placement={normalizeBlockPlacement(placement)}
      className="border-border bg-surface my-4 overflow-hidden border"
    >
      <div className="border-border bg-surface-raised flex min-h-10 items-center justify-between gap-3 border-b px-3 py-2">
        <span className="text-text-muted min-w-0 truncate font-mono text-xs">{title || label}</span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void copy()}
          aria-label={`Copy ${label} code`}
        >
          {copied ? <Check size={14} /> : <Copy size={14} />}
          <span className="hidden sm:inline">{copied ? "Copied" : "Copy"}</span>
        </Button>
      </div>
      <pre className="bg-surface-raised m-0 overflow-x-auto px-4 py-3">
        <code
          className={language ? `language-${language}` : undefined}
          dangerouslySetInnerHTML={{ __html: highlightedCodeToHtml(code, language) }}
        />
      </pre>
    </section>
  );
}

export function PortableVideoPlayer({ config }: { config: PortableVideoBlockConfig }) {
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [muted, setMuted] = useState(false);
  const [currentTime, setCurrentTime] = useState(0);
  const [duration, setDuration] = useState(0);
  const formatTime = (seconds: number) => {
    const safe = Number.isFinite(seconds) ? Math.max(0, Math.floor(seconds)) : 0;
    return `${Math.floor(safe / 60)}:${String(safe % 60).padStart(2, "0")}`;
  };
  const togglePlayback = async () => {
    const video = videoRef.current;
    if (!video) return;
    if (video.paused) await video.play();
    else video.pause();
  };
  const toggleMuted = () => {
    const video = videoRef.current;
    if (!video) return;
    video.muted = !video.muted;
    setMuted(video.muted);
  };
  return (
    <section
      data-portable-video="true"
      className="border-border bg-surface my-4 overflow-hidden border"
    >
      {config.title ? (
        <p className="border-border text-text border-b px-3 py-2 text-sm font-medium">
          {config.title}
        </p>
      ) : null}
      <video
        ref={videoRef}
        src={config.src}
        poster={config.poster}
        className="bg-bg aspect-video w-full object-contain"
        preload="metadata"
        onPlay={() => setPlaying(true)}
        onPause={() => setPlaying(false)}
        onTimeUpdate={(event) => setCurrentTime(event.currentTarget.currentTime)}
        onLoadedMetadata={(event) => setDuration(event.currentTarget.duration)}
      />
      <div className="border-border flex items-center gap-2 border-t px-3 py-2">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={() => void togglePlayback()}
          aria-label={playing ? "Pause video" : "Play video"}
        >
          {playing ? <Pause size={14} /> : <Play size={14} />}
        </Button>
        <input
          aria-label="Video progress"
          className="accent-primary h-1 min-w-0 flex-1"
          type="range"
          min="0"
          max={duration || 0}
          value={Math.min(currentTime, duration || 0)}
          onChange={(event) => {
            const video = videoRef.current;
            if (video) video.currentTime = Number(event.target.value);
          }}
        />
        <span className="text-text-muted shrink-0 font-mono text-xs">
          {formatTime(currentTime)} / {formatTime(duration)}
        </span>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          onClick={toggleMuted}
          aria-label={muted ? "Unmute video" : "Mute video"}
        >
          {muted ? <VolumeX size={14} /> : <Volume2 size={14} />}
        </Button>
      </div>
    </section>
  );
}

function GalleryImage({
  item,
  onOpen,
  className,
}: {
  item: PortableGalleryItem;
  onOpen: () => void;
  className?: string;
}) {
  return (
    <button
      type="button"
      className={cn(
        "border-border bg-surface-raised group block overflow-hidden border text-left",
        className,
      )}
      onClick={onOpen}
    >
      <img
        src={item.src}
        alt={item.alt}
        className="block h-full w-full object-cover"
        loading="lazy"
      />
      {item.caption ? (
        <span className="border-border text-text-muted block border-t px-2 py-1 text-xs">
          {item.caption}
        </span>
      ) : null}
    </button>
  );
}

export function PortableGallery({ config }: { config: PortableGalleryBlockConfig }) {
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [viewerIndex, setViewerIndex] = useState<number | null>(null);
  const selected = config.items[selectedIndex] || config.items[0];
  const layoutLabel = config.layout.replace(/-/g, " ");
  const orderedItems = useMemo(
    () => config.items.map((item, index) => ({ item, index })),
    [config.items],
  );
  useEffect(() => {
    if (selectedIndex >= config.items.length) setSelectedIndex(0);
  }, [config.items.length, selectedIndex]);
  if (!selected) return null;
  const open = (index: number) => {
    setSelectedIndex(index);
    setViewerIndex(index);
  };
  const previous = () =>
    setSelectedIndex((value) => (value + config.items.length - 1) % config.items.length);
  const next = () => setSelectedIndex((value) => (value + 1) % config.items.length);
  return (
    <section
      data-portable-gallery="true"
      data-gallery-layout={config.layout}
      className="border-border bg-surface my-4 border"
    >
      <div className="border-border flex items-center justify-between gap-3 border-b px-3 py-2">
        <p className="text-text text-sm font-medium">{config.title || "Gallery"}</p>
        <span className="text-text-muted text-xs">{layoutLabel}</span>
      </div>
      {config.layout === "carousel-7" ? (
        <div className="p-3">
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label="Previous gallery image"
              onClick={previous}
            >
              <ChevronLeft data-icon="inline-start" />
            </Button>
            <GalleryImage item={selected} onOpen={() => open(selectedIndex)} className="aspect-video flex-1" />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label="Next gallery image"
              onClick={next}
            >
              <ChevronRight data-icon="inline-start" />
            </Button>
          </div>
          <div className="mt-2 flex justify-center gap-1" aria-label="Carousel slides">
            {orderedItems.map(({ item, index }) => (
              <button
                type="button"
                key={`${item.src}-${index}`}
                className={cn(
                  "size-1.5 rounded-full border",
                  index === selectedIndex ? "border-primary bg-primary" : "border-border bg-bg",
                )}
                aria-label={`Show ${item.caption || item.alt}`}
                aria-current={index === selectedIndex}
                onClick={() => setSelectedIndex(index)}
              />
            ))}
          </div>
        </div>
      ) : config.layout === "masonry" ? (
        <div className="columns-2 gap-2 p-2 sm:columns-3">
          {orderedItems.map(({ item, index }) => (
            <div key={`${item.src}-${index}`} className="mb-2 break-inside-avoid">
              <GalleryImage item={item} onOpen={() => open(index)} className="w-full" />
            </div>
          ))}
        </div>
      ) : config.layout === "responsive-columns" ? (
        <div
          className={cn(
            "grid gap-2 p-2",
            config.items.length === 1
              ? "grid-cols-1"
              : config.items.length <= 4
                ? "grid-cols-2"
                : "grid-cols-2 sm:grid-cols-3",
          )}
        >
          {orderedItems.map(({ item, index }) => (
            <GalleryImage
              key={`${item.src}-${index}`}
              item={item}
              onOpen={() => open(index)}
              className="aspect-[4/3]"
            />
          ))}
        </div>
      ) : config.layout === "accordion" ? (
        <div className="divide-border divide-y">
          {orderedItems.map(({ item, index }) => (
            <div key={`${item.src}-${index}`}>
              <button
                type="button"
                className="text-text flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-sm"
                onClick={() => setSelectedIndex(index)}
                aria-expanded={selectedIndex === index}
              >
                <span>{item.caption || item.alt}</span>
                <ChevronDown size={16} />
              </button>
              {selectedIndex === index ? (
                <GalleryImage
                  item={item}
                  onOpen={() => open(index)}
                  className="mx-3 mb-3 aspect-video"
                />
              ) : null}
            </div>
          ))}
        </div>
      ) : config.layout === "timeline" ? (
        <ol className="divide-border divide-y">
          {orderedItems.map(({ item, index }) => (
            <li key={`${item.src}-${index}`} className="grid gap-2 p-3 sm:grid-cols-[7rem_1fr]">
              <span className="text-text-muted text-xs">{String(index + 1).padStart(2, "0")}</span>
              <GalleryImage item={item} onOpen={() => open(index)} className="aspect-video" />
            </li>
          ))}
        </ol>
      ) : config.layout === "thumbnail-strip" ? (
        <div className="p-3">
          <GalleryImage
            item={selected}
            onOpen={() => open(selectedIndex)}
            className="aspect-video"
          />
          <div className="mt-2 flex gap-2 overflow-x-auto">
            {orderedItems.map(({ item, index }) => (
              <GalleryImage
                key={`${item.src}-${index}`}
                item={item}
                onOpen={() => setSelectedIndex(index)}
                className={cn("h-16 w-24 shrink-0", index === selectedIndex && "border-primary")}
              />
            ))}
          </div>
        </div>
      ) : (
        <div className="p-3">
          <div className="flex items-center gap-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label="Previous gallery image"
              onClick={previous}
            >
              <ChevronLeft size={16} />
            </Button>
            <GalleryImage
              item={selected}
              onOpen={() => open(selectedIndex)}
              className={cn("aspect-video flex-1", config.layout === "kenburns" && "bg-bg")}
            />
            <Button
              type="button"
              variant="ghost"
              size="sm"
              aria-label="Next gallery image"
              onClick={next}
            >
              <ChevronRight size={16} />
            </Button>
          </div>
          <div className="mt-2 flex justify-center gap-1">
            {orderedItems.map(({ item, index }) => (
              <button
                type="button"
                key={`${item.src}-${index}`}
                className={cn(
                  "h-1.5 w-6 border",
                  index === selectedIndex
                    ? "border-primary bg-primary"
                    : "border-border bg-surface-raised",
                )}
                aria-label={`Show ${item.caption || item.alt}`}
                aria-current={index === selectedIndex}
                onClick={() => setSelectedIndex(index)}
              />
            ))}
          </div>
        </div>
      )}
      {viewerIndex !== null ? (
        <DialogShell
          labelledBy="portable-gallery-viewer-title"
          maxWidth="canvas"
          onClose={() => setViewerIndex(null)}
        >
          <div className="bg-surface">
            <div className="border-border flex items-center justify-between gap-3 border-b px-3 py-2">
              <div>
                <h2 id="portable-gallery-viewer-title" className="text-text text-sm font-medium">
                  {config.items[viewerIndex]?.caption || config.items[viewerIndex]?.alt}
                </h2>
                <p className="text-text-muted text-xs">
                  {viewerIndex + 1} of {config.items.length}
                </p>
              </div>
              <Button
                type="button"
                variant="secondary"
                size="sm"
                onClick={() => setViewerIndex(null)}
              >
                Close
              </Button>
            </div>
            <img
              src={config.items[viewerIndex]?.src}
              alt={config.items[viewerIndex]?.alt}
              className="max-h-[75dvh] w-full object-contain"
            />
          </div>
        </DialogShell>
      ) : null}
    </section>
  );
}
