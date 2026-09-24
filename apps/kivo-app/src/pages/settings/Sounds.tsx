/**
 * Settings → Sounds (VOICE-27/28, UX-31): the master switch, the sound set (each with a preview),
 * every cue on its own with its own preview and your own sound for it (a `.wav` or `.ogg`, in any
 * set; the Custom set is your sounds with Soft for the rest), a notification sound of its own,
 * and the volume relative to Windows.
 */
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Group,
  IconButton,
  Note,
  Row,
  Section,
  Select,
  Slider,
  Switch,
  Tag,
  useToast,
} from "../../components/ui";
import { Method, type SoundCue, type SoundSet } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { bool, num, oneOf, strings } from "../../lib/settings";
import { message, useConfig } from "./useConfig";

const SETS: ReadonlyArray<SoundSet> = ["soft", "glass", "pulse", "wood", "minimal", "custom"];

/** A file as base64, for the runtime. */
async function base64(file: File): Promise<string> {
  const bytes = new Uint8Array(await file.arrayBuffer());
  const chunks: string[] = [];
  // In slices, so a 2 MB file doesn't overflow the argument list.
  for (let i = 0; i < bytes.length; i += 0x8000) {
    chunks.push(String.fromCodePoint(...bytes.subarray(i, i + 0x8000)));
  }
  return btoa(chunks.join(""));
}
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
  const custom = strings(get("sounds", "custom"));
  const notificationSet = oneOf(get("sounds", "notification-set"), SETS) ?? "same";
  const thinking = bool(get("sounds", "thinking-cue"));
  const on = (cue: SoundCue) => (cue === "thinking" ? thinking : !off.includes(cue));
  const toggle = (cue: SoundCue, value: boolean) =>
    cue === "thinking"
      ? set("sounds", { "thinking-cue": value })
      : set("sounds", { off: value ? off.filter((c) => c !== cue) : [...off, cue] });
  const preview = (setName: SoundSet, cue?: SoundCue) =>
    request(Method.soundsPreview, { set: setName, cue: cue ?? null }).catch((e: unknown) => toast(message(e)));
  const importFor = (cue: SoundCue, file: File) => {
    if (file.size > 2 * 1024 * 1024) {
      toast(t("sounds.tooBig"));
      return;
    }
    void base64(file)
      .then((data) => request(Method.soundsImport, { cue, name: file.name, data }))
      .then(() => toast(t("sounds.imported", { cue: t(`sounds.cue.${cue}`) })))
      .catch((e: unknown) => toast(message(e)));
  };
  const clear = (cue: SoundCue) => void request(Method.soundsClear, { cue }).catch((e: unknown) => toast(message(e)));

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
              <>
                {custom.includes(cue) ? (
                  <>
                    <Tag tone="accent">{t("sounds.yours")}</Tag>
                    <Button size="sm" variant="plain" onClick={() => clear(cue)}>
                      {t("sounds.useSets")}
                    </Button>
                  </>
                ) : (
                  <ImportButton
                    cue={cue}
                    label={t("sounds.importFor", { cue: t(`sounds.cue.${cue}`) })}
                    onFile={importFor}
                  />
                )}
                <Switch
                  label={t(`sounds.cue.${cue}`)}
                  checked={enabled && on(cue)}
                  disabled={!enabled}
                  onChange={(v) => toggle(cue, v)}
                />
              </>
            }
          />
        ))}
        <Row
          icon="bell"
          title={t("sounds.notificationSound")}
          subtitle={t("sounds.notificationSoundHint")}
          end={
            <Select
              label={t("sounds.notificationSound")}
              value={notificationSet}
              onChange={(v) => set("sounds", { "notification-set": v === "same" ? null : v })}
              items={[
                { value: "same", label: t("sounds.sameAsSet") },
                ...SETS.filter((s) => s !== "custom").map((s) => ({ value: s, label: t(`sounds.sets.${s}`) })),
              ]}
            />
          }
        />
      </Group>
      <Note>{t("sounds.importNote")}</Note>

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

/** "Import…" for one cue: a hidden file picker for `.wav` and `.ogg`. */
function ImportButton({
  cue,
  label,
  onFile,
}: {
  cue: SoundCue;
  label: string;
  onFile: (cue: SoundCue, file: File) => void;
}) {
  const { t } = useTranslation();
  const input = useRef<HTMLInputElement>(null);
  return (
    <>
      <input
        ref={input}
        type="file"
        hidden
        accept=".wav,.ogg,audio/wav,audio/ogg"
        aria-label={label}
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) onFile(cue, file);
          e.target.value = "";
        }}
      />
      <Button size="sm" variant="plain" icon="upload" onClick={() => input.current?.click()}>
        {t("sounds.import")}
      </Button>
    </>
  );
}
