import { useState } from "react";
import { ImagePlus, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface CoverImageProps {
  src?: string;
  onUpdate: (url: string | null) => void;
  showEmptyPlaceholder?: boolean;
}

export function CoverImage({ src, onUpdate, showEmptyPlaceholder = true }: CoverImageProps) {
  const [hovering, setHovering] = useState(false);
  const [editing, setEditing] = useState(false);
  const [nextUrl, setNextUrl] = useState("");

  const closeEditor = () => {
    setEditing(false);
    setNextUrl("");
  };

  const saveCover = () => {
    const trimmed = nextUrl.trim();
    if (!trimmed) return;
    onUpdate(trimmed);
    closeEditor();
  };

  if (!src) {
    if (!showEmptyPlaceholder) return null;

    return (
      <div className="group mb-6 space-y-2">
        {!editing && (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="flex h-8 items-center gap-1 text-xs text-text-muted transition-colors duration-100 hover:text-text"
          >
            <ImagePlus size={14} />
            Add cover
          </button>
        )}
        {editing && (
          <form
            className="rounded-card border border-border bg-surface p-3"
            onSubmit={(event) => {
              event.preventDefault();
              saveCover();
            }}
          >
            <label htmlFor="cover-image-url" className="text-xs font-medium text-text">
              Cover image URL
            </label>
            <div className="mt-2 flex flex-col gap-2 sm:flex-row">
              <Input
                id="cover-image-url"
                value={nextUrl}
                onChange={(event) => setNextUrl(event.target.value)}
                placeholder="https://example.com/cover.jpg"
                aria-label="Cover image URL"
                className="h-8 bg-surface text-sm"
                autoComplete="off"
              />
              <div className="flex justify-end gap-2">
                <Button type="button" variant="ghost" size="sm" onClick={closeEditor}>
                  Cancel
                </Button>
                <Button type="submit" size="sm" disabled={!nextUrl.trim()}>
                  Save
                </Button>
              </div>
            </div>
          </form>
        )}
      </div>
    );
  }

  return (
    <div
      className="group relative mb-6 h-48 w-full overflow-hidden rounded-card"
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
    >
      <img src={src} alt="Cover" className="h-full w-full object-cover" />

      {hovering && (
        <button
          onClick={() => onUpdate(null)}
          className="absolute right-2 top-2 flex h-6 w-6 items-center justify-center rounded-button bg-black/50 text-white opacity-0 transition-opacity duration-100 group-hover:opacity-100"
          aria-label="Remove cover"
        >
          <X size={14} />
        </button>
      )}
    </div>
  );
}
