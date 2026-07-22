import { describe, expect, it } from "vitest";
import { createElement } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import {
  EDITOR_DANGER_MENU_ITEM_CLASS,
  EditorUploadWarning,
  formatEditorUploadLimitWarning,
  MOBILE_EDITOR_TOOLBAR_ACTION_CLASS,
  MOBILE_EDITOR_TOOLBAR_BASE_CLASS,
  MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS,
  MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS,
  MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS,
  MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS,
  getMobileKeyboardInsetMeasurement,
  getMobileToolbarPlacement,
  isCommandPaletteShortcut,
  markdownToHtml,
  serializeBookmarkCardDirective,
  serializeExcalidrawDirective,
} from "./Editor";

describe("markdownToHtml", () => {
  it("uses semantic danger tokens for editor destructive context actions", () => {
    expect(EDITOR_DANGER_MENU_ITEM_CLASS).toContain("text-danger");
    expect(EDITOR_DANGER_MENU_ITEM_CLASS).toContain("hover:bg-danger");
    expect(EDITOR_DANGER_MENU_ITEM_CLASS).not.toContain("red-");
  });

  it("formats upload-limit warnings for in-editor feedback", () => {
    expect(formatEditorUploadLimitWarning("archive.zip", 25)).toBe(
      "archive.zip is larger than the current 25 MB upload limit.",
    );
  });

  it("renders upload-limit warnings as Starlane in-editor feedback", () => {
    const html = renderToStaticMarkup(
      createElement(EditorUploadWarning, {
        message: "archive.zip is larger than the current 25 MB upload limit.",
        onDismiss: () => undefined,
      }),
    );

    expect(html).toContain('role="alert"');
    expect(html).toContain("border-warning");
    expect(html).toContain("bg-warning");
    expect(html).toContain("text-warning");
    expect(html).toContain('aria-label="Dismiss upload warning"');
    expect(html).not.toMatch(/alert\(|window\.alert/);
  });

  it("preserves fenced code blocks with blank lines and indentation", () => {
    const html = markdownToHtml(
      [
        "```powershell",
        "function Test-Thing {",
        "  if ($true) {",
        "    Write-Host \"ok\"",
        "  }",
        "",
        "  Write-Host \"done\"",
        "}",
        "```",
      ].join("\n"),
    );

    expect(html).toContain('<code class="language-powershell">');
    const codeText = html.replace(/<[^>]+>/g, "");
    expect(codeText).toContain("  if ($true) {");
    expect(codeText).toContain("\n\n  Write-Host");
  });

  it("supports hyphenated code fence language names", () => {
    const html = markdownToHtml("```objective-c\n#import <Foundation/Foundation.h>\n```");

    expect(html).toContain('class="language-objective-c"');
    expect(html).toContain("#import &lt;Foundation/Foundation.h&gt;");
  });

  it("parses images before links", () => {
    const html = markdownToHtml("![Alt text](starlane://asset/logo.png)");

    expect(html).toContain('<img src="starlane://asset/logo.png" alt="Alt text"');
    expect(html).not.toContain("<a href=");
  });

  it("parses inline markdown links", () => {
    const html = markdownToHtml("Read [the docs](https://example.com/docs).");

    expect(html).toContain('<a href="https://example.com/docs"');
    expect(html).toContain(">the docs</a>");
  });

  it("renders hostile HTML and unsafe URLs as inert text", () => {
    const html = markdownToHtml(
      '<script>alert(1)</script> [bad](javascript:alert(1)) ![bad](data:text/html;base64,PHNjcmlwdD4)',
    );

    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).not.toContain("<script>");
    expect(html).not.toContain('href="javascript:');
    expect(html).not.toContain('src="data:text/html');
  });

  it("keeps approved links and local image data usable", () => {
    const html = markdownToHtml(
      '[mail](mailto:hello@example.com) ![pixel](data:image/png;base64,aGVsbG8=)',
    );

    expect(html).toContain('href="mailto:hello@example.com"');
    expect(html).toContain('src="data:image/png;base64,aGVsbG8="');
  });

  it("parses a persisted bookmark directive as a bookmark card", () => {
    const html = markdownToHtml("[bookmark:Starlane Notes](https://starlane.example)");

    expect(html).toContain('data-bookmark-card="true"');
    expect(html).toContain('href="https://starlane.example"');
    expect(html).toContain('title="Starlane Notes"');
  });

  it("preserves an optional bookmark description in the portable directive", () => {
    const html = markdownToHtml(
      '[bookmark:Tea CLI](https://gitea.com "The official command line client for Gitea.")',
    );

    expect(html).toContain('description="The official command line client for Gitea."');
    expect(
      serializeBookmarkCardDirective(
        "Tea CLI",
        "https://gitea.com",
        "The official command line client for Gitea.",
      ),
    ).toBe('[bookmark:Tea CLI](https://gitea.com "The official command line client for Gitea.")');
  });

  it("serializes bookmark cards back to the portable directive", () => {
    expect(
      serializeBookmarkCardDirective("Starlane Docs", "https://starlane.example/docs"),
    ).toBe("[bookmark:Starlane Docs](https://starlane.example/docs)");
    expect(serializeBookmarkCardDirective(null, null)).toBe("[bookmark:Bookmark]()");
  });

  it("parses a persisted Excalidraw directive as a canvas placeholder", () => {
    const html = markdownToHtml("![[excalidraw:architecture.excalidraw]]");

    expect(html).toContain('data-excalidraw-canvas="true"');
    expect(html).toContain('filename="architecture.excalidraw"');
    expect(html).toContain('aria-label="Open drawing architecture.excalidraw"');
    expect(html).toContain('data-excalidraw-thumbnail="true"');
    expect(html).toContain("aspect-video");
    expect(html).not.toContain("Open drawing</button>");
  });

  it("renders an intentional empty Excalidraw thumbnail state instead of a blank panel", () => {
    const html = markdownToHtml("![[excalidraw:empty-map.excalidraw]]");

    expect(html).toContain('data-excalidraw-empty-preview="true"');
    expect(html).toContain("No thumbnail yet. Open drawing to edit.");
    expect(html).toContain("bg-surface-raised");
    expect(html).toContain("text-text-muted");
  });

  it("serializes Excalidraw canvases back to the portable directive", () => {
    expect(serializeExcalidrawDirective("architecture.excalidraw")).toBe(
      "![[excalidraw:architecture.excalidraw]]",
    );
    expect(serializeExcalidrawDirective("sketch")).toBe("![[excalidraw:sketch.excalidraw]]");
    expect(serializeExcalidrawDirective(null)).toBe("![[excalidraw:untitled.excalidraw]]");
  });

  it("normalizes unsafe Excalidraw filenames for portable sidecar files", () => {
    expect(serializeExcalidrawDirective("../system:map")).toBe(
      "![[excalidraw:system-map.excalidraw]]",
    );
    expect(serializeExcalidrawDirective("...")).toBe("![[excalidraw:untitled.excalidraw]]");
  });

  it("round-trips persisted Excalidraw scene JSON beside the portable directive", () => {
    const scene = {
      elements: [
        {
          id: "shape-1",
          type: "rectangle",
          x: 10,
          y: 20,
          width: 240,
          height: 120,
          strokeColor: "#58a6ff",
        },
      ],
      appState: {
        viewBackgroundColor: "#ffffff",
        collaborators: { should: "not persist" },
      },
      files: {},
    };
    const encoded = encodeURIComponent(JSON.stringify(scene));
    const markdown = serializeExcalidrawDirective("diagram.excalidraw", encoded);

    expect(markdown).toContain("![[excalidraw:diagram.excalidraw]]");
    expect(markdown).toContain("```excalidraw-json");
    expect(markdown).toContain('"shape-1"');
    expect(markdown).toContain('"viewBackgroundColor": "#ffffff"');
    expect(markdown).not.toContain("collaborators");

    const html = markdownToHtml(markdown);
    expect(html).toContain('data-scene="');
    expect(html).toContain('filename="diagram.excalidraw"');
    expect(html).toContain('data-excalidraw-preview="true"');
    expect(html).toContain("<rect");
  });

  it("parses inline formatting without swallowing following text", () => {
    const html = markdownToHtml("**Bold** plain `code`");

    expect(html).toContain("<strong>Bold</strong> plain <code>code</code>");
  });

  it("preserves nested unordered lists", () => {
    const html = markdownToHtml(["- Parent", "  - Child", "    - Grandchild", "- Second"].join("\n"));

    expect(html).toContain("<ul><li><p>Parent</p><ul><li><p>Child</p>");
    expect(html).toContain("<ul><li><p>Grandchild</p></li></ul>");
    expect(html).toContain("<li><p>Second</p></li>");
  });

  it("preserves nested task lists", () => {
    const html = markdownToHtml(["- [ ] Parent task", "  - [x] Child task"].join("\n"));

    expect(html).toContain('<ul data-type="taskList">');
    expect(html).toContain('<li data-type="taskItem" data-checked="false">');
    expect(html).toContain('<li data-type="taskItem" data-checked="true">');
    expect(html).toContain("<span>Child task</span>");
  });
});

