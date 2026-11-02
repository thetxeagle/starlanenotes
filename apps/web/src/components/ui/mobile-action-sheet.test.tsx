import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { MobileActionSheet, MOBILE_ACTION_SHEET_CLASS } from "./mobile-action-sheet";

describe("MobileActionSheet", () => {
  it("keeps long-press actions in a shared labelled menu with dismissal controls", () => {
    const html = renderToStaticMarkup(
      <MobileActionSheet
        open
        title="Roadmap"
        labelledBy="roadmap-actions"
        onClose={() => undefined}
      >
        <button type="button" role="menuitem">Open</button>
      </MobileActionSheet>,
    );

    expect(MOBILE_ACTION_SHEET_CLASS).toContain("starlane-menu-surface");
    expect(MOBILE_ACTION_SHEET_CLASS).toContain("safe-area-inset-bottom");
    expect(html).toContain('role="menu"');
    expect(html).toContain('aria-labelledby="roadmap-actions"');
    expect(html).toContain('tabindex="-1"');
    expect(html).toContain('aria-label="Close Roadmap actions"');
    expect(html).toContain("Open");
    expect(html).toContain("Cancel");
  });

  it("does not mount a closed action sheet", () => {
    const html = renderToStaticMarkup(
      <MobileActionSheet
        open={false}
        title="Roadmap"
        labelledBy="roadmap-actions"
        onClose={() => undefined}
      >
        <button type="button">Open</button>
      </MobileActionSheet>,
    );

    expect(html).toBe("");
  });

  it("uses the shared directional keyboard-navigation primitive", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/ui/mobile-action-sheet.tsx"),
      "utf8",
    );

    expect(source).toContain("useMenuKeyboardNavigation");
    expect(source).toContain("const handleMenuKeyDown");
    expect(source).toContain("onKeyDown={handleMenuKeyDown}");
  });
});
