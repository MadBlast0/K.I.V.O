/**
 * Setup's Listen, Speak and Voice pages (UX §4, UX-60), with Think (the brain) between Listen and
 * Speak: KIVO hears you, thinks, and answers.
 *
 * - Listen and Speak are one model picker each (owner, 2026-09-26, like Handy's): a searchable,
 *   scrollable list of the models that fit this PC and language, each with speed and accuracy
 *   (for voices, naturalness) bars, its size, and tags: Recommended for this PC, In use, On this
 *   PC. The chosen model opens to its one next step: Download (size and licence up front; a ring
 *   that pauses it, with MB of MB, speed and time left over the model and the models it needs;
 *   "Installing" while its files are checked), Test (`voice.test`, a separate worker: what a
 *   recognizer heard, or a voice speaking), then Use this (`voice.switch`).
 * - Voice is its own page: the voices of the engine KIVO speaks with, by Female, Male and Other,
 *   grouped by accent and language (Japanese-accented English among them), each playable.
 */
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import {
  Method,
  type ModelItem,
  type RecommendationItem,
  type SpeechEngineItem,
  type VoiceItem,
} from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Icon } from "../../icons";
import { cn } from "../../lib/cn";
import { Button, Segmented, Spinner, useToast } from "../ui";
import { working, type Slot, type Speech } from "../voice/useSpeech";

