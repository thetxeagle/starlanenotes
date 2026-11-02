import { useState } from "react";
import { Bell, Boxes, Settings2, Shield, ShieldCheck, UserRound } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { EditorToolbarSettingsPanel } from "@/components/editor/EditorToolbarSettingsPanel";
import { MobileProfileScreen } from "@/components/mobile/MobileScreens";
import { SettingsDialog, type SettingsDialogSection } from "@/components/settings/SettingsDialog";
import { useDesktopSettingsDialog } from "@/hooks/useDesktopSettingsDialog";

type AccountSettingsSection =
  | "profile"
  | "notifications"
  | "roles"
  | "spaces"
  | "security"
  | "editor";

const accountSections: SettingsDialogSection[] = [
  {
    id: "profile",
    label: "Profile",
    icon: <UserRound size={16} />,
    group: "Account",
  },
  {
    id: "notifications",
    label: "Notifications",
    icon: <Bell size={16} />,
    group: "Account",
  },
  {
    id: "roles",
    label: "Roles",
    icon: <ShieldCheck size={16} />,
    group: "Account",
  },
  {
    id: "spaces",
    label: "Spaces",
    icon: <Boxes size={16} />,
    group: "Account",
  },
  {
    id: "security",
    label: "Sign-in & security",
    icon: <Shield size={16} />,
    group: "Account",
  },
  {
    id: "editor",
    label: "Editor toolbar",
    icon: <Settings2 size={16} />,
    group: "Preferences",
  },
];

export function AccountSettingsPage({
  initialSection = "profile",
}: {
  initialSection?: AccountSettingsSection;
}) {
  const navigate = useNavigate();
  const desktop = useDesktopSettingsDialog();
  const [activeSection, setActiveSection] = useState<AccountSettingsSection>(initialSection);

  if (!desktop) {
    // Toolbar ordering is a web account preference. Native/mobile settings
    // keep their existing profile destinations instead of exposing a desktop
    // editor panel that cannot render in this workspace.
    return <MobileProfileScreen scope={initialSection === "editor" ? "profile" : initialSection} />;
  }

  return (
    <SettingsDialog
      title="Account settings"
      description="Manage your profile, notifications, sign-in methods, and active sessions."
      sections={accountSections}
      activeSection={activeSection}
      onSectionChange={(section) => setActiveSection(section as AccountSettingsSection)}
      onClose={() => navigate(-1)}
    >
      {activeSection === "editor" ? (
        <EditorToolbarSettingsPanel />
      ) : (
        <MobileProfileScreen scope={activeSection} embedded />
      )}
    </SettingsDialog>
  );
}
