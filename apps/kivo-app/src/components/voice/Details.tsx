/**
 * The Voice page's detail sections (UX-61, UX-23, VOICE-23, VOICE-49): speech recognition and
 * speaking at a glance (engine, status, streaming, language, the live transcript, microphone;
 * voice, preview, speed), KIVO's personality, the user's own words, and the advanced view.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Button,
  Group,
  IconButton,
  Note,
  Radio,
  RadioGroup,
  Row,
  Section,
  Segmented,
  Select,
  Switch,
  Tag,
  TextArea,
  TextField,
  useToast,
} from "../ui";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import type { Speech } from "./useSpeech";

interface Device {
  id: string;
  name: string;
  isDefault: boolean;
}

type Settings = {
  voice: Record<string, unknown>;
  general?: Record<string, unknown>;
  performance?: Record<string, unknown>;
};

function useFail() {
  const toast = useToast();
  return useCallback((e: unknown) => toast(e instanceof Error ? e.message : String(e)), [toast]);
}

function useSettings() {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [settings, setSettings] = useState<Settings | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<Settings>(Method.settingsGet)
      .then(setSettings)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  return { settings, load };
}

const DEFAULT_DEVICE = "default";

/** Speech recognition and speaking at a glance (UX-61). */
export function SpeechSummary({ speech, onChange }: { speech: Speech; onChange: () => void }) {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const { settings, load } = useSettings();
  const [devices, setDevices] = useState<{ inputs: Device[]; outputs: Device[] } | null>(null);
  const connected = link?.status === "connected";
  useEffect(() => {
    if (!connected) return;
    void request<{ inputs: Device[]; outputs: Device[] }>(Method.voiceDevices)
      .then(setDevices)
      .catch(() => {});
  }, [connected, request]);
  const choices = speech.choices;
  if (!choices || !settings) return null;
  const stt = choices.engines.find((e) => e.id === choices.stt);
  const tts = choices.engines.find((e) => e.id === choices.tts);
  const voice = tts?.voices.find((v) => v.id === choices.ttsVoice) ?? tts?.voices[0];
  const snapshot = link?.status === "connected" ? link.snapshot : null;
  const live = snapshot?.session === "listening" ? snapshot.turn?.transcript : null;
  const status = snapshot?.speech.state ?? "missing";
  const code = settings.general?.["language"];
  const language = new Intl.DisplayNames([i18n.language], { type: "language" }).of(
    typeof code === "string" ? code : "en",
  );
  const speed = Number(settings.voice["tts-speed"] ?? 100);
  const set = (section: string, key: string, value: unknown) =>
    request(Method.settingsSet, { [section]: { [key]: value } })
      .then(load)
      .catch(fail);
  const device = (value: unknown) => (typeof value === "string" && value ? value : DEFAULT_DEVICE);
  const deviceItems = (list: Device[]) => [
    { value: DEFAULT_DEVICE, label: t("voice.detail.windowsDefault") },
    ...list.map((d) => ({ value: d.id, label: d.name })),
  ];
  return (
    <>
      <Section title={t("voice.detail.recognition")} />
      <Group>
        <Row
          icon="mic"
          title={stt?.name ?? t("voice.detail.none")}
          subtitle={[
            t(`voice.detail.status.${status}`),
            stt?.streaming ? t("voice.detail.streaming") : t("voice.detail.notStreaming"),
            language,
          ]
            .filter(Boolean)
            .join(" · ")}
          end={
            <Button size="sm" onClick={onChange}>
              {t("voice.detail.change")}
            </Button>
          }
        />
        <Row
          icon="headset"
          title={t("voice.detail.microphone")}
          end={
            devices && (
              <Select
                label={t("voice.detail.microphone")}
                value={device(settings.voice["input-device"])}
                onChange={(v) => void set("voice", "input-device", v === DEFAULT_DEVICE ? null : v)}
                items={deviceItems(devices.inputs)}
              />
            )
          }
        />
        <Row icon="type" title={t("voice.detail.live")} subtitle={live || t("voice.detail.liveIdle")} />
      </Group>

      <Section title={t("voice.detail.speaking")} />
      <Group>
        <Row
          icon="volume"
          title={tts?.name ?? t("voice.detail.none")}
          subtitle={[voice?.name, language].filter(Boolean).join(" · ")}
          end={
            <>
              {tts && (
                <IconButton
                  icon="play"
                  label={t("voice.detail.preview")}
                  onClick={() => void speech.preview(tts.id, voice?.id ?? "").catch(fail)}
                />
              )}
              <Button size="sm" onClick={onChange}>
                {t("voice.detail.change")}
              </Button>
            </>
          }
        />
        <Row
          icon="speed"
          title={t("voice.detail.speed")}
          end={
            <Segmented
              label={t("voice.detail.speed")}
              value={String(speed)}
              onChange={(v) => void set("voice", "tts-speed", Number(v))}
              options={[
                { value: "80", label: t("voice.detail.slower") },
                { value: "100", label: t("voice.detail.normal") },
                { value: "125", label: t("voice.detail.faster") },
                { value: "150", label: t("voice.detail.fastest") },
              ]}
            />
          }
        />
        <Row
          icon="volume"
          title={t("voice.detail.speaker")}
          end={
            devices && (
              <Select
                label={t("voice.detail.speaker")}
                value={device(settings.voice["output-device"])}
                onChange={(v) => void set("voice", "output-device", v === DEFAULT_DEVICE ? null : v)}
                items={deviceItems(devices.outputs)}
              />
            )
          }
        />
        <Row icon="wave" title={t("voice.detail.expressiveness")} subtitle={t("voice.detail.expressivenessNone")} />
      </Group>
    </>
  );
}

