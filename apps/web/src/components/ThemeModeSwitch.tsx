import { Moon, Monitor, Sun } from "lucide-react";
import { useTheme, type Theme } from "@/hooks/useTheme";
import { cn } from "@/lib/utils";
import {
  CONTROL_CHROME_ACTION_CLASS,
  CONTROL_CHROME_ACTIVE_ACTION_CLASS,
  CONTROL_CHROME_RAIL_CLASS,
} from "@/components/ui/control-chrome";

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
        CONTROL_CHROME_RAIL_CLASS,
        "inline-flex text-xs",
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
              CONTROL_CHROME_ACTION_CLASS,
              "inline-flex h-7 items-center gap-1 px-2",
              active && CONTROL_CHROME_ACTIVE_ACTION_CLASS,
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
