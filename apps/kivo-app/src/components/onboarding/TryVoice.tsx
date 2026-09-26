/**
 * "How do you call KIVO?" made hands-on (UX §4, owner 2026-09-26): the real Island plays what
 * happens when you talk to KIVO, and "Try it now" follows the user's own first request live —
 * what KIVO heard and what it answered — whether it started with "Hey Kivo" or the keys. The
 * suggested request ("what time is it") needs no brain, so it works before one is connected.
 */
import { useReducedMotionConfig } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Island, type IslandModel } from "../island/Island";
import { islandPreset } from "../island/presets";
import { Button, useToast } from "../ui";
import { Method, type TurnView } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import type { CallWays } from "../voice/CallKivo";

/** How long each frame of the demo stays (listening, the first words, the whole request, thinking,
 * the answer). */
const FRAME_MS = [900, 700, 1000, 600, 2600] as const;

/** The Island's own states, playing "Hey Kivo, what time is it?" on a loop. */
export function IslandDemo() {
  const { t } = useTranslation();
  const reduce = useReducedMotionConfig() ?? false;
  const say = t("onboarding.activation.demoSay");
  const words = say.split(" ");
  const answer: IslandModel = {
    state: "speaking",
    width: 480,
    label: t("demo.kivo"),
    wave: true,
    voice: "kivo",
    body: (
      <>
        <div className="k-island__quote">{say}</div>
        <div className="k-island__answer">{t("onboarding.activation.demoAnswer")}</div>
      </>
    ),
  };
  const [at, setAt] = useState(0);
  useEffect(() => {
    if (reduce) return;
    const timer = window.setTimeout(() => setAt((i) => (i + 1) % FRAME_MS.length), FRAME_MS[at] ?? 1000);
    return () => window.clearTimeout(timer);
  }, [at, reduce]);
  const frame = reduce
    ? answer
    : [
        islandPreset("listening"),
        islandPreset("listening", { partial: words.slice(0, 2).join(" ") }),
        islandPreset("listening", { partial: say }),
        islandPreset("thinking"),
        answer,
      ][at];
  return (
    <div className="k-onboarding__demo" aria-hidden>
      <Island model={frame} />
    </div>
  );
}

/** "Try it now": the user's own request, followed live. */
export function TryItNow({ ways }: { ways: CallWays }) {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const snapshot = link?.status === "connected" ? link.snapshot : null;
  // Only requests made on this screen count: the turn that was showing when it opened doesn't.
  const [before] = useState(() => snapshot?.turn?.id ?? null);
  const [last, setLast] = useState<TurnView | null>(null);
  const turn = snapshot?.turn && snapshot.turn.id !== before ? snapshot.turn : null;
  // The last request stays shown after KIVO finishes it (the snapshot then has no turn).
  if (turn && turn !== last) setLast(turn);
  const listening = snapshot?.session === "listening" || snapshot?.session === "followUp";
  const shown = turn ?? last;
  const start = () => {
    void request(Method.sessionTalk).catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
  };
  return (
    <div className="k-tile k-onboarding__try" role="status" aria-live="polite">
      <div className="k-onboarding__try-title">{t("onboarding.activation.tryTitle")}</div>
      <p className="k-onboarding__try-hint">
        {/* Only the ways that are on: "Hey Kivo", the keys, or both. */}
        {t(
          ways.wake && ways.ptt
            ? "onboarding.activation.tryHint"
            : ways.wake
              ? "onboarding.activation.tryHintWake"
              : "onboarding.activation.tryHintKeys",
          {
            keys: ways.keys.join(" + "),
          },
        )}
      </p>
      {listening ? (
        <p className="k-onboarding__try-line">{t("onboarding.activation.tryListening")}</p>
      ) : shown ? (
        <>
          {shown.transcript && (
            <p className="k-onboarding__try-line">
              <b>{t("onboarding.activation.tryHeard")}</b> {shown.transcript}
            </p>
          )}
          {shown.answer && (
            <p className="k-onboarding__try-line">
              <b>{t("onboarding.activation.trySaid")}</b> {shown.answer}
            </p>
          )}
        </>
      ) : null}
      <Button size="sm" icon="mic" onClick={start} disabled={listening}>
        {t("onboarding.activation.tryButton")}
      </Button>
    </div>
  );
}
