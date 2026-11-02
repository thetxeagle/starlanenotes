import { ArrowLeft, Menu, PanelLeft, Search } from "lucide-react";
import { AboutButton } from "./AboutButton";
import { BrandLogo } from "@/components/BrandLogo";
import { NotificationBell } from "@/components/NotificationBell";
import { getCurrentLaunchMode } from "@/lib/launchMode";

interface MobileWorkspaceHeaderProps {
  navigationOpen?: boolean;
  onToggleNavigation?: () => void;
  containerClassName: string;
  controlsClassName?: string;
}

/**
 * Shared compact workspace header for responsive web and native mobile shells.
 * Workspace utilities live in the drawer so the header stays focused on
 * navigation and product identity.
 */
export function MobileWorkspaceHeader({
  navigationOpen = false,
  onToggleNavigation,
  containerClassName,
  controlsClassName = "flex h-11 items-center gap-2",
}: MobileWorkspaceHeaderProps) {
  return (
    <header className={containerClassName} aria-label="Mobile top bar">
      <div className={controlsClassName}>
        {onToggleNavigation && (
          <button
            type="button"
            onClick={onToggleNavigation}
            className="border-border bg-surface-raised text-text-muted hover:text-text flex h-11 w-11 shrink-0 items-center justify-center rounded-button border shadow-card transition-colors duration-100 active:bg-surface"
            aria-label={navigationOpen ? "Close navigation" : "Open navigation"}
          >
            {navigationOpen ? <PanelLeft size={21} /> : <Menu size={21} />}
          </button>
        )}
        <div className="flex min-w-0 flex-1 items-center">
          <BrandLogo size="md" variant="mobile-header" />
        </div>
        <AboutButton touchTarget />
      </div>
    </header>
  );
}

interface MobileDestinationHeaderProps {
  title: string;
  ariaLabel: string;
  backHref?: string;
  backLabel?: string;
  onOpenSearch: () => void;
  showUtilities?: boolean;
  showTitle?: boolean;
  onBack?: () => void;
  containerClassName: string;
  controlsClassName?: string;
}

/**
 * Long-lived account and administration surfaces are destinations, not
 * alternate workspace drawers. Keep their mobile chrome focused on an
 * explicit return path while preserving authenticated notifications and
 * Search.
 */
export function MobileDestinationHeader({
  title,
  ariaLabel,
  backHref = "/",
  backLabel = "Back to Notes",
  onOpenSearch,
  showUtilities = true,
  showTitle = true,
  onBack,
  containerClassName,
  controlsClassName = "flex h-11 items-center gap-2",
}: MobileDestinationHeaderProps) {
  const localMode = getCurrentLaunchMode() === "local";

  return (
    <header className={containerClassName} aria-label={ariaLabel}>
      <div className={controlsClassName}>
        <button
          type="button"
          onClick={onBack ?? (() => window.location.assign(backHref))}
          className="border-border bg-surface-raised text-text-muted hover:text-text flex h-11 w-11 shrink-0 items-center justify-center rounded-button border shadow-card transition-colors duration-100 active:bg-surface"
          aria-label={backLabel}
        >
          <ArrowLeft size={21} />
        </button>
        <div className="flex min-w-0 flex-1 items-center">
          <BrandLogo size="sm" variant="mobile-header" />
          {showTitle && <span className="truncate text-sm font-medium text-text">{title}</span>}
        </div>
        <AboutButton touchTarget />
        {showUtilities && !localMode && <NotificationBell touchTarget />}
        {showUtilities && (
          <button
            type="button"
            onClick={onOpenSearch}
            className="text-text-muted hover:bg-surface-raised hover:text-text flex h-11 w-11 shrink-0 items-center justify-center rounded-button transition-colors duration-100 active:bg-surface-raised"
            aria-label="Open search"
          >
            <Search size={20} />
          </button>
        )}
      </div>
    </header>
  );
}
