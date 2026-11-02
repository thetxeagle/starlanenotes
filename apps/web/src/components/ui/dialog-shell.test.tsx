import * as overlayLifecycle from "./useOverlayFocusLifecycle";
import { renderToStaticMarkup } from "react-dom/server";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";
import {
  DialogBody,
  DialogCloseAction,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogShell,
  DialogTitle,
} from "./dialog-shell";

describe("DialogShell", () => {
  it("uses the persistent caller opener instead of a removed launching palette option", () => {
    const triggerRef = { current: null as HTMLElement | null };
    const lifecycle = vi
      .spyOn(overlayLifecycle, "useOverlayFocusLifecycle")
      .mockReturnValue(() => undefined);
    const close = () => undefined;
    try {
      renderToStaticMarkup(
        <DialogShell labelledBy="persistent-opener" onClose={close} triggerRef={triggerRef}>
          <h2 id="persistent-opener">Upload</h2>
        </DialogShell>,
      );
      expect(lifecycle).toHaveBeenLastCalledWith(
        expect.objectContaining({ open: true, onClose: close, triggerRef }),
      );
      renderToStaticMarkup(
        <DialogShell labelledBy="default-opener" onClose={close}>
          <h2 id="default-opener">Existing caller</h2>
        </DialogShell>,
      );
      expect(lifecycle.mock.calls[lifecycle.mock.calls.length - 1]?.[0].triggerRef).toBeUndefined();
    } finally {
      lifecycle.mockRestore();
    }
  });

  it("provides one tokenized dialog anatomy for headers, content, actions, and close controls", () => {
    const html = renderToStaticMarkup(
      <DialogShell
        labelledBy="shared-dialog-title"
        describedBy="shared-dialog-description"
        onClose={() => undefined}
      >
        <DialogHeader>
          <div>
            <DialogTitle id="shared-dialog-title">Shared dialog</DialogTitle>
            <DialogDescription id="shared-dialog-description">
              Shared supporting copy.
            </DialogDescription>
          </div>
          <DialogCloseAction onClick={() => undefined} />
        </DialogHeader>
        <DialogBody>Dialog content</DialogBody>
        <DialogFooter>
          <button type="button">Save</button>
        </DialogFooter>
      </DialogShell>,
    );

    expect(html).toContain('aria-describedby="shared-dialog-description"');
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
    expect(html).toContain("bg-surface");
    expect(html).toContain('aria-label="Close dialog"');
    expect(html).toContain('title="Close dialog"');
  });

  it("keeps the accessible dialog contract for wide canvas workspaces", () => {
    const html = renderToStaticMarkup(
      <DialogShell labelledBy="canvas-dialog-title" maxWidth="canvas" onClose={() => undefined}>
        <h2 id="canvas-dialog-title">Canvas</h2>
        <button type="button" aria-label="Close canvas dialog">
          Close
        </button>
      </DialogShell>,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="canvas-dialog-title"');
    expect(html).toContain('tabindex="-1"');
    expect(html).toContain("max-w-[96rem]");
    expect(html).toContain("max-h-[calc(100dvh-2rem)]");
    expect(html).toContain("overflow-y-auto");
    expect(html).toContain("overscroll-contain");
    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain('tabindex="-1"');
    expect(html).toContain('aria-label="Close canvas dialog"');
    expect(html).not.toContain('aria-label="Close dialog"');

    const backdrop = html.match(/<button[^>]*aria-hidden="true"[^>]*>/)?.[0];
    expect(backdrop).toContain('tabindex="-1"');
    expect(backdrop).toContain("bg-black/40");
  });

  it("keeps pointer dismissal on the hidden backdrop and exposes the dialog's own close control", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/ui/dialog-shell.tsx"),
      "utf8",
    );
    const lifecycleSource = readFileSync(
      resolve(process.cwd(), "src/components/ui/useOverlayFocusLifecycle.ts"),
      "utf8",
    );

    expect(source).toMatch(
      /aria-hidden="true"\s+tabIndex=\{-1\}[\s\S]*?onClick=\{closeAndRestoreFocus\}/,
    );
    expect(lifecycleSource).toContain('event.key === "Escape"');
    expect(lifecycleSource).toContain("return restoreCapturedTriggerFocus;");
  });

  it("supports a shared full-screen presentation for touch-safe workspace surfaces", () => {
    const html = renderToStaticMarkup(
      <DialogShell
        labelledBy="mobile-search-title"
        presentation="fullscreen"
        onClose={() => undefined}
      >
        <h2 id="mobile-search-title">Search documents</h2>
      </DialogShell>,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain('aria-labelledby="mobile-search-title"');
    expect(html).toContain("z-[60]");
    expect(html).toContain("h-full w-full");
    expect(html).not.toContain('aria-label="Close dialog"');
    expect(html).not.toContain('aria-hidden="true"');
  });

  it("keeps drawer content inside the dynamic mobile viewport and safe area", () => {
    const html = renderToStaticMarkup(
      <DialogShell
        labelledBy="mobile-outline-title"
        presentation="drawer-right"
        onClose={() => undefined}
      >
        <h2 id="mobile-outline-title">On this page</h2>
      </DialogShell>,
    );

    expect(html).toContain("h-[100dvh]");
    expect(html).toContain("max-h-[100dvh]");
    expect(html).toContain("safe-area-inset-top");
    expect(html).toContain("safe-area-inset-bottom");
  });

  it("keeps the focus lifecycle mounted when a caller supplies a new close callback", () => {
    const source = readFileSync(
      resolve(process.cwd(), "src/components/ui/useOverlayFocusLifecycle.ts"),
      "utf8",
    );

    expect(source).toContain("const onCloseRef = useRef(onClose)");
    expect(source).toContain("onCloseRef.current = onClose");
    expect(source).toMatch(
      /const closeAndRestoreFocus = useCallback\(\(\) => \{[\s\S]*?onCloseRef\.current\(\);[\s\S]*?\}, \[containerRef\]\);/,
    );
  });

  it("restores the trigger once through the shared lifecycle on every dismissal path", () => {
    const lifecycleSource = readFileSync(
      resolve(process.cwd(), "src/components/ui/useOverlayFocusLifecycle.ts"),
      "utf8",
    );
    const shellSource = readFileSync(
      resolve(process.cwd(), "src/components/ui/dialog-shell.tsx"),
      "utf8",
    );

    expect(lifecycleSource).toContain("const restoreCapturedTriggerFocus = useCallback");
    expect(lifecycleSource).toContain("return restoreCapturedTriggerFocus;");
    expect(shellSource).not.toContain("triggerRef.current = document.activeElement");
    expect(shellSource).not.toContain("window.requestAnimationFrame");
  });
});
