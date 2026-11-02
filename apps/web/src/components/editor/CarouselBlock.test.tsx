import { describe, expect, it } from "vitest";
import {
  MAX_CAROUSEL_IMAGES,
  normalizeCarouselItems,
  parseCarouselDirective,
  serializeCarouselDirective,
} from "./CarouselBlock";

describe("CarouselBlock", () => {
  it("round trips a portable carousel directive and caps it at nine images", () => {
    const items = Array.from({ length: MAX_CAROUSEL_IMAGES + 2 }, (_, index) => ({
      src: `https://example.com/${index}.jpg`,
      alt: `Image ${index + 1}`,
    }));
    const source = serializeCarouselDirective("Screenshots", items);
    const parsed = parseCarouselDirective(source);

    expect(parsed?.title).toBe("Screenshots");
    expect(parsed?.items).toHaveLength(MAX_CAROUSEL_IMAGES);
    expect(parsed?.items[8]?.src).toBe("https://example.com/8.jpg");
    expect(normalizeCarouselItems(JSON.stringify(items))).toHaveLength(MAX_CAROUSEL_IMAGES);
  });

  it("rejects unsafe image sources", () => {
    const parsed = parseCarouselDirective(
      [
        "```carousel",
        "title: Safe",
        "---",
        "![Good](https://example.com/good.jpg)",
        "![Bad](javascript:alert(1))",
        "```",
      ].join("\n"),
    );
    expect(parsed?.items).toEqual([{ src: "https://example.com/good.jpg", alt: "Good" }]);

  });
});
