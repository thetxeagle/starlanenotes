import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { MemoryRouter } from "react-router-dom";

import { RESET_PASSWORD_SUCCESS_ALERT_CLASS, ResetPasswordPage } from "./ResetPasswordPage";

describe("ResetPasswordPage", () => {
  it("renders the reset request form without a token", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter initialEntries={["/reset-password"]}>
        <ResetPasswordPage />
      </MemoryRouter>,
    );

    expect(html).toContain("Reset your password");
    expect(html).toContain("Send reset link");
    expect(html).toContain("Back to sign in");
  });

  it("renders the new password form when a token is present", () => {
    const html = renderToStaticMarkup(
      <MemoryRouter initialEntries={["/reset-password?token=test-token"]}>
        <ResetPasswordPage />
      </MemoryRouter>,
    );

    expect(html).toContain("Set a new password");
    expect(html).toContain("Reset token");
    expect(html).toContain("Save new password");
  });

  it("uses semantic success tokens for reset feedback", () => {
    expect(RESET_PASSWORD_SUCCESS_ALERT_CLASS).toContain("border-success");
    expect(RESET_PASSWORD_SUCCESS_ALERT_CLASS).toContain("bg-success");
    expect(RESET_PASSWORD_SUCCESS_ALERT_CLASS).toContain("text-success");
    expect(RESET_PASSWORD_SUCCESS_ALERT_CLASS).not.toMatch(/green-/);
  });
});
