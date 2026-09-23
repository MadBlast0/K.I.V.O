/**
 * First-launch setup (UX §4, UX-33 steps 1–5, UX-60 and UX-34): Welcome, then the Voice phase —
 * check the microphone, how you call KIVO, how it hears and speaks (recommended for this PC, with
 * the other profiles a click away), a summary of the choices, and optionally the owner's voice —
 * then step 6, connecting a brain (optional; sign-in without keys, free options marked). One
 * decision per screen, the recommended answer preselected; Finish (or skipping from Welcome's fine
 * print) marks setup done and opens the Control Center. The control phase joins in M7.
 */
import { AnimatePresence, motion, useReducedMotionConfig } from "motion/react";
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Island } from "../components/island/Island";
import { Enrollment } from "../components/voice/Enrollment";
import { MicCheck } from "../components/voice/MicCheck";
import { Recommended, SpeechChooser, VoiceList } from "../components/voice/SpeechChooser";
import { useSpeech, type Speech } from "../components/voice/useSpeech";
import { HeyKivoSwitch } from "../components/voice/WakeWords";
import { Button, Group, Monogram, Pill, Row, Section, ShortcutRecorder, Spinner, useToast } from "../components/ui";
import { brainColor, monogram, type BrainsList, type DiscoverySection } from "../ipc/brains";
import { Icon } from "../icons";
import { Method } from "../ipc/generated";
import { useRuntime } from "../ipc/runtime";

const STEPS = ["welcome", "mic", "activation", "speech", "summary", "voice", "brain"] as const;
type Step = (typeof STEPS)[number];
/** The phases the progress capsule shows: Welcome, the Voice steps, then the brain. */
const PHASES = ["welcome", "voice", "brain"] as const;
const OPTIONAL: ReadonlySet<Step> = new Set<Step>(["voice", "brain"]);

function Header({ step }: { step: Step }) {
  const { t } = useTranslation();
  const phase = step === "welcome" ? 0 : step === "brain" ? 2 : 1;
  return (
    <div className="k-onboarding__bar">
      <span className="k-onboarding__progress">
        <span className="k-onboarding__dots" aria-hidden>
          {PHASES.map((p, i) => (
            <i key={p} className={i < phase ? "is-done" : i === phase ? "is-on" : undefined} />
          ))}
        </span>
        {t(`onboarding.phase.${PHASES[phase]}`)}
      </span>
      <span className="k-onboarding__title">{t("onboarding.setUp")}</span>
    </div>
  );
}

function Screen({ step, children }: { step: Step; children: ReactNode }) {
  const { t } = useTranslation();
  const index = STEPS.indexOf(step);
  return (
    <>
      <div className="k-onboarding__eyebrow">
        {t("onboarding.voiceStep", { n: index, total: STEPS.length - 1 })}
        {OPTIONAL.has(step) && ` · ${t("onboarding.optional")}`}
      </div>
      <h2 className="k-onboarding__h">{t(`onboarding.${step}.title`)}</h2>
      <p className="k-onboarding__sub">{t(`onboarding.${step}.sub`)}</p>
      {children}
    </>
  );
}

function Welcome({ onStart, onSkip }: { onStart: () => void; onSkip: () => void }) {
  const { t, i18n } = useTranslation();
  const language = new Intl.DisplayNames([i18n.language], { type: "language" }).of(i18n.language) ?? i18n.language;
  return (
    <div className="k-onboarding__welcome">
      <Island model={{ state: "welcome", width: 236, label: t("island.listening"), wave: true }} />
      <h1>{t("onboarding.welcome.title")}</h1>
      <p>{t("onboarding.welcome.sub")}</p>
      <Button variant="primary" onClick={onStart}>
        {t("onboarding.welcome.start")}
      </Button>
      <div className="k-onboarding__fine">
        <span>
          <Icon name="globe" />
          {language}
        </span>
        <span>{t("onboarding.welcome.time")}</span>
        <span>{t("onboarding.welcome.private")}</span>
        <button type="button" className="k-onboarding__skip" onClick={onSkip}>
          {t("onboarding.welcome.skip")}
        </button>
      </div>
    </div>
  );
}

