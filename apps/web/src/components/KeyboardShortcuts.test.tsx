import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { getKeyboardShortcutSections, KeyboardShortcuts } from "./KeyboardShortcuts";

describe("getKeyboardShortcutSections", () => {
  it("shows document side panel shortcuts only when the current route can use them", () => {
    const documentShortcuts = getKeyboardShortcutSections(true)
      .flatMap((section) => section.items)
      .map((item) => item.description);
    const nonDocumentShortcuts = getKeyboardShortcutSections(false)
      .flatMap((section) => section.items)
      .map((item) => item.description);

    expect(documentShortcuts).toContain("Toggle page side panel");
    expect(nonDocumentShortcuts).not.toContain("Toggle page side panel");
  });

  it("uses readable category labels instead of decorative eyebrow styling", () => {
    const html = renderToStaticMarkup(
      <KeyboardShortcuts open onClose={() => undefined} />,
    );

    expect(html).toContain("General");
    expect(html).not.toContain("uppercase");
    expect(html).not.toContain("tracking-wide");
  });

  it("uses the shared labelled dialog shell when open", () => {
    const html = renderToStaticMarkup(
      <KeyboardShortcuts open onClose={() => undefined} />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="keyboard-shortcuts-title"');
    expect(html).toContain("Keyboard Shortcuts");
    expect(html).toContain('aria-label="Close keyboard shortcuts"');
  });
});
