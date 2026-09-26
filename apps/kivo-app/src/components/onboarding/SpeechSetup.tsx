/**
 * "Choose how KIVO hears and speaks" (UX §4, UX-60): two columns, Listening and Speaking. Each
 * picks a level for this PC (Recommended, High, Medium, Low, from the recommendation's
 * priorities) and then walks its model through three steps the user sees and drives:
 *
 * 1. Download: its size and licence up front, nothing until the button; progress with Pause and
 *    Cancel; Resume or Try again after.
 * 2. Load and test (`voice.test`): loaded in a separate worker and tested; a recognizer hears
 *    KIVO's test sentence (what it heard is shown), a voice says it aloud.
 * 3. Use this (`voice.switch`, which checks once more and swaps it in safely).
 *
 * Setup goes on when both columns are in use (`speechReady`). Someone who knows what they want
 * picks any model below.
 */
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Method, type ModelItem, type RecommendationItem, type SpeechEngineItem } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Icon, type IconName } from "../../icons";
import { cn } from "../../lib/cn";
import { Button, Explain, Meter, Section, Segmented, Spinner, Tag, useToast } from "../ui";
import { SpeechChooser, VoiceList } from "../voice/SpeechChooser";
import { working, type Slot, type Speech } from "../voice/useSpeech";

/** The levels and what each asks the recommendation for (`kivo_voice::recommend::Priority`). */
const LEVELS = [
  { id: "recommended", priority: "balanced" },
  { id: "high", priority: "accuracy" },
  { id: "medium", priority: "speed" },
  { id: "low", priority: "resources" },
] as const;
type Level = (typeof LEVELS)[number]["id"];

/** What `voice.test` found. */
interface Tested {
  said: string;
  heard: string;
  passed: boolean;
}

const message = (e: unknown) => (e instanceof Error ? e.message : String(e));

/** Both chosen models are here, in use, and nothing is switching: setup can go on. */
export function speechReady(speech: Speech): boolean {
  const { choices, progress } = speech;
  if (!choices) return false;
  const ready = (id: string) => choices.engines.find((e) => e.id === id)?.ready ?? false;
  return ready(choices.stt) && ready(choices.tts) && !working(progress.stt) && !working(progress.tts);
}

export function SpeechSetup({ speech }: { speech: Speech }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [plans, setPlans] = useState<Partial<Record<Level, RecommendationItem>>>({});
  const [mine, setMine] = useState(false);
  useEffect(() => {
    if (!connected) return;
    for (const l of LEVELS) {
      void request<RecommendationItem>(Method.voiceRecommend, { priority: l.priority })
        .then((r) => setPlans((all) => ({ ...all, [l.id]: r })))
        .catch(() => {});
    }
  }, [connected, request]);
  if (!speech.choices) return <Spinner label={t("onboarding.speech.loading")} />;

  return (
    <>
      <div className="k-setup">
        <Column slot="stt" speech={speech} plans={plans} />
        <Column slot="tts" speech={speech} plans={plans} />
      </div>
      {plans.recommended && <p className="k-note">{plans.recommended.reason}</p>}
      <div className="k-speech__actions">
        <Button size="sm" variant="plain" icon={mine ? "up" : "chevronDown"} onClick={() => setMine((m) => !m)}>
          {t("onboarding.speech.myself")}
        </Button>
      </div>
      {mine && (
        <>
          <Section title={t("speech.sttTitle")} />
          <SpeechChooser slot="stt" speech={speech} />
          <Section title={t("speech.ttsTitle")} />
          <SpeechChooser slot="tts" speech={speech} />
        </>
      )}
    </>
  );
}

