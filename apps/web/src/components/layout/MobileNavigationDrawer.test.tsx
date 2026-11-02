import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  MobileNavigationDrawer,
  MOBILE_NAVIGATION_DRAWER_PANEL_CLASS,
} from "./MobileNavigationDrawer";

describe("MobileNavigationDrawer", () => {
  it("keeps web and native mobile navigation on one labelled, touch-safe drawer contract", () => {
    const html = renderToStaticMarkup(
      <MobileNavigationDrawer open onClose={() => undefined}>
        <nav>Files</nav>
      </MobileNavigationDrawer>,
    );

    expect(MOBILE_NAVIGATION_DRAWER_PANEL_CLASS).toContain("w-[min(19rem,86vw)]");
    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="mobile-navigation-title"');
    expect(html).toContain('tabindex="-1"');
    expect(html).toContain("Navigation");
    expect(html.match(/aria-label="Close navigation"/g)).toHaveLength(2);
    expect(html).toContain("safe-area-inset-top");
    expect(html).toContain("h-11 w-11");
    expect(html).toContain("Files");
  });

  it("does not mount a closed drawer", () => {
    const html = renderToStaticMarkup(
      <MobileNavigationDrawer open={false} onClose={() => undefined}>
        <nav>Files</nav>
      </MobileNavigationDrawer>,
    );

    expect(html).toBe("");
  });

  it("suspends its modal surface while a Files creation dialog owns focus", () => {
    const html = renderToStaticMarkup(
      <MobileNavigationDrawer open suspended onClose={() => undefined}>
        <nav>Files</nav>
      </MobileNavigationDrawer>,
    );

    expect(html).toContain('aria-hidden="true"');
    expect(html).not.toContain('aria-modal="true"');
    expect(html).toContain("invisible");
  });
});
