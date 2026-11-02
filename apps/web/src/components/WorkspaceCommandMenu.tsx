import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
} from "react";
import { useNavigate } from "react-router-dom";
import {
  CalendarDays,
  Clock3,
  Command,
  Files,
  FileText,
  FolderPlus,
  Home,
  Search,
  Star,
  Workflow,
  X,
} from "lucide-react";
import { DialogShell } from "@/components/ui/dialog-shell";
import { SearchInput } from "@/components/ui/search-input";
import { useWorkspaceCreateAction } from "@/components/layout/workspaceCreateActions";
import { dataClient, type TemplateRecord } from "@/lib/dataClient";

interface CommandAction {
  id: string;
  group: string;
  label: string;
  description: string;
  shortcut?: string;
  icon: ReactNode;
  run: () => void;
}

export const WORKSPACE_COMMAND_GROUP_HEADING_CLASS =
  "text-text-muted px-2 pb-1 pt-2 text-xs font-medium";

interface WorkspaceCommandMenuProps {
  open: boolean;
  onClose: () => void;
  spaceId?: string;
  includeCreateActions?: boolean;
}

export function getNextWorkspaceCommandIndex(
  currentIndex: number,
  actionCount: number,
  key: "ArrowDown" | "ArrowUp" | "Home" | "End",
): number {
  if (actionCount <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return actionCount - 1;
  if (key === "ArrowUp") return currentIndex <= 0 ? actionCount - 1 : currentIndex - 1;
  return currentIndex >= actionCount - 1 ? 0 : currentIndex + 1;
}

export function getWorkspaceCommandShortcutLabel(
  action: "search" | "new-document" | "new-space",
  isMac = typeof navigator === "undefined" || navigator.platform.includes("Mac"),
): string {
  const modifier = isMac ? "⌘" : "Ctrl+";
  if (action === "search") return `${modifier}K`;
  if (action === "new-document") return `${modifier}N`;
  return isMac ? "⌘⇧N" : "Ctrl+Shift+N";
}

export function WorkspaceCommandMenu({
  open,
  onClose,
  spaceId,
  includeCreateActions = true,
}: WorkspaceCommandMenuProps) {
  const navigate = useNavigate();
  const requestWorkspaceCreateAction = useWorkspaceCreateAction();
  const [query, setQuery] = useState("");
  const [templates, setTemplates] = useState<TemplateRecord[]>([]);
  const [activeCommandIndex, setActiveCommandIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const commandRefs = useRef<Array<HTMLButtonElement | null>>([]);

  useEffect(() => {
    if (!open) setQuery("");
  }, [open]);

  useEffect(() => {
    if (!open || !spaceId || !includeCreateActions) {
      setTemplates([]);
      return;
    }
    let current = true;
    void dataClient
      .listTemplates(spaceId)
      .then((items) => {
        if (current) setTemplates(items);
      })
      .catch(() => {
        if (current) setTemplates([]);
      });
    return () => {
      current = false;
    };
  }, [includeCreateActions, open, spaceId]);

  const actions = useMemo<CommandAction[]>(
    () => [
      {
        id: "search",
        group: "Workspace",
        label: "Search documents",
        description: "Search SNQL across this workspace",
        shortcut: getWorkspaceCommandShortcutLabel("search"),
        icon: <Search size={16} />,
        run: () => {
          onClose();
          document.dispatchEvent(
            new CustomEvent("starlane:open-search", { detail: { spaceId } }),
          );
        },
      },
      ...(includeCreateActions
        ? [
            {
              id: "new-document",
              group: "Create",
              label: "New page",
              description: "Choose a template or start blank",
              shortcut: getWorkspaceCommandShortcutLabel("new-document"),
              icon: <FileText size={16} />,
              run: () => {
                onClose();
                requestWorkspaceCreateAction("document");
              },
            },
            ...(templates.length > 0
              ? [
                  {
                    id: "new-document-from-template",
                    group: "Create",
                    label: "New page from template",
                    description: `Choose from ${templates.length} saved template${templates.length === 1 ? "" : "s"}`,
                    icon: <Files size={16} />,
                    run: () => {
                      onClose();
                      requestWorkspaceCreateAction("document-template");
                    },
                  },
                ]
              : []),
            {
              id: "new-folder",
              group: "Create",
              label: "New folder",
              description: "Create a top-level folder",
              icon: <FolderPlus size={16} />,
              run: () => {
                onClose();
                requestWorkspaceCreateAction("folder");
              },
            },
            {
              id: "new-space",
              group: "Create",
              label: "New space",
              description: "Create a separate workspace",
              shortcut: getWorkspaceCommandShortcutLabel("new-space"),
              icon: <FolderPlus size={16} />,
              run: () => {
                onClose();
                requestWorkspaceCreateAction("space");
              },
            },
          ]
        : []),
      {
        id: "home",
        group: "Navigate",
        label: "Open home",
        description: "Go to the current space home",
        icon: <Home size={16} />,
        run: () => {
          onClose();
          navigate(spaceId ? `/spaces/${encodeURIComponent(spaceId)}` : "/");
        },
      },
      {
        id: "recent",
        group: "Navigate",
        label: "Open recent",
        description: "Review recently changed pages",
        icon: <Clock3 size={16} />,
        run: () => {
          onClose();
          navigate(spaceId ? `/spaces/${encodeURIComponent(spaceId)}/recent` : "/");
        },
      },
      {
        id: "starred",
        group: "Navigate",
        label: "Open favorites",
        description: "Review starred pages in this workspace",
        icon: <Star size={16} />,
        run: () => {
          onClose();
          navigate(spaceId ? `/spaces/${encodeURIComponent(spaceId)}/starred` : "/");
        },
      },
      {
        id: "journal",
        group: "Navigate",
        label: "Open journal",
        description: "Browse daily notes",
        icon: <CalendarDays size={16} />,
        run: () => {
          onClose();
          navigate(spaceId ? `/spaces/${encodeURIComponent(spaceId)}/journal` : "/");
        },
      },
      {
        id: "daily-note",
        group: "Navigate",
        label: "Open daily note",
        description: "Open or create today's daily note",
        icon: <CalendarDays size={16} />,
        run: () => {
          onClose();
          requestWorkspaceCreateAction("daily-note");
        },
      },
      {
        id: "workflows",
        group: "Navigate",
        label: "Open workflows",
        description: "Review workspace workflows",
        icon: <Workflow size={16} />,
        run: () => {
          onClose();
          navigate(spaceId ? `/spaces/${encodeURIComponent(spaceId)}/workflows` : "/");
        },
      },
    ],
    [
      includeCreateActions,
      navigate,
      onClose,
      requestWorkspaceCreateAction,
      spaceId,
      templates.length,
    ],
  );

  const normalizedQuery = query.trim().toLocaleLowerCase();
  const filteredActions = actions.filter((action) =>
    !normalizedQuery || `${action.label} ${action.description}`.toLocaleLowerCase().includes(normalizedQuery),
  );
  const groups = ["Workspace", "Create", "Navigate"]
    .map((group) => ({
      group,
      actions: filteredActions.filter((action) => action.group === group),
    }))
    .filter(({ actions: groupActions }) => groupActions.length > 0);

  useEffect(() => {
    setActiveCommandIndex(0);
  }, [normalizedQuery, open]);

  const moveActiveCommand = (key: "ArrowDown" | "ArrowUp" | "Home" | "End") => {
    const nextIndex = getNextWorkspaceCommandIndex(activeCommandIndex, filteredActions.length, key);
    if (nextIndex < 0) return;
    setActiveCommandIndex(nextIndex);
    window.requestAnimationFrame(() => commandRefs.current[nextIndex]?.focus());
  };

  const handleCommandKeyDown = (event: ReactKeyboardEvent<HTMLElement>) => {
    if (event.key === "ArrowDown" || event.key === "ArrowUp" || event.key === "Home" || event.key === "End") {
      event.preventDefault();
      moveActiveCommand(event.key);
      return;
    }

    if (event.key === "Enter") {
      const targetIndex =
        event.currentTarget === inputRef.current
          ? activeCommandIndex
          : Number(event.currentTarget.dataset.commandIndex);
      const action = filteredActions[targetIndex];
      if (!action) return;
      event.preventDefault();
      action.run();
    }
  };

  if (!open) return null;

  return (
    <DialogShell
      labelledBy="workspace-command-menu-title"
      maxWidth="md"
      onClose={onClose}
    >
      <div className="border-border flex flex-col gap-2 border-b px-3 py-3">
        <SearchInput
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={handleCommandKeyDown}
          placeholder="Search commands"
          aria-label="Search workspace commands"
          className="order-2 h-9 text-sm"
        />
        <div className="order-1 flex items-center justify-between gap-3">
          <h2
            id="workspace-command-menu-title"
            className="text-text flex min-w-0 items-center gap-2 text-sm font-medium"
          >
            <Command size={16} className="text-text-muted shrink-0" aria-hidden="true" />
            Workspace commands
          </h2>
          <button
            type="button"
            onClick={onClose}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-11 w-11 shrink-0 items-center justify-center transition-colors duration-100 lg:h-7 lg:w-7"
            aria-label="Close command menu"
          >
            <X size={15} />
          </button>
        </div>
      </div>
      <div className="max-h-[min(28rem,calc(100dvh-10rem))] overflow-y-auto p-2">
        {groups.map(({ group, actions: groupActions }) => (
          <section key={group} className="mb-3 last:mb-0" aria-label={group}>
            <h3 className={WORKSPACE_COMMAND_GROUP_HEADING_CLASS}>
              {group}
            </h3>
            <div className="space-y-0.5" role="menu" aria-label={`${group} commands`}>
              {groupActions.map((action) => (
                <button
                  key={action.id}
                  type="button"
                  onClick={action.run}
                  onKeyDown={handleCommandKeyDown}
                  ref={(element) => {
                    commandRefs.current[filteredActions.indexOf(action)] = element;
                  }}
                  data-command-index={filteredActions.indexOf(action)}
                  role="menuitem"
                  tabIndex={filteredActions.indexOf(action) === activeCommandIndex ? 0 : -1}
                  className={`rounded-button flex w-full items-center gap-3 px-2 py-2 text-left transition-colors duration-100 ${
                    filteredActions.indexOf(action) === activeCommandIndex
                      ? "bg-surface-raised text-text"
                      : "hover:bg-surface-raised"
                  }`}
                >
                  <span className="border-border bg-bg text-text-muted flex h-8 w-8 shrink-0 items-center justify-center rounded border">
                    {action.icon}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-sm font-medium text-text">{action.label}</span>
                    <span className="block truncate text-xs text-text-muted">{action.description}</span>
                  </span>
                  {action.shortcut && (
                    <kbd className="border-border text-text-muted rounded border px-1.5 py-0.5 font-mono text-[10px]">
                      {action.shortcut}
                    </kbd>
                  )}
                </button>
              ))}
            </div>
          </section>
        ))}
        {groups.length === 0 && (
          <p className="text-text-muted px-2 py-6 text-center text-sm">No matching commands.</p>
        )}
      </div>
    </DialogShell>
  );
}
