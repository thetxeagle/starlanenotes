import { useEffect } from "react";
import { useTheme } from "@/hooks/useTheme";
import { TOGGLE_THEME_EVENT } from "@/lib/themeEvents";

/** Keeps keyboard and native-menu theme toggles on the same persisted theme owner. */
export function ThemeEventHandler() {
  const { resolvedTheme, setTheme } = useTheme();

  useEffect(() => {
    const toggleTheme = () => setTheme(resolvedTheme === "dark" ? "light" : "dark");
    document.addEventListener(TOGGLE_THEME_EVENT, toggleTheme);
    return () => document.removeEventListener(TOGGLE_THEME_EVENT, toggleTheme);
  }, [resolvedTheme, setTheme]);

  return null;
}
