/**
 * Settings (UX §5). Sounds is here now (VOICE-27): master switch, the sound set with a preview,
 * volume relative to the system, and every cue on its own with its own preview. Notifications
 * (UX-40) set how each source tells the user, quiet hours and catching up on return; Live
 * activities (UX-15) choose what the collapsed Island shows. The other tabs arrive with UX-31 (M7).
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import {
  Button,
  Group,
  IconButton,
  Note,
  Row,
  Section,
  Segmented,
  Select,
  Slider,
  Switch,
  TextField,
  useToast,
} from "../components/ui";
import { Method, type OverlayPosition, type SoundCue, type SoundSet } from "../ipc/generated";
import { useRuntime } from "../ipc/runtime";

interface Sounds {
  enabled: boolean;
  set: SoundSet;
  volume: number;
  "thinking-cue": boolean;
  off: SoundCue[];
}

/** The sets KIVO makes now; Custom (your own files) is VOICE-28. */
const SETS: ReadonlyArray<SoundSet> = ["soft", "glass", "pulse", "wood", "minimal"];
const CUES: ReadonlyArray<SoundCue> = [
  "listen-start",
  "listen-stop",
  "done",
  "error",
  "thinking",
  "question",
  "approved",
  "cancelled",
  "hangup",
  "notification",
];

export function SoundSettings() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [sounds, setSounds] = useState<Sounds | null>(null);

  useEffect(() => {
    if (!connected) return;
    void request<{ sounds: Sounds }>(Method.settingsGet)
      .then((s) => setSounds(s.sounds))
      .catch(() => {});
  }, [connected, request]);

  const save = useCallback(
    (patch: Partial<Sounds>) => {
      setSounds((s) => (s ? { ...s, ...patch } : s));
      request<{ sounds: Sounds }>(Method.settingsSet, { sounds: patch })
        .then((saved) => setSounds(saved.sounds))
        .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
    },
    [request, toast],
  );
  const preview = (cue?: SoundCue) => {
    if (!sounds) return;
    request(Method.soundsPreview, { set: sounds.set, cue: cue ?? null }).catch((e: unknown) =>
      toast(e instanceof Error ? e.message : String(e)),
    );
  };

  if (!connected) return <Note>{t("voice.notConnected")}</Note>;
  if (!sounds) return null;
  const on = (cue: SoundCue) => (cue === "thinking" ? sounds["thinking-cue"] : !sounds.off.includes(cue));
  const toggle = (cue: SoundCue, value: boolean) =>
    cue === "thinking"
      ? save({ "thinking-cue": value })
      : save({ off: value ? sounds.off.filter((c) => c !== cue) : [...sounds.off, cue] });

  return (
    <>
      <Group>
        <Row
          icon="volume"
          title={t("sounds.enabled")}
          subtitle={t("sounds.enabledHint")}
          end={<Switch label={t("sounds.enabled")} checked={sounds.enabled} onChange={(v) => save({ enabled: v })} />}
        />
        <Row
          icon="music"
          title={t("sounds.set")}
          subtitle={t(`sounds.sets.${sounds.set}Hint`)}
          end={
            <span className="k-inline">
              <Button size="sm" variant="plain" icon="play" onClick={() => preview()}>
                {t("sounds.preview")}
              </Button>
              <Select
                label={t("sounds.set")}
                value={sounds.set}
                onChange={(v) => save({ set: v })}
                items={SETS.map((value) => ({ value, label: t(`sounds.sets.${value}`) }))}
              />
            </span>
          }
        />
        <Row
          icon="speed"
          title={t("sounds.volume")}
          subtitle={t("sounds.volumeHint")}
          end={
            <span className="k-inline k-inline--slider">
              <Slider label={t("sounds.volume")} value={sounds.volume} onChange={(v) => save({ volume: v })} />
            </span>
          }
        />
      </Group>
      <Section title={t("sounds.cues")} aside={t("sounds.cuesHint")} />
      <Group>
        {CUES.map((cue) => (
          <Row
            key={cue}
            lead={
              <IconButton
                icon="play"
                label={t("sounds.previewCue", { cue: t(`sounds.cue.${cue}`) })}
                onClick={() => preview(cue)}
              />
            }
            title={t(`sounds.cue.${cue}`)}
            subtitle={t(`sounds.cueHint.${cue}`)}
            end={
              <Switch
                label={t(`sounds.cue.${cue}`)}
                checked={sounds.enabled && on(cue)}
                disabled={!sounds.enabled}
                onChange={(v) => toggle(cue, v)}
              />
            }
          />
        ))}
      </Group>
    </>
  );
}

