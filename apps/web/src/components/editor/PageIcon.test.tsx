import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  canClearPageIcon,
  getPageIconUpdateError,
  getPageEmojiResults,
  getPageIconResults,
  PAGE_ICON_UPDATE_ERROR_FALLBACK,
  PAGE_EMOJI_CATALOG,
  PageIcon,
} from "./PageIcon";

describe("PageIcon", () => {
  it("searches the curated page emoji catalog by label and keyword", () => {
    expect(PAGE_EMOJI_CATALOG.length).toBeGreaterThanOrEqual(40);
    expect(getPageEmojiResults("rocket").map((result) => result.emoji)).toContain("🚀");
    expect(getPageEmojiResults("knowledge").map((result) => result.emoji)).toEqual(
      expect.arrayContaining(["📚", "🧠"]),
    );
    expect(getPageEmojiResults("not-in-the-catalog")).toEqual([]);
    expect(canClearPageIcon(" 📝 ")).toBe(true);
    expect(canClearPageIcon("   ")).toBe(false);
  });

  it("offers a unified searchable catalog of emoji and matching Lucide icons", () => {
    expect(getPageIconResults("rocket").map((result) => result.value)).toEqual(
      expect.arrayContaining(["🚀", "lucide:rocket"]),
    );
    expect(getPageIconResults("", "icon").every((result) => result.source === "icon")).toBe(true);
    expect(getPageIconResults("", "emoji").every((result) => result.source === "emoji")).toBe(true);
  });

  it("renders the selected emoji as a title-row picker trigger", () => {
    const html = renderToStaticMarkup(<PageIcon icon="🚀" onUpdate={() => undefined} />);

    expect(html).toContain('aria-label="Change page icon, currently 🚀"');
    expect(html).toContain("🚀");
    expect(html).toContain("size-12");
    expect(html).not.toContain("prompt");
  });

  it("renders a namespaced Lucide icon in the title-row picker trigger", () => {
    const html = renderToStaticMarkup(
      <PageIcon icon="lucide:book-open" onUpdate={() => undefined} />,
    );

    expect(html).toContain('aria-label="Change page icon, currently Book"');
    expect(html).toContain("<svg");
    expect(html).not.toContain("lucide:book-open");
  });

  it("keeps an icon-only square picker affordance visible when a page has no icon yet", () => {
    const html = renderToStaticMarkup(<PageIcon onUpdate={() => undefined} />);

    expect(html).toContain('aria-label="Choose page icon"');
    expect(html).not.toContain("Add icon");
    expect(html).toContain("size-12");
    expect(html).not.toContain("size-12 text-3xl");
  });

  it("turns failed icon saves into useful picker feedback", () => {
    expect(getPageIconUpdateError(new Error("Draft update failed."))).toBe("Draft update failed.");
    expect(getPageIconUpdateError("unknown failure")).toBe(PAGE_ICON_UPDATE_ERROR_FALLBACK);
  });
});
