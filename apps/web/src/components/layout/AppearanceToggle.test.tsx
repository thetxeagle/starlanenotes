import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { AppearanceToggle, getNextAppearanceTheme } from "./AppearanceToggle";

vi.mock("@/hooks/useTheme", () => ({
  useTheme: () => ({ theme: "system", resolvedTheme: "dark", setTheme: vi.fn() }),
}));

describe("AppearanceToggle", () => {
  it("cycles through the complete appearance preference set", () => {
    expect(getNextAppearanceTheme("system")).toBe("light");
    expect(getNextAppearanceTheme("light")).toBe("dark");
    expect(getNextAppearanceTheme("dark")).toBe("system");
  });

  it("names the current preference and preserves a touch-safe variant", () => {
    expect(renderToStaticMarkup(<AppearanceToggle />)).toContain(
      'aria-label="Appearance: system. Switch appearance"',
    );
    expect(renderToStaticMarkup(<AppearanceToggle touchTarget />)).toContain("h-11 w-11");
  });
});
