import { describe, expect, it } from "vitest";
import {
  shouldRedirectToRequiredTwoFactorProfile,
  shouldShowNativeServerSetup,
  shouldUseMobileAppLayout,
} from "./App";

describe("App startup routing", () => {
  it("uses the mobile shell for native apps and phone-width browsers", () => {
    expect(shouldUseMobileAppLayout(true, false)).toBe(true);
    expect(shouldUseMobileAppLayout(false, true)).toBe(true);
    expect(shouldUseMobileAppLayout(false, false)).toBe(false);
  });

  it("does not send hosted web to native server setup when no linked server URL exists", () => {
    expect(
      shouldShowNativeServerSetup({
        launchMode: "server",
        serverReady: false,
        tauriRuntime: false,
      }),
    ).toBe(false);
  });

  it("keeps the missing linked-server URL recovery screen for native shells", () => {
    expect(
      shouldShowNativeServerSetup({
        launchMode: "server",
        serverReady: false,
        tauriRuntime: true,
      }),
    ).toBe(true);
  });

  it("does not show server setup after native server startup is ready", () => {
    expect(
      shouldShowNativeServerSetup({
        launchMode: "server",
        serverReady: true,
        tauriRuntime: true,
      }),
    ).toBe(false);
  });
});

describe("required 2FA route enforcement", () => {
  it("redirects unenrolled users to profile security when the server requires 2FA", () => {
    expect(
      shouldRedirectToRequiredTwoFactorProfile(
        { require_2fa: true },
        { totp_enabled: false },
        "/spaces/space-1/docs/doc-1",
      ),
    ).toBe(true);
  });

  it("keeps required 2FA users on profile enrollment routes", () => {
    expect(
      shouldRedirectToRequiredTwoFactorProfile(
        { require_2fa: true },
        { totp_enabled: false },
        "/profile",
      ),
    ).toBe(false);
    expect(
      shouldRedirectToRequiredTwoFactorProfile(
        { require_2fa: true },
        { totp_enabled: false },
        "/mobile/profile",
      ),
    ).toBe(false);
  });

  it("does not redirect users who already enabled TOTP or servers that do not require it", () => {
    expect(
      shouldRedirectToRequiredTwoFactorProfile(
        { require_2fa: true },
        { totp_enabled: true },
        "/spaces/space-1/docs/doc-1",
      ),
    ).toBe(false);
    expect(
      shouldRedirectToRequiredTwoFactorProfile(
        { require_2fa: false },
        { totp_enabled: false },
        "/spaces/space-1/docs/doc-1",
      ),
    ).toBe(false);
  });
});
