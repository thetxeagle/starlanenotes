import { useRef, useState, type KeyboardEvent as ReactKeyboardEvent } from "react";
import { LogOut, Shield, SlidersHorizontal, UserCircle } from "lucide-react";
import {
  useLocation,
  useNavigate,
  type Location,
  type NavigateOptions,
} from "react-router-dom";
import { Popover, PopoverContent, PopoverTrigger } from "@/components/ui/popover";
import { useMenuKeyboardNavigation } from "@/components/ui/useMenuKeyboardNavigation";
import { useAuth, type User } from "@/hooks/useAuth";
import { getCurrentLaunchMode } from "@/lib/launchMode";

export const WORKSPACE_ACCOUNT_SETTINGS_PATH = "/profile";
export const WORKSPACE_SITE_ADMINISTRATION_PATH = "/admin";
export const WORKSPACE_APP_SETTINGS_PATH = "/mobile/app-settings";

export function getWorkspaceSettingsNavigationState(location: Location) {
  return { backgroundLocation: location };
}

export function getWorkspaceSettingsNavigationOptions(
  path: string,
  location: Location,
): NavigateOptions | undefined {
  return path === WORKSPACE_SITE_ADMINISTRATION_PATH
    ? undefined
    : { state: getWorkspaceSettingsNavigationState(location) };
}

interface WorkspaceProfileMenuContentProps {
  localMode: boolean;
  user: User | null;
  menuRef: React.RefObject<HTMLDivElement | null>;
  onKeyDown: (event: ReactKeyboardEvent<HTMLDivElement>) => void;
  onNavigate: (path: string) => void;
  onSignOut: () => void;
}

export function WorkspaceProfileMenuContent({
  localMode,
  user,
  menuRef,
  onKeyDown,
  onNavigate,
  onSignOut,
}: WorkspaceProfileMenuContentProps) {
  const itemClass =
    "rounded-button text-text-muted hover:bg-surface hover:text-text flex min-h-10 w-full items-center gap-2 px-2 text-left text-sm transition-colors duration-100 disabled:cursor-not-allowed disabled:opacity-50";

  return (
    <div ref={menuRef} role="menu" aria-label="Account" onKeyDown={onKeyDown}>
      <div className="border-border border-b px-3 py-2">
        <p className="text-text truncate text-sm font-medium">
          {localMode ? "Local workspace" : user?.display_name || user?.email || "Account"}
        </p>
        {!localMode && user?.email ? (
          <p className="text-text-muted truncate text-xs">{user.email}</p>
        ) : (
          <p className="text-text-muted truncate text-xs">Stored on this device</p>
        )}
      </div>
      <div className="p-1">
        {!localMode ? (
          <button
            type="button"
            role="menuitem"
            className={itemClass}
            onClick={() => onNavigate(WORKSPACE_ACCOUNT_SETTINGS_PATH)}
          >
            <UserCircle size={15} />
            Account settings
          </button>
        ) : null}
        {localMode ? (
          <button
            type="button"
            role="menuitem"
            className={itemClass}
            onClick={() => onNavigate(WORKSPACE_SITE_ADMINISTRATION_PATH)}
          >
            <Shield size={15} />
            Storage administration
          </button>
        ) : null}
        {localMode ? (
          <button
            type="button"
            role="menuitem"
            className={itemClass}
            onClick={() => onNavigate(WORKSPACE_APP_SETTINGS_PATH)}
          >
            <SlidersHorizontal size={15} />
            App settings
          </button>
        ) : null}
      </div>
      {!localMode && user ? (
        <div className="border-border border-t p-1">
          <button
            type="button"
            role="menuitem"
            className={`${itemClass} text-danger hover:bg-danger/10 hover:text-danger`}
            onClick={onSignOut}
          >
            <LogOut size={15} />
            Sign out
          </button>
        </div>
      ) : null}
    </div>
  );
}

export function WorkspaceProfileMenu() {
  const { user, logout } = useAuth();
  const navigate = useNavigate();
  const location = useLocation();
  const localMode = getCurrentLaunchMode() === "local";
  const [open, setOpen] = useState(false);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const handleMenuNavigation = useMenuKeyboardNavigation(menuRef);
  const identity = localMode
    ? "L"
    : (user?.display_name || user?.email || "Account").trim().charAt(0).toUpperCase();

  const closeAndRestoreTrigger = () => {
    setOpen(false);
    window.requestAnimationFrame(() => triggerRef.current?.focus());
  };

  const handleMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      closeAndRestoreTrigger();
      return;
    }
    handleMenuNavigation(event);
  };

  const navigateToSettings = (path: string) => {
    setOpen(false);
    navigate(path, getWorkspaceSettingsNavigationOptions(path, location));
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          ref={triggerRef}
          type="button"
          className="rounded-button text-text hover:bg-surface-raised flex size-8 items-center justify-center transition-colors duration-100"
          aria-label="Open account menu"
          aria-haspopup="menu"
          aria-expanded={open}
        >
          {user?.avatar_url && !localMode ? (
            <img src={user.avatar_url} alt="" className="size-6 rounded-full object-cover" />
          ) : (
            <span className="bg-primary/15 text-primary flex size-6 items-center justify-center rounded-full text-xs font-semibold">
              {identity || <UserCircle size={15} />}
            </span>
          )}
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        sideOffset={6}
        className="w-64 overflow-hidden p-0"
        onOpenAutoFocus={(event) => {
          event.preventDefault();
          menuRef.current
            ?.querySelector<HTMLButtonElement>('button[role="menuitem"]:not(:disabled)')
            ?.focus();
        }}
      >
        <WorkspaceProfileMenuContent
          localMode={localMode}
          user={user}
          menuRef={menuRef}
          onKeyDown={handleMenuKeyDown}
          onNavigate={navigateToSettings}
          onSignOut={() => {
            setOpen(false);
            logout();
          }}
        />
      </PopoverContent>
    </Popover>
  );
}
