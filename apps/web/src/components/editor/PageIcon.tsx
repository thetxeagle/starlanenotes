import { SmilePlus, X } from "lucide-react";
import { useMemo, useRef, useState, type KeyboardEvent } from "react";
import {
  CONTENT_ICON_CATALOG,
  ContentIdentityIcon,
  getContentIconLabel,
  type ContentIconOption,
} from "@/components/ContentIdentityIcon";
import { EmojiPickerContent } from "@/components/editor/EmojiPickerContent";
import { Button } from "@/components/ui/button";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { SearchInput } from "@/components/ui/search-input";

interface PageIconProps {
  icon?: string;
  onUpdate?: (icon: string) => void | Promise<void>;
}

interface PageEmoji {
  emoji: string;
  label: string;
  keywords: string;
  category: string;
}

interface PageIconChoice {
  value: string;
  label: string;
  keywords: string;
  category: string;
  source: "emoji" | "icon";
}

type PageIconSourceFilter = "all" | PageIconChoice["source"];

export const PAGE_ICON_UPDATE_ERROR_FALLBACK = "Could not save the page icon. Try again.";

export function getPageIconUpdateError(error: unknown): string {
  return error instanceof Error && error.message.trim()
    ? error.message
    : PAGE_ICON_UPDATE_ERROR_FALLBACK;
}

export const PAGE_EMOJI_CATALOG: PageEmoji[] = [
  { emoji: "📄", label: "Page", keywords: "document paper file", category: "Pages" },
  { emoji: "📝", label: "Notes", keywords: "write pencil memo", category: "Pages" },
  { emoji: "📘", label: "Blue book", keywords: "reference guide", category: "Pages" },
  { emoji: "📚", label: "Books", keywords: "library knowledge reference", category: "Pages" },
  { emoji: "📌", label: "Pinned", keywords: "important pin", category: "Pages" },
  { emoji: "🔖", label: "Bookmark", keywords: "saved reference", category: "Pages" },
  { emoji: "🗂️", label: "Index", keywords: "organize files tabs", category: "Pages" },
  { emoji: "📋", label: "Clipboard", keywords: "list checklist", category: "Pages" },
  { emoji: "💡", label: "Idea", keywords: "light bulb insight", category: "Ideas" },
  { emoji: "🧠", label: "Brain", keywords: "thinking knowledge", category: "Ideas" },
  { emoji: "🔍", label: "Research", keywords: "search inspect", category: "Ideas" },
  { emoji: "🧪", label: "Experiment", keywords: "test lab science", category: "Ideas" },
  { emoji: "🎯", label: "Goal", keywords: "target objective", category: "Ideas" },
  { emoji: "🧭", label: "Direction", keywords: "compass roadmap", category: "Ideas" },
  { emoji: "🗺️", label: "Map", keywords: "plan roadmap", category: "Ideas" },
  { emoji: "✨", label: "Sparkles", keywords: "new idea magic", category: "Ideas" },
  { emoji: "💼", label: "Work", keywords: "business briefcase", category: "Work" },
  { emoji: "📊", label: "Report", keywords: "chart analytics", category: "Work" },
  { emoji: "📈", label: "Growth", keywords: "trend progress", category: "Work" },
  { emoji: "🗓️", label: "Plan", keywords: "calendar schedule", category: "Work" },
  { emoji: "🤝", label: "Meeting", keywords: "team agreement", category: "Work" },
  { emoji: "🚀", label: "Launch", keywords: "ship project rocket", category: "Work" },
  { emoji: "🛠️", label: "Build", keywords: "tools engineering", category: "Work" },
  { emoji: "💻", label: "Code", keywords: "computer development", category: "Work" },
  { emoji: "✅", label: "Complete", keywords: "done check success", category: "Status" },
  { emoji: "⏳", label: "In progress", keywords: "time waiting", category: "Status" },
  { emoji: "🚧", label: "Blocked", keywords: "construction work", category: "Status" },
  { emoji: "⚠️", label: "Warning", keywords: "attention risk", category: "Status" },
  { emoji: "🔥", label: "Priority", keywords: "hot urgent", category: "Status" },
  { emoji: "⭐", label: "Star", keywords: "favorite important", category: "Status" },
  { emoji: "❓", label: "Question", keywords: "help unknown", category: "Status" },
  { emoji: "💬", label: "Discussion", keywords: "chat comment", category: "Status" },
  { emoji: "🏠", label: "Home", keywords: "house personal", category: "Life" },
  { emoji: "❤️", label: "Heart", keywords: "love favorite", category: "Life" },
  { emoji: "🌱", label: "Growing", keywords: "plant garden progress", category: "Life" },
  { emoji: "🍳", label: "Recipe", keywords: "food cooking", category: "Life" },
  { emoji: "🏋️", label: "Fitness", keywords: "workout health", category: "Life" },
  { emoji: "✈️", label: "Travel", keywords: "flight trip", category: "Life" },
  { emoji: "🎓", label: "Learning", keywords: "school education", category: "Life" },
  { emoji: "🎨", label: "Creative", keywords: "art design", category: "Life" },
  { emoji: "🔐", label: "Secure", keywords: "lock private security", category: "Objects" },
  { emoji: "🔑", label: "Key", keywords: "access password", category: "Objects" },
  { emoji: "⚙️", label: "Settings", keywords: "gear configuration", category: "Objects" },
  { emoji: "🔔", label: "Notification", keywords: "bell alert", category: "Objects" },
  { emoji: "📎", label: "Attachment", keywords: "paperclip file", category: "Objects" },
  { emoji: "🔗", label: "Link", keywords: "chain url", category: "Objects" },
  { emoji: "📦", label: "Package", keywords: "box release", category: "Objects" },
  { emoji: "🗄️", label: "Archive", keywords: "storage cabinet", category: "Objects" },
];