/** Where the Island appears (UX-13): top center, bottom center, or where it was dragged. */
export function IslandSettings() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const position = connected ? link.snapshot?.island?.position : undefined;
  if (!connected || !position) return null;
  return (
    <Group>
      <Row
        icon="island"
        title={t("settings.island.position")}
        subtitle={t("settings.island.positionHint")}
        end={
          <Select<OverlayPosition>
            label={t("settings.island.position")}
            value={position}
            onChange={(v) => {
              request(Method.settingsSet, { overlay: { position: v } }).catch((e: unknown) =>
                toast(e instanceof Error ? e.message : String(e)),
              );
            }}
            items={(["top-center", "bottom-center", "remember-drag"] as const).map((value) => ({
              value,
              label: t(`settings.island.positions.${value}`),
            }))}
          />
        }
      />
    </Group>
  );
}

type Announce = "speak" | "toast" | "silent";

interface Automation {
  "quiet-hours": string;
  sources: Record<string, Announce>;
  "catch-up-on-return": boolean;
  "live-activities": { media: boolean; timer: boolean; download: boolean; agent: boolean };
}

/** The sources a notification comes from (UX-40); agents that need an answer always show. */
const SOURCES = ["tasks", "reminders", "watchers", "routines", "agents"] as const;
const ACTIVITIES = ["media", "timer", "download", "agent"] as const;

/** "22:00-07:00", or empty for none. */
export function validQuietHours(text: string): boolean {
  if (text.trim() === "") return true;
  return /^([01]?\d|2[0-3]):[0-5]\d\s*-\s*([01]?\d|2[0-3]):[0-5]\d$/.test(text.trim());
}

/** Notifications (UX-40) and live activities (UX-15). */
export function NotificationSettings() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [auto, setAuto] = useState<Automation | null>(null);
  const [quiet, setQuiet] = useState("");

  useEffect(() => {
    if (!connected) return;
    void request<{ automation: Automation }>(Method.settingsGet)
      .then((s) => {
        setAuto(s.automation);
        setQuiet(s.automation["quiet-hours"]);
      })
      .catch(() => {});
  }, [connected, request]);

  const save = useCallback(
    (patch: Partial<Automation>) => {
      request<{ automation: Automation }>(Method.settingsSet, { automation: patch })
        .then((saved) => setAuto(saved.automation))
        .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
    },
    [request, toast],
  );

  if (!connected || !auto) return null;
  const quietOk = validQuietHours(quiet);
  return (
    <>
      <Group>
        {SOURCES.map((source) => (
          <Row
            key={source}
            icon={source === "reminders" ? "clock" : source === "agents" ? "agent" : "bell"}
            title={t(`settings.notify.source.${source}`)}
            // Unset: it speaks when the request said "tell me", else it's a toast.
            subtitle={auto.sources[source] ? undefined : t("settings.notify.asAsked")}
            end={
              <Segmented<Announce>
                label={t(`settings.notify.source.${source}`)}
                value={auto.sources[source]}
                onChange={(mode) => save({ sources: { ...auto.sources, [source]: mode } })}
                options={(["speak", "toast", "silent"] as const).map((value) => ({
                  value,
                  label: t(`settings.notify.mode.${value}`),
                }))}
              />
            }
          />
        ))}
        <Row
          icon="moon"
          title={t("settings.notify.quietHours")}
          subtitle={quietOk ? t("settings.notify.quietHoursHint") : t("settings.notify.quietHoursBad")}
          end={
            <TextField
              aria-label={t("settings.notify.quietHours")}
              placeholder="22:00-07:00"
              value={quiet}
              aria-invalid={!quietOk}
              onChange={(e) => setQuiet(e.target.value)}
              onBlur={() => quietOk && quiet !== auto["quiet-hours"] && save({ "quiet-hours": quiet.trim() })}
              style={{ width: 140 }}
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
              checked={auto["catch-up-on-return"]}
              onChange={(v) => save({ "catch-up-on-return": v })}
            />
          }
        />
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
                checked={auto["live-activities"][kind]}
                onChange={(v) => save({ "live-activities": { ...auto["live-activities"], [kind]: v } })}
              />
            }
          />
        ))}
      </Group>
    </>
  );
}

export function Settings() {
  const { t } = useTranslation();
  return (
    <>
      <PageHeader title={t("nav.settings")} subtitle={t("settings.subtitle")} />
      <Section title={t("settings.island.title")} />
      <IslandSettings />
      <Section title={t("settings.sounds")} />
      <SoundSettings />
      <Section title={t("settings.notify.title")} />
      <NotificationSettings />
    </>
  );
}
