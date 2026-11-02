import { Monitor, Moon, Sun } from "lucide-react";
import { useTheme, type Theme } from "@/hooks/useTheme";
import { cn } from "@/lib/utils";

export function getNextAppearanceTheme(theme: Theme): Theme {
  if (theme === "light") return "dark";
  if (theme === "dark") return "system";
  return "light";
}

interface AppearanceToggleProps {
  touchTarget?: boolean;
  className?: string;
}

/** Shared three-state appearance control for persistent web chrome. */
export function AppearanceToggle({ touchTarget = false, className }: AppearanceToggleProps) {
  const { theme, setTheme } = useTheme();
  const ThemeIcon = theme === "light" ? Sun : theme === "dark" ? Moon : Monitor;

  return (
    <button
      type="button"
      onClick={() => setTheme(getNextAppearanceTheme(theme))}
      className={cn(
        "rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex items-center justify-center transition-colors duration-100",
        touchTarget ? "h-11 w-11" : "h-7 w-7",
        className,
      )}
      aria-label={`Appearance: ${theme}. Switch appearance`}
      title={`Appearance: ${theme}`}
    >
      <ThemeIcon size={touchTarget ? 20 : 16} aria-hidden="true" />
    </button>
  );
}
