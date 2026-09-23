/**
 * Settings (UX §5, UX-31): General · Appearance · Island · Sounds · Notifications ·
 * Accessibility · Shortcuts · Performance · Diagnostics · About. Every control is a setting the
 * runtime keeps in `kivo.toml` with its default (UX-37); each tab is its own file in `settings/`.
 */
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import { PageTabs } from "../components/ui";
import { AboutTab } from "./settings/About";
import { AccessibilityTab } from "./settings/Accessibility";
import { AppearanceTab } from "./settings/Appearance";
import { DiagnosticsTab } from "./settings/Diagnostics";
import { GeneralTab } from "./settings/General";
import { IslandTab } from "./settings/Island";
import { NotificationsTab } from "./settings/Notifications";
import { PerformanceTab } from "./settings/Performance";
import { ShortcutsTab } from "./settings/Shortcuts";
import { SoundsTab } from "./settings/Sounds";

export { validQuietHours } from "./settings/Notifications";

export type SettingsTab =
  | "general"
  | "appearance"
  | "island"
  | "sounds"
  | "notifications"
  | "accessibility"
  | "shortcuts"
  | "performance"
  | "diagnostics"
  | "about";

export const SETTINGS_TABS: ReadonlyArray<SettingsTab> = [
  "general",
  "appearance",
  "island",
  "sounds",
  "notifications",
  "accessibility",
  "shortcuts",
  "performance",
  "diagnostics",
  "about",
];

export function Settings({
  initialTab = "general",
  onNavigate,
}: {
  initialTab?: SettingsTab;
  onNavigate?: (page: string) => void;
}) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<SettingsTab>(initialTab);
  const content: Record<SettingsTab, React.ReactNode> = {
    general: <GeneralTab />,
    appearance: <AppearanceTab />,
    island: <IslandTab />,
    sounds: <SoundsTab />,
    notifications: <NotificationsTab />,
    accessibility: <AccessibilityTab />,
    shortcuts: <ShortcutsTab />,
    performance: <PerformanceTab />,
    diagnostics: <DiagnosticsTab onNavigate={onNavigate} />,
    about: <AboutTab />,
  };
  return (
    <>
      <PageHeader title={t("nav.settings")} subtitle={t(`settings.subtitles.${tab}`)} />
      <PageTabs<SettingsTab>
        label={t("nav.settings")}
        value={tab}
        onChange={setTab}
        tabs={SETTINGS_TABS.map((value) => ({
          value,
          label: t(`settings.tabs.${value}`),
          content: content[value],
        }))}
      />
    </>
  );
}
