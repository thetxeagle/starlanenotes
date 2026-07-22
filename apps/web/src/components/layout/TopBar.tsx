import { useEffect, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import {
  Menu,
  PanelLeft,
  PanelRight,
  Moon,
  Sun,
  Monitor,
  LogOut,
  Settings,
  User,
} from "lucide-react";
import { useTheme } from "@/hooks/useTheme";
import { useAuth } from "@/hooks/useAuth";
import { NotificationBell } from "@/components/NotificationBell";
import { SyncStatus } from "@/components/SyncStatus";
import { SearchModal, type SearchModalPreset } from "@/components/SearchModal";
import { BrandLogo } from "@/components/BrandLogo";
import { SearchInput } from "@/components/ui/search-input";
import { getCurrentLaunchMode } from "@/lib/launchMode";

interface TopBarProps {
  sidebarOpen: boolean;
  onToggleSidebar: () => void;
  rightPanelOpen: boolean;
  onToggleRightPanel: () => void;
  showSidebarToggle?: boolean;
  showRightPanelToggle?: boolean;
  initialUserMenuOpen?: boolean;
}

export const WEB_PROFILE_PATH = "/profile";
export const LOCAL_SETTINGS_PATH = "/admin?section=storage";
export const SITE_ADMIN_PATH = "/admin";
export const TOP_BAR_DANGER_MENU_ITEM_CLASS =
  "rounded-button flex w-full items-center gap-2 px-2 py-1.5 text-sm text-danger transition-colors duration-100 hover:bg-danger/10";

export function TopBar({
  sidebarOpen,
  onToggleSidebar,
  rightPanelOpen,
  onToggleRightPanel,
  showSidebarToggle = true,
  showRightPanelToggle = true,
  initialUserMenuOpen = false,
}: TopBarProps) {
  const navigate = useNavigate();
  const { theme, setTheme } = useTheme();
  const { user, logout } = useAuth();
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchPreset, setSearchPreset] = useState<SearchModalPreset | undefined>();
  const [userMenuOpen, setUserMenuOpen] = useState(initialUserMenuOpen);
  const userMenuRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  const localMode = getCurrentLaunchMode() === "local";

  // Global Cmd+K shortcut
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "k") {
        e.preventDefault();
        setSearchPreset(undefined);
        setSearchOpen(true);
        window.requestAnimationFrame(() => searchInputRef.current?.focus());
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  useEffect(() => {
    const handler = (event: Event) => {
      const detail = (event as CustomEvent<{ tag?: string; spaceId?: string }>).detail;
      setSearchPreset({
        nonce: Date.now(),
        filters: {
          tag: detail?.tag,
          space_id: detail?.spaceId,
        },
      });
      setSearchOpen(true);
      window.requestAnimationFrame(() => searchInputRef.current?.focus());
    };
    document.addEventListener("starlane:open-search", handler);
    return () => document.removeEventListener("starlane:open-search", handler);
  }, []);

  useEffect(() => {
    if (!searchOpen) return;
    const handler = (event: PointerEvent) => {
      if (!searchRef.current?.contains(event.target as Node)) {
        setSearchOpen(false);
      }
    };
    document.addEventListener("pointerdown", handler);
    return () => document.removeEventListener("pointerdown", handler);
  }, [searchOpen]);

  // Close user menu on outside click
  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (userMenuRef.current && !userMenuRef.current.contains(e.target as Node)) {
        setUserMenuOpen(false);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, []);

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        setUserMenuOpen(false);
        setSearchOpen(false);
      }
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, []);

  const cycleTheme = () => {
    const next = theme === "light" ? "dark" : theme === "dark" ? "system" : "light";
    setTheme(next);
  };

  const ThemeIcon = theme === "light" ? Sun : theme === "dark" ? Moon : Monitor;

  return (
    <>
      <header className="border-border bg-surface grid h-12 shrink-0 grid-cols-[1fr_auto_1fr] items-center gap-3 border-b px-3">
        <div className="flex min-w-0 items-center gap-2">
          {showSidebarToggle && (
            <button
              type="button"
              onClick={onToggleSidebar}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-8 w-8 items-center justify-center transition-colors duration-100"
              aria-label="Toggle sidebar"
            >
              {sidebarOpen ? <PanelLeft size={16} /> : <Menu size={16} />}
            </button>
          )}
          <button
            type="button"
            onClick={() => navigate("/")}
            className="rounded-button hidden min-w-0 items-center px-1 py-1 text-left transition-colors duration-100 hover:bg-surface-raised md:flex"
            aria-label="Open Starlane Notes home"
          >
            <BrandLogo size="sm" />
          </button>
        </div>

        <div ref={searchRef} className="relative flex w-[min(38rem,48vw)] min-w-0 justify-center">
          <SearchInput
            ref={searchInputRef}
            value={searchQuery}
            onChange={(event) => {
              setSearchPreset(undefined);
              setSearchQuery(event.target.value);
              setSearchOpen(true);
            }}
            onFocus={() => setSearchOpen(true)}
            aria-label="Search Starlane Notes with SNQL"
            aria-haspopup="dialog"
            aria-expanded={searchOpen}
            placeholder={'Search SNQL: "notes" tag:work folder:project has:attachment'}
            containerClassName="w-full"
            className="h-8 bg-bg pr-12 text-sm"
          />
          <kbd className="border-border text-text-muted pointer-events-none absolute right-2 top-1/2 hidden -translate-y-1/2 rounded border px-1 font-mono text-[10px] leading-4 lg:inline">
            ⌘K
          </kbd>
          <SearchModal
            open={searchOpen}
            onClose={() => setSearchOpen(false)}
            preset={searchPreset}
            presentation="popover"
            query={searchQuery}
            onQueryChange={setSearchQuery}
          />
        </div>

        {/* Right section */}
        <div className="flex min-w-0 items-center justify-end gap-1">
          {!localMode && (
            <>
              {/* Sync status */}
              <div className="hidden sm:block">
                <SyncStatus />
              </div>

              <div className="bg-border mx-1 hidden h-4 w-px sm:block" />
            </>
          )}

          {!localMode && <NotificationBell />}

          {/* Theme toggle */}
          <button
            type="button"
            onClick={cycleTheme}
            className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
            aria-label={`Theme: ${theme}`}
          >
            <ThemeIcon size={14} />
          </button>

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

          {localMode && (
            <button
              type="button"
              onClick={() => navigate(LOCAL_SETTINGS_PATH)}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 w-7 items-center justify-center transition-colors duration-100"
              aria-label="Open local settings"
            >
              <Settings size={14} />
            </button>
          )}

          {/* User menu */}
          {!localMode && user && (
            <div className="relative ml-1" ref={userMenuRef}>
              <button
                type="button"
                onClick={() => setUserMenuOpen(!userMenuOpen)}
                className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-7 items-center gap-2 px-2 text-xs transition-colors duration-100"
                aria-label="Open user menu"
                aria-haspopup="menu"
                aria-expanded={userMenuOpen}
                aria-controls="user-menu"
              >
                {user.avatar_url ? (
                  <img
                    src={user.avatar_url}
                    alt=""
                    className="h-5 w-5 rounded-full object-cover"
                  />
                ) : (
                  <div className="bg-primary flex h-5 w-5 items-center justify-center rounded-full text-[10px] font-medium text-white">
                    {user.display_name[0]?.toUpperCase()}
                  </div>
                )}
              </button>

              {userMenuOpen && (
                <div
                  id="user-menu"
                  role="menu"
                  aria-label="User menu"
                  className="starlane-menu-surface absolute right-0 top-full z-50 mt-1 w-56"
                >
                  <div className="border-border border-b px-3 py-2">
                    <div className="text-text text-sm font-medium">{user.display_name}</div>
                    <div className="text-text-muted text-xs">{user.email}</div>
                  </div>
                  <div className="p-1">
                    <button
                      type="button"
                      onClick={() => {
                        setUserMenuOpen(false);
                        navigate(WEB_PROFILE_PATH);
                      }}
                      role="menuitem"
                      className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100"
                    >
                      <User size={14} />
                      Profile
                    </button>
                    {user.role === "admin" && (
                      <button
                        type="button"
                        onClick={() => {
                          setUserMenuOpen(false);
                          navigate(SITE_ADMIN_PATH);
                        }}
                        role="menuitem"
                        className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex w-full items-center gap-2 px-2 py-1.5 text-sm transition-colors duration-100"
                      >
                        <Settings size={14} />
                        Site administration
                      </button>
                    )}
                  </div>
                  <div className="border-border border-t p-1">
                    <button
                      type="button"
                      onClick={() => {
                        setUserMenuOpen(false);
                        logout();
                      }}
                      role="menuitem"
                      className={TOP_BAR_DANGER_MENU_ITEM_CLASS}
                    >
                      <LogOut size={14} />
                      Sign out
                    </button>
                  </div>
                </div>
              )}
            </div>
          )}
        </div>
      </header>
    </>
  );
}
