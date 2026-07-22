import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { formatSmartDate, SlashCommandPalette } from "./SlashCommands";

describe("SlashCommandPalette", () => {
  it("formats smart dates as an unambiguous local calendar date", () => {
    expect(formatSmartDate(new Date(2026, 6, 12))).toBe("2026-07-12");
    expect(formatSmartDate(new Date(2026, 0, 3))).toBe("2026-01-03");
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
    expect(html).toContain('type="search"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("pl-8");
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

  it("advertises the wired Excalidraw canvas command", () => {
    const html = renderToStaticMarkup(
      <SlashCommandPalette
        query="excalidraw"
        position={{ top: 24, left: 32 }}
        searchable
        onSelect={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain("Excalidraw canvas");
    expect(html).toContain("Insert a drawing canvas placeholder");
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
});
