import { createContext, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { capturePageMetadataLoadGuard } from "@/lib/pageMetadataLoad";
import { EditorZoomContext } from "@/components/editor/EditorZoomContext";
import { useLocation, useParams } from "react-router-dom";
import {
  OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT,
  OPEN_SIDEBAR_CREATE_SPACE_EVENT,
  OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT,
  Sidebar,
} from "./Sidebar";
import { MobileNavigationDrawer } from "./MobileNavigationDrawer";
import { MobileDocumentPanel } from "./MobileDocumentPanel";
import { TopBar, type TopBarDestination } from "./TopBar";
import { RightPanel, type RightPanelTab } from "./RightPanel";
import { KeyboardShortcuts } from "@/components/KeyboardShortcuts";
import { WorkspaceCommandMenu } from "@/components/WorkspaceCommandMenu";
import { OfflineBar } from "@/components/OfflineBar";
import { UpdateBanner } from "@/components/UpdateBanner";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { writeLastViewedSpaceId } from "@/lib/lastViewedSpace";
import { dispatchThemeToggle } from "@/lib/themeEvents";
import {
  getWorkspaceSidebarEvent,
  WorkspaceCreateActionContext,
  type WorkspaceSidebarAction,
} from "./workspaceCreateActions";
import {
  getWorkspaceShortcutAction,
  isWorkspaceOverlayOpen,
  isWorkspaceTextEntryTarget,
  shouldHandleWorkspaceShortcutEvent,
} from "./workspaceShortcuts";
import { WorkspaceSpaceProvider } from "./WorkspaceSpaces";
import { WorkspaceTopBarSlotContext } from "./workspaceTopBarSlots";
import {
  DocumentPageDetailsRegistrationContext,
  type DocumentPageDetailsControls,
} from "./documentPageDetails";

export { EditorZoomContext };
export const AdminSidebarContext = createContext<{ open: boolean }>({ open: true });
export const RightPanelControlContext = createContext<{
  open: boolean;
  activeTab: RightPanelTab;
  selectedCommentId: string | null;
  commentPanelScope: "all" | "inline";
  toggle: () => void;
  close: () => void;
  toggleTab: (tab: RightPanelTab) => void;
  openTab: (tab: RightPanelTab) => void;
  openComment: (commentId?: string) => void;
}>({
  open: false,
  activeTab: "properties",
  selectedCommentId: null,
  commentPanelScope: "all",
  toggle: () => undefined,
  close: () => undefined,
  toggleTab: () => undefined,
  openTab: () => undefined,
  openComment: () => undefined,
});
export { getWorkspaceSidebarEvent as getMobileSidebarCreateEvent } from "./workspaceCreateActions";
export type { WorkspaceSidebarAction as MobileSidebarCreateAction } from "./workspaceCreateActions";

interface AppLayoutProps {
  children: ReactNode;
}

export function isDocumentPanelRoute(pathname: string): boolean {
  return /^\/spaces\/[^/]+\/docs\/[^/]+\/?$/.test(pathname);
}

export function getRightPanelTabToggleState(
  open: boolean,
  activeTab: RightPanelTab,
  targetTab: RightPanelTab,
): { open: boolean; activeTab: RightPanelTab } {
  return {
    open: !(open && activeTab === targetTab),
    activeTab: targetTab,
  };
}

export function getRightPanelAriaLabel(activeTab: RightPanelTab): string {
  if (activeTab === "comments") return "Comments panel";
  if (activeTab === "properties") return "Properties panel";
  return "Document panel";
}

export function shouldCloseRightPanelFromPointer(target: EventTarget | null): boolean {
  if (!target || typeof target !== "object" || !("closest" in target)) return false;
  const closest = (target as { closest: (selector: string) => unknown }).closest;
  if (typeof closest !== "function") return false;
  return (
    Boolean(closest.call(target, "[data-document-editor-surface]")) &&
    !Boolean(closest.call(target, "[data-right-panel-trigger]"))
  );
}

export function isAdminRoute(pathname: string): boolean {
  return /^\/admin\/?$/.test(pathname);
}

export function getWorkspaceDestination(
  pathname: string,
  _spaceId?: string,
): TopBarDestination | null {
  if (isAdminRoute(pathname)) {
    return { title: "Site Administration", backHref: "/", backLabel: "Back to Notes" };
  }
  if (/^\/profile\/security\/?$/.test(pathname)) {
    return { title: "Account Settings", backHref: "/", backLabel: "Back to Notes" };
  }
  if (/^\/profile\/?$/.test(pathname)) {
    return { title: "Account Settings", backHref: "/", backLabel: "Back to Notes" };
  }
  return null;
}

export function isWorkspaceDestinationRoute(pathname: string): boolean {
  return getWorkspaceDestination(pathname) !== null;
}

export const SIDEBAR_COMPACT_STORAGE_KEY = "starlane_sidebar_compact";
export function getDesktopSidebarToggleLabel(collapsed: boolean): string {
  return collapsed ? "Expand sidebar" : "Collapse sidebar";
}

export function readSidebarCompactPreference(): boolean {
  try {
    return (
      typeof window !== "undefined" &&
      window.localStorage.getItem(SIDEBAR_COMPACT_STORAGE_KEY) === "true"
    );
  } catch {
    return false;
  }
}

export function writeSidebarCompactPreference(compact: boolean): void {
  try {
    window.localStorage.setItem(SIDEBAR_COMPACT_STORAGE_KEY, String(compact));
  } catch {
    // A compact rail is a convenience; native WebView storage must not block navigation.
  }
}

interface MobileMoveTarget { mobile: boolean; open: boolean; pathname: string; spaceId?: string; docId?: string; }
/** One original page action, delivered only after its sidebar has mounted. */
export function prepareMobileSidebarMoveHandoff(
  documentId: string,
  current: () => MobileMoveTarget,
  verify: () => void,
  dispatch: (documentId: string) => void,
) {
  const original = { ...current() };
  if (!documentId || !original.mobile || original.open || !original.spaceId || original.docId !== documentId || !isDocumentPanelRoute(original.pathname)) return null;
  verify();
  let consumed = false;
  return { deliver() {
    if (consumed) return false;
    consumed = true;
    const latest = current();
    if (!latest.mobile || !latest.open || latest.pathname !== original.pathname || latest.spaceId !== original.spaceId || latest.docId !== original.docId) return false;
    try { verify(); } catch { return false; }
    dispatch(documentId);
    return true;
  } };
}

export function AppLayout({ children }: AppLayoutProps) {
  const [topBarActionsSlot, setTopBarActionsSlot] = useState<HTMLDivElement | null>(null);
  const [documentPageDetails, setDocumentPageDetails] =
    useState<DocumentPageDetailsControls | null>(null);
  const registerDocumentPageDetails = useCallback(
    (controls: DocumentPageDetailsControls | null) => {
      setDocumentPageDetails(controls);
    },
    [],
  );
  const [isMobile, setIsMobile] = useState(() =>
    typeof window !== "undefined" ? window.matchMedia("(max-width: 1023px)").matches : false,
  );
  const [sidebarOpen, setSidebarOpen] = useState(() =>
    typeof window !== "undefined" ? !window.matchMedia("(max-width: 1023px)").matches : true,
  );
  const [sidebarCollapsed, setSidebarCollapsed] = useState(readSidebarCompactPreference);
  const [rightPanelOpen, setRightPanelOpen] = useState(false);
  const [rightPanelActiveTab, setRightPanelActiveTab] = useState<RightPanelTab>("properties");
  const [selectedCommentId, setSelectedCommentId] = useState<string | null>(null);
  const [commentPanelScope, setCommentPanelScope] = useState<"all" | "inline">("all");
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const [commandMenuOpen, setCommandMenuOpen] = useState(false);
  const [pendingMobileSidebarAction, setPendingMobileSidebarAction] =
    useState<WorkspaceSidebarAction | null>(null);
  const [mobileSidebarOverlayOpen, setMobileSidebarOverlayOpen] = useState(false);
  const location = useLocation();
  const { spaceId, docId } = useParams<{ spaceId: string; docId: string }>();
  const [pendingMobileMove, setPendingMobileMove] = useState<ReturnType<typeof prepareMobileSidebarMoveHandoff>>(null);
  const mobileMoveTarget = useRef<MobileMoveTarget>({ mobile: isMobile, open: sidebarOpen, pathname: location.pathname, spaceId, docId });
  mobileMoveTarget.current = { mobile: isMobile, open: sidebarOpen, pathname: location.pathname, spaceId, docId };
  const { zoom, cssVar } = useEditorZoom();
  const canUseRightPanel = isDocumentPanelRoute(location.pathname);
  const isAdmin = isAdminRoute(location.pathname);
  const destination = getWorkspaceDestination(location.pathname, spaceId);
  const isWorkspaceDestination = destination !== null;

  // Track viewport and switch layout behavior for mobile drawers
  useEffect(() => {
    const media = window.matchMedia("(max-width: 1023px)");
    const onChange = (e: MediaQueryListEvent) => {
      setIsMobile(e.matches);
      if (!e.matches) {
        setSidebarOpen(true);
      } else {
        setSidebarOpen(false);
        setRightPanelOpen(false);
      }
    };

    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, []);

  // Close mobile drawers on route change
  useEffect(() => {
    if (!isMobile) return;
    setSidebarOpen(false);
    setPendingMobileMove(null);
    setRightPanelOpen(false);
  }, [location.pathname, isMobile]);

  useEffect(() => {
    if (!canUseRightPanel) setRightPanelOpen(false);
  }, [canUseRightPanel]);

  useEffect(() => {
    if (!isMobile || !sidebarOpen || !pendingMobileSidebarAction) return;
    const action = pendingMobileSidebarAction;
    const frame = window.requestAnimationFrame(() => {
      document.dispatchEvent(new CustomEvent(getWorkspaceSidebarEvent(action)));
      setPendingMobileSidebarAction((pending) => (pending === action ? null : pending));
    });
    return () => window.cancelAnimationFrame(frame);
  }, [isMobile, pendingMobileSidebarAction, sidebarOpen]);

  useEffect(() => {
    const handler = (event: Event) => {
      const documentId = (event as CustomEvent<{ documentId?: string }>).detail?.documentId;
      if (typeof documentId !== "string" || !mobileMoveTarget.current.mobile || mobileMoveTarget.current.open) return;
      try {
        const pending = prepareMobileSidebarMoveHandoff(documentId, () => mobileMoveTarget.current, capturePageMetadataLoadGuard(), id => {
          document.dispatchEvent(new CustomEvent(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, { detail: { documentId: id } }));
        });
        if (!pending) return; // Desktop and already-mounted sidebar consume the original event.
        event.stopImmediatePropagation();
        setPendingMobileMove(pending);
        setSidebarOpen(true);
      } catch { /* A foreign original action must not wake this drawer. */ }
    };
    document.addEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, handler);
    return () => document.removeEventListener(OPEN_SIDEBAR_MOVE_DOCUMENT_EVENT, handler);
  }, []);

  useEffect(() => {
    if (!isMobile || !sidebarOpen || !pendingMobileMove) return;
    const pending = pendingMobileMove;
    const frame = window.requestAnimationFrame(() => {
      pending.deliver();
      setPendingMobileMove(current => current === pending ? null : current);
    });
    return () => window.cancelAnimationFrame(frame);
  }, [isMobile, sidebarOpen, pendingMobileMove]);

  useEffect(() => {
    if (spaceId) writeLastViewedSpaceId(spaceId);
  }, [spaceId]);

  // Keyboard shortcut: ? for shortcuts modal
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (!shouldHandleWorkspaceShortcutEvent(e, isWorkspaceOverlayOpen(document))) return;
      const target = e.target as HTMLElement;
      if (isWorkspaceTextEntryTarget(target)) return;

      if (!isWorkspaceDestination && e.key === "?" && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        setShortcutsOpen((prev) => !prev);
      }

      const workspaceShortcut = getWorkspaceShortcutAction(e);

      if (workspaceShortcut === "toggle-theme") {
        e.preventDefault();
        dispatchThemeToggle();
        return;
      }

      if (isWorkspaceDestination) return;

      if (workspaceShortcut === "command") {
        e.preventDefault();
        setCommandMenuOpen((prev) => !prev);
      }

      if (workspaceShortcut === "new-document") {
        e.preventDefault();
        if (isMobile) {
          setPendingMobileSidebarAction("document");
          setRightPanelOpen(false);
          setSidebarOpen(true);
        } else {
          document.dispatchEvent(new CustomEvent(OPEN_SIDEBAR_CREATE_DOCUMENT_EVENT));
        }
      }

      if (workspaceShortcut === "new-space") {
        e.preventDefault();
        if (isMobile) {
          setPendingMobileSidebarAction("space");
          setRightPanelOpen(false);
          setSidebarOpen(true);
        } else {
          document.dispatchEvent(new CustomEvent(OPEN_SIDEBAR_CREATE_SPACE_EVENT));
        }
      }

      // Cmd+\ to toggle sidebar
      if ((e.metaKey || e.ctrlKey) && e.key === "\\") {
        e.preventDefault();
        if (isMobile) {
          setSidebarOpen((prev) => !prev);
        } else {
          setSidebarCollapsed((prev) => {
            const next = !prev;
            writeSidebarCompactPreference(next);
            return next;
          });
        }
      }

      // Cmd+Shift+P to toggle right panel
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key === "P") {
        e.preventDefault();
        if (!canUseRightPanel) {
          setRightPanelOpen(false);
          return;
        }
        setRightPanelOpen((prev) => !prev);
      }

      if (e.key === "Escape" && isMobile) {
        setSidebarOpen(false);
        setRightPanelOpen(false);
      }
    };

    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [canUseRightPanel, isMobile, isWorkspaceDestination]);

  const toggleSidebar = () => {
    if (isMobile) {
      setSidebarOpen((prev) => {
        const next = !prev;
        if (next) setRightPanelOpen(false);
        return next;
      });
      return;
    }
    setSidebarCollapsed((prev) => {
      const next = !prev;
      writeSidebarCompactPreference(next);
      return next;
    });
  };

  const requestMobileSidebarCreateAction = (action: WorkspaceSidebarAction) => {
    setPendingMobileSidebarAction(action);
    setRightPanelOpen(false);
    setSidebarOpen(true);
  };

  const requestWorkspaceCreateAction = (action: WorkspaceSidebarAction) => {
    if (isMobile) {
      requestMobileSidebarCreateAction(action);
      return;
    }
    document.dispatchEvent(new CustomEvent(getWorkspaceSidebarEvent(action)));
  };

  const openRightPanelTab = (tab: RightPanelTab) => {
    if (!canUseRightPanel) {
      setRightPanelOpen(false);
      return;
    }
    setRightPanelActiveTab(tab);
    if (tab === "comments") setCommentPanelScope("all");
    if (tab !== "comments") setSelectedCommentId(null);
    setRightPanelOpen(true);
    if (isMobile) setSidebarOpen(false);
  };

  const toggleRightPanelTab = (tab: RightPanelTab) => {
    if (!canUseRightPanel) {
      setRightPanelOpen(false);
      return;
    }
    const next = getRightPanelTabToggleState(rightPanelOpen, rightPanelActiveTab, tab);
    setRightPanelActiveTab(next.activeTab);
    if (tab === "comments") setCommentPanelScope("all");
    if (tab !== "comments") setSelectedCommentId(null);
    setRightPanelOpen(next.open);
    if (next.open && isMobile) setSidebarOpen(false);
  };

  const openComment = (commentId?: string) => {
    if (!canUseRightPanel) {
      setRightPanelOpen(false);
      return;
    }
    setSelectedCommentId(commentId ?? null);
    setCommentPanelScope("inline");
    setRightPanelActiveTab("comments");
    setRightPanelOpen(true);
    if (isMobile) setSidebarOpen(false);
  };

  const workspaceTopBar = (
    <TopBar
      sidebarOpen={isMobile ? sidebarOpen : !sidebarCollapsed}
      onToggleSidebar={toggleSidebar}
      rightPanelOpen={rightPanelOpen}
      onToggleRightPanel={() => {
        if (!canUseRightPanel) {
          setRightPanelOpen(false);
          return;
        }
        setRightPanelOpen((prev) => {
          const next = !prev;
          if (next && isMobile) setSidebarOpen(false);
          return next;
        });
      }}
      showRightPanelToggle={false}
      showSidebarToggle={!isWorkspaceDestination}
      destination={destination ?? undefined}
      mobile={isMobile}
      onActionsSlotChange={setTopBarActionsSlot}
    />
  );

  return (
    <EditorZoomContext.Provider value={{ cssVar }}>
      <WorkspaceSpaceProvider>
        <WorkspaceCreateActionContext.Provider value={requestWorkspaceCreateAction}>
          <WorkspaceTopBarSlotContext.Provider
            value={{ actions: topBarActionsSlot }}
          >
            <div className="app-viewport app-background text-text flex flex-col">
              <a
                href="#main-content"
                className="rounded-button bg-primary sr-only z-50 px-3 py-2 text-sm text-white focus:not-sr-only focus:fixed focus:top-3 focus:left-3"
              >
                Skip to main content
              </a>

              {/* Mobile sidebar drawer */}
              {isMobile && !isWorkspaceDestination && (
                <MobileNavigationDrawer
                  open={sidebarOpen}
                  suspended={mobileSidebarOverlayOpen}
                  onClose={() => { setPendingMobileMove(null); setSidebarOpen(false); }}
                >
                  <Sidebar
                    showHeader={false}
                    mobileSettingsMenu
                    onOpenCommandMenu={() => {
                      setSidebarOpen(false);
                      setRightPanelOpen(false);
                      setCommandMenuOpen(true);
                    }}
                    onMobileOverlayChange={setMobileSidebarOverlayOpen}
                  />
                </MobileNavigationDrawer>
              )}

              <UpdateBanner />
              {isMobile && workspaceTopBar}

              {/* Main content area */}
              <div className="flex min-h-0 min-w-0 flex-1">
                {/* Desktop sidebar */}
                {!isMobile && !isWorkspaceDestination && (
                  <aside
                    className={`border-border bg-surface relative flex shrink-0 flex-col border-r transition-[width] duration-100 ${
                      sidebarCollapsed ? "w-14" : "w-sidebar"
                    }`}
                    aria-label="Sidebar"
                  >
                    <div className="min-h-0 min-w-0 flex-1">
                      <Sidebar
                        showHeader
                        collapsed={sidebarCollapsed}
                        onOpenCommandMenu={() => setCommandMenuOpen(true)}
                        onShowFiles={() => {
                          setSidebarCollapsed(false);
                          writeSidebarCompactPreference(false);
                          window.requestAnimationFrame(() => {
                            document.getElementById("sidebar-files-heading")?.focus();
                          });
                        }}
                      />
                    </div>
                  </aside>
                )}

                <div
                  className={`flex min-w-0 flex-1 flex-col transition-opacity duration-100 ${
                    isMobile && sidebarOpen ? "opacity-0" : ""
                  }`}
                >
                  {!isMobile && workspaceTopBar}
                  <div className="relative flex min-h-0 flex-1">
                    {/* Editor / main content */}
                    <main
                      id="main-content"
                      className="min-w-0 flex-1 overflow-y-auto"
                      onPointerDown={(event) => {
                        if (rightPanelOpen && shouldCloseRightPanelFromPointer(event.target)) {
                          setRightPanelOpen(false);
                          setSelectedCommentId(null);
                        }
                      }}
                    >
                      <AdminSidebarContext.Provider
                        value={{ open: isAdmin || (isMobile ? sidebarOpen : !sidebarCollapsed) }}
                      >
                        <RightPanelControlContext.Provider
                          value={{
                            open: rightPanelOpen,
                            activeTab: rightPanelActiveTab,
                            selectedCommentId,
                            commentPanelScope,
                            toggle: () => {
                              if (!canUseRightPanel) {
                                setRightPanelOpen(false);
                                return;
                              }
                              setRightPanelOpen((prev) => {
                                const next = !prev;
                                if (next && isMobile) setSidebarOpen(false);
                                return next;
                              });
                            },
                            close: () => {
                              setRightPanelOpen(false);
                              setSelectedCommentId(null);
                            },
                            toggleTab: toggleRightPanelTab,
                            openTab: openRightPanelTab,
                            openComment,
                          }}
                        >
                          <DocumentPageDetailsRegistrationContext.Provider
                            value={registerDocumentPageDetails}
                          >
                            {children}
                          </DocumentPageDetailsRegistrationContext.Provider>
                        </RightPanelControlContext.Provider>
                      </AdminSidebarContext.Provider>
                    </main>

                    {/* Desktop right panel */}
                    {!isMobile && canUseRightPanel && rightPanelOpen && (
                      <aside
                        className="workspace-document-inspector border-border bg-surface absolute inset-y-0 right-0 z-30 w-80 max-w-full border-l"
                        aria-label={getRightPanelAriaLabel(rightPanelActiveTab)}
                      >
                        <RightPanel
                          spaceId={spaceId}
                          docId={docId}
                          activeTab={rightPanelActiveTab}
                          onTabChange={setRightPanelActiveTab}
                          selectedCommentId={selectedCommentId}
                          commentPanelScope={commentPanelScope}
                          pageDetailsControls={documentPageDetails}
                          onClose={() => {
                            setRightPanelOpen(false);
                            setSelectedCommentId(null);
                          }}
                        />
                      </aside>
                    )}
                  </div>

                  <OfflineBar zoomPercent={zoom} />
                </div>
              </div>

              {/* Mobile right panel drawer */}
              {isMobile && canUseRightPanel && (
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

              {/* Keyboard shortcuts modal */}
              <KeyboardShortcuts
                open={shortcutsOpen}
                onClose={() => setShortcutsOpen(false)}
                canUseDocumentPanelShortcut={canUseRightPanel}
              />
              {!isWorkspaceDestination && (
                <WorkspaceCommandMenu
                  open={commandMenuOpen}
                  onClose={() => setCommandMenuOpen(false)}
                  spaceId={spaceId}
                />
              )}
            </div>
          </WorkspaceTopBarSlotContext.Provider>
        </WorkspaceCreateActionContext.Provider>
      </WorkspaceSpaceProvider>
    </EditorZoomContext.Provider>
  );
}
