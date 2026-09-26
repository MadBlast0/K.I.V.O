/**
 * First-launch setup (UX §4, UX-33–35, UX-60): short screens in five phases, in the order someone
 * new needs them. Welcome; Voice — the microphone and speaker (picked and tested), how KIVO hears
 * and speaks (a preset for this PC, downloaded with consent, loaded and tested before Continue),
 * how you call KIVO ("Hey Kivo", push-to-talk, and a first try), and optionally the owner's voice;
 * Brain — connect a brain and apps; Control — the permission mode, look & feel, startup; Ready —
 * Try it. One decision per screen, the recommended answer preselected from this PC (UX-36), plain
 * words with the technical ones explained on hover. Finish (or skipping from Welcome's fine print)
 * marks setup done and opens the Control Center.
 */
import { AnimatePresence, motion, useReducedMotionConfig } from "motion/react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Island } from "../components/island/Island";
import { Enrollment } from "../components/voice/Enrollment";
import { CallKivo, useCallWays } from "../components/voice/CallKivo";
import { useSpeech } from "../components/voice/useSpeech";
import { Button, Group, Monogram, Pill, Row, Section, Spinner, useToast } from "../components/ui";
import { DevicesStep } from "../components/onboarding/Devices";
import { SpeechSetup, speechReady } from "../components/onboarding/SpeechSetup";
import { AppsStep, LookStep, ModeStep, Reasons, StartupStep, TryStep, useAdvice } from "../components/onboarding/steps";
import { IslandDemo, TryItNow } from "../components/onboarding/TryVoice";
import { brainColor, monogram, type BrainsList, type DiscoverySection } from "../ipc/brains";
import { Icon } from "../icons";
import { Method, type SetupAdvice } from "../ipc/generated";
import { useRuntime } from "../ipc/runtime";

const STEPS = [
  "welcome",
  "devices",
  "speech",
  "activation",
  "voice",
  "brain",
  "apps",
  "mode",
  "look",
  "startup",
  "try",
] as const;
type Step = (typeof STEPS)[number];
/** The phases the progress capsule shows, and the steps in each. */
const PHASES = ["welcome", "voice", "brain", "control", "ready"] as const;
const PHASE_OF: Record<Step, number> = {
  welcome: 0,
  devices: 1,
  speech: 1,
  activation: 1,
  voice: 1,
  brain: 2,
  apps: 2,
  mode: 3,
  look: 3,
  startup: 3,
  try: 4,
};
const OPTIONAL: ReadonlySet<Step> = new Set<Step>(["voice", "brain", "apps"]);

function Header({ step }: { step: Step }) {
  const { t } = useTranslation();
  const phase = PHASE_OF[step];
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
  // "Voice · 2 of 5", "Control · 1 of 3", "Ready".
  const phase = PHASE_OF[step];
  const inPhase = STEPS.filter((s) => PHASE_OF[s] === phase);
  const name = t(`onboarding.phase.${PHASES[phase]}`);
  return (
    <>
      <div className="k-onboarding__eyebrow">
        {inPhase.length > 1
          ? t("onboarding.stepOf", { phase: name, n: inPhase.indexOf(step) + 1, total: inPhase.length })
          : name}
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
  const ways = useCallWays();
  return (
    <>
      <IslandDemo />
      <CallKivo ways={ways} />
      <TryItNow ways={ways} />
    </>
  );
}

/** Step 6 (UX-34): connect a brain — what's already on this PC first, then OpenRouter's sign-in;
 * free options marked (CONV-08). Nothing is connected without a click (DISC-03). */
function ConnectBrain({ advice }: { advice: SetupAdvice | null }) {
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
  // The one setup suggests for this PC (UX-36).
  const suggested = (id: string) =>
    advice?.brain === id && !isConnected(id) ? <Pill tone="accent">{t("permissions.recommended")}</Pill> : null;
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
                    {suggested(i.id)}
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
              <>
                {suggested("openrouter")}
                <Button
                  size="sm"
                  onClick={() => void request(Method.brainsSignIn, { id: "openrouter" }).then(load).catch(fail)}
                >
                  {t("onboarding.brain.connect")}
                </Button>
              </>
            )
          }
        />
      </Group>
      <Reasons advice={advice && { ...advice, reasons: advice.reasons.slice(0, 1) }} />
      <p className="k-note">{t("onboarding.brain.keysLater")}</p>
    </>
  );
}

export function Onboarding({ onFinish }: { onFinish: (then?: string) => void }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const reduce = useReducedMotionConfig() ?? false;
  const speech = useSpeech();
  const advice = useAdvice();
  // A page to open once setup is done ("add an MCP server" from the Apps step).
  const [after, setAfter] = useState<string | undefined>();
  // The recommended performance profile and privacy mode are preselected once (UX-36); the
  // Startup step shows them with their reasons, to change there or later in Settings.
  const applied = useRef(false);
  useEffect(() => {
    if (!advice || applied.current) return;
    applied.current = true;
    void request(Method.settingsSet, {
      performance: { profile: advice.performance },
      privacy: { mode: advice.privacy },
    }).catch(() => {});
  }, [advice, request]);
  const [index, setIndex] = useState(0);
  const [direction, setDirection] = useState(1);
  const step: Step = STEPS[index] ?? "welcome";
  const last = index === STEPS.length - 1;
  // KIVO has to hear and speak before the steps that use it: Continue waits until both models
  // are here, loaded and tested; after a failure, "Set up later" is the way on.
  const blocked = step === "speech" && !speechReady(speech);
  const speechFailed = (["stt", "tts"] as const).some((slot) => speech.progress[slot]?.stage === "failed");

  const finish = () =>
    request(Method.settingsSet, { general: { onboarded: true } })
      .then(() => onFinish(after))
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
    devices: <DevicesStep />,
    speech: <SpeechSetup speech={speech} />,
    activation: <Activation />,
    voice: <Enrollment onDone={() => go(1)} />,
    brain: <ConnectBrain advice={advice} />,
    apps: <AppsStep onAfter={setAfter} />,
    mode: <ModeStep advice={advice} />,
    look: <LookStep />,
    startup: <StartupStep advice={advice} />,
    try: <TryStep />,
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
        {(OPTIONAL.has(step) || (step === "speech" && speechFailed)) && (
          <Button variant="plain" onClick={() => (last ? void finish() : go(1))}>
            {step === "speech" ? t("onboarding.speech.later") : t("onboarding.skip")}
          </Button>
        )}
        <Button variant="primary" disabled={blocked} onClick={() => (last ? void finish() : go(1))}>
          {last ? t("onboarding.finish") : t("onboarding.continue")}
        </Button>
      </div>
    </div>
  );
}
