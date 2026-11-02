import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("PopoverContent", () => {
  it("renders above editor gutter and drag-drop chrome", () => {
    const source = readFileSync(resolve(process.cwd(), "src/components/ui/popover.tsx"), "utf8");

    expect(source).toContain("z-[70]");
    expect(source).not.toContain("shadow-card z-50");
  });
});
