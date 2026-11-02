import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { ApiKeyCreateDialog } from "./ApiKeyCreateDialog";

describe("ApiKeyCreateDialog", () => {
  it("uses the shared dialog and styled form controls", () => {
    const html = renderToStaticMarkup(
      <ApiKeyCreateDialog
        onCreate={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(html).toContain('role="dialog"');
    expect(html).toContain("Create API key");
    expect(html).toContain("Personal integration");
    expect(html).toContain("Service account");
    expect(html).toContain('placeholder="Production API"');
    expect(html).toContain('type="datetime-local"');
    expect(html).toContain('aria-label="API key scopes"');
    expect(html).toContain("Read");
    expect(html).toContain("Write");
    expect(html).toContain("Admin");
    expect(html).toContain("focus:ring-2");
  });
});
