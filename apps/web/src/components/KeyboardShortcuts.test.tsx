import { describe, expect, it } from "vitest";
import { getKeyboardShortcutSections } from "./KeyboardShortcuts";

describe("getKeyboardShortcutSections", () => {
  it("shows document side panel shortcuts only when the current route can use them", () => {
    const documentShortcuts = getKeyboardShortcutSections(true)
      .flatMap((section) => section.items)
      .map((item) => item.description);
    const nonDocumentShortcuts = getKeyboardShortcutSections(false)
      .flatMap((section) => section.items)
      .map((item) => item.description);

    expect(documentShortcuts).toContain("Toggle document side panel");
    expect(nonDocumentShortcuts).not.toContain("Toggle document side panel");
  });
});