/** KIVO's personality (BRAIN-38): Calm by default, Friendly, Witty, or the user's own. */
export function Personality() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const connected = link?.status === "connected";
  const [persona, setPersona] = useState<string | null>(null);
  const [custom, setCustom] = useState("");
  useEffect(() => {
    if (!connected) return;
    void request<{ persona: string; customPersona: string }>(Method.brainsList)
      .then((l) => {
        setPersona(l.persona);
        setCustom(l.customPersona);
      })
      .catch(() => {});
  }, [connected, request]);
  if (persona === null) return null;
  const save = (next: string, style = custom) =>
    request(Method.brainsSetDefault, { persona: next, customPersona: style })
      .then(() => setPersona(next))
      .catch(fail);
  return (
    <>
      <Section title={t("voice.personality.title")} aside={t("voice.personality.hint")} />
      <Group>
        <RadioGroup value={persona} onChange={(v) => void save(v)} label={t("voice.personality.title")}>
          {(["calm", "friendly", "witty", "custom"] as const).map((p) => (
            <Row
              key={p}
              lead={<Radio value={p} label={t(`persona.${p}`)} />}
              title={t(`persona.${p}`)}
              subtitle={t(`voice.personality.${p}`)}
            />
          ))}
        </RadioGroup>
      </Group>
      {persona === "custom" && (
        <div className="k-voice__custom">
          <TextArea
            value={custom}
            maxLength={600}
            placeholder={t("voice.personality.customPlaceholder")}
            aria-label={t("voice.personality.custom")}
            onChange={(e) => setCustom(e.target.value)}
          />
          <Button size="sm" onClick={() => void save("custom", custom)}>
            {t("voice.personality.save")}
          </Button>
        </div>
      )}
      <Note>{t("voice.personality.note")}</Note>
    </>
  );
}

/** The user's own words (VOICE-23): names, apps, projects, for recognition and repair. */
export function Vocabulary() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const connected = link?.status === "connected";
  const [words, setWords] = useState<string[]>([]);
  const [word, setWord] = useState("");
  useEffect(() => {
    if (!connected) return;
    void request<string[] | null>(Method.voiceVocabulary)
      .then((w) => setWords(Array.isArray(w) ? w : []))
      .catch(() => {});
  }, [connected, request]);
  const add = () =>
    request<string[]>(Method.voiceAddWord, { word })
      .then((w) => {
        setWords(w);
        setWord("");
      })
      .catch(fail);
  return (
    <>
      <Section title={t("voice.words.title")} aside={t("voice.words.hint")} />
      <div className="k-voice__words">
        {words.map((w) => (
          <Tag key={w}>
            {w}
            <button
              type="button"
              className="k-link"
              aria-label={t("voice.words.remove", { word: w })}
              onClick={() => void request<string[]>(Method.voiceRemoveWord, { word: w }).then(setWords).catch(fail)}
            >
              ×
            </button>
          </Tag>
        ))}
        {words.length === 0 && <Note>{t("voice.words.none")}</Note>}
      </div>
      <div className="k-voice__custom">
        <TextField
          value={word}
          placeholder={t("voice.words.placeholder")}
          aria-label={t("voice.words.placeholder")}
          onChange={(e) => setWord(e.target.value)}
        />
        <Button size="sm" disabled={!word.trim()} onClick={() => void add()}>
          {t("voice.words.add")}
        </Button>
      </div>
    </>
  );
}

