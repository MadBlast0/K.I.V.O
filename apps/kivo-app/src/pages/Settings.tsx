/**
 * Settings (UX §5). Sounds is here now (VOICE-27): master switch, the sound set with a preview,
 * volume relative to the system, and every cue on its own with its own preview. The other tabs
 * arrive with UX-31 (M7).
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import { Button, Group, IconButton, Note, Row, Section, Select, Slider, Switch, useToast } from "../components/ui";
import { Method, type SoundCue, type SoundSet } from "../ipc/generated";
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

export function Settings() {
  const { t } = useTranslation();
  return (
    <>
      <PageHeader title={t("nav.settings")} subtitle={t("settings.subtitle")} />
      <Section title={t("settings.sounds")} />
      <SoundSettings />
    </>
  );
}