function Activation() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const [keys, setKeys] = useState<string[] | null>(null);
  const connected = link?.status === "connected";
  useEffect(() => {
    if (!connected) return;
    void request<{ voice: Record<string, unknown> }>(Method.settingsGet)
      .then((s) => {
        const v = s.voice["push-to-talk"];
        setKeys(Array.isArray(v) ? v.map(String) : ["Ctrl", "Space"]);
      })
      .catch(() => {});
  }, [connected, request]);
  const save = (next: string[]) =>
    request(Method.settingsSet, { voice: { "push-to-talk": next } })
      .then(() => setKeys(next))
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
  return (
    <>
      <Group>
        <Row
          icon="keyboard"
          title={t("onboarding.activation.hold")}
          subtitle={t("onboarding.activation.holdHint")}
          end={keys && <ShortcutRecorder value={keys} onChange={(k) => void save(k)} />}
        />
        <Row
          icon="mic"
          title={t("onboarding.activation.wake")}
          subtitle={t("onboarding.activation.wakeHint")}
          end={<HeyKivoSwitch />}
        />
      </Group>
      <p className="k-note">{t("onboarding.activation.note")}</p>
    </>
  );
}

function Speaking({ speech }: { speech: Speech }) {
  const { t } = useTranslation();
  const [others, setOthers] = useState(false);
  return (
    <>
      <Recommended speech={speech} />
      <div className="k-speech__actions">
        <Button size="sm" variant="plain" onClick={() => setOthers((o) => !o)}>
          {others ? t("onboarding.speech.fewer") : t("onboarding.speech.others")}
        </Button>
      </div>
      {others && (
        <>
          <Section title={t("speech.sttTitle")} />
          <SpeechChooser slot="stt" speech={speech} />
          <Section title={t("speech.ttsTitle")} />
          <SpeechChooser slot="tts" speech={speech} />
        </>
      )}
      <VoiceList speech={speech} />
      <p className="k-note">{t("onboarding.speech.background")}</p>
    </>
  );
}

/** Step 6 (UX-34): connect a brain — what's already on this PC first, then OpenRouter's sign-in;
 * free options marked (CONV-08). Nothing is connected without a click (DISC-03). */
function ConnectBrain() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [found, setFound] = useState<DiscoverySection["items"] | null>(null);
  const [brains, setBrains] = useState<BrainsList | null>(null);
  const fail = (e: unknown) => toast(e instanceof Error ? e.message : String(e));
  const load = () => {
    void request<BrainsList>(Method.brainsList)
      .then(setBrains)
      .catch(() => {});
  };
  useEffect(() => {
    if (!connected) return;
    load();
    // Look now: setup is when the user most wants to see what's here.
    void Promise.all(
      ["cli", "local"].map((section) =>
        request<DiscoverySection>(Method.brainsRefresh, { section })
          .then((s) => s.items)
          .catch(() => []),
      ),
    ).then((lists) => setFound(lists.flat()));
    // eslint-disable-next-line react-hooks/exhaustive-deps -- once, when connected
  }, [connected]);
  const isConnected = (id: string) => brains?.connected.some((b) => b.id === id) ?? false;
  const use = (id: string, baseUrl?: string) =>
    request(Method.brainsConnect, { id, ...(baseUrl ? { baseUrl } : {}) })
      .then(load)
      .catch(fail);
  return (
    <>
      <Section title={t("onboarding.brain.found")} />
      {found === null ? (
        <Spinner label={t("onboarding.brain.searching")} />
      ) : found.length === 0 ? (
        <p className="k-note">{t("onboarding.brain.nothingFound")}</p>
      ) : (
        <Group>
          {found.map((i) => {
            const name = i.data.name ?? i.id;
            const free = i.data.free ?? (i.data.url ? t("onboarding.brain.free") : null);
            return (
              <Row
                key={i.id}
                lead={<Monogram text={monogram(name)} color={brainColor(i.id)} />}
                title={name}
                subtitle={free ?? undefined}
                end={
                  <>
                    {free && <Pill tone="success">{t("onboarding.brain.free")}</Pill>}
                    {isConnected(i.id) ? (
                      <Pill tone="success">{t("onboarding.brain.connected")}</Pill>
                    ) : i.data.signedIn === false ? (
                      <Button size="sm" onClick={() => void request(Method.brainsSignIn, { id: i.id }).catch(fail)}>
                        {t("onboarding.brain.signIn")}
                      </Button>
                    ) : i.data.needsAdapter ? null : (
                      <Button size="sm" variant="primary" onClick={() => void use(i.id, i.data.url)}>
                        {t("onboarding.brain.use")}
                      </Button>
                    )}
                  </>
                }
              />
            );
          })}
        </Group>
      )}
      <Group>
        <Row
          lead={<Monogram text={monogram("Open Router")} color={brainColor("openrouter")} />}
          title={t("onboarding.brain.openRouter")}
          subtitle={t("onboarding.brain.openRouterHint")}
          end={
            isConnected("openrouter") ? (
              <Pill tone="success">{t("onboarding.brain.connected")}</Pill>
            ) : (
              <Button
                size="sm"
                onClick={() => void request(Method.brainsSignIn, { id: "openrouter" }).then(load).catch(fail)}
              >
                {t("onboarding.brain.connect")}
              </Button>
            )
          }
        />
      </Group>
      <p className="k-note">{t("onboarding.brain.keysLater")}</p>
    </>
  );
}

