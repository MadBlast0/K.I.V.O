/**
 * Settings → Accessibility (UX §10, UX-31, UX-54): screen-reader announcements, captions for
 * spoken replies, larger Island text, voice-only use, Windows' high contrast (followed, shown
 * here), and the warning when KIVO would give no sign it's listening.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Group, Row, Switch, Tag } from "../../components/ui";
import { bool } from "../../lib/settings";
import { useConfig } from "./useConfig";

/** Windows' high contrast (forced colours), as it changes. */
function useForcedColors(): boolean {
  const [on, setOn] = useState(() => matchMedia("(forced-colors: active)").matches);
  useEffect(() => {
    const mq = matchMedia("(forced-colors: active)");
    const change = () => setOn(mq.matches);
    mq.addEventListener("change", change);
    return () => mq.removeEventListener("change", change);
  }, []);
  return on;
}

const SWITCHES = [
  { key: "announcements", icon: "accessibility", fallback: true },
  { key: "captions", icon: "chat", fallback: true },
  { key: "large-island-text", icon: "type", fallback: false },
  { key: "voice-only", icon: "wave", fallback: false },
] as const;

export function AccessibilityTab() {
  const { t } = useTranslation();
  const { get, set } = useConfig();
  const highContrast = useForcedColors();
  return (
    <Group>
      {SWITCHES.map((s) => (
        <Row
          key={s.key}
          icon={s.icon}
          title={t(`settings.access.${s.key}`)}
          subtitle={t(`settings.access.${s.key}Hint`, { defaultValue: "" }) || undefined}
          end={
            <Switch
              label={t(`settings.access.${s.key}`)}
              checked={bool(get("accessibility", s.key), s.fallback)}
              onChange={(v) => set("accessibility", { [s.key]: v })}
            />
          }
        />
      ))}
      <Row
        icon="theme"
        title={t("settings.access.contrast")}
        subtitle={t("settings.access.contrastHint")}
        end={<Tag>{highContrast ? t("settings.access.contrastOn") : t("settings.access.contrastOff")}</Tag>}
      />
      <Row
        icon="warning"
        title={t("settings.access.warn-silent")}
        subtitle={t("settings.access.warn-silentHint")}
        end={
          <Switch
            label={t("settings.access.warn-silent")}
            checked={bool(get("accessibility", "warn-silent"), true)}
            onChange={(v) => set("accessibility", { "warn-silent": v })}
          />
        }
      />
    </Group>
  );
}
