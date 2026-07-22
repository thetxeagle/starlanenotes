import { Moon, Monitor, Sun } from "lucide-react";
import { useTheme, type Theme } from "@/hooks/useTheme";
import { cn } from "@/lib/utils";

const options: Array<{ value: Theme; label: string; icon: typeof Sun }> = [
  { value: "light", label: "Light", icon: Sun },
  { value: "system", label: "System", icon: Monitor },
  { value: "dark", label: "Dark", icon: Moon },
];

interface ThemeModeSwitchProps {
  className?: string;
}

export function ThemeModeSwitch({ className }: ThemeModeSwitchProps) {
  const { theme, setTheme } = useTheme();

  return (
    <div
      className={cn(
        "inline-flex items-center gap-1 rounded-card border border-border bg-surface-raised p-1 text-xs text-text",
        className,
      )}
      role="group"
      aria-label="Theme mode"
    >
      {options.map(({ value, label, icon: Icon }) => {
        const active = theme === value;
        return (
          <button
            key={value}
            type="button"
            onClick={() => setTheme(value)}
            aria-pressed={active}
            className={cn(
              "inline-flex h-7 items-center gap-1 rounded-button px-2 transition-colors duration-100",
              active
                ? "bg-surface text-text shadow-card"
                : "text-text hover:bg-surface",
            )}
          >
            <Icon size={13} />
            <span>{label}</span>
          </button>
        );
      })}
    </div>
  );
}
