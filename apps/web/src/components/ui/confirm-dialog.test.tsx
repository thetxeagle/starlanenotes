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
    expect(html).toContain('aria-describedby="confirm-title-description"');
    expect(html).toContain('id="confirm-title-description"');
    expect(html).toContain("Delete page");
    expect(html).toContain("Delete page?");
    expect(html).toContain("bg-danger");
    expect(html).toContain("min-h-11");
    expect(html).not.toContain("red-");
  });

  it("keeps a busy-state explanation connected to the disabled destructive action", () => {
    const html = renderToStaticMarkup(
      <ConfirmDialog
        labelledBy="passkey-delete-title"
        title="Delete passkey"
        message="Delete this passkey?"
        confirmLabel="Delete passkey"
        destructive
        disabled
        helperText="Passkey deletion is already in progress."
        onConfirm={vi.fn()}
        onCancel={vi.fn()}
      />,
    );

    expect(html).toContain('id="passkey-delete-title-helper"');
    expect(html).toContain('aria-describedby="passkey-delete-title-description"');
    expect(html).toContain('id="passkey-delete-title-description"');
    expect(html).toContain('aria-describedby="passkey-delete-title-helper"');
    expect(html).toContain("Passkey deletion is already in progress.");
    const cancelButton = html.match(/<button[^>]*>Cancel<\/button>/)?.[0];
    expect(cancelButton).toBeDefined();
    expect(cancelButton).not.toContain(' disabled=""');
    expect(html).toMatch(/<button[^>]*disabled=""[^>]*>Delete passkey<\/button>/);
  });
});
