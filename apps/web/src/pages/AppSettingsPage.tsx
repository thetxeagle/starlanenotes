import { HardDrive } from "lucide-react";
import { useNavigate } from "react-router-dom";
import { MobileProfileScreen } from "@/components/mobile/MobileScreens";
import { SettingsDialog } from "@/components/settings/SettingsDialog";
import { useDesktopSettingsDialog } from "@/hooks/useDesktopSettingsDialog";

const appSettingsSections = [
  {
    id: "application",
    label: "Application",
    icon: <HardDrive size={16} />,
    group: "This device",
  },
];

export function AppSettingsPage() {
  const navigate = useNavigate();
  const desktop = useDesktopSettingsDialog();

  if (!desktop) {
    return <MobileProfileScreen scope="app" />;
  }

  return (
    <SettingsDialog
      title="Settings"
      description="Manage local storage, server sync, appearance, and this installation."
      sections={appSettingsSections}
      activeSection="application"
      onSectionChange={() => undefined}
      onClose={() => navigate(-1)}
    >
      <MobileProfileScreen scope="app" embedded />
    </SettingsDialog>
  );
}
