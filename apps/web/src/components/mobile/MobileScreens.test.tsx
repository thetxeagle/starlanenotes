import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import {
  canCreateMobileChildFolder,
  canDisableMobileTotp,
  canRefreshNativeVaultFolders,
  canStartMobileTotpSetup,
  canVerifyMobileTotpSetup,
  flattenFolderChildren,
  getActiveVaultSettingsRowCopy,
  formatMobileActionError,
  getMobileDocumentPath,
  getMobileDocumentUrl,
  getMobilePasskeyDeleteDisabledReason,
  getMobilePasskeyDeleteConfirmationMessage,
  getMobilePasskeyDeleteHelpId,
  getMobilePasskeyDateLabel,
  getMobilePasskeyAddDisabledReason,
  getMobileResetAppConfirmationMessage,
  getMobileSessionRevokeConfirmationMessage,
  getMobileTotpDisableConfirmationMessage,
  getMobileTotpDisableDisabledReason,
  getMobileTotpSetupDisabledReason,
  getMobileTotpVerifyDisabledReason,
  getMobileTotpUserPatch,
  isMobileTotpDisableCodeFormat,
  getMobileVaultTrashConfirmationMessage,
  getMobileJournalPath,
  getMobileFolderDepth,
  getMobilePrimaryCreateActions,
  getMobileDailyDocuments,
  getProfileSettingsFrameClass,
  mergeMobileNotificationPages,
  getProfileSettingsVisibility,
  isDailyDocumentTitle,
  MobileNotificationsScreen,
  MobilePasskeyRow,
  MobileProfileScreen,
  MOBILE_TREE_ICON_SIZE,
  MOBILE_TREE_TOGGLE_LANE_CLASS,
  MOBILE_REQUIRED_2FA_ENROLLMENT_NOTICE_COPY,
  MOBILE_REQUIRED_2FA_POLICY_NOTICE_CLASS,
  MOBILE_TOTP_RECOVERY_ACK_CLASS,
  MOBILE_TOTP_RECOVERY_ACK_COPY,
  MOBILE_TOTP_RECOVERY_ACK_INPUT_CLASS,
  MOBILE_TOTP_RECOVERY_CARD_CLASS,
  MOBILE_TOTP_RECOVERY_WARNING_CLASS,
  MOBILE_TOTP_RECOVERY_WARNING_COPY,
  MOBILE_TOTP_SETUP_ACTION_GRID_CLASS,
  MOBILE_TOTP_SETUP_PANEL_CLASS,
  MOBILE_TOTP_VERIFY_BUTTON_CLASS,
  MOBILE_TOTP_VERIFY_GRID_CLASS,
  SERVER_MODE_VAULT_SWITCH_ROW_COPY,
  MobileTotpSetupPanel,
  MobileTextInput,
  MobilePreferenceSelect,
  ProfilePasskeyUnavailableNotice,
  normalizeMobileServerVaultUrl,
  sessionDeviceLabel,
  shouldShowMobileCreateSpace,
  shouldShowMobileNotifications,
  shouldShowMobileSpaceSwitcher,
  shouldShowServerVaultSwitchInAppSettings,
  shouldShowMobileSiteAdministration,
  isOpenableMobileTotpSetupUri,
  shouldShowProfileOpenFolderRow,
  shouldShowProfileUseAppFolderRow,
  sortMobileDocuments,
  sortMobileFolders,
  sortMobileSpaces,
} from "./MobileScreens";
import { setCurrentLaunchMode } from "@/lib/launchMode";
import type { NotificationRecord } from "@/lib/dataClient";

const runtimeMock = vi.hoisted(() => ({
  runtime: "web",
  loading: false,
  isMobileRuntime: false,
  isDesktopRuntime: false,
}));

const authMock = vi.hoisted(() => ({
  totpEnabled: false,
  require2fa: false,
  role: "owner",
}));

const passkeySupportMock = vi.hoisted(() => ({
  supported: false,
}));

vi.mock("@tauri-apps/plugin-haptics", () => ({
  impactFeedback: vi.fn(),
  notificationFeedback: vi.fn(),
  selectionFeedback: vi.fn(),
  vibrate: vi.fn(),
}));

vi.mock("react-router-dom", () => ({
  useNavigate: () => vi.fn(),
}));

vi.mock("@/hooks/useAuth", () => ({
  useAuth: () => ({
    user: {
      id: "user-1",
      email: "eagle@example.test",
      display_name: "The Eagle",
      role: authMock.role,
      totp_enabled: authMock.totpEnabled,
    },
    config: {
      google_enabled: false,
      github_enabled: false,
      oidc_enabled: false,
      oidc_display_name: "",
      require_2fa: authMock.require2fa,
      allow_public_signups: false,
      setup_completed: true,
      site_name: "Starlane Notes",
      max_upload_size_mb: 30,
    },
    logout: vi.fn(),
    updateCurrentUser: vi.fn(),
    updateProfile: vi.fn(),
  }),
}));

vi.mock("@/hooks/useRuntimePlatform", () => ({
  useRuntimePlatform: () => runtimeMock,
}));

vi.mock("@/lib/passkeys", () => ({
  createPasskeyCredential: vi.fn(),
  isPasskeySupported: () => passkeySupportMock.supported,
}));

