import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  SLASH_EDITOR_PINNED_VERSION,
  STARLANE_BLOCK_ID_EXCLUSIONS,
  createStarlaneBlockInteractionExtensions,
} from "./slashEditorBridge";

describe("SlashEditor compatibility boundary", () => {
  it("pins the audited package version exactly", () => {
    const packageJson = JSON.parse(
      readFileSync(resolve(process.cwd(), "package.json"), "utf8"),
    ) as { dependencies?: Record<string, string> };

    expect(packageJson.dependencies?.["@slash-editor/core"]).toBe(SLASH_EDITOR_PINNED_VERSION);
    expect(packageJson.dependencies?.["@slash-editor/react"]).toBe(SLASH_EDITOR_PINNED_VERSION);
  });

  it("adopts block identity without importing the competing block kit", () => {
    const extensions = createStarlaneBlockInteractionExtensions();

    expect(extensions.map((extension) => extension.name)).toEqual(["blockId"]);
    expect(extensions[0]?.options.exclude).toEqual(STARLANE_BLOCK_ID_EXCLUSIONS);
  });

  it("keeps Starlane structural wrappers outside upstream block identity", () => {
    const extensions = createStarlaneBlockInteractionExtensions({
      excludeBlockIdTypes: ["legacyLayout"],
    });

    expect(extensions[0]?.options.exclude).toEqual(["section", "sectionColumn", "legacyLayout"]);
  });
});