describe("isCommandPaletteShortcut", () => {
  const shortcut = (
    key: string,
    code: string,
    modifiers: Partial<Pick<KeyboardEvent, "altKey" | "ctrlKey" | "metaKey">> = {},
  ) => ({
    altKey: false,
    code,
    ctrlKey: true,
    key,
    metaKey: false,
    ...modifiers,
  });

  it("accepts Ctrl plus the slash key across browser key variants", () => {
    expect(isCommandPaletteShortcut(shortcut("/", "Slash"))).toBe(true);
    expect(isCommandPaletteShortcut(shortcut("?", "Slash"))).toBe(true);
    expect(isCommandPaletteShortcut(shortcut("Divide", "NumpadDivide"))).toBe(true);
  });

  it("rejects non-Ctrl or non-slash shortcuts", () => {
    expect(isCommandPaletteShortcut(shortcut("/", "Slash", { ctrlKey: false }))).toBe(false);
    expect(isCommandPaletteShortcut(shortcut("/", "Slash", { metaKey: true }))).toBe(false);
    expect(isCommandPaletteShortcut(shortcut("/", "Slash", { altKey: true }))).toBe(false);
    expect(isCommandPaletteShortcut(shortcut("k", "KeyK"))).toBe(false);
  });
});

describe("getMobileKeyboardInsetMeasurement", () => {
  it("measures keyboard overlap from the visual viewport", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 844, visualHeight: 520, visualTop: 0, visualWidth: 390 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(324);
    expect(result.state).toEqual({ baseline: 844, width: 390 });
  });

  it("accounts for visual viewport scroll offset", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 844, visualHeight: 520, visualTop: 44, visualWidth: 390 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(280);
  });

  it("treats small viewport differences as a closed keyboard", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 844, visualHeight: 812, visualTop: 0, visualWidth: 390 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(0);
  });

  it("resets the baseline after a large width change", () => {
    const result = getMobileKeyboardInsetMeasurement(
      { layoutHeight: 430, visualHeight: 430, visualTop: 0, visualWidth: 844 },
      { baseline: 844, width: 390 },
    );

    expect(result.inset).toBe(0);
    expect(result.state).toEqual({ baseline: 430, width: 844 });
  });

  it("places the mobile toolbar by keyboard overlap when the keyboard is open", () => {
    expect(getMobileToolbarPlacement(280)).toEqual({
      keyboardOpen: true,
      toolbarBottom: "280px",
      menuBottom: "calc(280px + 3.25rem)",
      reason: "keyboard-overlap",
    });
  });

  it("falls back to safe-area placement when the keyboard is closed", () => {
    expect(getMobileToolbarPlacement(0)).toEqual({
      keyboardOpen: false,
      toolbarBottom: "max(0.75rem, calc(env(safe-area-inset-bottom) + 0.75rem))",
      menuBottom: "max(4.75rem, calc(env(safe-area-inset-bottom) + 4.75rem))",
      reason: "safe-area",
    });
  });

  it("keeps the mobile editor toolbar keyboard-safe and horizontally scrollable", () => {
    expect(MOBILE_EDITOR_TOOLBAR_BASE_CLASS).toContain("mobile-editor-toolbar");
    expect(MOBILE_EDITOR_TOOLBAR_BASE_CLASS).toContain("fixed");
    expect(MOBILE_EDITOR_TOOLBAR_BASE_CLASS).toContain("transition-[bottom]");
    expect(MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS).toContain("left-0");
    expect(MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS).toContain("right-0");
    expect(MOBILE_EDITOR_TOOLBAR_KEYBOARD_OPEN_CLASS).toContain("bg-surface");
    expect(MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS).toContain("left-3");
    expect(MOBILE_EDITOR_TOOLBAR_SAFE_AREA_CLASS).toContain("right-3");
    expect(MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS).toContain("h-11");
    expect(MOBILE_EDITOR_TOOLBAR_SCROLL_CLASS).toContain("overflow-x-auto");
    expect(MOBILE_EDITOR_TOOLBAR_ACTION_CLASS).toContain("h-9");
    expect(MOBILE_EDITOR_TOOLBAR_ACTION_CLASS).toContain("w-9");
    expect(MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS).toContain("h-11");
    expect(MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS).toContain("w-11");
    expect(MOBILE_EDITOR_TOOLBAR_CHROME_BUTTON_CLASS).toContain("shrink-0");
  });
});