/** What `voice.test` found. */
interface Tested {
  said: string;
  heard: string;
  passed: boolean;
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** The model for `slot` is here, in use, and nothing is switching: setup can go on. */
export function slotReady(speech: Speech, slot: Slot): boolean {
  const { choices, progress } = speech;
  if (!choices) return false;
  const id = slot === "stt" ? choices.stt : choices.tts;
  return (choices.engines.find((e) => e.id === id)?.ready ?? false) && !working(progress[slot]);
}

/** This PC's recommendation (the balanced one). */
function useRecommendation(): RecommendationItem | null {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [plan, setPlan] = useState<RecommendationItem | null>(null);
  useEffect(() => {
    if (!connected) return;
    void request<RecommendationItem>(Method.voiceRecommend)
      .then(setPlan)
      .catch(() => {});
  }, [connected, request]);
  return plan;
}

/** Five small bars, like Handy's: a 0–100 score at a glance. */
function Bars({ label, value }: { label: string; value: number }) {
  const filled = Math.max(1, Math.round(value / 20));
  return (
    <span className="k-bars" role="img" aria-label={`${label}: ${filled} / 5`}>
      <span className="k-bars__label">{label}</span>
      <span className="k-bars__cells" aria-hidden>
        {[0, 1, 2, 3, 4].map((i) => (
          <i key={i} className={cn(i < filled && "is-on")} />
        ))}
      </span>
    </span>
  );
}

/** The Listen or Speak page's model list. */
export function ModelPicker({ slot, speech }: { slot: Slot; speech: Speech }) {
  const { t } = useTranslation();
  const plan = useRecommendation();
  const [query, setQuery] = useState("");
  const [picked, setPicked] = useState<string | null>(null);
  const choices = speech.choices;
  const recommended = slot === "stt" ? plan?.sttEngine : plan?.ttsEngine;
  const current = slot === "stt" ? choices?.stt : choices?.tts;
  // Models for this language that run on this PC, the likely choices first.
  const engines = useMemo(() => {
    const all = (choices?.engines ?? []).filter((e) => e.slot === slot && e.fitsLanguage && e.privacy !== "cloud");
    const rank = (e: SpeechEngineItem) => [e.id !== recommended, e.id !== current, !e.ready, -e.accuracy];
    return all.toSorted((a, b) => {
      const [x, y] = [rank(a), rank(b)];
      for (let i = 0; i < x.length; i++) if (x[i] !== y[i]) return x[i] < y[i] ? -1 : 1;
      return 0;
    });
  }, [choices, slot, recommended, current]);
  if (!choices) return <Spinner label={t("onboarding.speech.loading")} />;
  const shown = engines.filter((e) => e.name.toLowerCase().includes(query.trim().toLowerCase()));
  const readyCurrent = engines.find((e) => e.id === current && e.ready);
  const selected = picked ?? readyCurrent?.id ?? recommended ?? engines[0]?.id;

  return (
    <div className="k-picker">
      <label className="k-picker__search">
        <Icon name="search" />
        <input
          type="search"
          value={query}
          placeholder={t("onboarding.speech.search")}
          aria-label={t("onboarding.speech.search")}
          onChange={(e) => setQuery(e.target.value)}
        />
      </label>
      <ul className="k-picker__list" aria-label={t(`onboarding.speech.${slot}.list`)}>
        {shown.length === 0 && <li className="k-picker__empty">{t("onboarding.speech.noMatch")}</li>}
        {shown.map((e) => {
          const on = e.id === selected;
          const inUse = e.id === current && e.ready;
          return (
            <li key={e.id} className={cn("k-picker__item", on && "is-on")}>
              <button
                type="button"
                className="k-picker__row"
                aria-pressed={on}
                aria-label={e.name}
                onClick={() => setPicked(e.id)}
              >
                <span className="k-picker__radio" aria-hidden />
                <span className="k-picker__main">
                  <span className="k-picker__name">
                    {e.name}
                    {e.id === recommended && (
                      <span className="k-chip k-chip--accent">{t("onboarding.speech.recommended")}</span>
                    )}
                    {inUse && <span className="k-chip k-chip--good">{t("onboarding.speech.inUse")}</span>}
                    {!inUse && e.ready && e.model && <span className="k-chip">{t("onboarding.speech.onThisPc")}</span>}
                  </span>
                  <span className="k-picker__meta">
                    <Bars label={t("onboarding.speech.speed")} value={e.speed} />
                    <Bars label={t(`onboarding.speech.${slot}.accuracy`)} value={e.accuracy} />
                    <span className="k-picker__size">
                      {e.model ? t("onboarding.speech.mb", { size: e.downloadMb }) : t("onboarding.speech.builtIn")}
                    </span>
                  </span>
                </span>
              </button>
              {on && <ModelAction key={e.id} slot={slot} engine={e} speech={speech} />}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

/** How a model's download is going: its own files and the models it needs, together, from the
 * bytes each has written (their `modelChanged` percentages of their sizes), with the speed and
 * time left over the last few seconds, and whether the files are being checked and unpacked. */
function useDownload(speech: Speech, engine: SpeechEngineItem) {
  const main = engine.model ? speech.models.find((m) => m.id === engine.model) : undefined;
  const parts = (main ? [main.id, ...main.requires] : [])
    .map((id) => speech.models.find((m) => m.id === id))
    .filter((m): m is ModelItem => m !== undefined);
  // The models that weren't here when the download started stay in the total, so it doesn't
  // shrink as the small ones finish.
  const [involved, setInvolved] = useState<string[]>([]);
  const missing = parts.filter((m) => m.state !== "ready" && !involved.includes(m.id)).map((m) => m.id);
  if (missing.length > 0) setInvolved([...involved, ...missing]);
  const counted = parts.filter((m) => involved.includes(m.id) || missing.includes(m.id));
  const total = counted.reduce((sum, m) => sum + m.size, 0);
  const done = counted.reduce(
    (sum, m) =>
      sum +
      (m.state === "ready" || m.state === "installing" || m.state === "updateAvailable"
        ? m.size
        : m.state === "downloading" || m.state === "paused"
          ? (m.size * (m.downloading ?? 0)) / 100
          : 0),
    0,
  );
  const active = counted.some((m) => m.state === "downloading" || m.state === "installing");
  const installing = active && counted.every((m) => m.state !== "downloading");
  const speeds = counted.map((m) => speech.speeds[m.id]).filter((v): v is number => v !== undefined);
  const speed = speeds.length > 0 ? speeds.reduce((a, b) => a + b, 0) : null;
  const left = speed && speed > 0 ? (total - done) / speed : null;
  return {
    active,
    installing,
    paused: main?.state === "paused",
    failed: main?.state === "error" ? (main.error ?? "") : null,
    done,
    total,
    percent: total > 0 ? Math.min(100, Math.round((done * 100) / total)) : 0,
    speed,
    left,
  };
}

/** A ring that fills as a download goes; a spinning arc while its files are checked. */
function Ring({ percent, busy = false }: { percent: number; busy?: boolean }) {
  const r = 11;
  const c = 2 * Math.PI * r;
  return (
    <svg className={cn("k-ring", busy && "is-busy")} viewBox="0 0 28 28" aria-hidden>
      <circle className="k-ring__track" cx="14" cy="14" r={r} />
      <circle
        className="k-ring__fill"
        cx="14"
        cy="14"
        r={r}
        strokeDasharray={c}
        strokeDashoffset={busy ? c * 0.72 : c * (1 - percent / 100)}
      />
    </svg>
  );
}

/** The chosen model's next step, under its row: a line saying how it's going and the one thing
 * to do (Download → a ring while it downloads, which pauses it → Test → Use this → In use). */
function ModelAction({ slot, engine, speech }: { slot: Slot; engine: SpeechEngineItem; speech: Speech }) {
  const { t, i18n } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [result, setResult] = useState<Tested | null>(null);
  // "Use this" was pressed: the switch's own stages (it checks once more) aren't a new test.
  const [using, setUsing] = useState(false);
  const download = useDownload(speech, engine);
  const choices = speech.choices;
  if (!choices) return null;

  const progress = speech.progress[slot];
  const stage = progress?.engine === engine.id ? progress.stage : null;
  const failure = stage === "failed" ? (progress?.message ?? null) : null;
  const current = slot === "stt" ? choices.stt : choices.tts;
  const inUse = current === engine.id && engine.ready && !working(progress);
  const testing = !using && (stage === "loading" || stage === "testing");
  const passed = result?.passed ?? false;
  const switching = using && !inUse && working(progress);

  const mb = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 0 });
  const rate = new Intl.NumberFormat(i18n.language, { maximumFractionDigits: 1 });
  const act = (method: Method) => void request(method, { id: engine.model }).catch((e: unknown) => toast(message(e)));
  const test = () => {
    setResult(null);
    setUsing(false);
    request<Tested>(Method.voiceTest, {
      slot,
      engine: engine.id,
      voice: slot === "tts" ? choices.ttsVoice || null : null,
    })
      .then(setResult)
      // A failure shows here, from its `failed` stage.
      .catch(() => {});
  };
  const use = () => {
    setUsing(true);
    speech.choose(slot, engine.id).catch((e: unknown) => {
      setUsing(false);
      toast(message(e));
    });
  };

  let action: ReactNode = null;
  let line: ReactNode;
  let tone: "plain" | "good" | "bad" = "plain";
  if (inUse) {
    line = t(`onboarding.speech.${slot}.ready`, { name: engine.name });
    tone = "good";
  } else if (!engine.ready) {
    if (download.active) {
      action = download.installing ? (
        <span className="k-model__ring" role="img" aria-label={t("onboarding.speech.installing")}>
          <Ring percent={100} busy />
        </span>
      ) : (
        <button
          type="button"
          className="k-model__ring"
          aria-label={t("onboarding.speech.pauseAt", { percent: download.percent / 100 })}
          onClick={() => act(Method.modelsPause)}
        >
          <Ring percent={download.percent} />
          <Icon name="pause" />
        </button>
      );
      line = download.installing ? (
        t("onboarding.speech.installing")
      ) : (
        <>
          {t("onboarding.speech.bytes", {
            done: mb.format(download.done / 1e6),
            total: mb.format(download.total / 1e6),
          })}
          {download.speed !== null &&
            ` · ${t("onboarding.speech.speedRate", { speed: rate.format(download.speed / 1e6) })}`}
          {download.left !== null && ` · ${leftText(t, download.left)}`}
          <button type="button" className="k-model__link" onClick={() => act(Method.modelsCancel)}>
            {t("onboarding.speech.cancel")}
          </button>
        </>
      );
    } else {
      action = (
        <Button variant="primary" onClick={() => act(Method.modelsInstall)}>
          {download.paused
            ? t("onboarding.speech.resume")
            : download.failed !== null
              ? t("onboarding.speech.retry")
              : t("onboarding.speech.download")}
        </Button>
      );
      if (download.paused) {
        line = (
          <>
            {t("onboarding.speech.paused", {
              done: mb.format(download.done / 1e6),
              total: mb.format(download.total / 1e6),
            })}
            <button type="button" className="k-model__link" onClick={() => act(Method.modelsCancel)}>
              {t("onboarding.speech.cancel")}
            </button>
          </>
        );
      } else if (download.failed !== null) {
        line = download.failed || t("onboarding.speech.downloadFailed");
        tone = "bad";
      } else {
        line =
          t("onboarding.speech.size", { size: engine.downloadMb, license: engine.license }) +
          (engine.commercialUse ? "" : ` · ${t("speech.personalUseTitle")}`) +
          ` · ${t("onboarding.speech.downloadHint")}`;
      }
    }
  } else if (testing || switching) {
    action = <Spinner label={t(`speech.stage.${stage ?? "loading"}`)} />;
    line = switching ? t("onboarding.speech.switching") : t(`speech.stage.${stage ?? "loading"}`);
  } else if (passed) {
    action = (
      <Button variant="primary" onClick={use}>
        {t("onboarding.speech.useThis")}
      </Button>
    );
    line = (
      <>
        {failure ??
          (slot === "stt" ? t("onboarding.speech.heard", { text: result?.heard ?? "" }) : t("onboarding.speech.spoke"))}
        <button type="button" className="k-model__link" onClick={test}>
          {t("onboarding.speech.testAgain")}
        </button>
      </>
    );
    tone = failure ? "bad" : "good";
  } else {
    action = (
      <Button variant="primary" onClick={test}>
        {result || failure ? t("onboarding.speech.testAgain") : t("onboarding.speech.test")}
      </Button>
    );
    if (failure) {
      line = failure;
      tone = "bad";
    } else if (result) {
      line = t("onboarding.speech.heardWrong", { heard: result.heard || "…", said: result.said });
      tone = "bad";
    } else {
      line = t(`onboarding.speech.${slot}.testHint`);
    }
  }

  return (
    <div className="k-model">
      <p className={cn("k-model__line", tone === "good" && "is-good", tone === "bad" && "is-bad")} role="status">
        {tone === "good" && <Icon name="check" />}
        {line}
      </p>
      {action && <div className="k-model__action">{action}</div>}
    </div>
  );
}

/** "About 20 seconds left", "About 3 minutes left". */
function leftText(t: (key: string, options?: Record<string, unknown>) => string, seconds: number): string {
  return seconds < 60
    ? t("onboarding.speech.secondsLeft", { count: Math.max(1, Math.round(seconds)) })
    : t("onboarding.speech.minutesLeft", { count: Math.round(seconds / 60) });
}

type Kind = "female" | "male" | "other";

const kindOf = (v: VoiceItem): Kind => (v.style === "female" ? "female" : v.style === "male" ? "male" : "other");

/** "Heart (American, female)" → "American"; otherwise the voice's language in words. */
function accentOf(v: VoiceItem, language: (code: string) => string): string {
  const inside = /\(([^,)]*)/.exec(v.name)?.[1];
  if (inside) return inside;
  const code = v.languages[0];
  return code && code !== "*" ? language(code) : "";
}

/** The Voice page: the voices of the engine KIVO speaks with, by Female, Male and Other, grouped
 * by accent and language, each playable; voices whose files aren't here yet come with one small
 * download. */
export function VoicePicker({ speech }: { speech: Speech }) {
  const { t, i18n } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [playing, setPlaying] = useState<string | null>(null);
  const { choices } = speech;
  const engine = choices?.engines.find((e) => e.id === choices.tts);
  const selected = choices?.ttsVoice || engine?.voices[0]?.id;
  const [kind, setKind] = useState<Kind>(() => {
    const now = engine?.voices.find((v) => v.id === selected);
    return now ? kindOf(now) : "female";
  });
  if (!choices || !engine) return <Spinner label={t("onboarding.speech.loading")} />;
  if (engine.voices.length === 0) return <p className="k-note">{t("onboarding.voices.none", { name: engine.name })}</p>;

  const names = new Intl.DisplayNames([i18n.language], { type: "language" });
  const language = (code: string) => names.of(code) ?? code;
  const model = speech.models.find((m) => m.id === engine.model);
  const missing = engine.voices.filter((v) => !v.ready);
  const fetching = model?.state === "downloading";
  const counts: Record<Kind, number> = { female: 0, male: 0, other: 0 };
  for (const v of engine.voices) counts[kindOf(v)] += 1;
  // By accent and language, Anime (Japanese-accented English) last.
  const groups = new Map<string, VoiceItem[]>();
  for (const v of engine.voices.filter((x) => kindOf(x) === kind)) {
    const label = v.character === "anime" ? t("speech.anime") : accentOf(v, language) || t("onboarding.voices.more");
    groups.set(label, [...(groups.get(label) ?? []), v]);
  }
  const ordered = [...groups.entries()].toSorted(([a], [b]) =>
    a === t("speech.anime") ? 1 : b === t("speech.anime") ? -1 : a.localeCompare(b),
  );
  const play = (voice: string) => {
    setPlaying(voice);
    speech
      .preview(engine.id, voice)
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setPlaying(null));
  };

  return (
    <div className="k-voices">
      <Segmented<Kind>
        label={t("onboarding.voices.kind")}
        value={kind}
        onChange={setKind}
        options={(["female", "male", "other"] as const).map((k) => ({
          value: k,
          label: `${t(`onboarding.voices.${k}`)} ${counts[k] > 0 ? counts[k] : ""}`.trim(),
        }))}
      />
      {missing.length > 0 && model && (
        <div className="k-voices__new">
          <Icon name="download" />
          <span>{t("onboarding.speech.newVoices", { count: missing.length })}</span>
          <Button
            size="sm"
            disabled={fetching}
            onClick={() =>
              void request(Method.modelsInstall, { id: model.id }).catch((e: unknown) => toast(message(e)))
            }
          >
            {fetching ? t("speech.stage.downloading") : t("onboarding.speech.getVoices")}
          </Button>
        </div>
      )}
      {ordered.length === 0 && <p className="k-note">{t("onboarding.voices.noneOfKind")}</p>}
      {ordered.map(([label, voices]) => (
        <section key={label} className="k-voices__group" aria-label={label}>
          <h3 className="k-voices__title">
            {label}
            {label === t("speech.anime") && <span>{t("speech.animeHint")}</span>}
          </h3>
          <ul className="k-voices__list">
            {voices.map((v) => {
              const on = v.id === selected;
              return (
                <li key={v.id} className={cn("k-voice", on && "is-on", !v.ready && "is-off")}>
                  <button
                    type="button"
                    className="k-voice__pick"
                    disabled={!v.ready}
                    aria-pressed={on}
                    aria-label={v.name}
                    onClick={() => void speech.pickVoice(v.id).catch((e: unknown) => toast(message(e)))}
                  >
                    <span className="k-picker__radio" aria-hidden />
                    <b>{v.name.replace(/\s*\(.*\)$/, "")}</b>
                    {on && <span className="k-chip k-chip--good">{t("onboarding.speech.inUse")}</span>}
                    {!v.ready && <span className="k-chip">{t("onboarding.voices.notHere")}</span>}
                  </button>
                  <button
                    type="button"
                    className="k-voice__play"
                    aria-label={t("speech.preview", { name: v.name })}
                    disabled={!v.ready || playing !== null}
                    onClick={() => play(v.id)}
                  >
                    {playing === v.id ? (
                      <Spinner label={t("speech.preview", { name: v.name })} />
                    ) : (
                      <Icon name="play" />
                    )}
                  </button>
                </li>
              );
            })}
          </ul>
        </section>
      ))}
    </div>
  );
}
