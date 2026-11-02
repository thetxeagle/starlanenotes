import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  getSettingsOverlayBackground,
  shouldRedirectToRequiredTwoFactorProfile,
  shouldShowNativeServerSetup,
  shouldUseMobileAppLayout,
} from "./App";
import type { Location } from "react-router-dom";

const workspaceLocation: Location = {
  pathname: "/spaces/space-1/docs/doc-1",
  search: "",
  hash: "",
  state: null,
  key: "workspace",
};

describe("App startup routing", () => {
  it("keeps authenticated route transitions concurrent without warming heavy workspace chunks", () => {
    const source = readFileSync(resolve(process.cwd(), "src/App.tsx"), "utf8");

    expect(source).toContain("v7_startTransition: true");
    expect(source).toContain("<AuthenticatedRoutePreloader />");
    expect(source).toContain("Route chunks intentionally load on navigation");
    expect(source).not.toContain("requestIdleCallback");
    expect(source).not.toContain("preloadAuthenticatedWorkspace");
  });

  it("hosts the existing route tree inside a disposable data router for route blockers", () => {
    const source = readFileSync(resolve(process.cwd(), "src/App.tsx"), "utf8");

    expect(source).toContain("createBrowserRouter(");
    expect(source).toContain('[{ path: "*", element: <AppRoutes /> }]');
    expect(source).toContain("nextRouter.dispose()");
    expect(source).toContain(
      "<RouterProvider router={router} future={{ v7_startTransition: true }} />",
    );
    expect(source).toContain("<Routes location={settingsBackground ?? location}>");
    expect(source).toContain("<AuthProvider mode={mode}>");
  });

  it("chooses the authenticated shell from stable runtime and device signals", () => {
    expect(shouldUseMobileAppLayout(true, false)).toBe(true);
    expect(shouldUseMobileAppLayout(false, true)).toBe(true);
    expect(shouldUseMobileAppLayout(false, false)).toBe(false);
  });

  it("does not bind authenticated shell ownership to the responsive viewport", () => {
    const source = readFileSync(resolve(process.cwd(), "src/App.tsx"), "utf8");

    expect(source).toContain(
      "const { isMobileRuntime, isMobileBrowser, loading: runtimeLoading } = useRuntimePlatform();",
    );
    expect(source).toContain("shouldUseMobileAppLayout(isMobileRuntime, isMobileBrowser)");
  });

  it("keeps the prior workspace beneath desktop settings dialogs", () => {
    const settingsLocation: Location = {
      pathname: "/profile",
      search: "",
      hash: "",
      state: { backgroundLocation: workspaceLocation },
      key: "settings",
    };

    expect(getSettingsOverlayBackground(settingsLocation, false)).toBe(workspaceLocation);
    expect(getSettingsOverlayBackground(settingsLocation, true)).toBeNull();
  });

  it("uses the standalone settings destination for direct links", () => {
    const directLocation: Location = {
      pathname: "/admin",
      search: "",
      hash: "",
      state: null,
      key: "admin",
    };

    expect(getSettingsOverlayBackground(directLocation, false)).toBeNull();
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