interface Advanced {
  stt: EngineDetail;
  tts: EngineDetail;
  sttFallback: string | null;
  fallbackOn: boolean;
  threads: number;
  threadsSetting: number;
  cores: number;
  sttWarmMinutes: number;
  ttsWarmMinutes: number;
  timing: Record<string, number>;
  modelsFolder: string;
}

interface EngineDetail {
  engine: string;
  name: string | null;
  model: string | null;
  path: string | null;
  devices: string[] | null;
  streaming: boolean;
  license: string | null;
}

/** Advanced (VOICE-49): the exact engines, models and paths, devices, timings, the thread
 * limit, the fallback and how long models stay loaded. */
export function AdvancedVoice() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const fail = useFail();
  const connected = link?.status === "connected";
  const [open, setOpen] = useState(false);
  const [data, setData] = useState<Advanced | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<Advanced>(Method.voiceAdvanced)
      .then(setData)
      .catch(() => {});
  }, [connected, request]);
  useEffect(() => {
    if (open) load();
  }, [open, load]);
  const set = (section: string, key: string, value: unknown) =>
    request(Method.settingsSet, { [section]: { [key]: value } })
      .then(load)
      .catch(fail);
  const engine = (label: string, e: EngineDetail) => (
    <Row
      icon="cpu"
      title={`${label}: ${e.name ?? e.engine}`}
      subtitle={[
        `${t("voice.advanced.engine")} ${e.engine}`,
        e.model && `${t("voice.advanced.model")} ${e.model}`,
        e.devices && `${t("voice.advanced.device")} ${e.devices.join(", ")}`,
        e.path ?? t("voice.advanced.noPath"),
      ]
        .filter(Boolean)
        .join(" · ")}
    />
  );
  return (
    <>
      <Section
        title={t("voice.advanced.title")}
        aside={<Switch checked={open} onChange={setOpen} label={t("voice.advanced.show")} />}
      />
      {open && data && (
        <>
          <Group>
            {engine(t("voice.advanced.recognition"), data.stt)}
            {engine(t("voice.advanced.speaking"), data.tts)}
            <Row
              icon="repeat"
              title={t("voice.advanced.fallback")}
              subtitle={data.sttFallback ?? t("voice.advanced.noFallback")}
              end={
                <Switch
                  checked={data.fallbackOn}
                  onChange={(v) => void set("voice", "stt-fallback", v)}
                  label={t("voice.advanced.fallback")}
                />
              }
            />
            <Row
              icon="performance"
              title={t("voice.advanced.threads", { threads: data.threads, cores: data.cores })}
              end={
                <Select
                  label={t("voice.advanced.threadLimit")}
                  value={String(data.threadsSetting)}
                  onChange={(v) => void set("performance", "speech-threads", Number(v))}
                  items={[
                    { value: "0", label: t("voice.advanced.auto") },
                    ...[1, 2, 4, 6, 8]
                      .filter((n) => n <= data.cores)
                      .map((n) => ({ value: String(n), label: String(n) })),
                  ]}
                />
              }
            />
            <Row
              icon="clock"
              title={t("voice.advanced.cache")}
              subtitle={t("voice.advanced.cacheLine", { stt: data.sttWarmMinutes, tts: data.ttsWarmMinutes })}
              end={
                <Select
                  label={t("voice.advanced.cache")}
                  value={String(data.sttWarmMinutes)}
                  onChange={(v) =>
                    void request(Method.settingsSet, {
                      performance: { "stt-warm-minutes": Number(v), "tts-warm-minutes": Number(v) },
                    })
                      .then(load)
                      .catch(fail)
                  }
                  items={["1", "5", "10", "30", "60"].map((m) => ({
                    value: m,
                    label: t("voice.advanced.minutes", { count: Number(m) }),
                  }))}
                />
              }
            />
            <Row
              icon="speed"
              title={t("voice.advanced.timing")}
              subtitle={Object.entries(data.timing)
                .map(([k, v]) => `${t(`voice.advanced.t.${k}`)} ${v}`)
                .join(" · ")}
            />
            <Row icon="folder" title={t("voice.advanced.folder")} subtitle={data.modelsFolder} />
          </Group>
        </>
      )}
    </>
  );
}
