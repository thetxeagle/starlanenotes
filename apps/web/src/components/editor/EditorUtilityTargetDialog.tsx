import { useRef } from "react";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { RESPONSIVE_MENU_OPTION_HEIGHT_CLASS } from "@/components/ui/form-control";
import { Input } from "@/components/ui/input";
import { useListboxKeyboardNavigation } from "@/components/ui/useListboxKeyboardNavigation";

export interface EditorUtilityTarget {
  id: string;
  label: string;
  detail?: string;
}

export function filterEditorUtilityTargets(
  targets: readonly EditorUtilityTarget[],
  query: string,
): EditorUtilityTarget[] {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return [...targets];
  return targets.filter((target) =>
    `${target.label} ${target.detail || ""}`.toLowerCase().includes(normalized),
  );
}

interface EditorUtilityTargetDialogProps {
  kind: "space" | "author";
  targets: readonly EditorUtilityTarget[];
  query: string;
  loading: boolean;
  error: boolean;
  onQueryChange: (query: string) => void;
  onSelect: (target: EditorUtilityTarget) => void;
  onCancel: () => void;
}

export function EditorUtilityTargetDialog({
  kind,
  targets,
  query,
  loading,
  error,
  onQueryChange,
  onSelect,
  onCancel,
}: EditorUtilityTargetDialogProps) {
  const filteredTargets = filterEditorUtilityTargets(targets, query);
  const label = kind === "space" ? "Space" : "Author";
  const listboxRef = useRef<HTMLDivElement | null>(null);
  const handleListboxKeyDown = useListboxKeyboardNavigation(listboxRef);
  return (
    <DialogShell
      labelledBy="editor-utility-target-title"
      describedBy="editor-utility-target-description"
      maxWidth="sm"
      onClose={onCancel}
    >
      <DialogHeader>
        <div>
          <DialogTitle id="editor-utility-target-title">
            Choose {label.toLowerCase()}
          </DialogTitle>
          <DialogDescription id="editor-utility-target-description">
            Search for the {label.toLowerCase()} to insert instead of using only the current context.
          </DialogDescription>
        </div>
      </DialogHeader>
      <DialogBody>
        <label
          htmlFor="editor-utility-target-search"
          className="text-text block text-sm font-medium"
        >
          Search {label.toLowerCase()}s
        </label>
        <Input
          id="editor-utility-target-search"
          value={query}
          onChange={(event) => onQueryChange(event.target.value)}
          className="bg-surface mt-1 h-9 text-sm"
          placeholder={`Search ${label.toLowerCase()}s`}
          autoFocus
          autoComplete="off"
        />
        <div
          ref={listboxRef}
          role="listbox"
          aria-label={`${label}s`}
          onKeyDown={handleListboxKeyDown}
          className="rounded-input border-border mt-3 max-h-56 overflow-y-auto border p-1"
        >
          {loading ? (
            <p className="text-text-muted px-2 py-2 text-sm">Loading {label.toLowerCase()}s…</p>
          ) : error ? (
            <p className="text-danger px-2 py-2 text-sm">Unable to load {label.toLowerCase()}s.</p>
          ) : filteredTargets.length ? (
            filteredTargets.map((target, index) => (
              <button
                key={target.id}
                type="button"
                role="option"
                aria-selected="false"
                tabIndex={index === 0 ? 0 : -1}
                className={`rounded-button text-text hover:bg-surface-raised hover:text-primary flex w-full flex-col items-start px-2 py-2 text-left text-sm transition-colors duration-100 ${RESPONSIVE_MENU_OPTION_HEIGHT_CLASS}`}
                onClick={() => onSelect(target)}
              >
                <span>{target.label}</span>
                {target.detail ? (
                  <span className="text-text-muted mt-0.5 text-xs">{target.detail}</span>
                ) : null}
              </button>
            ))
          ) : (
            <p className="text-text-muted px-2 py-2 text-sm">No matches.</p>
          )}
        </div>
      </DialogBody>
      <DialogFooter>
        <Button type="button" variant="secondary" size="sm" onClick={onCancel}>
          Cancel
        </Button>
      </DialogFooter>
    </DialogShell>
  );
}
