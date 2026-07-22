import notesIcon from "@/assets/brand/starlane-notes-icon.svg";

interface BrandLogoProps {
  label?: string;
  size?: "sm" | "md" | "lg";
  showText?: boolean;
  variant?: "default" | "sidebar";
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

export function BrandLogo({
  label = "Starlane Notes",
  size = "md",
  showText = true,
  variant = "default",
}: BrandLogoProps) {
  const classes = sizeClasses[size];
  const sidebar = variant === "sidebar";

  return (
    <div className="flex items-center gap-3">
      <img src={notesIcon} alt="" aria-hidden="true" className={`${classes.icon} shrink-0`} />
      {showText && (
        <div className="min-w-0">
          {sidebar ? (
            <div
              className="truncate leading-none text-text"
              style={{ fontFamily: '"Exo 2", Inter, system-ui, sans-serif' }}
              aria-label={label}
            >
              <div className="text-[10px] font-medium uppercase tracking-normal text-text-muted">
                Starlane
              </div>
              <div className="mt-0.5 text-base font-semibold tracking-normal">Notes</div>
            </div>
          ) : (
            <>
              <div className={`${classes.title} truncate font-semibold text-text`}>{label}</div>
              <div className={`${classes.subtitle} truncate text-text-muted`}>
                Open-source notes workspace
              </div>
            </>
          )}
        </div>
      )}
    </div>
  );
}
