import { describe, expect, it } from "vitest";
import { buildAuthCallbackCleanUrl, mergeAuthUserUpdate, parseAuthCallbackFragment } from "./AuthProvider";

describe("auth callback fragment parsing", () => {
  it("extracts complete OAuth tokens and removes them from the hash", () => {
    const parsed = parseAuthCallbackFragment(
      "#access_token=access.jwt&refresh_token=refresh.jwt&state=kept",
    );

    expect(parsed.hasAuthParams).toBe(true);
    expect(parsed.tokens).toEqual({
      accessToken: "access.jwt",
      refreshToken: "refresh.jwt",
    });
    expect(parsed.sanitizedHash).toBe("#state=kept");
  });

  it("treats partial callback tokens as invalid auth params", () => {
    const parsed = parseAuthCallbackFragment("#access_token=access.jwt&view=home");

    expect(parsed.hasAuthParams).toBe(true);
    expect(parsed.tokens).toBeNull();
    expect(parsed.sanitizedHash).toBe("#view=home");
  });

  it("preserves search params when building the cleaned callback URL", () => {
    expect(
      buildAuthCallbackCleanUrl(
        { pathname: "/documents/doc-1", search: "?mode=preview" },
        "#outline",
      ),
    ).toBe("/documents/doc-1?mode=preview#outline");
  });

  it("merges partial auth user updates without inventing a missing user", () => {
    const currentUser = {
      id: "user-1",
      email: "eagle@example.test",
      display_name: "The Eagle",
      role: "owner",
      totp_enabled: false,
    };

    expect(mergeAuthUserUpdate(currentUser, { totp_enabled: true })).toEqual({
      ...currentUser,
      totp_enabled: true,
    });
    expect(mergeAuthUserUpdate(null, { totp_enabled: true })).toBeNull();
  });
});
