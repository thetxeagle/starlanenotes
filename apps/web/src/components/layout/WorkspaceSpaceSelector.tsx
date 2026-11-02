import {
  useEffect,
  useRef,
  useState,
  type FormEvent,
  type KeyboardEvent as ReactKeyboardEvent,
  type RefObject,
} from "react";
import { Check, ChevronDown, Layers3, Plus } from "lucide-react";
import { ContentIdentityIcon } from "@/components/ContentIdentityIcon";
import { Button } from "@/components/ui/button";
import {
  DialogBody,
  DialogCloseAction,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "@/components/ui/dialog-shell";
import { Input } from "@/components/ui/input";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { SearchInput } from "@/components/ui/search-input";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { getPersonalSpaceOwnerLabel } from "@/lib/spaceLabels";
import {
  getWorkspaceSpaceSections,
  useWorkspaceSpaces,
  type WorkspaceSpace,
  type WorkspaceSpaceSection,
} from "./WorkspaceSpaces";

export type WorkspaceSpaceSelectorInputAction = "close" | "focus-options" | null;

export function getWorkspaceSpaceSelectorInputAction(
  key: string,
): WorkspaceSpaceSelectorInputAction {
  if (key === "Escape") return "close";
  if (key === "ArrowDown") return "focus-options";
  return null;
}

interface WorkspaceSpaceSelectorMenuProps {
  sections: WorkspaceSpaceSection[];
  activeSpaceId?: string;
  loading: boolean;
  canCreateSpace: boolean;
  menuRef: RefObject<HTMLDivElement | null>;
  onKeyDown: (event: ReactKeyboardEvent<HTMLDivElement>) => void;
  onSelect: (space: WorkspaceSpace) => void;
  onCreate: () => void;
}

export function WorkspaceSpaceSelectorMenu({
  sections,
  activeSpaceId,
  loading,
  canCreateSpace,
  menuRef,
  onKeyDown,
  onSelect,
  onCreate,
}: WorkspaceSpaceSelectorMenuProps) {
  const visibleSpaceCount = sections.reduce(
    (count, section) => count + section.spaces.length,
    0,
  );

  return (
    <div ref={menuRef} role="menu" aria-label="Spaces" onKeyDown={onKeyDown}>
      <div className="scrollbar-none max-h-72 overflow-y-auto p-1">
        {loading ? (
          <p className="text-text-muted px-2 py-3 text-sm" role="status" aria-live="polite">
            Loading spaces…
          </p>
        ) : visibleSpaceCount === 0 ? (
          <p className="text-text-muted px-2 py-3 text-sm">No matching spaces</p>
        ) : (
          sections.map((section) => (
            <div key={section.label} role="group" aria-label={section.label} className="py-1">
              <p className="text-text-muted px-2 pb-1 text-xs font-medium">{section.label}</p>
              {section.spaces.map((space) => {
                const selected = space.id === activeSpaceId;
                const ownerLabel = getPersonalSpaceOwnerLabel(space);
                return (
                  <button
                    key={space.id}
                    type="button"
                    role="menuitemradio"
                    aria-checked={selected}
                    onClick={() => onSelect(space)}
                    className={`rounded-button flex min-h-10 w-full items-center gap-2 px-2 text-left text-sm transition-colors duration-100 ${
                      selected
                        ? "bg-surface text-text"
                        : "text-text-muted hover:bg-surface hover:text-text"
                    }`}
                  >
                    <span className="bg-bg rounded-input flex size-6 shrink-0 items-center justify-center">
                      <ContentIdentityIcon
                        value={space.icon}
                        size={14}
                        fallback={space.name[0]?.toUpperCase()}
                      />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate font-medium">{space.name}</span>
                      {ownerLabel ? (
                        <span className="block truncate text-xs text-text-muted">{ownerLabel}</span>
                      ) : null}
                    </span>
                    {selected ? <Check size={14} className="text-primary shrink-0" /> : null}
                  </button>
                );
              })}
            </div>
          ))
        )}
      </div>
      {canCreateSpace ? (
        <div className="border-border border-t p-1">
          <button
            type="button"
            role="menuitem"
            onClick={onCreate}
            className="rounded-button text-text-muted hover:bg-surface hover:text-text flex min-h-10 w-full items-center gap-2 px-2 text-sm transition-colors duration-100"
          >
            <Plus size={14} />
            New space
          </button>
        </div>
      ) : null}
    </div>
  );
}

interface WorkspaceSpaceCreateDialogProps {
  name: string;
  creating: boolean;
  error: string;
  inputRef: RefObject<HTMLInputElement | null>;
  onNameChange: (name: string) => void;
  onCancel: () => void;
  onSubmit: (event: FormEvent<HTMLFormElement>) => void;
}

export function WorkspaceSpaceCreateDialog({
  name,
  creating,
  error,
  inputRef,
  onNameChange,
  onCancel,
  onSubmit,
}: WorkspaceSpaceCreateDialogProps) {
  return (
    <DialogShell
      labelledBy="workspace-space-create-title"
      describedBy="workspace-space-create-description"
      maxWidth="sm"
      onClose={onCancel}
    >
      <form onSubmit={onSubmit}>
        <DialogHeader>
          <div>
            <DialogTitle id="workspace-space-create-title">Create space</DialogTitle>
            <DialogDescription id="workspace-space-create-description">
              Give the new workspace a clear name. You can configure members and access afterward.
            </DialogDescription>
          </div>
          <DialogCloseAction label="Close create space" onClick={onCancel} disabled={creating} />
        </DialogHeader>
        <DialogBody>
          <label htmlFor="workspace-space-name" className="text-text text-sm font-medium">
            Space name
          </label>
          <Input
            ref={inputRef}
            id="workspace-space-name"
            value={name}
            onChange={(event) => onNameChange(event.target.value)}
            disabled={creating}
            maxLength={120}
            autoComplete="off"
            className="mt-2"
          />
          {error ? (
            <p className="text-danger mt-2 text-sm" role="alert">
              {error}
            </p>
          ) : null}
        </DialogBody>
        <DialogFooter>
          <Button type="button" variant="secondary" onClick={onCancel} disabled={creating}>
            Cancel
          </Button>
          <Button type="submit" disabled={creating || !name.trim()}>
            {creating ? "Creating…" : "Create space"}
          </Button>
        </DialogFooter>
      </form>
    </DialogShell>
  );
}

export function WorkspaceSpaceSelector() {
  const {
    spaces,
    activeSpace,
    recentSpaceIds,
    loading,
    canCreateSpace,
    selectSpace,
    createSpace,
  } = useWorkspaceSpaces();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [createOpen, setCreateOpen] = useState(false);
  const [newSpaceName, setNewSpaceName] = useState("");
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState("");
  const triggerRef = useRef<HTMLButtonElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const createInputRef = useRef<HTMLInputElement>(null);
  const handleMenuNavigation = useMenuKeyboardNavigation(menuRef);
  const sections = getWorkspaceSpaceSections(spaces, recentSpaceIds, query);

  const closeAndRestoreTrigger = () => {
    setOpen(false);
    setQuery("");
    window.requestAnimationFrame(() => triggerRef.current?.focus());
  };

  useEffect(() => {
    if (!createOpen) return;
    const frame = window.requestAnimationFrame(() => createInputRef.current?.focus());
    return () => window.cancelAnimationFrame(frame);
  }, [createOpen]);

  const handleSearchKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    const action = getWorkspaceSpaceSelectorInputAction(event.key);
    if (!action) return;
    event.preventDefault();
    if (action === "close") {
      closeAndRestoreTrigger();
      return;
    }
    const selected = menuRef.current?.querySelector<HTMLButtonElement>(
      'button[role="menuitemradio"][aria-checked="true"]',
    );
    const first = menuRef.current?.querySelector<HTMLButtonElement>(
      'button[role="menuitemradio"]',
    );
    (selected ?? first)?.focus();
  };

  const handleMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeAndRestoreTrigger();
      return;
    }
    handleMenuNavigation(event);
  };

  const openCreateDialog = () => {
    setOpen(false);
    setQuery("");
    setCreateError("");
    setCreateOpen(true);
  };

  const closeCreateDialog = () => {
    if (creating) return;
    setCreateOpen(false);
    setNewSpaceName("");
    setCreateError("");
    window.requestAnimationFrame(() => triggerRef.current?.focus());
  };

  const submitCreateSpace = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    const name = newSpaceName.trim();
    if (!name || creating) return;
    setCreating(true);
    setCreateError("");
    try {
      await createSpace(name);
      setCreateOpen(false);
      setNewSpaceName("");
    } catch {
      setCreateError("Couldn’t create the space. Check your connection and try again.");
    } finally {
      setCreating(false);
    }
  };

  return (
    <>
      <Popover
        open={open}
        onOpenChange={(nextOpen) => {
          setOpen(nextOpen);
          if (!nextOpen) setQuery("");
        }}
      >
        <PopoverTrigger asChild>
          <button
            ref={triggerRef}
            type="button"
            className="rounded-button text-text hover:bg-surface-raised flex h-8 max-w-64 items-center gap-2 px-2 text-sm transition-colors duration-100"
            aria-label={`Select space${activeSpace ? `, current space ${activeSpace.name}` : ""}`}
            aria-haspopup="menu"
            aria-expanded={open}
          >
            <span className="bg-surface-raised rounded-input flex size-6 shrink-0 items-center justify-center">
              {activeSpace ? (
                <ContentIdentityIcon
                  value={activeSpace.icon}
                  size={14}
                  fallback={activeSpace.name[0]?.toUpperCase()}
                />
              ) : (
                <Layers3 size={14} className="text-text-muted" />
              )}
            </span>
            <span className="truncate font-medium">{activeSpace?.name ?? "Select space"}</span>
            <ChevronDown size={14} className="text-text-muted shrink-0" />
          </button>
        </PopoverTrigger>
        <PopoverContent
          align="start"
          sideOffset={6}
          className="w-[min(22rem,calc(100vw-2rem))] overflow-hidden p-0"
          onOpenAutoFocus={(event) => {
            event.preventDefault();
            searchRef.current?.focus();
          }}
        >
          <div className="border-border border-b p-2">
            <SearchInput
              ref={searchRef}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
              onKeyDown={handleSearchKeyDown}
              placeholder="Search spaces"
              aria-label="Search spaces"
              className="h-8 text-sm"
            />
          </div>
          <WorkspaceSpaceSelectorMenu
            sections={sections}
            activeSpaceId={activeSpace?.id}
            loading={loading}
            canCreateSpace={canCreateSpace}
            menuRef={menuRef}
            onKeyDown={handleMenuKeyDown}
            onSelect={(space) => {
              setOpen(false);
              setQuery("");
              selectSpace(space);
            }}
            onCreate={openCreateDialog}
          />
        </PopoverContent>
      </Popover>

      {createOpen ? (
        <WorkspaceSpaceCreateDialog
          name={newSpaceName}
          creating={creating}
          error={createError}
          inputRef={createInputRef}
          onNameChange={setNewSpaceName}
          onCancel={closeCreateDialog}
          onSubmit={(event) => void submitCreateSpace(event)}
        />
      ) : null}
    </>
  );
}
