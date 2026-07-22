import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { CoverImage } from "./CoverImage";

describe("CoverImage", () => {
  it("can omit the empty hover placeholder for compact mobile document headers", () => {
    const html = renderToStaticMarkup(
      <CoverImage onUpdate={() => undefined} showEmptyPlaceholder={false} />,
    );

    expect(html).toBe("");
  });

  it("keeps the empty hover placeholder by default for desktop editing", () => {
    const html = renderToStaticMarkup(<CoverImage onUpdate={() => undefined} />);

    expect(html).toContain("group mb-6 space-y-2");
    expect(html).toContain("Add cover");
    expect(html).not.toContain("prompt");
  });
});
