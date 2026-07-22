import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { ConfirmDialog, getConfirmDialogConfirmVariant } from "./confirm-dialog";

describe("ConfirmDialog", () => {
  it("uses the shared danger button variant for destructive confirmations", () => {
    expect(getConfirmDialogConfirmVariant(true)).toBe("danger");
    expect(getConfirmDialogConfirmVariant(false)).toBe("primary");

    const html = renderToStaticMarkup(
      <ConfirmDialog
        labelledBy="confirm-title"
        title="Delete page"
        message="Delete page?"
        confirmLabel="Delete"
        destructive
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain('aria-labelledby="confirm-title"');
    expect(html).toContain("Delete page");
    expect(html).toContain("Delete page?");
    expect(html).toContain("bg-danger");
    expect(html).not.toContain("red-");
  });
});
