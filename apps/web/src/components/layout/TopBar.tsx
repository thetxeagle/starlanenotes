import { canUseHumanAdministration } from "@/lib/guestClassification";
import { useEffect, useState } from "react";
import { Menu, ArrowLeft, PanelLeft, PanelRight, Settings } from "lucide-react";
import { Link } from "react-router-dom";
import { useAuth } from "@/hooks/useAuth";
import { NotificationBell } from "@/components/NotificationBell";
import { SyncStatus } from "@/components/SyncStatus";
import {
  getSearchModalPresetFromEvent,
  SearchModal,
  type SearchModalPreset,
  type WorkspaceSearchEventDetail,
} from "@/components/SearchModal";
import { getCurrentLaunchMode } from "@/lib/launchMode";
import { MobileWorkspaceHeader } from "./MobileWorkspaceHeader";
import { WorkspaceProfileMenu } from "./WorkspaceProfileMenu";
import { AboutButton } from "./AboutButton";
import { AppearanceToggle } from "./AppearanceToggle";
import { isWorkspaceOverlayOpen, shouldHandleWorkspaceShortcutEvent } from "./workspaceShortcuts";

export function shouldOpenTopBarSearchShortcut(
  event: Pick<KeyboardEvent, "ctrlKey" | "defaultPrevented" | "key" | "metaKey">,
  overlayOpen: boolean,
): boolean {
  return (
    shouldHandleWorkspaceShortcutEvent(event, overlayOpen) &&
    (event.metaKey || event.ctrlKey) &&
    event.key.toLowerCase() === "k"
  );
}

export interface TopBarDestination {
  title: string;
  backHref: string;
  backLabel: string;
}

interface TopBarProps {
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  rightPanelOpen: boolean;
  onToggleRightPanel: () => void;
  showSidebarToggle?: boolean;
  showRightPanelToggle?: boolean;
  mobile?: boolean;
  destination?: TopBarDestination;
  onActionsSlotChange?: (node: HTMLDivElement | null) => void;
}

export function TopBar({
  sidebarOpen,
  onToggleSidebar,
  rightPanelOpen,
  onToggleRightPanel,
  showSidebarToggle = true,
  showRightPanelToggle = true,
  mobile = false,
  destination,
  onActionsSlotChange,
}: TopBarProps) {
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchPreset, setSearchPreset] = useState<SearchModalPreset | undefined>();
  const localMode = getCurrentLaunchMode() === "local";
  const { user } = useAuth();

  // Global Cmd+K shortcut
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!shouldOpenTopBarSearchShortcut(e, isWorkspaceOverlayOpen(document))) return;
      e.preventDefault();
      setSearchPreset(undefined);
      setSearchOpen(true);
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  useEffect(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<WorkspaceSearchEventDetail>).detail;
      setSearchPreset(getSearchModalPresetFromEvent(detail));
      setSearchOpen(true);
    };
    document.addEventListener("starlane:open-search", handler);
    return () => document.removeEventListener("starlane:open-search", handler);
  }, []);

  if (mobile) {
    return (
      <>
        <MobileWorkspaceHeader
          navigationOpen={sidebarOpen}
          onToggleNavigation={showSidebarToggle ? onToggleSidebar : undefined}
          containerClassName="border-border bg-bg border-b px-4 py-3"
        />
        <SearchModal
          open={searchOpen}
          onClose={() => setSearchOpen(false)}
          preset={searchPreset}
          presentation="fullscreen"
        />
      </>
    );
  }

  return (
    <>
      <header className="border-border bg-surface grid h-10 shrink-0 grid-cols-[minmax(0,1fr)_minmax(0,auto)] items-center gap-3 border-b px-3">
        <div className="flex min-w-0 items-center gap-2">
          {destination ? (
            <>
              <a
                href={destination.backHref}
                className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 shrink-0 items-center justify-center transition-colors duration-100"
                aria-label={destination.backLabel}
                title={destination.backLabel}
              >
                <ArrowLeft size={16} />
              </a>
              <span className="text-text truncate text-sm font-medium">{destination.title}</span>
            </>
          ) : (
            <>
              {showSidebarToggle ? (
                <button
                  type="button"
                  onClick={onToggleSidebar}
                  className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 shrink-0 items-center justify-center transition-colors duration-100"
                  aria-label={sidebarOpen ? "Collapse sidebar" : "Expand sidebar"}
                  aria-expanded={sidebarOpen}
                  title={sidebarOpen ? "Collapse sidebar" : "Expand sidebar"}
                >
                  {sidebarOpen ? <PanelLeft size={16} /> : <Menu size={16} />}
                </button>
              ) : null}
              <span className="text-text-muted text-xs">Workspace</span>
            </>
          )}
        </div>

        {/* Right section */}
        <div className="flex min-w-0 items-center justify-end gap-1">
          <div
            ref={onActionsSlotChange}
            data-workspace-top-bar-slot="actions"
            className="flex max-w-full min-w-0 scrollbar-none items-center overflow-x-auto"
          />
          {!localMode && (
            <div className="hidden sm:block">
              <SyncStatus hideSynced />
            </div>
          )}

          {showRightPanelToggle && (
            <button
              type="button"
              onClick={onToggleRightPanel}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
              aria-label={rightPanelOpen ? "Close properties panel" : "Open properties panel"}
              aria-expanded={rightPanelOpen}
            >
              <PanelRight size={16} />
            </button>
          )}
          <AboutButton />
          <AppearanceToggle />
          {!localMode && <NotificationBell />}
          {!localMode && canUseHumanAdministration(user?.role, user?.account_kind) && (
            <Link
              to="/admin"
              aria-label="Admin settings"
              title="Admin settings"
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 shrink-0 items-center justify-center transition-colors duration-100"
            >
              <Settings size={16} aria-hidden="true" />
            </Link>
          )}
          <WorkspaceProfileMenu />
        </div>
      </header>
      <SearchModal
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        preset={searchPreset}
        presentation="dialog"
      />
    </>
  );
}
