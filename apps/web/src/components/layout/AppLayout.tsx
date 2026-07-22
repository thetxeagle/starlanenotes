import { createContext, useEffect, useState, type ReactNode } from "react";
import { useLocation, useParams } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { TopBar } from "./TopBar";
import { RightPanel } from "./RightPanel";
import { KeyboardShortcuts } from "@/components/KeyboardShortcuts";
import { OfflineBar } from "@/components/OfflineBar";
import { UpdateBanner } from "@/components/UpdateBanner";
import { useEditorZoom } from "@/hooks/useEditorZoom";
import { writeLastViewedSpaceId } from "@/lib/lastViewedSpace";

export const EditorZoomContext = createContext<{ cssVar: number }>({ cssVar: 1 });
export const AdminSidebarContext = createContext<{ open: boolean }>({ open: true });
export const RightPanelControlContext = createContext<{
  open: boolean;
  toggle: () => void;
}>({ open: false, toggle: () => undefined });

interface AppLayoutProps {
  children: ReactNode;
}

export function isDocumentPanelRoute(pathname: string): boolean {
  return /^\/spaces\/[^/]+\/docs\/[^/]+\/?$/.test(pathname);
}

export function isAdminRoute(pathname: string): boolean {
  return /^\/admin\/?$/.test(pathname);
}

export const MOBILE_DOCUMENT_PANEL_CLASS =
  "border-border bg-surface shadow-card rounded-card fixed left-1/2 top-1/2 z-40 flex h-[min(42rem,calc(100dvh-2rem))] w-[min(38rem,calc(100vw-1.5rem))] -translate-x-1/2 -translate-y-1/2 flex-col overflow-hidden border";

export function AppLayout({ children }: AppLayoutProps) {
  const [isMobile, setIsMobile] = useState(() =>
    typeof window !== "undefined" ? window.matchMedia("(max-width: 1023px)").matches : false,
  );
  const [sidebarOpen, setSidebarOpen] = useState(() =>
    typeof window !== "undefined" ? !window.matchMedia("(max-width: 1023px)").matches : true,
  );
  const [rightPanelOpen, setRightPanelOpen] = useState(false);
  const [shortcutsOpen, setShortcutsOpen] = useState(false);
  const location = useLocation();
  const { spaceId, docId } = useParams<{ spaceId: string; docId: string }>();
  const { zoom, cssVar } = useEditorZoom();
  const canUseRightPanel = isDocumentPanelRoute(location.pathname);
  const isAdmin = isAdminRoute(location.pathname);

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
    setRightPanelOpen(false);
  }, [location.pathname, isMobile]);

  useEffect(() => {
    if (!canUseRightPanel) setRightPanelOpen(false);
  }, [canUseRightPanel]);

  useEffect(() => {
    if (spaceId) writeLastViewedSpaceId(spaceId);
  }, [spaceId]);

  // Keyboard shortcut: ? for shortcuts modal
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;

      if (e.key === "?" && !e.metaKey && !e.ctrlKey) {
        e.preventDefault();
        setShortcutsOpen((prev) => !prev);
      }

      // Cmd+\ to toggle sidebar
      if ((e.metaKey || e.ctrlKey) && e.key === "\\") {
        if (isAdmin) return;
        e.preventDefault();
        setSidebarOpen((prev) => !prev);
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
  }, [canUseRightPanel, isAdmin, isMobile]);

  return (
    <EditorZoomContext.Provider value={{ cssVar }}>
      <div className="app-viewport app-background text-text flex flex-col">
        <a
          href="#main-content"
          className="rounded-button bg-primary sr-only z-50 px-3 py-2 text-sm text-white focus:not-sr-only focus:fixed focus:left-3 focus:top-3"
        >
          Skip to main content
        </a>

        {/* Mobile sidebar drawer */}
        {isMobile && sidebarOpen && !isAdmin && (
          <>
            <div
              className="fixed inset-0 z-30 bg-black/40"
              onClick={() => setSidebarOpen(false)}
              aria-hidden="true"
            />
            <aside
              className="w-sidebar border-border bg-surface fixed inset-y-0 left-0 z-40 flex flex-col border-r"
              role="dialog"
              aria-modal="true"
              aria-label="Sidebar"
            >
              <Sidebar />
            </aside>
          </>
        )}

        <UpdateBanner />
        <TopBar
          sidebarOpen={sidebarOpen}
          onToggleSidebar={() => {
            setSidebarOpen((prev) => {
              const next = !prev;
              if (next && isMobile) setRightPanelOpen(false);
              return next;
            });
          }}
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
          showSidebarToggle={!isAdmin}
        />

        {/* Main content area */}
        <div className="flex min-h-0 min-w-0 flex-1">
          {/* Desktop sidebar */}
          {!isMobile && sidebarOpen && !isAdmin && (
            <aside
              className="w-sidebar border-border bg-surface flex shrink-0 flex-col border-r"
              aria-label="Sidebar"
            >
              <Sidebar showHeader={false} />
            </aside>
          )}

          <div
            className={`flex min-w-0 flex-1 flex-col transition-opacity duration-100 ${
              isMobile && sidebarOpen ? "opacity-0" : ""
            }`}
          >
            <div className="flex min-h-0 flex-1">
              {/* Editor / main content */}
              <main id="main-content" className="min-w-0 flex-1 overflow-y-auto">
                <AdminSidebarContext.Provider value={{ open: isAdmin || sidebarOpen }}>
                  <RightPanelControlContext.Provider
                    value={{
                      open: rightPanelOpen,
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
                    }}
                  >
                    {children}
                  </RightPanelControlContext.Provider>
                </AdminSidebarContext.Provider>
              </main>

              {/* Desktop right panel */}
              {!isMobile && canUseRightPanel && rightPanelOpen && (
                <aside
                  className="border-border bg-surface w-80 shrink-0 border-l"
                  aria-label="Properties panel"
                >
                  <RightPanel spaceId={spaceId} docId={docId} />
                </aside>
              )}
            </div>

            <OfflineBar zoomPercent={zoom} />
          </div>
        </div>

        {/* Mobile right panel drawer */}
        {isMobile && canUseRightPanel && rightPanelOpen && (
          <>
            <div
              className="fixed inset-0 z-30 bg-black/40"
              onClick={() => setRightPanelOpen(false)}
              aria-hidden="true"
            />
            <aside
              className={MOBILE_DOCUMENT_PANEL_CLASS}
              role="dialog"
              aria-modal="true"
              aria-label="Properties panel"
            >
              <RightPanel spaceId={spaceId} docId={docId} />
            </aside>
          </>
        )}

        {/* Keyboard shortcuts modal */}
        <KeyboardShortcuts
          open={shortcutsOpen}
          onClose={() => setShortcutsOpen(false)}
          canUseDocumentPanelShortcut={canUseRightPanel}
        />
      </div>
    </EditorZoomContext.Provider>
  );
}