export function canClearPageIcon(icon?: string): boolean {
  return Boolean(icon?.trim());
}

export function getPageEmojiResults(query: string): PageEmoji[] {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) return PAGE_EMOJI_CATALOG;
  return PAGE_EMOJI_CATALOG.filter(({ emoji, label, keywords, category }) =>
    `${emoji} ${label} ${keywords} ${category}`.toLocaleLowerCase().includes(normalizedQuery),
  );
}

export function getPageIconResults(
  query: string,
  source: PageIconSourceFilter = "all",
): PageIconChoice[] {
  const emojiOptions: PageIconChoice[] = PAGE_EMOJI_CATALOG.map((option) => ({
    value: option.emoji,
    label: option.label,
    keywords: option.keywords,
    category: option.category,
    source: "emoji",
  }));
  const lucideOptions: PageIconChoice[] = CONTENT_ICON_CATALOG.map((option: ContentIconOption) => ({
    value: option.value,
    label: option.label,
    keywords: option.keywords,
    category: option.category,
    source: "icon",
  }));
  const normalizedQuery = query.trim().toLocaleLowerCase();
  return [...emojiOptions, ...lucideOptions].filter((option) => {
    if (source !== "all" && option.source !== source) return false;
    if (!normalizedQuery) return true;
    return `${option.value} ${option.label} ${option.keywords} ${option.category}`
      .toLocaleLowerCase()
      .includes(normalizedQuery);
  });
}

