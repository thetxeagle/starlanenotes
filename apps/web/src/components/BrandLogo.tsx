import notesIcon from "@/assets/brand/starlane-notes-icon.svg";

interface BrandLogoProps {
  label?: string;
  size?: "sm" | "md" | "lg";
  showText?: boolean;
  variant?: "default" | "sidebar" | "mobile-header";
}

const sizeClasses = {
  sm: {
    icon: "h-6 w-6",
    title: "text-sm",
    subtitle: "text-[10px]",
  },
  md: {
    icon: "h-9 w-9",
    title: "text-base",
    subtitle: "text-xs",
  },
  lg: {
    icon: "h-14 w-14",
    title: "text-xl",
    subtitle: "text-sm",
  },
};

export const NOTES_BRAND_SUBTITLE = "Source-available notes workspace";

export function BrandLogo({
  label = "Starlane Notes",
  size = "md",
  showText = true,
  variant = "default",
}: BrandLogoProps) {
  const classes = sizeClasses[size];
  const mobileHeader = variant === "mobile-header";
  const workspaceLockup = variant === "sidebar" || mobileHeader;

  return (
    <div className="flex min-w-0 items-center gap-3">
      <img src={notesIcon} alt="" aria-hidden="true" className={`${classes.icon} shrink-0`} />
      {showText && (
        <div className="min-w-0 flex-1">
          {workspaceLockup ? (
            <div className="min-w-0" aria-label={label}>
              <div className={`${classes.title} truncate font-semibold leading-tight text-text`}>{label}</div>
              <div className={`${classes.subtitle} truncate leading-tight text-text-muted`}>
                {NOTES_BRAND_SUBTITLE}
              </div>
            </div>
          ) : (
            <>
              <div className={`${classes.title} truncate font-semibold text-text`}>{label}</div>
              <div className={`${classes.subtitle} truncate text-text-muted`}>
                {NOTES_BRAND_SUBTITLE}
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
