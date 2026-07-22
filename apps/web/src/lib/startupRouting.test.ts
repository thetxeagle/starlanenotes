import { describe, expect, it } from "vitest";
import { shouldResolveDesktopStartupSpaceFallback } from "./startupRouting";

describe("startupRouting", () => {
  it("resolves the desktop space fallback only after document restore is exhausted", () => {
    expect(
      shouldResolveDesktopStartupSpaceFallback({
        pathname: "/",
        isMobileRuntime: false,
        checkingDocumentRestore: false,
        documentRestorePath: null,
      }),
    ).toBe(true);

    expect(
      shouldResolveDesktopStartupSpaceFallback({
        pathname: "/",
        isMobileRuntime: false,
        checkingDocumentRestore: true,
        documentRestorePath: null,
      }),
    ).toBe(false);

    expect(
      shouldResolveDesktopStartupSpaceFallback({
        pathname: "/",
        isMobileRuntime: false,
        checkingDocumentRestore: false,
        documentRestorePath: "/spaces/space-1/docs/doc-1",
      }),
    ).toBe(false);
  });

  it("does not run the desktop fallback on mobile or non-root routes", () => {
    expect(
      shouldResolveDesktopStartupSpaceFallback({
        pathname: "/",
        isMobileRuntime: true,
        checkingDocumentRestore: false,
        documentRestorePath: null,
      }),
    ).toBe(false);

    expect(
      shouldResolveDesktopStartupSpaceFallback({
        pathname: "/spaces/space-1/docs",
        isMobileRuntime: false,
        checkingDocumentRestore: false,
        documentRestorePath: null,
      }),
    ).toBe(false);
  });
});
