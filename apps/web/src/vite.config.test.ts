import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("Vite build asset policy", () => {
  const configSource = readFileSync(resolve(process.cwd(), "vite.config.ts"), "utf8");

  it("keeps font files on same-origin asset paths without disabling ordinary inlining", () => {
    expect(configSource).toContain("assetsInlineLimit: shouldInlineBuildAsset");
    expect(configSource).toContain("/\\.(?:woff2?|ttf|otf)$/i.test(filePath)");
    expect(configSource).toContain("? false : undefined");
  });
});