export function PageIcon({ icon, onUpdate }: PageIconProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [source, setSource] = useState<PageIconSourceFilter>("all");
  const [pendingIcon, setPendingIcon] = useState<string | undefined>();
  const [updateError, setUpdateError] = useState("");
  const [updating, setUpdating] = useState(false);
  const searchRef = useRef<HTMLInputElement | null>(null);
  const results = useMemo(() => getPageIconResults(query, source), [query, source]);
  const selectedIcon = pendingIcon ?? icon;
  const categories = useMemo(
    () => Array.from(new Set(results.map((result) => result.category))),
    [results],
  );

  const selectIcon = async (nextIcon: string) => {
    if (!onUpdate || updating) return;
    setUpdating(true);
    setPendingIcon(nextIcon);
    setUpdateError("");
    try {
      await onUpdate(nextIcon);
      setOpen(false);
      setQuery("");
      setSource("all");
    } catch (error) {
      setPendingIcon(undefined);
      setUpdateError(getPageIconUpdateError(error));
    } finally {
      setUpdating(false);
    }
  };

  const handleGridKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const buttons = Array.from(
      event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="option"]'),
    );
    const currentIndex = buttons.findIndex((button) => button === document.activeElement);
    if (currentIndex < 0) return;

    const columns = 6;
    let nextIndex = currentIndex;
    if (event.key === "ArrowRight") nextIndex = Math.min(buttons.length - 1, currentIndex + 1);
    else if (event.key === "ArrowLeft") nextIndex = Math.max(0, currentIndex - 1);
    else if (event.key === "ArrowDown")
      nextIndex = Math.min(buttons.length - 1, currentIndex + columns);
    else if (event.key === "ArrowUp") nextIndex = Math.max(0, currentIndex - columns);
    else if (event.key === "Home") nextIndex = 0;
    else if (event.key === "End") nextIndex = buttons.length - 1;
    else return;

    event.preventDefault();
    buttons[nextIndex]?.focus();
  };

  return (
    <Popover
      open={open}
      onOpenChange={(nextOpen) => {
        if (!nextOpen && updating) return;
        setOpen(nextOpen);
        if (!nextOpen) {
          setQuery("");
          setSource("all");
          setPendingIcon(undefined);
          setUpdateError("");
        }
      }}
    >
      <PopoverTrigger asChild>
        <button
          type="button"
          disabled={!onUpdate}
          className={`rounded-button text-text-muted hover:bg-surface-raised hover:text-text focus-ring flex shrink-0 items-center justify-center leading-none transition-colors duration-100 disabled:opacity-50 ${
            icon ? "size-12 text-3xl" : "size-12 text-sm"
          }`}
          aria-label={
            icon
              ? `Change page icon, currently ${getContentIconLabel(icon) ?? "selected"}`
              : "Choose page icon"
          }
          title={icon ? "Change page icon" : "Choose page icon"}
        >
          <ContentIdentityIcon
            value={icon}
            size={28}
            fallback={<SmilePlus size={18} aria-hidden="true" />}
          />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="start"
        sideOffset={8}
        className="w-[min(20rem,calc(100vw-2rem))] p-0"
        aria-label="Page icon picker"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          window.requestAnimationFrame(() => searchRef.current?.focus());
        }}
      >
        <div className="border-border border-b p-3">
          <div className="mb-2 flex items-center justify-between gap-3">
            <h2 className="text-sm font-semibold">Page icon</h2>
            <span className="text-text-muted text-xs">
              {source === "emoji" ? "Every emoji" : `${results.length} choices`}
            </span>
          </div>
          {source !== "emoji" && (
            <SearchInput
              ref={searchRef}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={(event) => {
                if (event.key !== "ArrowDown") return;
                const firstOption = event.currentTarget
                  .closest('[aria-label="Page icon picker"]')
                  ?.querySelector<HTMLButtonElement>('[role="option"]');
                if (!firstOption) return;
                event.preventDefault();
                firstOption.focus();
              }}
              placeholder="Search icons and emoji"
              aria-label="Search page icons"
              className="h-9"
            />
          )}
          <div className="border-border mt-2 flex border-b" role="tablist" aria-label="Icon source">
            {(["all", "emoji", "icon"] as const).map((filter) => (
              <button
                key={filter}
                type="button"
                role="tab"
                aria-selected={source === filter}
                onClick={() => setSource(filter)}
                className={`focus-ring -mb-px border-b-2 px-3 py-2 text-xs font-medium transition-colors duration-100 ${
                  source === filter
                    ? "border-primary text-text"
                    : "text-text-muted hover:text-text border-transparent"
                }`}
              >
                {filter === "all" ? "All" : filter === "emoji" ? "Emoji" : "Icons"}
              </button>
            ))}
          </div>
        </div>
        {source === "emoji" ? (
          <div className="starlane-emoji-picker w-full p-2">
            <EmojiPickerContent onSelect={(nextIcon) => void selectIcon(nextIcon)} />
          </div>
        ) : (
          <div
            role="listbox"
            aria-label="Available page icons"
            aria-busy={updating}
            className="scrollbar-none max-h-72 overflow-y-auto p-3"
            onKeyDown={handleGridKeyDown}
          >
            {results.length === 0 ? (
              <p className="text-text-muted py-6 text-center text-sm">No icons found.</p>
            ) : (
              <div className="flex flex-col gap-3">
                {categories.map((category) => (
                  <div
                    key={category}
                    role="group"
                    aria-labelledby={`page-icon-${category.toLowerCase()}`}
                  >
                    <h3
                      id={`page-icon-${category.toLowerCase()}`}
                      className="text-text-muted mb-1.5 text-xs font-medium"
                    >
                      {category}
                    </h3>
                    <div className="grid grid-cols-6 gap-1">
                      {results
                        .filter((result) => result.category === category)
                        .map((result) => (
                          <button
                            key={result.value}
                            type="button"
                            role="option"
                            aria-selected={selectedIcon === result.value}
                            aria-label={result.label}
                            title={result.label}
                            disabled={updating}
                            onClick={() => void selectIcon(result.value)}
                            className={`rounded-button focus-ring text-text flex size-10 items-center justify-center border transition-colors duration-100 ${
                              selectedIcon === result.value
                                ? "border-primary bg-primary/10"
                                : "hover:border-border hover:bg-surface-raised border-transparent"
                            } disabled:cursor-wait disabled:opacity-60`}
                          >
                            <ContentIdentityIcon value={result.value} size={20} />
                          </button>
                        ))}
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
        {updateError && (
          <p
            role="alert"
            className="border-danger/30 bg-danger/10 text-danger border-t px-3 py-2 text-xs"
          >
            {updateError}
          </p>
        )}
        {canClearPageIcon(icon) && (
          <div className="border-border flex justify-end border-t p-2">
            <Button
              type="button"
              variant="ghost"
              size="sm"
              disabled={updating}
              onClick={() => void selectIcon("")}
            >
              <X size={14} data-icon="inline-start" aria-hidden="true" />
              Remove icon
            </Button>
          </div>
        )}
      </PopoverContent>
    </Popover>
  );
}
