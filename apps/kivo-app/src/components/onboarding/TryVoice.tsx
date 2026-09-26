/**
 * "How do you call KIVO?" made hands-on (UX §4, owner 2026-09-26): "Try it now" follows the
 * user's own first request live — what KIVO heard and what it answered — whether it started with
 * "Hey Kivo" or the keys. The suggested request ("what time is it") needs no brain, so it works
 * before one is connected.
 */
import { useState } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../../icons";
import { cn } from "../../lib/cn";
import { useToast } from "../ui";
import { Method, type TurnView } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import type { CallWays } from "../voice/CallKivo";

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
    <div className="k-try-now" role="status" aria-live="polite">
      <button
        type="button"
        className={cn("k-try-now__mic", listening && "is-listening")}
        onClick={start}
        disabled={listening}
        aria-label={t("onboarding.activation.tryButton")}
      >
        <Icon name="mic" />
      </button>
      <div className="k-try-now__text">
        <b>{t("onboarding.activation.tryTitle")}</b>
        {listening ? (
          <span>{t("onboarding.activation.tryListening")}</span>
        ) : shown ? (
          <>
            {shown.transcript && (
              <span>
                <em>{t("onboarding.activation.tryHeard")}</em> {shown.transcript}
              </span>
            )}
            {shown.answer && (
              <span>
                <em>{t("onboarding.activation.trySaid")}</em> {shown.answer}
              </span>
            )}
          </>
        ) : (
          <span>
            {t(
              ways.wake && ways.ptt
                ? "onboarding.activation.tryHint"
                : ways.wake
                  ? "onboarding.activation.tryHintWake"
                  : "onboarding.activation.tryHintKeys",
              { keys: ways.keys.join(" + ") },
            )}
          </span>
        )}
      </div>
    </div>
  );
}
