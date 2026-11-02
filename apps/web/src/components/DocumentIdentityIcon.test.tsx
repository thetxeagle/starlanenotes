import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { DOCUMENT_IDENTITY_ICON_CLASS, DocumentIdentityIcon } from "./DocumentIdentityIcon";

describe("DocumentIdentityIcon", () => {
  it("puts a stored emoji in the dedicated document icon lane", () => {
    const html = renderToStaticMarkup(<DocumentIdentityIcon icon="📘" />);

    expect(html).toContain("📘");
    expect(html).toContain(DOCUMENT_IDENTITY_ICON_CLASS);
    expect(html).not.toContain("<svg");
  });

  it("uses the ordinary document glyph only when no emoji is stored", () => {
    const html = renderToStaticMarkup(<DocumentIdentityIcon icon="  " />);

    expect(html).toContain("<svg");
    expect(html).not.toContain("📘");
  });

  it("renders a namespaced Lucide icon instead of exposing its stored value", () => {
    const html = renderToStaticMarkup(<DocumentIdentityIcon icon="lucide:book-open" />);

    expect(html).toContain("<svg");
    expect(html).not.toContain("lucide:book-open");
  });
});
