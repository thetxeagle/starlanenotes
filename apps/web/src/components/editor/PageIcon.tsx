import { useState } from "react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";

interface PageIconProps {
  icon?: string;
  onUpdate?: (icon: string) => void;
}

export function getInitialPageIconInput(icon?: string): string {
  return icon?.trim() ?? "";
}

export function canSavePageIconInput(icon: string): boolean {
  return icon.trim().length > 0;
}

export function PageIcon({ icon, onUpdate }: PageIconProps) {
  const [editing, setEditing] = useState(false);
  const [nextIcon, setNextIcon] = useState(() => getInitialPageIconInput(icon));

  if (!icon) return null;

  const startEditing = () => {
    setNextIcon(getInitialPageIconInput(icon));
    setEditing(true);
  };

  const cancelEditing = () => {
    setNextIcon(getInitialPageIconInput(icon));
    setEditing(false);
  };

  const saveIcon = () => {
    if (!onUpdate || !canSavePageIconInput(nextIcon)) return;
    onUpdate(nextIcon.trim());
    setEditing(false);
  };

  return (
    <div className="mb-2 space-y-2">
      <button
        type="button"
        onClick={startEditing}
        disabled={!onUpdate}
        className="text-4xl leading-none transition-opacity duration-100 hover:opacity-80"
        aria-label="Edit page icon"
      >
        {icon}
      </button>
      {editing && (
        <form
          className="flex max-w-xs items-center gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            saveIcon();
          }}
        >
          <Input
            value={nextIcon}
            onChange={(event) => setNextIcon(event.target.value)}
            aria-label="Page icon"
            className="h-8 bg-surface text-sm"
            autoComplete="off"
          />
          <Button type="button" variant="ghost" size="sm" onClick={cancelEditing}>
            Cancel
          </Button>
          <Button type="submit" size="sm" disabled={!canSavePageIconInput(nextIcon)}>
            Save
          </Button>
        </form>
      )}
    </div>
  );
}
