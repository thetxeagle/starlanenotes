import { describe, expect, it } from "vitest";
import {
  GALLERY_LAYOUTS,
  MAX_CAROUSEL_IMAGES,
  normalizeGalleryLayout,
  parseGalleryDirective,
  serializeGalleryDirective,
} from "./GalleryBlock";

describe("GalleryBlock", () => {
  it("supports the responsive, masonry, accordion, coverflow, thumbnail, carousel, and timeline layouts", () => {
    expect(GALLERY_LAYOUTS).toEqual([
      "responsive-columns",
      "masonry",
      "accordion",
      "coverflow",
      "thumbnail-strip",
      "carousel",
      "timeline",
    ]);
    expect(normalizeGalleryLayout("unknown")).toBe("responsive-columns");
  });

  it("round trips a gallery and caps it at nine safe images", () => {
    const items = Array.from({ length: MAX_CAROUSEL_IMAGES + 1 }, (_, index) => ({
      src: `https://example.com/${index}.png`,
      alt: `Photo ${index + 1}`,
    }));
    const parsed = parseGalleryDirective(serializeGalleryDirective("Trip", "masonry", items));
    expect(parsed?.title).toBe("Trip");
    expect(parsed?.layout).toBe("masonry");
    expect(parsed?.items).toHaveLength(MAX_CAROUSEL_IMAGES);
  });
});
