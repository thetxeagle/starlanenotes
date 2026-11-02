import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
  type TouchEvent,
} from "react";
import { useLocation, useNavigate, useParams } from "react-router-dom";
import {
  getRightPanelTabToggleState,
  isAdminRoute,
  isDocumentPanelRoute,
  RightPanelControlContext,
} from "./AppLayout";
import {
  getWorkspaceSidebarEvent,
  WorkspaceCreateActionContext,
  type WorkspaceSidebarAction,
} from "./workspaceCreateActions";
import { type RightPanelTab } from "./RightPanel";
import { Sidebar } from "./Sidebar";
import { MobileNavigationDrawer } from "./MobileNavigationDrawer";
import { MobileDocumentPanel } from "./MobileDocumentPanel";
import { buildSpaceHomePath, writeLastViewedSpaceId } from "@/lib/lastViewedSpace";
import {
  getSearchModalPresetFromEvent,
  SearchModal,
  type SearchModalPreset,
  type WorkspaceSearchEventDetail,
} from "@/components/SearchModal";
import { OfflineBar } from "@/components/OfflineBar";
import { WorkspaceCommandMenu } from "@/components/WorkspaceCommandMenu";
import { MobileDestinationHeader, MobileWorkspaceHeader } from "./MobileWorkspaceHeader";
import {
  getWorkspaceShortcutAction,
  isWorkspaceOverlayOpen,
  isWorkspaceTextEntryTarget,
  shouldHandleWorkspaceShortcutEvent,
} from "./workspaceShortcuts";
import { dispatchThemeToggle } from "@/lib/themeEvents";
import { WorkspaceSpaceProvider } from "./WorkspaceSpaces";
import {
  DocumentPageDetailsRegistrationContext,
  type DocumentPageDetailsControls,
} from "./documentPageDetails";

interface MobileAppLayoutProps {
  children: ReactNode;
}

export const MOBILE_SEARCH_PATH = "/mobile/search";
export function canUseMobileRightPanel(pathname: string): boolean {
  return isDocumentPanelRoute(pathname);
}

export interface MobileDestination {
  title: string;
  headerLabel: string;
  backTarget: "notes" | "space";
  showUtilities?: boolean;
  showTitle?: boolean;
}

