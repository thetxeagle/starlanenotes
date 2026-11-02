import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { MenuSelect } from "./menu-select";

describe("MenuSelect", () => {
  it("renders the shared anchored listbox trigger and selected option state", () => {
    const html = renderToStaticMarkup(
      <MenuSelect
        value="private"
        options={[
          { value: "private", label: "Private" },
          { value: "public", label: "Public web" },
        ]}
        onChange={() => undefined}
        aria-label="Visibility"
      />,
    );

    expect(html).toContain('aria-haspopup="listbox"');
    expect(html).toContain("bg-surface");
    expect(html).toContain("Private");
  });
});
