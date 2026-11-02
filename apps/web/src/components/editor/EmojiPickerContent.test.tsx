import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { STARLANE_EMOJI_DATA_URL } from "./EmojiPickerContent";

describe("EmojiPickerContent", () => {
  it("uses the app-bundled EmojiBase catalog instead of a runtime CDN", () => {
    expect(STARLANE_EMOJI_DATA_URL).toBe("/emoji-data");

    const data = JSON.parse(
      readFileSync(resolve(process.cwd(), "public/emoji-data/en/data.json"), "utf8"),
    ) as unknown[];
    const messages = JSON.parse(
      readFileSync(resolve(process.cwd(), "public/emoji-data/en/messages.json"), "utf8"),
    ) as { groups?: Record<string, string> };

    expect(data.length).toBeGreaterThan(1_000);
    expect(Object.keys(messages.groups ?? {}).length).toBeGreaterThan(5);
  });
});
