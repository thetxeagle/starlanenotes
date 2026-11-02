import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CoverImage, validateCoverImageFile } from "./CoverImage";

describe("CoverImage", () => {
  it("can omit the empty hover placeholder for compact mobile document headers", () => {
    const html = renderToStaticMarkup(
      <CoverImage onUpdate={() => undefined} showEmptyPlaceholder={false} />,
    );

    expect(html).toBe("");
  });

  it("keeps the empty hover placeholder by default for desktop editing", () => {
    const html = renderToStaticMarkup(<CoverImage onUpdate={() => undefined} />);

    expect(html).toContain("group mb-2");
    expect(html).toContain("Add cover");
    expect(html).not.toContain("prompt");
  });

  it("renders Page-view covers without add or remove controls", () => {
    const emptyHtml = renderToStaticMarkup(
      <CoverImage editable={false} onUpdate={() => undefined} />,
    );
    const coverHtml = renderToStaticMarkup(
      <CoverImage
        src="https://example.com/cover.jpg"
        editable={false}
        onUpdate={() => undefined}
      />,
    );

    expect(emptyHtml).toBe("");
    expect(coverHtml).toContain('src="https://example.com/cover.jpg"');
    expect(coverHtml).not.toContain("Add cover");
    expect(coverHtml).not.toContain("Remove cover");
  });

  it("offers a compact cover preview for Page Details", () => {
    const html = renderToStaticMarkup(
      <CoverImage src="https://example.com/cover.jpg" compact onUpdate={() => undefined} />,
    );

    expect(html).toContain("h-28");
    expect(html).not.toContain("h-48");
    expect(html).toContain('aria-label="Remove cover"');
    expect(html).toContain("focus-visible:opacity-100");
  });

  it("validates browser-picked cover image formats and size", () => {
    expect(
      validateCoverImageFile(new File(["image"], "cover.png", { type: "image/png" })),
    ).toBeNull();
    expect(validateCoverImageFile(new File(["text"], "cover.txt", { type: "text/plain" }))).toBe(
      "Choose a PNG, JPEG, WebP, or GIF image.",
    );
    expect(
      validateCoverImageFile({
        name: "cover.jpg",
        type: "image/jpeg",
        size: 30 * 1024 * 1024 + 1,
      } as File),
    ).toBe("Cover images must be 30 MB or smaller.");
  });

  it("keeps file upload implementation in the cover dialog source contract", async () => {
    const source = await import("./CoverImage.tsx?raw").then(
      (module) => (module as { default: string }).default,
    );

    expect(source).toContain('type="file"');
    expect(source).toContain('accept="image/png,image/jpeg,image/webp,image/gif"');
    expect(source).toContain("Click to upload or drag an image here");
    expect(source).toContain('labelledBy="cover-image-dialog-title"');
    expect(source).toContain('describedBy="cover-image-dialog-description"');
    expect(source).toContain("<DialogHeader");
    expect(source).toContain("<DialogBody");
    expect(source).toContain("<DialogFooter");
  });
});
