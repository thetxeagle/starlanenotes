import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { LoadingState } from "./loading-state";

describe("LoadingState", () => {
  it("announces its label without exposing decorative skeleton blocks", () => {
    const html = renderToStaticMarkup(<LoadingState label="Loading document" />);

    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain("Loading document");
    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain("motion-reduce:animate-none");
  });

  it("supports full-viewport startup loading", () => {
    const html = renderToStaticMarkup(<LoadingState viewport />);

    expect(html).toContain("app-viewport app-background");
  });
});
