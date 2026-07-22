import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { SetupPage } from "./SetupPage";
import {
  AUTH_ERROR_ALERT_CLASS,
  AUTH_PROVIDER_SETUP_ROWS,
  SIGNUP_POLICY_SETUP_ROWS,
} from "./setupShared";

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    register: vi.fn(),
  }),
}));

describe("SetupPage auth provider setup rows", () => {
  it("distinguishes server Google OAuth from Google Drive provider PKCE setup", () => {
    const google = AUTH_PROVIDER_SETUP_ROWS.find((row) => row.name === "Google OAuth");

    expect(google).toMatchObject({
      vars: "GOOGLE_CLIENT_ID, GOOGLE_CLIENT_SECRET",
      note: expect.stringContaining("Server login only"),
    });
    expect(google?.note).toContain("VITE_GOOGLE_DRIVE_CLIENT_ID");
    expect(google?.note).toContain("must not expose a browser client secret");
  });

  it("uses semantic danger tokens for auth/setup error alerts", () => {
    expect(AUTH_ERROR_ALERT_CLASS).toContain("border-danger");
    expect(AUTH_ERROR_ALERT_CLASS).toContain("bg-danger");
    expect(AUTH_ERROR_ALERT_CLASS).toContain("text-danger");
    expect(AUTH_ERROR_ALERT_CLASS).not.toContain("red-");
  });

  it("keeps first-run progress available at the bottom of the setup page", () => {
    const html = renderToStaticMarkup(<SetupPage />);

    expect(html).toContain('aria-label="Setup progress"');
    expect(html.lastIndexOf('aria-label="Setup progress"')).toBeGreaterThan(
      html.lastIndexOf("Get Started"),
    );
  });

  it("surfaces the setup-only authentication policy choices", () => {
    expect(SIGNUP_POLICY_SETUP_ROWS.map((row) => row.value)).toEqual(["invite", "oidc", "blocked"]);
    expect(SIGNUP_POLICY_SETUP_ROWS.find((row) => row.value === "invite")?.detail).toContain(
      "Block open sign-ups",
    );
    expect(SIGNUP_POLICY_SETUP_ROWS.find((row) => row.value === "oidc")?.detail).toContain(
      "first OIDC login",
    );
  });
});