function notificationRecord(id: string, read: boolean): NotificationRecord {
  return {
    id,
    notification_type: "comment",
    title: `Notification ${id}`,
    target_type: "document",
    target_id: "doc-1",
    read,
    created_at: "2026-07-14T12:00:00Z",
  };
}

describe("MobileScreens daily note helpers", () => {
  it("builds encoded mobile document routes and share URLs", () => {
    expect(getMobileDocumentPath("space-1", "doc-1")).toBe("/spaces/space-1/docs/doc-1");
    expect(getMobileDocumentPath("space/one", "doc/one")).toBe(
      "/spaces/space%2Fone/docs/doc%2Fone",
    );
    expect(getMobileDocumentUrl("https://notes.example.com", "space/one", "doc/one")).toBe(
      "https://notes.example.com/spaces/space%2Fone/docs/doc%2Fone",
    );
    expect(getMobileJournalPath("space/one")).toBe("/spaces/space%2Fone/journal");
  });

  it("sorts mobile spaces, folders, and pages alphabetically without mutating source lists", () => {
    const spaces = [
      { id: "z", name: "Zeta", slug: "zeta" },
      { id: "a", name: "alpha", slug: "alpha" },
      { id: "m", name: "Middle", slug: "middle" },
    ];
    const folders = [
      { id: "z", name: "Zebra" },
      { id: "a", name: "alpha" },
      { id: "m", name: "Middle" },
    ];
    const documents = [
      { id: "z", title: "Zebra" },
      { id: "a", title: "alpha" },
      { id: "m", title: "Meeting" },
    ];

    expect(sortMobileSpaces(spaces).map((space) => space.id)).toEqual(["a", "m", "z"]);
    expect(sortMobileFolders(folders).map((folder) => folder.id)).toEqual(["a", "m", "z"]);
    expect(sortMobileDocuments(documents).map((doc) => doc.id)).toEqual(["a", "m", "z"]);
    expect(spaces.map((space) => space.id)).toEqual(["z", "a", "m"]);
    expect(folders.map((folder) => folder.id)).toEqual(["z", "a", "m"]);
    expect(documents.map((doc) => doc.id)).toEqual(["z", "a", "m"]);
  });

  it("returns sorted mobile child folders for a parent", () => {
    const folders = [
      { id: "z", name: "Zulu", parent_id: "root" },
      { id: "root", name: "Root" },
      { id: "a", name: "alpha", parent_id: "root" },
      { id: "m", name: "Middle", parent_id: "other" },
    ];

    expect(flattenFolderChildren(folders, "root").map((folder) => folder.id)).toEqual([
      "a",
      "z",
    ]);
    expect(flattenFolderChildren(folders).map((folder) => folder.id)).toEqual(["root"]);
  });

  it("limits mobile child folder creation to the level-3 hierarchy cap", () => {
    const folders = [
      { id: "root", name: "Root" },
      { id: "child", name: "Child", parent_id: "root" },
      { id: "grandchild", name: "Grandchild", parent_id: "child" },
      { id: "cycle-a", name: "Cycle A", parent_id: "cycle-b" },
      { id: "cycle-b", name: "Cycle B", parent_id: "cycle-a" },
    ];

    expect(getMobileFolderDepth(folders, "root")).toBe(1);
    expect(getMobileFolderDepth(folders, "child")).toBe(2);
    expect(getMobileFolderDepth(folders, "grandchild")).toBe(3);
    expect(getMobileFolderDepth(folders, "missing")).toBe(0);
    expect(getMobileFolderDepth(folders, "cycle-a")).toBe(0);

    expect(canCreateMobileChildFolder(folders, "root")).toBe(true);
    expect(canCreateMobileChildFolder(folders, "child")).toBe(true);
    expect(canCreateMobileChildFolder(folders, "grandchild")).toBe(false);
    expect(canCreateMobileChildFolder(folders, "missing")).toBe(false);
  });

  it("accepts only normalized daily-note titles", () => {
    expect(isDailyDocumentTitle("2026-07-05")).toBe(true);
    expect(isDailyDocumentTitle("Daily 2026-07-05")).toBe(false);
    expect(isDailyDocumentTitle("2026-02-31")).toBe(false);
  });

  it("returns recent daily documents from the daily folder first", () => {
    const documents = [
      { id: "a", title: "2026-07-05", folder_id: "daily" },
      { id: "b", title: "2026-07-04", folder_id: "daily" },
      { id: "c", title: "2026-07-06", folder_id: "other" },
      { id: "d", title: "Project note", folder_id: "daily" },
    ];
    const folders = [{ id: "daily", name: "_daily" }];

    expect(getMobileDailyDocuments(documents, folders).map((doc) => doc.id)).toEqual([
      "a",
      "b",
    ]);
  });

  it("keeps server-mode sync and vault switching visible while hiding native-only storage rows", () => {
    expect(getProfileSettingsVisibility("web", false)).toEqual({
      showDeviceStorageControls: false,
      showServerSyncRow: true,
      showServerVaultSwitchRow: true,
      showResetControls: false,
    });

    expect(getProfileSettingsVisibility("tauri-mobile", false)).toEqual({
      showDeviceStorageControls: false,
      showServerSyncRow: true,
      showServerVaultSwitchRow: true,
      showResetControls: true,
    });

    expect(getProfileSettingsVisibility("tauri-mobile", false, false)).toEqual({
      showDeviceStorageControls: false,
      showServerSyncRow: true,
      showServerVaultSwitchRow: true,
      showResetControls: true,
    });

    expect(getProfileSettingsVisibility("tauri-mobile", true)).toEqual({
      showDeviceStorageControls: true,
      showServerSyncRow: false,
      showServerVaultSwitchRow: false,
      showResetControls: true,
    });

    expect(getProfileSettingsVisibility("web", true)).toEqual({
      showDeviceStorageControls: false,
      showServerSyncRow: false,
      showServerVaultSwitchRow: false,
      showResetControls: false,
    });

    expect(getProfileSettingsVisibility("web", false, true)).toEqual({
      showDeviceStorageControls: false,
      showServerSyncRow: true,
      showServerVaultSwitchRow: true,
      showResetControls: true,
    });

    expect(getProfileSettingsVisibility("web", true, true)).toEqual({
      showDeviceStorageControls: true,
      showServerSyncRow: false,
      showServerVaultSwitchRow: false,
      showResetControls: true,
    });
  });

  it("keeps native vault discovery available in local and server modes", () => {
    expect(canRefreshNativeVaultFolders("tauri-mobile")).toBe(true);
    expect(canRefreshNativeVaultFolders("tauri-desktop")).toBe(true);
    expect(canRefreshNativeVaultFolders("web")).toBe(false);
    expect(canRefreshNativeVaultFolders("web", true)).toBe(true);
  });

  it("keeps the mobile folder/page tree chevron lane and icon size uniform", () => {
    expect(MOBILE_TREE_TOGGLE_LANE_CLASS).toContain("w-5");
    expect(MOBILE_TREE_TOGGLE_LANE_CLASS).toContain("shrink-0");
    expect(MOBILE_TREE_ICON_SIZE).toBe(14);
  });

  it("keeps the server-mode vault switch paired with the visible sync row", () => {
    expect(
      shouldShowServerVaultSwitchInAppSettings({
        showServerSyncRow: true,
        showServerVaultSwitchRow: false,
      }),
    ).toBe(true);
    expect(
      shouldShowServerVaultSwitchInAppSettings({
        showServerSyncRow: false,
        showServerVaultSwitchRow: false,
      }),
    ).toBe(false);
  });

  it("uses a wider profile settings frame on web and desktop while preserving mobile sizing", () => {
    expect(getProfileSettingsFrameClass("tauri-mobile", "profile")).toContain("max-w-xl");
    expect(getProfileSettingsFrameClass("web", "profile")).toContain("max-w-3xl");
    expect(getProfileSettingsFrameClass("tauri-desktop", "profile")).toContain("max-w-3xl");
    expect(getProfileSettingsFrameClass("web", "profile")).toContain("sm:px-6");
  });

  it("keeps mobile TOTP action buttons gated by setup state, code entry, and busy actions", () => {
    expect(canStartMobileTotpSetup(false, "", null)).toBe(true);
    expect(canStartMobileTotpSetup(true, "", null)).toBe(false);
    expect(canStartMobileTotpSetup(false, "otpauth://totp/Starlane", null)).toBe(false);
    expect(canStartMobileTotpSetup(false, "", "totp-setup")).toBe(false);
    expect(canStartMobileTotpSetup(false, "", "passkey-add")).toBe(false);

    expect(getMobileTotpSetupDisabledReason(true, "", null)).toBe(
      "Two-factor authentication is already enabled.",
    );
    expect(getMobileTotpSetupDisabledReason(false, "otpauth://totp/Starlane", null)).toBe(
      "Finish verifying the current authenticator setup first.",
    );
    expect(getMobileTotpSetupDisabledReason(false, "", "totp-setup")).toBe(
      "Authenticator setup is already in progress.",
    );
    expect(getMobileTotpSetupDisabledReason(false, "", "passkey-add")).toBe(
      "Finish the current action before setting up an authenticator app.",
    );
    expect(getMobileTotpSetupDisabledReason(false, "", null)).toBeNull();

    expect(canVerifyMobileTotpSetup("", null)).toBe(false);
    expect(canVerifyMobileTotpSetup("   ", null)).toBe(false);
    expect(canVerifyMobileTotpSetup("12345", null)).toBe(false);
    expect(canVerifyMobileTotpSetup("12345a", null)).toBe(false);
    expect(canVerifyMobileTotpSetup("123456", "totp-verify")).toBe(false);
    expect(canVerifyMobileTotpSetup("123456", "passkey-add")).toBe(false);
    expect(canVerifyMobileTotpSetup("123456", null, false)).toBe(false);
    expect(canVerifyMobileTotpSetup("123456", null)).toBe(true);
    expect(canVerifyMobileTotpSetup(" 123456 ", null)).toBe(true);
    expect(canVerifyMobileTotpSetup("123456", null, true)).toBe(true);

    expect(getMobileTotpVerifyDisabledReason("", null)).toBe(
      "Enter an authenticator code to verify setup.",
    );
    expect(getMobileTotpVerifyDisabledReason("12345", null)).toBe(
      "Enter the 6-digit authenticator code.",
    );
    expect(getMobileTotpVerifyDisabledReason("12345a", null)).toBe(
      "Enter the 6-digit authenticator code.",
    );
    expect(getMobileTotpVerifyDisabledReason("123456", "totp-verify")).toBe(
      "Verification is already in progress.",
    );
    expect(getMobileTotpVerifyDisabledReason("123456", "passkey-add")).toBe(
      "Finish the current security action before verifying setup.",
    );
    expect(getMobileTotpVerifyDisabledReason("123456", null, false)).toBe(
      "Save your recovery codes before verifying setup.",
    );
    expect(getMobileTotpVerifyDisabledReason("123456", null, true)).toBeNull();

    expect(canDisableMobileTotp("", null)).toBe(false);
    expect(canDisableMobileTotp("12345", null)).toBe(false);
    expect(canDisableMobileTotp("12345a", null)).toBe(false);
    expect(canDisableMobileTotp("123456", "totp-disable")).toBe(false);
    expect(canDisableMobileTotp("123456", "passkey-delete-phone")).toBe(false);
    expect(canDisableMobileTotp("123456", null, true)).toBe(false);
    expect(canDisableMobileTotp("123456", null)).toBe(true);
    expect(canDisableMobileTotp("12345678", null)).toBe(true);
    expect(canDisableMobileTotp("1234-5678", null)).toBe(true);

    expect(getMobileTotpDisableDisabledReason("123456", null, true)).toBe(
      "This server requires 2FA, so an admin must turn off the requirement before disabling it.",
    );
    expect(getMobileTotpDisableDisabledReason("", null)).toBe(
      "Enter an authenticator or recovery code to disable 2FA.",
    );
    expect(getMobileTotpDisableDisabledReason("12345", null)).toBe(
      "Enter a 6-digit authenticator code or recovery code like 1234-5678.",
    );
    expect(getMobileTotpDisableDisabledReason("12345a", null)).toBe(
      "Enter a 6-digit authenticator code or recovery code like 1234-5678.",
    );
    expect(getMobileTotpDisableDisabledReason("123456", "totp-disable")).toBe(
      "Disable request is already in progress.",
    );
    expect(getMobileTotpDisableDisabledReason("123456", "passkey-delete-phone")).toBe(
      "Finish the current security action before disabling 2FA.",
    );
    expect(getMobileTotpDisableDisabledReason("123456", null)).toBeNull();
  });

  it("accepts mobile TOTP disable authenticator and recovery-code formats", () => {
    expect(isMobileTotpDisableCodeFormat("123456")).toBe(true);
    expect(isMobileTotpDisableCodeFormat("12345678")).toBe(true);
    expect(isMobileTotpDisableCodeFormat("1234-5678")).toBe(true);
    expect(isMobileTotpDisableCodeFormat("12345")).toBe(false);
    expect(isMobileTotpDisableCodeFormat("1234567")).toBe(false);
    expect(isMobileTotpDisableCodeFormat("1234 5678")).toBe(false);
    expect(isMobileTotpDisableCodeFormat("1234-567a")).toBe(false);
  });

  it("uses clear destructive mobile passkey deletion confirmation copy", () => {
    expect(getMobilePasskeyDeleteConfirmationMessage("Phone Face ID")).toBe(
      'Delete passkey "Phone Face ID"? You can add it again later from this device.',
    );
    expect(getMobilePasskeyDeleteConfirmationMessage("   ")).toBe(
      "Delete this passkey? You can add it again later from this device.",
    );
  });

  it("explains disabled mobile passkey delete actions", () => {
    expect(getMobilePasskeyDeleteDisabledReason("passkey-1", null)).toBeNull();
    expect(getMobilePasskeyDeleteDisabledReason("passkey-1", "passkey-delete-passkey-1")).toBe(
      "Passkey deletion is already in progress.",
    );
    expect(getMobilePasskeyDeleteDisabledReason("passkey-1", "totp-setup")).toBe(
      "Finish the current action before deleting a passkey.",
    );
  });

  it("normalizes mobile passkey delete helper ids for accessibility linkage", () => {
    expect(getMobilePasskeyDeleteHelpId("passkey-1")).toBe(
      "mobile-passkey-delete-passkey-1-help",
    );
    expect(getMobilePasskeyDeleteHelpId(" credential/id with spaces ")).toBe(
      "mobile-passkey-delete-credential-id-with-spaces-help",
    );
    expect(getMobilePasskeyDeleteHelpId("!!!")).toBe("mobile-passkey-delete-passkey-help");
  });

  it("formats mobile passkey date labels without leaking invalid dates", () => {
    expect(
      getMobilePasskeyDateLabel({
        created_at: "2026-07-16T04:00:00Z",
        last_used_at: null,
      }),
    ).toMatch(/^Added /);
    expect(
      getMobilePasskeyDateLabel({
        created_at: "2026-07-16T04:00:00Z",
        last_used_at: "2026-07-17T04:00:00Z",
      }),
    ).toMatch(/^Last used /);
    expect(
      getMobilePasskeyDateLabel({
        created_at: "not-a-date",
        last_used_at: null,
      }),
    ).toBe("Added Unknown date");
  });

  it("renders mobile passkey delete disabled reasons on the passkey row", () => {
    const html = renderToStaticMarkup(
      <MobilePasskeyRow
        passkey={{
          id: "phone",
          name: "Phone Face ID",
          created_at: "2026-07-16T04:00:00Z",
          last_used_at: null,
        }}
        busyAction="totp-setup"
        onDelete={() => undefined}
      />,
    );

    expect(html).toContain("Phone Face ID");
    expect(html).toContain("disabled");
    expect(html).toContain('aria-describedby="mobile-passkey-delete-phone-help"');
    expect(html).toContain('id="mobile-passkey-delete-phone-help"');
    expect(html).toContain('role="status"');
    expect(html).toContain("Finish the current action before deleting a passkey.");
    expect(html).toContain("rounded-card border border-border bg-bg");
    expect(html).not.toMatch(/bg-red-|text-red-|rounded-xl/);
  });

  it("explains disabled mobile passkey add actions", () => {
    expect(getMobilePasskeyAddDisabledReason("", null)).toBe("Name this passkey before adding it.");
    expect(getMobilePasskeyAddDisabledReason("   ", null)).toBe("Name this passkey before adding it.");
    expect(getMobilePasskeyAddDisabledReason("Phone Face ID", "passkey-add")).toBe(
      "Passkey setup is already in progress.",
    );
    expect(getMobilePasskeyAddDisabledReason("Phone Face ID", "totp-setup")).toBe(
      "Finish the current action before adding a passkey.",
    );
    expect(getMobilePasskeyAddDisabledReason(`${"a".repeat(80)} `, null)).toBeNull();
    expect(getMobilePasskeyAddDisabledReason("a".repeat(81), null)).toBe(
      "Passkey names can be up to 80 characters.",
    );
    expect(getMobilePasskeyAddDisabledReason("Phone Face ID", null)).toBeNull();
  });

  it("uses clear destructive mobile session revoke confirmation copy", () => {
    const session = {
      id: "session-1",
      device_name: "Harley's iPhone",
      ip_address: "100.64.0.2",
      last_seen_at: "2026-07-16T04:00:00Z",
      expires_at: "2026-07-17T04:00:00Z",
      created_at: "2026-07-15T04:00:00Z",
    };

    expect(sessionDeviceLabel(session)).toBe("Harley's iPhone");
    expect(getMobileSessionRevokeConfirmationMessage(sessionDeviceLabel(session))).toBe(
      'Revoke the session for "Harley\'s iPhone"? That device will need to sign in again.',
    );
    expect(getMobileSessionRevokeConfirmationMessage("   ")).toBe(
      "Revoke this session? That device will need to sign in again.",
    );
  });

  it("uses clear destructive mobile 2FA disable confirmation copy", () => {
    expect(getMobileTotpDisableConfirmationMessage()).toBe(
      "Disable two-factor authentication? Password sign-in will no longer require an authenticator or recovery code.",
    );
  });

  it("uses clear destructive mobile vault trash confirmation copy", () => {
    expect(getMobileVaultTrashConfirmationMessage("Field Vault")).toBe(
      'Move "Field Vault" to trash? Vault files are not permanently deleted.',
    );
    expect(getMobileVaultTrashConfirmationMessage("   ")).toBe(
      "Move this vault to trash? Vault files are not permanently deleted.",
    );
  });

  it("uses clear destructive mobile reset app confirmation copy", () => {
    expect(getMobileResetAppConfirmationMessage()).toBe(
      "Reset Starlane Notes on this device? This removes local notes and sign-in data.",
    );
  });

  it("makes active vault rename and switch affordance explicit in local native settings", () => {
    expect(getActiveVaultSettingsRowCopy(true, true, "Field Vault")).toEqual({
      value: "Rename / Switch",
      description: "Field Vault",
    });

    expect(getActiveVaultSettingsRowCopy(true, false, "Field Vault")).toEqual({
      value: "Field Vault",
    });

    expect(getActiveVaultSettingsRowCopy(false, true, "App folder")).toEqual({
      value: "App folder",
    });
  });

  it("normalizes mobile server vault URLs before switching to server mode", () => {
    expect(normalizeMobileServerVaultUrl(" https://notes.example.com/// ")).toBe(
      "https://notes.example.com",
    );
    expect(normalizeMobileServerVaultUrl("http://localhost:8080/base/")).toBe(
      "http://localhost:8080/base",
    );
    expect(normalizeMobileServerVaultUrl("notes.example.com")).toBeNull();
    expect(normalizeMobileServerVaultUrl("   ")).toBeNull();
  });

  it("formats native mobile action failures from Tauri string and object payloads", () => {
    expect(formatMobileActionError("Default vault folder cannot be renamed.")).toBe(
      "Default vault folder cannot be renamed.",
    );
    expect(formatMobileActionError({ message: "Unable to create vault folder." })).toBe(
      "Unable to create vault folder.",
    );
    expect(formatMobileActionError({ error: "iCloud Drive is unavailable." })).toBe(
      "iCloud Drive is unavailable.",
    );
    expect(formatMobileActionError({ code: "EACCES" }, "Unable to read storage folder.")).toBe(
      "Unable to read storage folder. (EACCES)",
    );
    expect(formatMobileActionError(null)).toBe("Action failed.");
  });

  it("shows the profile open-folder row only when native vault reveal and a path are available", () => {
    expect(shouldShowProfileOpenFolderRow(true, { path: "/vaults/notes" })).toBe(true);
    expect(shouldShowProfileOpenFolderRow(true, null)).toBe(false);
    expect(shouldShowProfileOpenFolderRow(true, { path: "" })).toBe(false);
    expect(shouldShowProfileOpenFolderRow(false, { path: "/vaults/notes" })).toBe(false);
  });

  it("shows the profile use-app-folder action only in local mode", () => {
    expect(shouldShowProfileUseAppFolderRow(true)).toBe(true);
    expect(shouldShowProfileUseAppFolderRow(false)).toBe(false);
  });

  it("shows the space switcher whenever multiple spaces exist", () => {
    expect(shouldShowMobileSpaceSwitcher(0)).toBe(false);
    expect(shouldShowMobileSpaceSwitcher(1)).toBe(false);
    expect(shouldShowMobileSpaceSwitcher(2)).toBe(true);
  });

  it("shows mobile space creation only in local mode", () => {
    expect(shouldShowMobileCreateSpace(true)).toBe(true);
    expect(shouldShowMobileCreateSpace(false)).toBe(false);
  });

  it("keeps primary mobile create actions paired", () => {
    expect(getMobilePrimaryCreateActions()).toEqual(["New Doc", "New Folder"]);
  });

  it("hides mobile notifications in local mode", () => {
    expect(shouldShowMobileNotifications(true)).toBe(false);
    expect(shouldShowMobileNotifications(false)).toBe(true);

    setCurrentLaunchMode("local");
    let html = "";
    try {
      html = renderToStaticMarkup(<MobileNotificationsScreen />);
    } finally {
      setCurrentLaunchMode("server");
    }

    expect(html).toContain("Notifications are unavailable in local standalone mode.");
    expect(html).not.toContain("Refresh notifications");
  });

  it("merges mobile notification pages without duplicating delivery rows", () => {
    const current = [
      notificationRecord("notification-1", false),
      notificationRecord("notification-2", true),
    ];
    const nextPage = [
      notificationRecord("notification-2", true),
      notificationRecord("notification-3", false),
    ];

    expect(mergeMobileNotificationPages(current, nextPage).map((item) => item.id)).toEqual([
      "notification-1",
      "notification-2",
      "notification-3",
    ]);
  });

  it("uses the shared select control for mobile preference pickers", () => {
    const html = renderToStaticMarkup(
      <MobilePreferenceSelect
        value="both"
        onChange={() => undefined}
        ariaLabel="Comments delivery"
      >
        <option value="both">In-app + email</option>
      </MobilePreferenceSelect>,
    );

    expect(html).toContain('aria-label="Comments delivery"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("max-w-[48%]");
    expect(html).toContain("In-app + email");
  });

  it("uses the shared input control for mobile profile fields", () => {
    const html = renderToStaticMarkup(
      <MobileTextInput
        value="Eagle"
        onChange={() => undefined}
        ariaLabel="Display name"
        placeholder="Display name"
        id="mobile-display-name"
        className="h-10 min-w-0 flex-1"
        autoCorrect="off"
        autoCapitalize="none"
        autoFocus
      />,
    );

    expect(html).toContain('id="mobile-display-name"');
    expect(html).toContain('aria-label="Display name"');
    expect(html).toContain('placeholder="Display name"');
    expect(html).toContain("rounded-input");
    expect(html).toContain("focus:ring-2");
    expect(html).toContain("h-10");
    expect(html).toContain("min-w-0");
    expect(html).toContain('autoCorrect="off"');
    expect(html).toContain('autoCapitalize="none"');
    expect(html).toContain("autofocus");
  });

  it("places account security controls in profile settings", () => {
    authMock.totpEnabled = false;
    const html = renderToStaticMarkup(<MobileProfileScreen scope="profile" />);

    expect(html).toContain("Profile settings");
    expect(html).not.toContain("App settings");
    expect(html).toContain("max-w-3xl");
    expect(html).toContain("Security");
    expect(html).toContain("Two-factor authentication");
    expect(html).toContain("Set up authenticator app");
    expect(html).toContain("Not enabled yet.");
    expect(html).toContain("rounded-full");
    expect(html).toContain("border-border");
    expect(html).toContain("Passkeys");
    expect(html).toContain("Passkeys are unavailable in this browser or webview.");
  });

  it("renders profile security controls with mobile-friendly Starlane sizing and tokens", () => {
    authMock.totpEnabled = false;
    authMock.require2fa = false;
    passkeySupportMock.supported = false;
    const html = renderToStaticMarkup(<MobileProfileScreen scope="profile" />);

    expect(html).toContain("Protect this server account with an authenticator app and passkeys.");
    expect(html).toContain("h-11 rounded-button border border-border bg-surface-raised");
    expect(html).toContain("Set up authenticator app");
    expect(html).toContain("rounded-full border px-2 py-1 text-xs");
    expect(html).toContain("border-border bg-surface-raised text-text-muted");
    expect(html).toContain("rounded-card border border-border bg-surface-raised");
    expect(html).toContain("Passkeys are unavailable in this browser or webview.");
    expect(html).toContain("Use password sign-in with two-factor");
    expect(html).not.toMatch(/bg-blue-|text-red-|border-gray-|rounded-xl/);
  });

  it("explains unavailable profile passkeys with a tokenized status notice", () => {
    const html = renderToStaticMarkup(<ProfilePasskeyUnavailableNotice />);

    expect(html).toContain('role="status"');
    expect(html).toContain("Passkeys are unavailable in this browser or webview.");
    expect(html).toContain("Use password sign-in with two-factor authentication on this device.");
    expect(html).toContain("rounded-card");
    expect(html).toContain("border-border");
    expect(html).toContain("bg-surface-raised");
    expect(html).toContain("text-text-muted");
  });

  it("renders supported passkey management with the server-aligned name limit", () => {
    authMock.totpEnabled = false;
    authMock.require2fa = false;
    passkeySupportMock.supported = true;
    const html = renderToStaticMarkup(<MobileProfileScreen scope="profile" />);

    expect(html).toContain("Passkey name");
    expect(html).toContain("sm:grid-cols-[minmax(0,1fr)_auto]");
    expect(html).toContain("maxLength=\"80\"");
    expect(html).toContain("autoComplete=\"off\"");
    expect(html).toContain("aria-describedby=\"passkey-name-help\"");
    expect(html).toContain("Passkey names can be up to 80 characters.");
    expect(html).toContain('id="passkey-name-help" class="text-xs leading-5 text-text-muted sm:col-span-2"');
    expect(html).toContain("Add passkey");
    expect(html).toContain("aria-describedby=\"passkey-add-help\"");
    expect(html).toContain("Name this passkey before adding it.");
    expect(html).toContain('role="status"');
    expect(html).toContain("h-11 rounded-button border border-border bg-surface-raised");
    expect(html).not.toContain("Passkeys are unavailable in this browser or webview.");
    expect(html).not.toMatch(/bg-blue-|border-gray-|rounded-xl/);

    passkeySupportMock.supported = false;
  });

  it("shows a semantic enrollment notice when the server requires 2FA", () => {
    authMock.totpEnabled = false;
    authMock.require2fa = true;
    const html = renderToStaticMarkup(<MobileProfileScreen scope="profile" />);

    expect(html).toContain(MOBILE_REQUIRED_2FA_ENROLLMENT_NOTICE_COPY);
    expect(html).toContain("Set up an authenticator app");
    expect(html).toContain('role="status"');
    expect(html).toContain(MOBILE_REQUIRED_2FA_POLICY_NOTICE_CLASS);
    expect(html).not.toMatch(/bg-yellow-|text-orange-|border-amber-|rounded-xl/);

    authMock.require2fa = false;
  });

  it("shows a policy notice instead of disable controls when required 2FA is already enabled", () => {
    authMock.totpEnabled = true;
    authMock.require2fa = true;
    const html = renderToStaticMarkup(<MobileProfileScreen scope="profile" />);

    expect(html).toContain("Enabled for password sign-in.");
    expect(html).not.toContain("This server requires two-factor authentication.");
    expect(html).toContain(
      "This server requires 2FA, so an admin must turn off the requirement before disabling it.",
    );
    expect(html).toContain('role="status"');
    expect(html).toContain(MOBILE_REQUIRED_2FA_POLICY_NOTICE_CLASS);
    expect(html).not.toContain("123456 or 1234-5678");
    expect(html).not.toContain("Disable 2FA");

    authMock.totpEnabled = false;
    authMock.require2fa = false;
  });

  it("lets enabled mobile 2FA disable accept authenticator or recovery codes when policy allows it", () => {
    authMock.totpEnabled = true;
    authMock.require2fa = false;
    const html = renderToStaticMarkup(<MobileProfileScreen scope="profile" />);

    expect(html).toContain("Enabled for password sign-in.");
    expect(html).not.toContain("This server requires two-factor authentication.");
    expect(html).not.toContain("admin must turn off the requirement");
    expect(html).toContain("border-success/30 bg-success/10 text-success");
    expect(html).toContain("123456 or 1234-5678");
    expect(html).toContain(
      'aria-label="Authenticator or recovery code to disable two-factor authentication"',
    );
    expect(html).toContain("maxLength=\"9\"");
    expect(html).toContain("inputMode=\"text\"");
    expect(html).toContain("autoCapitalize=\"none\"");
    expect(html).toContain("autoCorrect=\"off\"");
    expect(html).toContain("aria-describedby=\"totp-disable-help\"");
    expect(html).toContain("Enter an authenticator or recovery code to disable 2FA.");
    expect(html).toContain("h-11 rounded-button border border-danger");
    expect(html).toContain("active:bg-danger/10");

    authMock.totpEnabled = false;
    authMock.require2fa = false;
  });

  it("keeps vault switching reachable from server-mode app settings", () => {
    setCurrentLaunchMode("server");
    authMock.role = "admin";
    runtimeMock.runtime = "tauri-mobile";
    runtimeMock.isMobileRuntime = true;
    runtimeMock.isDesktopRuntime = false;
    const html = renderToStaticMarkup(<MobileProfileScreen scope="app" />);

    expect(html).toContain("App settings");
    expect(html).toContain("Server sync keeps a local device copy first.");
    expect(html).toContain("Sync");
    expect(html).toContain("Server");
    expect(html).toContain(SERVER_MODE_VAULT_SWITCH_ROW_COPY.label);
    expect(html).toContain(SERVER_MODE_VAULT_SWITCH_ROW_COPY.description);
    expect(html).toContain(SERVER_MODE_VAULT_SWITCH_ROW_COPY.value);
    expect(html.indexOf("Sync")).toBeLessThan(html.indexOf(SERVER_MODE_VAULT_SWITCH_ROW_COPY.label));
    expect(html).toContain("Site administration");
    expect(html).toContain("Manage users, access, authentication, and server settings.");
    expect(html).toContain("Manage");
    setCurrentLaunchMode("server");
    authMock.role = "owner";
    runtimeMock.runtime = "web";
    runtimeMock.isMobileRuntime = false;
  });

  it("limits Site Administration to server-connected administrators", () => {
    expect(shouldShowMobileSiteAdministration(false, "admin")).toBe(true);
    expect(shouldShowMobileSiteAdministration(false, "member")).toBe(false);
    expect(shouldShowMobileSiteAdministration(true, "admin")).toBe(false);
    expect(shouldShowMobileSiteAdministration(false, undefined)).toBe(false);
  });

  it("keeps vault switching reachable when the mobile shell reports web before platform detection settles", () => {
    setCurrentLaunchMode("server");
    runtimeMock.runtime = "web";
    runtimeMock.isMobileRuntime = true;
    runtimeMock.isDesktopRuntime = false;
    Object.defineProperty(globalThis, "window", {
      value: {
        __TAURI_INTERNALS__: { invoke: vi.fn() },
        matchMedia: vi.fn(() => ({
          matches: true,
          addEventListener: vi.fn(),
          removeEventListener: vi.fn(),
        })),
      },
      configurable: true,
    });

    const html = renderToStaticMarkup(<MobileProfileScreen scope="app" />);

    expect(html).toContain("App settings");
    expect(html).toContain(SERVER_MODE_VAULT_SWITCH_ROW_COPY.label);
    expect(html).toContain(SERVER_MODE_VAULT_SWITCH_ROW_COPY.description);
    expect(html).toContain(SERVER_MODE_VAULT_SWITCH_ROW_COPY.value);

    delete (globalThis as unknown as { window?: unknown }).window;
    runtimeMock.runtime = "web";
    runtimeMock.isMobileRuntime = false;
  });

  it("builds the auth-context TOTP patch used after mobile setup and disable", () => {
    expect(getMobileTotpUserPatch(true)).toEqual({ totp_enabled: true });
    expect(getMobileTotpUserPatch(false)).toEqual({ totp_enabled: false });
  });

  it("treats only otpauth setup URIs as authenticator-app handoff links", () => {
    expect(isOpenableMobileTotpSetupUri("otpauth://totp/Starlane:user@example.test")).toBe(true);
    expect(isOpenableMobileTotpSetupUri("https://notes.example.test/setup")).toBe(false);
    expect(isOpenableMobileTotpSetupUri("not a url")).toBe(false);
  });

  it("renders mobile TOTP setup with copy, authenticator handoff, verify, and recovery codes", () => {
    const setupUrl = "otpauth://totp/Starlane:user@example.test?secret=ABC&issuer=Starlane";
    const html = renderToStaticMarkup(
      <MobileTotpSetupPanel
        setupUrl={setupUrl}
        code="123456"
        recoveryCodes={["1111-2222", "3333-4444"]}
        busyAction={null}
        onCopySetupUrl={() => undefined}
        onCopyRecoveryCodes={() => undefined}
        onCodeChange={() => undefined}
        onVerify={() => undefined}
      />,
    );

    expect(html).toContain("Authenticator setup URI");
    expect(html).toContain(MOBILE_TOTP_SETUP_PANEL_CLASS);
    expect(html).toContain(MOBILE_TOTP_SETUP_ACTION_GRID_CLASS);
    expect(html).toContain(MOBILE_TOTP_VERIFY_GRID_CLASS);
    expect(html).toContain("Copy setup URI");
    expect(html).toContain("Open in authenticator app");
    expect(html).toContain('href="otpauth://totp/Starlane:user@example.test?secret=ABC&amp;issuer=Starlane"');
    expect(html).toContain("Authenticator verification code");
    expect(html).toContain("Recovery codes");
    expect(html).toContain("Copy all");
    expect(html).toContain(MOBILE_TOTP_RECOVERY_WARNING_COPY);
    expect(html).toContain(MOBILE_TOTP_RECOVERY_WARNING_CLASS);
    expect(html).toContain('role="status"');
    expect(html).toContain(MOBILE_TOTP_RECOVERY_ACK_COPY);
    expect(html).toContain(MOBILE_TOTP_RECOVERY_ACK_CLASS);
    expect(html).toContain(MOBILE_TOTP_RECOVERY_ACK_INPUT_CLASS);
    expect(html).toContain("type=\"checkbox\"");
    expect(html).toContain("focus-ring");
    expect(html).toContain("aria-describedby=\"totp-verify-help\"");
    expect(html).toContain("Save your recovery codes before verifying setup.");
    expect(html).toContain("1111-2222");
    expect(html).toContain("3333-4444");
    expect(html).toContain("disabled");
    expect(html).toContain("rounded-button border border-border bg-surface-raised");
    expect(html).toContain("rounded-button border border-border bg-surface px-2 text-xs");
    expect(html).toContain(MOBILE_TOTP_RECOVERY_CARD_CLASS);
    expect(html).toContain("text-warning");
    expect(html).toContain(MOBILE_TOTP_VERIFY_BUTTON_CLASS);
    expect(html).toContain("text-on-primary");
    expect(html).not.toMatch(/bg-blue-|border-gray-|rounded-xl/);
  });
});
