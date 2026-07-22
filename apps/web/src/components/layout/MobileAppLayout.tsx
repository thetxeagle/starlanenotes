import { useEffect, useRef, useState, type CSSProperties, type ReactNode, type TouchEvent } from "react";
import { useLocation, useParams } from "react-router-dom";
import { Menu, Search, X } from "lucide-react";
import { BrandLogo } from "@/components/BrandLogo";
import { RightPanel } from "./RightPanel";
import { isDocumentPanelRoute, MOBILE_DOCUMENT_PANEL_CLASS, RightPanelControlContext } from "./AppLayout";
import { Sidebar } from "./Sidebar";
import { writeLastViewedSpaceId } from "@/lib/lastViewedSpace";
import { SearchModal } from "@/components/SearchModal";
import { OfflineBar } from "@/components/OfflineBar";
import { NotificationBell } from "@/components/NotificationBell";

interface MobileAppLayoutProps {
  children: ReactNode;
}

export const MOBILE_SEARCH_PATH = "/mobile/search";
export const MOBILE_DOCUMENT_PANEL_BODY_CLASS = "min-h-0 flex-1 overflow-hidden";

export function canUseMobileRightPanel(pathname: string): boolean {
  return isDocumentPanelRoute(pathname);
}

export function MobileAppLayout({ children }: MobileAppLayoutProps) {
  const location = useLocation();
  const { spaceId, docId } = useParams<{ spaceId: string; docId: string }>();
  const [rightPanelOpen, setRightPanelOpen] = useState(false);
  const [navOpen, setNavOpen] = useState(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const canUseRightPanel = canUseMobileRightPanel(location.pathname);
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
    if (!start || !touch || navOpen || rightPanelOpen) return;
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
  }, [location.pathname]);

  useEffect(() => {
    if (spaceId) writeLastViewedSpaceId(spaceId);
  }, [spaceId]);

  useEffect(() => {
    if (!canUseRightPanel) setRightPanelOpen(false);
  }, [canUseRightPanel]);

  return (
    <div
      className={`app-viewport app-background text-text flex flex-col ${
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

      <header
        className="pointer-events-none fixed left-0 right-0 top-0 z-20 border-b border-border bg-bg px-4 pb-3 pt-[max(2.25rem,calc(env(safe-area-inset-top)+1.75rem))]"
        aria-label="Mobile top bar"
      >
        <div className="pointer-events-auto flex h-11 items-center gap-3">
          <button
            type="button"
            onClick={() => setNavOpen(true)}
            className="flex h-11 w-11 shrink-0 items-center justify-center rounded-button border border-border bg-surface-raised text-text-muted shadow-card transition-colors duration-100 active:bg-surface hover:text-text"
            aria-label="Open navigation"
          >
            <Menu size={21} />
          </button>
          <div className="min-w-0 flex-1">
            <BrandLogo size="sm" />
          </div>
          <NotificationBell />
          <button
            type="button"
            onClick={() => setSearchOpen(true)}
            className="flex h-11 w-11 shrink-0 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:bg-surface-raised hover:text-text"
            aria-label="Open search"
          >
            <Search size={20} />
          </button>
        </div>
      </header>

      <main
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto pb-[max(1rem,env(safe-area-inset-bottom))] pt-[max(7.25rem,calc(env(safe-area-inset-top)+5rem))]"
        onTouchStart={handleTouchStart}
        onTouchEnd={handleTouchEnd}
      >
        <RightPanelControlContext.Provider
          value={{
            open: canUseRightPanel && rightPanelOpen,
            toggle: () => {
              if (!canUseRightPanel) {
                setRightPanelOpen(false);
                return;
              }
              setRightPanelOpen((prev) => !prev);
            },
          }}
        >
          {children}
        </RightPanelControlContext.Provider>
      </main>

      <OfflineBar />

      {navOpen && (
        <>
          <button
            type="button"
            className="fixed inset-0 z-40 bg-black/45"
            aria-label="Close navigation"
            onClick={() => setNavOpen(false)}
          />
          <aside
            className="fixed bottom-0 left-0 top-0 z-50 w-[min(19rem,86vw)] border-r border-border bg-surface shadow-card"
            role="dialog"
            aria-modal="true"
            aria-label="Mobile navigation"
          >
            <div className="flex h-full min-h-0 flex-col pt-[max(0.75rem,env(safe-area-inset-top))]">
              <Sidebar showHeader={false} mobileSettingsMenu />
            </div>
          </aside>
        </>
      )}

      <SearchModal
        open={searchOpen}
        onClose={() => setSearchOpen(false)}
        presentation="fullscreen"
      />

      {canUseRightPanel && rightPanelOpen && (
        <>
          <button
            type="button"
            className="fixed inset-0 z-30 bg-black/40"
            aria-label="Close properties panel"
            onClick={() => setRightPanelOpen(false)}
          />
          <aside
            className={MOBILE_DOCUMENT_PANEL_CLASS}
            role="dialog"
            aria-modal="true"
            aria-label="Properties panel"
          >
            <div className="flex shrink-0 items-center justify-between border-b border-border px-3 py-2">
              <span className="text-sm font-medium text-text">Document panel</span>
              <button
                type="button"
                onClick={() => setRightPanelOpen(false)}
                className="flex h-8 w-8 items-center justify-center rounded-button text-text-muted transition-colors duration-100 active:bg-surface-raised hover:text-text"
                aria-label="Close properties panel"
              >
                <X size={16} />
              </button>
            </div>
            <div className={MOBILE_DOCUMENT_PANEL_BODY_CLASS}>
              <RightPanel spaceId={spaceId} docId={docId} />
            </div>
          </aside>
        </>
      )}
    </div>
  );
}
