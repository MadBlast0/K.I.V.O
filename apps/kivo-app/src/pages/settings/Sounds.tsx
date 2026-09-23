/**
 * Settings → Sounds (VOICE-27, UX-31): the master switch, the sound set (each with a preview),
 * every cue on its own with its own preview, and the volume relative to Windows. Custom sounds
 * (your own files) arrive with VOICE-28.
 */
import { useTranslation } from "react-i18next";
import { Group, IconButton, Note, Pill, Row, Section, Slider, Switch, useToast } from "../../components/ui";
import { Icon } from "../../icons";
import { Method, type SoundCue, type SoundSet } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { bool, num, oneOf, strings } from "../../lib/settings";
import { message, useConfig } from "./useConfig";

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

export function SoundsTab() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const { settings, get, set } = useConfig();
  if (link?.status !== "connected") return <Note>{t("voice.notConnected")}</Note>;
  if (!settings) return null;
  const enabled = bool(get("sounds", "enabled"), true);
  const chosen = oneOf(get("sounds", "set"), SETS) ?? "soft";
  const off = strings(get("sounds", "off"));
  const thinking = bool(get("sounds", "thinking-cue"));
  const on = (cue: SoundCue) => (cue === "thinking" ? thinking : !off.includes(cue));
  const toggle = (cue: SoundCue, value: boolean) =>
    cue === "thinking"
      ? set("sounds", { "thinking-cue": value })
      : set("sounds", { off: value ? off.filter((c) => c !== cue) : [...off, cue] });
  const preview = (setName: SoundSet, cue?: SoundCue) =>
    request(Method.soundsPreview, { set: setName, cue: cue ?? null }).catch((e: unknown) => toast(message(e)));

  return (
    <>
      <Group>
        <Row
          icon="volume"
          title={t("sounds.enabled")}
          subtitle={t("sounds.enabledHint")}
          end={<Switch label={t("sounds.enabled")} checked={enabled} onChange={(v) => set("sounds", { enabled: v })} />}
        />
      </Group>
      <Section title={t("sounds.set")} />
      <div className="k-sound-sets" role="radiogroup" aria-label={t("sounds.set")}>
        {SETS.map((s) => (
          <div key={s} className="k-sound-set" data-on={chosen === s ? "" : undefined}>
            <button
              type="button"
              role="radio"
              aria-checked={chosen === s}
              className="k-sound-set__pick"
              onClick={() => set("sounds", { set: s })}
            >
              <span className="k-radio" aria-hidden>
                {chosen === s && <span className="k-radio__dot" />}
              </span>
              <span>
                <b>{t(`sounds.sets.${s}`)}</b>
                <span className="k-meta">{t(`sounds.sets.${s}Hint`)}</span>
              </span>
            </button>
            <IconButton
              icon="play"
              label={t("sounds.previewSet", { set: t(`sounds.sets.${s}`) })}
              onClick={() => void preview(s)}
            />
          </div>
        ))}
        <div className="k-sound-set k-sound-set--later">
          <span className="k-sound-set__pick">
            <Icon name="upload" />
            <span>
              <b>{t("sounds.sets.custom")}</b>
              <span className="k-meta">{t("sounds.sets.customHint")}</span>
            </span>
          </span>
          <Pill>{t("settings.general.later")}</Pill>
        </div>
      </div>

      <Section title={t("sounds.cues")} aside={t("sounds.cuesHint")} />
      <Group>
        {CUES.map((cue) => (
          <Row
            key={cue}
            lead={
              <IconButton
                icon="play"
                label={t("sounds.previewCue", { cue: t(`sounds.cue.${cue}`) })}
                onClick={() => void preview(chosen, cue)}
              />
            }
            title={t(`sounds.cue.${cue}`)}
            subtitle={t(`sounds.cueHint.${cue}`)}
            end={
              <Switch
                label={t(`sounds.cue.${cue}`)}
                checked={enabled && on(cue)}
                disabled={!enabled}
                onChange={(v) => toggle(cue, v)}
              />
            }
          />
        ))}
      </Group>

      <Section title={t("sounds.volume")} />
      <Group>
        <Row
          icon="speed"
          title={t("sounds.volume")}
          subtitle={t("sounds.volumeHint")}
          end={
            <span className="k-inline k-inline--slider">
              <Slider
                label={t("sounds.volume")}
                value={num(get("sounds", "volume"), 70)}
                onChange={(v) => set("sounds", { volume: v })}
              />
            </span>
          }
        />
      </Group>
    </>
  );
}