export function getMobileDestination(pathname: string): MobileDestination | null {
  if (isAdminRoute(pathname)) {
    return {
      title: "Site Administration",
      headerLabel: "Site administration top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    };
  }
  if (/^\/(?:mobile\/)?profile\/security\/?$/.test(pathname)) {
    return {
      title: "Account Settings",
      headerLabel: "Account settings top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    };
  }
  if (/^\/(?:mobile\/)?profile\/?$/.test(pathname)) {
    return {
      title: "Account Settings",
      headerLabel: "Account settings top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    };
  }
  if (/^\/mobile\/app-settings\/?$/.test(pathname)) {
    return {
      title: "Settings",
      headerLabel: "Settings top bar",
      backTarget: "notes",
      showUtilities: false,
      showTitle: false,
    };
  }
  if (/^\/spaces\/[^/]+\/settings\/?$/.test(pathname)) {
    return {
      title: "Space settings",
      headerLabel: "Space settings top bar",
      backTarget: "space",
      showUtilities: false,
      showTitle: false,
    };
  }
  return null;
}

export function getMobileDestinationBackHref(
  destination: MobileDestination | null,
  spaceId?: string,
): string {
  return destination?.backTarget === "space" && spaceId ? buildSpaceHomePath(spaceId) : "/";
}

export function getMobileDestinationBackLabel(destination: MobileDestination | null): string {
  return destination?.backTarget === "space" ? "Back to space" : "Back to Notes";
}

export function shouldRenderMobileWorkspaceNavigation(pathname: string): boolean {
  return getMobileDestination(pathname) === null;
}

export function getMobileNavigationToggleState(open: boolean): boolean {
  return !open;
}

export function MobileAppLayout({ children }: MobileAppLayoutProps) {
  const location = useLocation();
  const navigate = useNavigate();
  const { spaceId, docId } = useParams<{ spaceId: string; docId: string }>();
  const [rightPanelOpen, setRightPanelOpen] = useState(false);
  const [rightPanelActiveTab, setRightPanelActiveTab] = useState<RightPanelTab>("properties");
  const [selectedCommentId, setSelectedCommentId] = useState<string | null>(null);
  const [commentPanelScope, setCommentPanelScope] = useState<"all" | "inline">("all");
  const [documentPageDetails, setDocumentPageDetails] =
    useState<DocumentPageDetailsControls | null>(null);
  const registerDocumentPageDetails = useCallback(
    (controls: DocumentPageDetailsControls | null) => setDocumentPageDetails(controls),
    [],
  );
  const [navOpen, setNavOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchPreset, setSearchPreset] = useState<SearchModalPreset | undefined>();
  const [commandMenuOpen, setCommandMenuOpen] = useState(false);
  const [pendingSidebarAction, setPendingSidebarAction] =
    useState<WorkspaceSidebarAction | null>(null);
  const [mobileSidebarOverlayOpen, setMobileSidebarOverlayOpen] = useState(false);
  const canUseRightPanel = canUseMobileRightPanel(location.pathname);
  const destination = getMobileDestination(location.pathname);
  const destinationBackHref = getMobileDestinationBackHref(destination, spaceId);
  const destinationBackLabel = getMobileDestinationBackLabel(destination);
  const showWorkspaceNavigation = shouldRenderMobileWorkspaceNavigation(location.pathname);
  const swipeStartRef = useRef<{ x: number; y: number; edge: "left" | "right" } | null>(null);

  const handleTouchStart = (event: TouchEvent<HTMLElement>) => {
    const target = event.target as HTMLElement;
    if (target.closest("input, textarea, [contenteditable='true'], [data-starlane-disable-edge-swipe]")) {
      swipeStartRef.current = null;
      return;
    }
    const touch = event.touches[0];
    if (!touch) return;
    const edge = touch.clientX <= 28 ? "left" : touch.clientX >= window.innerWidth - 28 ? "right" : null;
    swipeStartRef.current = edge ? { x: touch.clientX, y: touch.clientY, edge } : null;
  };

  const handleTouchEnd = (event: TouchEvent<HTMLElement>) => {
    const start = swipeStartRef.current;
    swipeStartRef.current = null;
    const touch = event.changedTouches[0];
    if (!start || !touch || !showWorkspaceNavigation || navOpen || rightPanelOpen) return;
    const deltaX = touch.clientX - start.x;
    const deltaY = touch.clientY - start.y;
    if (Math.abs(deltaY) > 56 || Math.abs(deltaX) < 72) return;
    if (start.edge === "left" && deltaX > 0) setNavOpen(true);
    if (start.edge === "right" && deltaX < 0 && canUseRightPanel) setRightPanelOpen(true);
  };

  useEffect(() => {
    setRightPanelOpen(false);
    setNavOpen(false);
    setSearchOpen(false);
    setSearchPreset(undefined);
  }, [location.pathname]);

  useEffect(() => {
    if (spaceId) writeLastViewedSpaceId(spaceId);
  }, [spaceId]);

  useEffect(() => {
    if (!canUseRightPanel) setRightPanelOpen(false);
  }, [canUseRightPanel]);

  useEffect(() => {
    if (!navOpen || !pendingSidebarAction) return;
    const action = pendingSidebarAction;
    const frame = window.requestAnimationFrame(() => {
      document.dispatchEvent(new CustomEvent(getWorkspaceSidebarEvent(action)));
      setPendingSidebarAction((pending) => (pending === action ? null : pending));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [navOpen, pendingSidebarAction]);

  useEffect(() => {
    const openSearch = (event: Event) => {
      const detail = (event as CustomEvent<WorkspaceSearchEventDetail>).detail;
      setNavOpen(false);
      setRightPanelOpen(false);
      setSearchPreset(getSearchModalPresetFromEvent(detail));
      setSearchOpen(true);
    };
    document.addEventListener("starlane:open-search", openSearch);
    return () => document.removeEventListener("starlane:open-search", openSearch);
  }, []);

  const requestSidebarCreateAction = (action: WorkspaceSidebarAction) => {
    setPendingSidebarAction(action);
    setRightPanelOpen(false);
    setNavOpen(true);
  };

  const toggleNavigation = () => {
    setNavOpen((open) => {
      const next = getMobileNavigationToggleState(open);
      if (next) setRightPanelOpen(false);
      return next;
    });
  };

  const openCommandMenu = () => {
    setNavOpen(false);
    setRightPanelOpen(false);
    setCommandMenuOpen(true);
  };

  const openSearch = () => {
    setNavOpen(false);
    setRightPanelOpen(false);
    setSearchPreset(undefined);
    setSearchOpen(true);
  };

  const openAppSettings = () => {
    setNavOpen(false);
    setRightPanelOpen(false);
    navigate("/mobile/app-settings");
  };

  useEffect(() => {
    const handleWorkspaceShortcut = (event: KeyboardEvent) => {
      if (!shouldHandleWorkspaceShortcutEvent(event, isWorkspaceOverlayOpen(document))) return;
      if (isWorkspaceTextEntryTarget(event.target as HTMLElement)) return;
      const action = getWorkspaceShortcutAction(event);
      if (!action) return;
      event.preventDefault();

      if (action === "search") {
        setNavOpen(false);
        setRightPanelOpen(false);
        setSearchPreset(undefined);
        setSearchOpen(true);
        return;
      }

      if (action === "toggle-theme") {
        dispatchThemeToggle();
        return;
      }

      if (!showWorkspaceNavigation) return;

      if (action === "command") {
        setNavOpen(false);
        setRightPanelOpen(false);
        setCommandMenuOpen(true);
        return;
      }

      if (action === "new-document") {
        setPendingSidebarAction("document");
        setRightPanelOpen(false);
        setNavOpen(true);
        return;
      }

      if (action === "new-space") {
        setPendingSidebarAction("space");
        setRightPanelOpen(false);
        setNavOpen(true);
        return;
      }

      setNavOpen((open) => {
        const next = getMobileNavigationToggleState(open);
        if (next) setRightPanelOpen(false);
        return next;
      });
    };

    document.addEventListener("keydown", handleWorkspaceShortcut);
    return () => document.removeEventListener("keydown", handleWorkspaceShortcut);
  }, [showWorkspaceNavigation]);

  return (
    <WorkspaceSpaceProvider>
    <WorkspaceCreateActionContext.Provider value={requestSidebarCreateAction}>
    <div
      className={`starlane-mobile-app app-viewport app-background text-text flex flex-col ${
        navOpen ? "mobile-nav-open" : ""
      }`}
      style={
        {
          "--starlane-mobile-tabbar-height": "0px",
        } as CSSProperties
      }
    >
      <a
        href="#main-content"
        className="rounded-button bg-primary sr-only z-50 px-3 py-2 text-sm text-white focus:not-sr-only focus:fixed focus:left-3 focus:top-3"
      >
        Skip to main content
      </a>

      {showWorkspaceNavigation ? (
        <MobileWorkspaceHeader
          navigationOpen={navOpen}
          onToggleNavigation={toggleNavigation}
          containerClassName="pointer-events-none fixed left-0 right-0 top-0 z-20 border-b border-border bg-bg px-4 pb-3 pt-[max(2.25rem,calc(env(safe-area-inset-top)+1.75rem))]"
          controlsClassName="pointer-events-auto flex h-11 items-center gap-2"
        />
      ) : (
        <MobileDestinationHeader
          title={destination?.title ?? "Starlane Notes"}
          ariaLabel={destination?.headerLabel ?? "Destination top bar"}
          backHref={destinationBackHref}
          backLabel={destinationBackLabel}
          onOpenSearch={openSearch}
          showUtilities={destination?.showUtilities}
          showTitle={destination?.showTitle}
          onBack={() => {
            if (window.history.length > 1) window.history.back();
            else window.location.assign("/");
          }}
          containerClassName="pointer-events-none fixed left-0 right-0 top-0 z-20 border-b border-border bg-bg px-4 pb-3 pt-[max(2.25rem,calc(env(safe-area-inset-top)+1.75rem))]"
          controlsClassName="pointer-events-auto flex h-11 items-center gap-2"
        />
      )}

      <main
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto pb-[max(1rem,env(safe-area-inset-bottom))] pt-[max(7.25rem,calc(env(safe-area-inset-top)+5rem))]"
        onTouchStart={handleTouchStart}
        onTouchEnd={handleTouchEnd}
      >
        <RightPanelControlContext.Provider
          value={{
            open: canUseRightPanel && rightPanelOpen,
            activeTab: rightPanelActiveTab,
            selectedCommentId,
            commentPanelScope,
            toggle: () => {
              if (!canUseRightPanel) {
                setRightPanelOpen(false);
                return;
              }
              setRightPanelOpen((prev) => !prev);
            },
            close: () => {
              setRightPanelOpen(false);
              setSelectedCommentId(null);
            },
            toggleTab: (tab) => {
              if (!canUseRightPanel) {
                setRightPanelOpen(false);
                return;
              }
              const next = getRightPanelTabToggleState(
                rightPanelOpen,
                rightPanelActiveTab,
                tab,
              );
              setRightPanelActiveTab(next.activeTab);
              if (tab === "comments") setCommentPanelScope("all");
              if (tab !== "comments") setSelectedCommentId(null);
              setRightPanelOpen(next.open);
              if (next.open) setNavOpen(false);
            },
            openTab: (tab) => {
              if (!canUseRightPanel) {
                setRightPanelOpen(false);
                return;
              }
              setRightPanelActiveTab(tab);
              if (tab === "comments") setCommentPanelScope("all");
              if (tab !== "comments") setSelectedCommentId(null);
              setRightPanelOpen(true);
              setNavOpen(false);
            },
            openComment: (commentId) => {
              if (!canUseRightPanel) {
                setRightPanelOpen(false);
                return;
              }
              setSelectedCommentId(commentId ?? null);
              setCommentPanelScope("inline");
              setRightPanelActiveTab("comments");
              setRightPanelOpen(true);
              setNavOpen(false);
            },
          }}
        >
          <DocumentPageDetailsRegistrationContext.Provider value={registerDocumentPageDetails}>
            {children}
          </DocumentPageDetailsRegistrationContext.Provider>
        </RightPanelControlContext.Provider>
      </main>

      <OfflineBar />

      {showWorkspaceNavigation && (
        <MobileNavigationDrawer
          open={navOpen}
          suspended={mobileSidebarOverlayOpen}
          onClose={() => setNavOpen(false)}
        >
          <Sidebar
            showHeader={false}
            mobileSettingsMenu
            onOpenCommandMenu={openCommandMenu}
            onOpenAppSettings={openAppSettings}
            onMobileOverlayChange={setMobileSidebarOverlayOpen}
          />
        </MobileNavigationDrawer>
      )}

      <SearchModal
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        preset={searchPreset}
        presentation="fullscreen"
      />
      {showWorkspaceNavigation && (
        <WorkspaceCommandMenu
          open={commandMenuOpen}
          onClose={() => setCommandMenuOpen(false)}
          spaceId={spaceId}
        />
      )}

      {canUseRightPanel && (
        <MobileDocumentPanel
          open={rightPanelOpen}
          onClose={() => {
            setRightPanelOpen(false);
            setSelectedCommentId(null);
          }}
          spaceId={spaceId}
          docId={docId}
          activeTab={rightPanelActiveTab}
          selectedCommentId={selectedCommentId}
          commentPanelScope={commentPanelScope}
          onTabChange={setRightPanelActiveTab}
          pageDetailsControls={documentPageDetails}
        />
      )}
    </div>
    </WorkspaceCreateActionContext.Provider>
    </WorkspaceSpaceProvider>
  );
}
