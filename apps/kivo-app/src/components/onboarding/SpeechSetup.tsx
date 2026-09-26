/**
 * "Choose how KIVO hears and speaks" (UX §4, UX-60): four presets for this PC (Recommended, High,
 * Medium, Low), each naming the two models it uses and what still has to download. Setting one up
 * asks first when something must download (with sizes and licences), then each model is
 * downloaded, loaded once and tested, and the step shows how far it got. Anything that fails says
 * why, with Try again and another preset a click away. Someone who knows what they want picks the
 * models themselves below.
 */
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Method, type RecommendationItem, type SpeechEngineItem } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Alert, Button, Explain, Group, OptionCard, Pill, RadioGroup, Row, Section, Spinner, useToast } from "../ui";
import { Icon } from "../../icons";
import { SpeechChooser, VoiceList } from "../voice/SpeechChooser";
import type { Slot, Speech } from "../voice/useSpeech";

/** The presets and what each asks the recommendation for (`kivo_voice::recommend::Priority`). */
const PRESETS = [
  { id: "recommended", priority: "balanced" },
  { id: "high", priority: "accuracy" },
  { id: "medium", priority: "speed" },
  { id: "low", priority: "resources" },
] as const;
type PresetId = (typeof PRESETS)[number]["id"];

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Both models are on this PC, chosen and not switching: setup can go on. */
export function speechReady(speech: Speech): boolean {
  const { choices, progress } = speech;
  if (!choices) return false;
  const ready = (id: string) => choices.engines.find((e) => e.id === id)?.ready ?? false;
  const busy = (["stt", "tts"] as const).some((s) => {
    const p = progress[s];
    return p !== null && p.stage !== "ready" && p.stage !== "failed";
  });
  return ready(choices.stt) && ready(choices.tts) && !busy;
}

export function SpeechSetup({ speech }: { speech: Speech }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [plans, setPlans] = useState<Partial<Record<PresetId, RecommendationItem>>>({});
  const [preset, setPreset] = useState<PresetId>("recommended");
  const [asking, setAsking] = useState(false);
  const [mine, setMine] = useState(false);
  useEffect(() => {
    if (!connected) return;
    for (const p of PRESETS) {
      void request<RecommendationItem>(Method.voiceRecommend, { priority: p.priority })
        .then((r) => setPlans((all) => ({ ...all, [p.id]: r })))
        .catch(() => {});
    }
  }, [connected, request]);
  const { choices } = speech;
  if (!choices) return <Spinner label={t("onboarding.speech.loading")} />;

  const engine = (id: string | null | undefined) => choices.engines.find((e) => e.id === id);
  const plan = plans[preset];
  const wanted = plan ? [engine(plan.sttEngine), engine(plan.ttsEngine)] : [];
  const missing = wanted.filter((e): e is SpeechEngineItem => e !== undefined && !e.ready);
  const inUse =
    plan !== undefined && choices.stt === plan.sttEngine && choices.tts === plan.ttsEngine && missing.length === 0;
  const busy = (["stt", "tts"] as const).some((s) => {
    const p = speech.progress[s];
    return p !== null && p.stage !== "ready" && p.stage !== "failed";
  });

  const setUp = async () => {
    if (!plan) return;
    setAsking(false);
    try {
      // Also when it's already the choice but not on this PC yet (a first launch's default).
      const needs = (slot: Slot, id: string) =>
        id !== (slot === "stt" ? choices.stt : choices.tts) || missing.some((e) => e.id === id);
      if (plan.sttEngine && needs("stt", plan.sttEngine)) await speech.choose("stt", plan.sttEngine);
      if (needs("tts", plan.ttsEngine)) await speech.choose("tts", plan.ttsEngine);
    } catch (e) {
      toast(message(e));
    }
  };
  const start = () => {
    if (missing.length > 0) setAsking(true);
    else void setUp();
  };

  // One line per preset: what it hears and speaks with, and what's left to download.
  const describe = (id: PresetId) => {
    const p = plans[id];
    if (!p) return t("onboarding.speech.checking");
    const stt = engine(p.sttEngine);
    const tts = engine(p.ttsEngine);
    const download = [stt, tts]
      .filter((e): e is SpeechEngineItem => e !== undefined && !e.ready)
      .reduce((mb, e) => mb + e.downloadMb, 0);
    return (
      <>
        {t(`onboarding.speech.presetHint.${id}`)}
        <span className="k-setup__uses">
          {t("onboarding.speech.uses", { stt: stt?.name ?? "—", tts: tts?.name ?? "—" })}
          {" · "}
          {download > 0 ? t("onboarding.speech.toDownload", { size: download }) : t("onboarding.speech.onThisPc")}
        </span>
      </>
    );
  };

  return (
    <>
      <RadioGroup<PresetId> label={t("onboarding.speech.title")} value={preset} onChange={setPreset}>
        {PRESETS.map((p) => (
          <OptionCard
            key={p.id}
            value={p.id}
            title={t(`onboarding.speech.preset.${p.id}`)}
            badge={p.id === "recommended" ? <Pill tone="accent">{t("onboarding.speech.forThisPc")}</Pill> : undefined}
            description={describe(p.id)}
          />
        ))}
      </RadioGroup>
      {plan && preset === "recommended" && <p className="k-note">{plan.reason}</p>}

      <div className="k-setup__action">
        {inUse && !busy ? (
          <p className="k-setup__done" role="status">
            <Icon name="check" />
            {t("onboarding.speech.allSet")}
          </p>
        ) : (
          <Button
            variant="primary"
            icon={missing.length > 0 ? "download" : "check"}
            disabled={!plan || busy}
            onClick={start}
          >
            {missing.length > 0 ? t("onboarding.speech.downloadAndSetUp") : t("onboarding.speech.setUp")}
          </Button>
        )}
      </div>

      {asking && (
        // Nothing downloads unasked (DIST-13): what, how big, whose licence.
        <Alert kind="info" title={t("speech.missingTitle")}>
          <p className="k-setup__ask">{t("speech.missingBody")}</p>
          {missing.map((e) => (
            <p key={e.id} className="k-setup__ask">
              <b>{e.name}</b> · {t("speech.missingLine", { size: e.downloadMb, license: e.license })}
              {!e.commercialUse && ` · ${t("speech.personalUseTitle")}`}
            </p>
          ))}
          <div className="k-speech__actions">
            <Button variant="primary" icon="download" onClick={() => void setUp()}>
              {t("speech.downloadAndUse")}
            </Button>
            <Button variant="plain" onClick={() => setAsking(false)}>
              {t("voice.cancel")}
            </Button>
          </div>
        </Alert>
      )}

      <Steps speech={speech} onRetry={() => void setUp()} />

      {speechReady(speech) && <VoiceList speech={speech} />}

      <div className="k-speech__actions">
        <Button size="sm" variant="plain" icon={mine ? "up" : "chevronDown"} onClick={() => setMine((m) => !m)}>
          {t("onboarding.speech.myself")}
        </Button>
      </div>
      {mine && (
        <>
          <Section
            title={t("speech.sttTitle")}
            aside={<Explain tip={t("onboarding.speech.sttTip")}>{t("onboarding.speech.sttTerm")}</Explain>}
          />
          <SpeechChooser slot="stt" speech={speech} />
          <Section
            title={t("speech.ttsTitle")}
            aside={<Explain tip={t("onboarding.speech.ttsTip")}>{t("onboarding.speech.ttsTerm")}</Explain>}
          />
          <SpeechChooser slot="tts" speech={speech} />
        </>
      )}
    </>
  );
}

