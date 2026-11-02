import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("useListboxKeyboardNavigation", () => {
  it("owns standard directional listbox traversal without adding Tab or Escape behavior", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/ui/useListboxKeyboardNavigation.ts"),
      "utf8",
    );

    expect(source).toContain("ArrowDown");
    expect(source).toContain("ArrowUp");
    expect(source).toContain('event.key !== "Home"');
    expect(source).toContain('event.key !== "End"');
    expect(source).toContain("getNextMenuItemIndex");
    expect(source).not.toContain('event.key !== "Tab"');
    expect(source).not.toContain('event.key !== "Escape"');
  });
});
