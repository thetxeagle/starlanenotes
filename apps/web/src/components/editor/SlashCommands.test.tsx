import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  doesSlashCommandMatchQuery,
  formatSmartDate,
  getSlashCommandOptionId,
  getSlashCommandCatalog,
  getSlashCommandGroup,
  getSlashCommandPaletteKeyboardAction,
  getSlashCommandPaletteScopedKeyboardAction,
  SlashCommandPalette,
} from "./SlashCommands";

describe("SlashCommandPalette", () => {
  it("formats smart dates as an unambiguous local calendar date", () => {
    expect(formatSmartDate(new Date(2026, 6, 12))).toBe("2026-07-12");
    expect(formatSmartDate(new Date(2026, 0, 3))).toBe("2026-01-03");
  });

  it("exposes one reusable catalog for slash and searchable block insertion", () => {
    const catalog = getSlashCommandCatalog([], {});
    const commandIds = catalog.map((command) => command.id);

    expect(commandIds).toContain("paragraph");
    expect(commandIds).toContain("collection");
    expect(commandIds).not.toContain("toc");
    expect(catalog.find((command) => command.id === "carousel")).toMatchObject({
      label: "Carousel",
    });
    expect(catalog.find((command) => command.id === "callout")).toMatchObject({
      action: "> [!NOTE]\n> ",
    });
    expect(catalog.find((command) => command.id === "math")).toMatchObject({
      action: "$$\n\n$$",
    });
    expect(catalog.filter((command) => command.columnLayout)).toMatchObject([
      { id: "columns:two-equal", columnLayout: "two-equal" },
      { id: "columns:three-equal", columnLayout: "three-equal" },
      { id: "columns:wide-left", columnLayout: "wide-left" },
      { id: "columns:wide-right", columnLayout: "wide-right" },
    ]);
    expect(commandIds).toContain("dynamic:date");
    expect(commandIds).toContain("template");
    expect(catalog.find((command) => command.id === "template")).toMatchObject({
      label: "Browse reusable sections",
    });
  });

  it("groups portable commands without changing their shared catalog", () => {
    expect(getSlashCommandGroup("paragraph")).toBe("Basic blocks");
    expect(getSlashCommandGroup("bullet")).toBe("Lists & tables");
    expect(getSlashCommandGroup("toc")).toBe("Knowledge blocks");
    expect(getSlashCommandGroup("columns:wide-left")).toBe("Knowledge blocks");
    expect(getSlashCommandGroup("upload")).toBe("Media & embeds");
    expect(getSlashCommandGroup("wiki-link")).toBe("Links & people");
    expect(getSlashCommandGroup("dynamic:date")).toBe("Utilities");
    expect(getSlashCommandGroup("template:weekly")).toBe("Templates");
  });

  it("searches the shared portable catalog by documented aliases as well as labels", () => {
    const catalog = getSlashCommandCatalog([], {});
    const command = (id: string) => catalog.find((candidate) => candidate.id === id)!;

    expect(doesSlashCommandMatchQuery(command("task"), "/todo")).toBe(true);
    expect(doesSlashCommandMatchQuery(command("collection"), "dataview")).toBe(true);
    expect(doesSlashCommandMatchQuery(command("columns:two-equal"), "/columns")).toBe(true);
    expect(doesSlashCommandMatchQuery(command("columns:three-equal"), "three columns")).toBe(true);
    expect(doesSlashCommandMatchQuery(command("columns:wide-left"), "wide left")).toBe(true);
    expect(doesSlashCommandMatchQuery(command("columns:wide-right"), "wide right")).toBe(true);
    expect(doesSlashCommandMatchQuery(command("task"), "equation")).toBe(false);
  });

  it("leaves lifecycle-owned keyboard dismissal to the focus-restoring overlay", () => {
    expect(getSlashCommandPaletteKeyboardAction("ArrowDown", false)).toBe("next");
    expect(getSlashCommandPaletteKeyboardAction("ArrowUp", false)).toBe("previous");
    expect(getSlashCommandPaletteKeyboardAction("Enter", false)).toBe("select");
    expect(getSlashCommandPaletteKeyboardAction("Escape", false)).toBe("close");
    expect(getSlashCommandPaletteKeyboardAction("Escape", true)).toBe("none");
    expect(getSlashCommandPaletteKeyboardAction("x", false)).toBe("none");
  });

  it("routes arrows and Enter only from the active palette or its editor, before editor keymaps", () => {
    const slashEditorScope = {
      searchable: false,
      targetInPalette: false,
      targetInTrigger: true,
      targetInOtherDialog: false,
    };
    expect(getSlashCommandPaletteScopedKeyboardAction("ArrowDown", false, slashEditorScope)).toBe(
      "next",
    );
    expect(getSlashCommandPaletteScopedKeyboardAction("Enter", false, slashEditorScope)).toBe(
      "select",
    );
    expect(getSlashCommandPaletteScopedKeyboardAction("ArrowDown", true, slashEditorScope)).toBe(
      "none",
    );

    expect(
      getSlashCommandPaletteScopedKeyboardAction("Enter", false, {
        ...slashEditorScope,
        targetInTrigger: false,
        targetInOtherDialog: true,
      }),
    ).toBe("none");
    expect(
      getSlashCommandPaletteScopedKeyboardAction("ArrowDown", false, {
        ...slashEditorScope,
        targetInTrigger: false,
      }),
    ).toBe("none");
    expect(
      getSlashCommandPaletteScopedKeyboardAction("Enter", false, {
        searchable: true,
        targetInPalette: true,
        targetInTrigger: false,
        targetInOtherDialog: false,
      }),
    ).toBe("select");
  });

  it("gives each active command a stable assistive-technology option id", () => {
    expect(getSlashCommandOptionId("task")).toBe("slash-command-option-task");
    expect(getSlashCommandOptionId("dynamic:date")).toBe("slash-command-option-dynamic-date");
    expect(getSlashCommandOptionId("template:weekly/notes")).toBe(
      "slash-command-option-template-weekly-notes",
    );
  });

  it("uses the shared search textbox when searchable", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query=""
        position={{ top: 24, left: 32 }}
        searchable
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain('aria-label="Search commands"');
    expect(html).toContain('aria-controls="slash-command-options"');
    expect(html).toContain('aria-activedescendant="slash-command-option-paragraph"');
    expect(html).toContain('type="search"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("pl-8");
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain('tabindex="-1"');
    expect(html).toContain("Basic blocks");
    expect(html).toContain("Knowledge blocks");
    expect(html).toContain("Media &amp; embeds");
    expect(html).toContain('role="listbox"');
    expect(html).toContain('role="option"');
    expect(html).toContain('aria-selected="true"');
  });

  it("does not advertise an unwired AI slash command", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query=""
        position={{ top: 24, left: 32 }}
        searchable
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).not.toContain("Ask AI to write");
  });

  it("advertises the wired bookmark card command", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query="bookmark"
        position={{ top: 24, left: 32 }}
        searchable
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain("Bookmark card");
    expect(html).toContain("Link preview card");
  });

  it("renders an alias-selected portable command through the same palette", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query="/todo"
        position={{ top: 24, left: 32 }}
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain("Task list");
    expect(html).toContain("Checkbox list");
    expect(html).not.toContain("Table of contents");
  });

  it("advertises the portable collection-view starter", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query="collection"
        position={{ top: 24, left: 32 }}
        searchable
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain("Collection view");
    expect(html).toContain("Saved property-backed page collection");
  });

  it("shows every columns layout before insertion", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query="/columns"
        position={{ top: 24, left: 32 }}
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain("Column layouts");
    expect(html).toContain("Two equal columns");
    expect(html).toContain("Three equal columns");
    expect(html).toContain("Wide left");
    expect(html).toContain("Wide right");
    expect(html).toContain('data-layout-preview="three-equal"');
  });

  it("does not advertise the generated table of contents block", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query="table of contents"
        position={{ top: 24, left: 32 }}
        searchable
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).not.toContain("Table of contents");
  });
});