/** The confirm step (UX-60): what KIVO will use, in words. */
function Summary({ speech }: { speech: Speech }) {
  const { t } = useTranslation();
  const { choices, recommendation } = speech;
  if (!choices) return null;
  const engine = (id: string) => choices.engines.find((e) => e.id === id);
  const stt = engine(choices.stt);
  const tts = engine(choices.tts);
  const voice = tts?.voices.find((v) => v.id === choices.ttsVoice) ?? tts?.voices[0];
  return (
    <Group>
      <Row
        icon="mic"
        title={t("onboarding.summary.understanding")}
        subtitle={
          stt
            ? `${stt.name} · ${t(`speech.privacy.${stt.privacy}`)}${stt.ready ? "" : ` · ${t("onboarding.summary.downloading")}`}`
            : t("onboarding.summary.none")
        }
      />
      <Row
        icon="volume"
        title={t("onboarding.summary.speaking")}
        subtitle={tts ? [tts.name, voice?.name].filter(Boolean).join(" · ") : t("onboarding.summary.none")}
      />
      <Row icon="brain" title={t("onboarding.summary.ai")} subtitle={t("onboarding.summary.aiLater")} />
      <Row
        icon="performance"
        title={t("onboarding.summary.resources")}
        subtitle={
          recommendation
            ? t("onboarding.summary.resourcesLine", {
                tier: t(`onboarding.summary.tier.${recommendation.tier}`),
                threads: recommendation.threads,
              })
            : t("onboarding.summary.none")
        }
      />
    </Group>
  );
}

export function Onboarding({ onFinish }: { onFinish: () => void }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const reduce = useReducedMotionConfig() ?? false;
  const speech = useSpeech();
  const [index, setIndex] = useState(0);
  const [direction, setDirection] = useState(1);
  const step: Step = STEPS[index] ?? "welcome";
  const last = index === STEPS.length - 1;

  const finish = () =>
    request(Method.settingsSet, { general: { onboarded: true } })
      .then(onFinish)
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
  const go = (by: number) => {
    setDirection(by);
    setIndex((i) => Math.min(STEPS.length - 1, Math.max(0, i + by)));
  };

  if (step === "welcome") {
    return (
      <div className="k-onboarding k-onboarding--welcome">
        <Welcome onStart={() => go(1)} onSkip={() => void finish()} />
      </div>
    );
  }

  const body: Record<Exclude<Step, "welcome">, ReactNode> = {
    mic: <MicCheck />,
    activation: <Activation />,
    speech: <Speaking speech={speech} />,
    summary: <Summary speech={speech} />,
    voice: <Enrollment onDone={() => go(1)} />,
    brain: <ConnectBrain />,
  };

  // Steps slide 18 px in the direction of travel (DESIGN_SYSTEM §5).
  return (
    <div className="k-onboarding">
      <Header step={step} />
      <div className="k-onboarding__body">
        <AnimatePresence mode="wait" initial={false} custom={direction}>
          <motion.div
            key={step}
            className="k-onboarding__col"
            initial={reduce ? false : { opacity: 0, x: 18 * direction }}
            animate={{ opacity: 1, x: 0 }}
            exit={reduce ? { opacity: 0 } : { opacity: 0, x: -18 * direction }}
            transition={{ duration: reduce ? 0 : 0.2, ease: "easeOut" }}
          >
            <Screen step={step}>{body[step]}</Screen>
          </motion.div>
        </AnimatePresence>
      </div>
      <div className="k-onboarding__foot">
        <Button variant="plain" onClick={() => go(-1)}>
          {t("onboarding.back")}
        </Button>
        <span className="k-onboarding__spacer" />
        {OPTIONAL.has(step) && (
          <Button variant="plain" onClick={() => (last ? void finish() : go(1))}>
            {t("onboarding.skip")}
          </Button>
        )}
        <Button variant="primary" onClick={() => (last ? void finish() : go(1))}>
          {last ? t("onboarding.finish") : t("onboarding.continue")}
        </Button>
      </div>
    </div>
  );
}
