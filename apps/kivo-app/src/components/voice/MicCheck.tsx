/**
 * The microphone check (UX-33 step 2): KIVO records one sentence through its own listener (the
 * microphone chosen beside it), the bars follow the live level, and the result says whether it
 * heard the user clearly.
 */
import { isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState, type ReactNode } from "react";
import { useTranslation } from "react-i18next";
import { Method } from "../../ipc/generated";
import { useRuntime } from "../../ipc/runtime";
import { Alert, Button, Group, Row } from "../ui";

interface Checked {
  heard: boolean;
  peakDb: number;
  seconds: number;
}

/** The microphone level (0–1) the runtime streams while it listens. */
export function useMicLevel(active: boolean): number {
  const [level, setLevel] = useState(0);
  useEffect(() => {
    if (!active || !isTauri()) return;
    let stop: (() => void) | undefined;
    let cancelled = false;
    void listen<number>("runtime://level", (e) => setLevel(e.payload)).then((u) => {
      if (cancelled) u();
      else stop = u;
    });
    return () => {
      cancelled = true;
      stop?.();
      setLevel(0);
    };
  }, [active]);
  return level;
}

/** Bars that follow a level; the middle ones move most. */
export function LiveLevel({ level, bars = 14 }: { level: number; bars?: number }) {
  return (
    <span className="k-level k-level--live" aria-hidden>
      {Array.from({ length: bars }, (_, i) => {
        const shape = 1 - Math.abs(i - (bars - 1) / 2) / bars;
        return <i key={i} style={{ height: `${5 + 17 * Math.min(1, level * 1.4) * shape}px` }} />;
      })}
    </span>
  );
}

/** `children`: rows shown above the test in the same group (the microphone picker). */
export function MicCheck({ onResult, children }: { onResult?: (ok: boolean) => void; children?: ReactNode }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const [state, setState] = useState<"idle" | "listening" | "good" | "quiet" | "failed">("idle");
  const [error, setError] = useState<string | null>(null);
  const level = useMicLevel(state === "listening");

  const check = () => {
    setState("listening");
    setError(null);
    request<Checked>(Method.voiceMicCheck)
      .then((r) => {
        setState(r.heard ? "good" : "quiet");
        onResult?.(r.heard);
      })
      .catch((e: unknown) => {
        setState("failed");
        setError(e instanceof Error ? e.message : String(e));
        onResult?.(false);
      });
  };

  return (
    <>
      <Group>
        {children}
        <Row
          icon="wave"
          title={t("onboarding.mic.level")}
          subtitle={state === "listening" ? t("onboarding.mic.speakNow") : t("onboarding.mic.levelHint")}
          end={
            state === "listening" ? (
              <LiveLevel level={level} />
            ) : (
              <Button size="sm" icon="mic" onClick={check}>
                {state === "idle" ? t("onboarding.mic.test") : t("onboarding.mic.again")}
              </Button>
            )
          }
        />
      </Group>
      <div className="k-onboarding__result">
        {state === "good" && (
          <Alert kind="success" title={t("onboarding.mic.good")}>
            {t("onboarding.mic.goodBody")}
          </Alert>
        )}
        {state === "quiet" && (
          <Alert kind="warning" title={t("onboarding.mic.quiet")}>
            {t("onboarding.mic.quietBody")}
          </Alert>
        )}
        {state === "failed" && (
          <Alert kind="warning" title={t("onboarding.mic.failed")}>
            {error}
          </Alert>
        )}
      </div>
    </>
  );
}