/** One column: the level, then the chosen model's download → load and test → use. */
function Column({
  slot,
  speech,
  plans,
}: {
  slot: Slot;
  speech: Speech;
  plans: Partial<Record<Level, RecommendationItem>>;
}) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [level, setLevel] = useState<Level>("recommended");
  const [tested, setTested] = useState<{ engine: string; result: Tested } | null>(null);
  // "Use this" was pressed for this engine: its switch's stages belong to step 3, not step 2.
  const [using, setUsing] = useState<string | null>(null);
  const choices = speech.choices;
  if (!choices) return null;

  const plan = plans[level];
  const id = slot === "stt" ? plan?.sttEngine : plan?.ttsEngine;
  const engine: SpeechEngineItem | undefined = choices.engines.find((e) => e.id === id);
  const model: ModelItem | undefined = engine?.model ? speech.models.find((m) => m.id === engine.model) : undefined;
  const current = slot === "stt" ? choices.stt : choices.tts;
  const progress = speech.progress[slot];
  const stage = progress && progress.engine === id ? progress.stage : null;
  const failure = progress && progress.engine === id && stage === "failed" ? progress.message : null;
  const inUse = engine !== undefined && current === engine.id && engine.ready && !working(progress);
  const result = tested && tested.engine === id ? tested.result : null;

  // Step 1: the model on this PC.
  const downloaded = engine?.ready ?? false;
  const percent = model?.downloading ?? 0;
  const downloading = model?.state === "downloading" || model?.state === "installing";
  const paused = model?.state === "paused";
  const downloadFailed = model?.state === "error";
  // Step 2: loaded and tested (in use counts: it passed when it was chosen).
  const chosen = engine !== undefined && using === engine.id;
  const testing = !chosen && (stage === "loading" || stage === "testing");
  const testFailed = !chosen && failure !== null;
  const passed = inUse || (result?.passed ?? false);
  // Step 3: switching to it (a switch checks and tests once more before it swaps it in).
  const switching = chosen && !inUse && working(progress);
  const switchFailed = chosen && failure !== null;

  const act = (method: Method, params: unknown) =>
    void request(method, params).catch((e: unknown) => toast(message(e)));
  const loadAndTest = () => {
    if (!engine) return;
    setTested(null);
    setUsing(null);
    request<Tested>(Method.voiceTest, {
      slot,
      engine: engine.id,
      voice: slot === "tts" ? choices.ttsVoice || null : null,
    })
      .then((r) => setTested({ engine: engine.id, result: r }))
      // A failure shows in the step, from its `failed` stage.
      .catch(() => {});
  };
  const use = () => {
    if (!engine) return;
    setUsing(engine.id);
    speech.choose(slot, engine.id).catch((e: unknown) => {
      setUsing(null);
      toast(message(e));
    });
  };

  const downloadStatus = !engine?.model
    ? t("onboarding.speech.builtIn")
    : downloaded
      ? t("onboarding.speech.downloaded", { size: engine.downloadMb })
      : downloading
        ? t("onboarding.speech.downloading", { percent: percent / 100 })
        : paused
          ? t("onboarding.speech.paused", { percent: percent / 100 })
          : downloadFailed
            ? (model?.error ?? t("onboarding.speech.downloadFailed"))
            : t("onboarding.speech.size", { size: engine.downloadMb, license: engine.license }) +
              (engine.commercialUse ? "" : ` · ${t("speech.personalUseTitle")}`);
  const testStatus = inUse
    ? t("onboarding.speech.works")
    : testing
      ? t(`speech.stage.${stage}`)
      : testFailed
        ? (failure ?? "")
        : result
          ? slot === "tts"
            ? t("onboarding.speech.spoke")
            : result.passed
              ? t("onboarding.speech.heard", { text: result.heard })
              : t("onboarding.speech.heardWrong", { heard: result.heard || "…", said: result.said })
          : t(`onboarding.speech.${slot}.testHint`);
  const useStatus = inUse
    ? t("onboarding.speech.inUseLine")
    : switching
      ? t("onboarding.speech.switching")
      : switchFailed
        ? (failure ?? "")
        : t(`onboarding.speech.${slot}.useHint`);

  return (
    <section className="k-setup__col" aria-labelledby={`setup-${slot}`}>
      <header className="k-setup__head">
        <span className="k-setup__icon" aria-hidden>
          <Icon name={slot === "stt" ? "mic" : "volume"} />
        </span>
        <div className="k-setup__heading">
          <h3 id={`setup-${slot}`} className="k-setup__title">
            {t(`onboarding.speech.${slot}.title`)}
          </h3>
          <Explain tip={t(`onboarding.speech.${slot}.tip`)}>{t(`onboarding.speech.${slot}.term`)}</Explain>
        </div>
      </header>

      <Segmented<Level>
        label={t(`onboarding.speech.${slot}.title`)}
        value={level}
        onChange={(l) => {
          setLevel(l);
          setTested(null);
        }}
        options={LEVELS.map((l) => ({ value: l.id, label: t(`onboarding.speech.level.${l.id}`) }))}
      />
      <p className="k-setup__about">{t(`onboarding.speech.${slot}.${level}`)}</p>
      <p className="k-setup__model">
        <span>{engine ? engine.name : t("onboarding.speech.checking")}</span>
        {inUse && <Tag tone="success">{t("onboarding.speech.inUse")}</Tag>}
      </p>

      {engine && (
        <ol className="k-setup__steps">
          <Step
            n={1}
            title={t("onboarding.speech.download")}
            status={downloadStatus}
            done={downloaded}
            active={!downloaded}
            bad={downloadFailed}
          >
            {downloading && engine.model && (
              <>
                <Meter value={percent} label={t("onboarding.speech.download")} />
                <div className="k-setup__buttons">
                  <Button
                    size="sm"
                    variant="plain"
                    icon="pause"
                    onClick={() => act(Method.modelsPause, { id: engine.model })}
                  >
                    {t("onboarding.speech.pause")}
                  </Button>
                  <Button
                    size="sm"
                    variant="plain"
                    icon="close"
                    onClick={() => act(Method.modelsCancel, { id: engine.model })}
                  >
                    {t("onboarding.speech.cancel")}
                  </Button>
                </div>
              </>
            )}
            {!downloaded && !downloading && engine.model && (
              <div className="k-setup__buttons">
                <Button
                  size="sm"
                  variant="primary"
                  icon="download"
                  onClick={() => act(Method.modelsInstall, { id: engine.model })}
                >
                  {paused
                    ? t("onboarding.speech.resume")
                    : downloadFailed
                      ? t("onboarding.speech.retry")
                      : t("onboarding.speech.downloadButton")}
                </Button>
                {paused && (
                  <Button size="sm" variant="plain" onClick={() => act(Method.modelsCancel, { id: engine.model })}>
                    {t("onboarding.speech.cancel")}
                  </Button>
                )}
              </div>
            )}
          </Step>

          <Step
            n={2}
            title={t("onboarding.speech.test")}
            status={testStatus}
            done={passed}
            active={downloaded && !passed}
            bad={testFailed || (result !== null && !result.passed)}
          >
            {testing && <Spinner label={testStatus} />}
            {downloaded && !inUse && !testing && (
              <div className="k-setup__buttons">
                <Button
                  size="sm"
                  variant={result?.passed ? "plain" : "primary"}
                  icon={result || testFailed ? "refresh" : "play"}
                  onClick={loadAndTest}
                >
                  {result || testFailed ? t("onboarding.speech.testAgain") : t("onboarding.speech.loadTest")}
                </Button>
              </div>
            )}
          </Step>

          <Step
            n={3}
            title={t("onboarding.speech.use")}
            status={useStatus}
            done={inUse}
            active={passed && !inUse}
            bad={switchFailed}
          >
            {switching && <Spinner label={useStatus} />}
            {passed && !inUse && !switching && (
              <div className="k-setup__buttons">
                <Button size="sm" variant="primary" icon="check" onClick={use}>
                  {t("onboarding.speech.useThis")}
                </Button>
              </div>
            )}
          </Step>
        </ol>
      )}

      {slot === "tts" && inUse && <VoiceList speech={speech} />}
    </section>
  );
}

/** One numbered step: a check when done, its line of status, and its controls. */
function Step({
  n,
  title,
  status,
  done,
  active,
  bad = false,
  children,
}: {
  n: number;
  title: string;
  status: string;
  done: boolean;
  active: boolean;
  bad?: boolean;
  children?: ReactNode;
}) {
  const icon: IconName | null = done ? "check" : bad ? "warning" : null;
  return (
    <li className={cn("k-setup__step", done && "is-done", active && "is-active", bad && "is-bad")}>
      <span className="k-setup__num" aria-hidden>
        {icon ? <Icon name={icon} /> : n}
      </span>
      <div className="k-setup__body">
        <div className="k-setup__step-title">{title}</div>
        <div className="k-setup__status">{status}</div>
        {children}
      </div>
    </li>
  );
}