/** Each model's way to ready: downloading, starting, testing, ready; or why it didn't work. */
function Steps({ speech, onRetry }: { speech: Speech; onRetry: () => void }) {
  const { t } = useTranslation();
  const { choices, progress } = speech;
  if (!choices) return null;
  const rows = (["stt", "tts"] as const).flatMap((slot) => {
    const p = progress[slot];
    if (!p) return [];
    const name = choices.engines.find((e) => e.id === p.engine)?.name ?? p.engine;
    const label =
      p.stage === "downloading" && p.percent !== null
        ? t("speech.stage.downloadingPercent", { percent: p.percent / 100 })
        : t(`speech.stage.${p.stage}`);
    return [{ slot, name, stage: p.stage, label, message: p.message }];
  });
  if (rows.length === 0) return null;
  const failed = rows.some((r) => r.stage === "failed");
  return (
    <>
      <Group>
        {rows.map((r) => (
          <Row
            key={r.slot}
            lead={
              r.stage === "ready" ? (
                <span className="k-row__icon k-setup__ok">
                  <Icon name="check" />
                </span>
              ) : r.stage === "failed" ? (
                <span className="k-row__icon k-setup__bad">
                  <Icon name="warning" />
                </span>
              ) : (
                <span className="k-row__icon">
                  <Spinner label={r.label} />
                </span>
              )
            }
            title={t(`onboarding.speech.slot.${r.slot}`, { name: r.name })}
            subtitle={r.stage === "failed" && r.message ? r.message : r.label}
          />
        ))}
      </Group>
      {failed && (
        <Alert kind="warning" title={t("onboarding.speech.failedTitle")}>
          <p className="k-setup__ask">{t("onboarding.speech.failedBody")}</p>
          <div className="k-speech__actions">
            <Button icon="refresh" onClick={onRetry}>
              {t("onboarding.speech.retry")}
            </Button>
          </div>
        </Alert>
      )}
    </>
  );
}
