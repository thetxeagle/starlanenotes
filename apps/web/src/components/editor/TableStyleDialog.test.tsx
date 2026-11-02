import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { TableStyleDialog } from "./TableStyleDialog";

describe("TableStyleDialog", () => {
  it("uses the shared large-dialog anatomy without changing table-style choices", () => {
    const html = renderToStaticMarkup(
      <TableStyleDialog
        initialStyle="directory"
        onApply={() => undefined}
        onClose={() => undefined}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="table-style-dialog-title"');
    expect(html).toContain('aria-describedby="table-style-dialog-description"');
    expect(html).toContain('aria-label="Close table style"');
    expect(html).toContain('aria-label="Table style options"');
    expect(html).toContain("Apply style");
    expect(html).toContain("border-b");
    expect(html).toContain("border-t");
  });
});
