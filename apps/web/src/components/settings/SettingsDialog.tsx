import { useEffect, useRef, type ReactNode } from "react";
import { X } from "lucide-react";
import { DialogShell } from "@/components/ui/dialog-shell";

export interface SettingsDialogSection {
  id: string;
  label: string;
  icon: ReactNode;
  group?: string;
}

interface SettingsWorkspaceProps {
  title: string;
  description: string;
  sections: SettingsDialogSection[];
  activeSection: string;
  onSectionChange: (section: string) => void;
  onClose?: () => void;
  children: ReactNode;
  compactHeader?: boolean;
}

interface SettingsDialogProps extends SettingsWorkspaceProps {
  onClose: () => void;
  maxWidth?: "xl" | "settings" | "canvas";
}

export const SETTINGS_WORKSPACE_NAV_CLASS =
  "scrollbar-none border-border bg-surface flex items-center gap-1 overflow-x-auto border-b p-3 sm:block sm:overflow-y-auto sm:border-r sm:border-b-0";
export const SETTINGS_WORKSPACE_SECTION_CLASS =
  "rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex h-10 min-h-10 shrink-0 items-center gap-2 px-3 text-left text-sm transition-colors duration-100 sm:w-full";
export const SETTINGS_WORKSPACE_ACTIVE_SECTION_CLASS = "bg-surface-raised text-text";

/** Shared settings information architecture used by desktop dialogs and mobile destination screens. */
export function SettingsWorkspace({
  title,
  description,
  sections,
  activeSection,
  onSectionChange,
  onClose,
  children,
  compactHeader = false,
}: SettingsWorkspaceProps) {
  const navigationRef = useRef<HTMLElement>(null);
  const activeSectionRef = useRef<HTMLButtonElement>(null);
  const active = sections.find((section) => section.id === activeSection) ?? sections[0];
  const groupedSections = sections.reduce<Array<{ group?: string; sections: SettingsDialogSection[] }>>(
    (groups, section) => {
      const current = groups[groups.length - 1];
      if (!current || current.group !== section.group) {
        groups.push({ group: section.group, sections: [section] });
      } else {
        current.sections.push(section);
      }
      return groups;
    },
    [],
  );

  useEffect(() => {
    const navigation = navigationRef.current;
    const activeButton = activeSectionRef.current;
    if (!navigation || !activeButton) return;
    const navigationBounds = navigation.getBoundingClientRect();
    const buttonBounds = activeButton.getBoundingClientRect();
    if (buttonBounds.right > navigationBounds.right) {
      navigation.scrollLeft += buttonBounds.right - navigationBounds.right + 12;
    } else if (buttonBounds.left < navigationBounds.left) {
      navigation.scrollLeft += buttonBounds.left - navigationBounds.left - 12;
    }
  }, [activeSection]);

  return (
    <div className="bg-surface flex h-full min-h-0 flex-col">
      <header
          className={`border-border flex items-start justify-between gap-4 border-b ${
            compactHeader ? "px-4 py-3" : "px-4 py-4 sm:px-6"
          }`}
        >
          <div className="min-w-0">
            <h1 id="settings-workspace-title" className="text-text text-heading-lg font-semibold">
              {title}
            </h1>
            <p className="text-text-muted mt-1 text-sm">{description}</p>
          </div>
          {onClose && (
            <button
              type="button"
              onClick={onClose}
              className="rounded-button text-text-muted hover:bg-surface-raised hover:text-text flex size-9 shrink-0 items-center justify-center"
              aria-label={`Close ${title}`}
            >
              <X size={16} />
            </button>
          )}
      </header>
      <div className="grid min-h-0 flex-1 grid-cols-1 sm:grid-cols-[12rem_minmax(0,1fr)]">
          <nav
            ref={navigationRef}
            className={SETTINGS_WORKSPACE_NAV_CLASS}
            aria-label={`${title} sections`}
          >
            {groupedSections.map((group, groupIndex) => (
              <div
                key={`${group.group ?? "settings"}-${groupIndex}`}
                className="flex items-center gap-1 sm:mb-4 sm:flex-col sm:items-stretch sm:last:mb-0"
              >
                {group.group && (
                  <p className="text-text-muted hidden px-3 pb-1 text-xs font-medium sm:block">
                    {group.group}
                  </p>
                )}
                {group.sections.map((section) => {
                  const selected = active?.id === section.id;
                  return (
                    <button
                      key={section.id}
                      ref={selected ? activeSectionRef : undefined}
                      type="button"
                      aria-current={selected ? "page" : undefined}
                      aria-controls="settings-workspace-panel"
                      onClick={() => onSectionChange(section.id)}
                      className={`${SETTINGS_WORKSPACE_SECTION_CLASS} ${
                        selected ? SETTINGS_WORKSPACE_ACTIVE_SECTION_CLASS : ""
                      }`}
                    >
                      {section.icon}
                      <span>{section.label}</span>
                    </button>
                  );
                })}
              </div>
            ))}
          </nav>
          <section
            id="settings-workspace-panel"
            aria-label={active?.label}
            className="min-h-0 overflow-y-auto p-4 sm:p-6"
          >
            {children}
          </section>
      </div>
    </div>
  );
}

/** Centered desktop settings dialog. Mobile/native routes use SettingsWorkspace directly. */
export function SettingsDialog({
  maxWidth = "settings",
  onClose,
  ...workspaceProps
}: SettingsDialogProps) {
  return (
    <DialogShell
      labelledBy="settings-workspace-title"
      maxWidth={maxWidth}
      onClose={onClose}
    >
      <div data-dialog-frame="" className="h-[min(54rem,calc(100dvh-2rem))]">
        <SettingsWorkspace {...workspaceProps} onClose={onClose} />
      </div>
    </DialogShell>
  );
}
