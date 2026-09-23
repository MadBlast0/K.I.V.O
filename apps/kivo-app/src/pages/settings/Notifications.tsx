/**
 * Settings → Notifications (UX-40, UX-31): how KIVO tells the user — out loud, as Windows
 * notifications, with or without a sound — quiet hours and Windows Focus, how each source tells
 * the user, catching up on return, and the Island's live activities (UX-15).
 */
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Group, Note, Row, Section, Segmented, Select, Switch, TextField } from "../../components/ui";
import { bool, field, oneOf, str } from "../../lib/settings";
import { useConfig } from "./useConfig";

type Announce = "speak" | "toast" | "silent";
type Speak = "always" | "when-free" | "never";

/** The sources a notification comes from (UX-40); agents that need an answer always show. */
const SOURCES = ["tasks", "reminders", "watchers", "routines", "agents"] as const;
const ACTIVITIES = ["media", "timer", "download", "agent"] as const;

/** "22:00-07:00", or empty for none. */
export function validQuietHours(text: string): boolean {
  if (text.trim() === "") return true;
  return /^([01]?\d|2[0-3]):[0-5]\d\s*-\s*([01]?\d|2[0-3]):[0-5]\d$/.test(text.trim());
}

export function NotificationsTab() {
  const { t } = useTranslation();
  const { settings, get, set } = useConfig();
  const [quiet, setQuiet] = useState<string | null>(null);
  if (!settings) return null;
  const stored = str(get("automation", "quiet-hours"));
  const quietText = quiet ?? stored;
  const quietOk = validQuietHours(quietText);
  const sources = get("automation", "sources");
  const live = get("automation", "live-activities");
  const mode = (source: string) => oneOf(field(sources, source), ["speak", "toast", "silent"] as const);

  return (
    <>
      <Section title={t("settings.notify.how")} />
      <Group>
        <Row
          icon="wave"
          title={t("settings.notify.speak")}
          end={
            <Segmented<Speak>
              label={t("settings.notify.speak")}
              value={oneOf(get("automation", "speak"), ["always", "when-free", "never"]) ?? "when-free"}
              onChange={(v) => set("automation", { speak: v })}
              options={(["always", "when-free", "never"] as const).map((value) => ({
                value,
                label: t(`settings.notify.speaks.${value}`),
              }))}
            />
          }
        />
        <Row
          icon="bell"
          title={t("settings.notify.toasts")}
          subtitle={t("settings.notify.toastsHint")}
          end={
            <Switch
              label={t("settings.notify.toasts")}
              checked={bool(get("automation", "toasts"), true)}
              onChange={(v) => set("automation", { toasts: v })}
            />
          }
        />
        <Row
          icon="volume"
          title={t("settings.notify.sound")}
          end={
            <Select<"on" | "off">
              label={t("settings.notify.sound")}
              value={bool(get("automation", "notification-sound"), true) ? "on" : "off"}
              onChange={(v) => set("automation", { "notification-sound": v === "on" })}
              items={[
                { value: "on", label: t("settings.notify.soundOn") },
                { value: "off", label: t("settings.notify.soundOff") },
              ]}
            />
          }
        />
      </Group>
      <Note>{t("settings.notify.whenFree")}</Note>

      <Section title={t("settings.notify.quiet")} />
      <Group>
        <Row
          icon="moon"
          title={t("settings.notify.quietHours")}
          subtitle={quietOk ? t("settings.notify.quietHoursHint") : t("settings.notify.quietHoursBad")}
          end={
            <TextField
              aria-label={t("settings.notify.quietHours")}
              placeholder="22:00-07:00"
              value={quietText}
              aria-invalid={!quietOk}
              onChange={(e) => setQuiet(e.target.value)}
              onBlur={() => {
                if (quietOk && quietText.trim() !== stored) set("automation", { "quiet-hours": quietText.trim() });
                setQuiet(null);
              }}
              style={{ width: 140 }}
            />
          }
        />
        <Row
          icon="bell"
          title={t("settings.notify.focus")}
          end={
            <Switch
              label={t("settings.notify.focus")}
              checked={bool(get("automation", "follow-focus"), true)}
              onChange={(v) => set("automation", { "follow-focus": v })}
            />
          }
        />
        <Row
          icon="user"
          title={t("settings.notify.catchUp")}
          subtitle={t("settings.notify.catchUpHint")}
          end={
            <Switch
              label={t("settings.notify.catchUp")}
              checked={bool(get("automation", "catch-up-on-return"), true)}
              onChange={(v) => set("automation", { "catch-up-on-return": v })}
            />
          }
        />
      </Group>

      <Section title={t("settings.notify.about")} />
      <Group>
        {SOURCES.map((source) => (
          <Row
            key={source}
            icon={source === "reminders" ? "clock" : source === "agents" ? "agent" : "bell"}
            title={t(`settings.notify.source.${source}`)}
            // Unset: it speaks when the request said "tell me", else it's a toast.
            subtitle={mode(source) ? undefined : t("settings.notify.asAsked")}
            end={
              <Segmented<Announce>
                label={t(`settings.notify.source.${source}`)}
                value={mode(source)}
                onChange={(m) =>
                  set("automation", {
                    sources: { ...(typeof sources === "object" && sources !== null ? sources : {}), [source]: m },
                  })
                }
                options={(["speak", "toast", "silent"] as const).map((value) => ({
                  value,
                  label: t(`settings.notify.mode.${value}`),
                }))}
              />
            }
          />
        ))}
      </Group>
      <Note>{t("settings.notify.note")}</Note>

      <Section title={t("settings.activities.title")} aside={t("settings.activities.hint")} />
      <Group>
        {ACTIVITIES.map((kind) => (
          <Row
            key={kind}
            icon={kind === "media" ? "music" : kind === "timer" ? "clock" : kind === "download" ? "download" : "code"}
            title={t(`settings.activities.${kind}`)}
            end={
              <Switch
                label={t(`settings.activities.${kind}`)}
                checked={bool(field(live, kind), true)}
                onChange={(v) =>
                  set("automation", {
                    "live-activities": {
                      ...Object.fromEntries(ACTIVITIES.map((k) => [k, bool(field(live, k), true)])),
                      [kind]: v,
                    },
                  })
                }
              />
            }
          />
        ))}
      </Group>
    </>
  );
}
