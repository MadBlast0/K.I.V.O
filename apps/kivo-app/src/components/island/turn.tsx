/**
 * The Island for what KIVO is doing now (UX §2, UX-07/09): the runtime's session state plus its
 * live turn — what KIVO heard, the steps it is taking, what it answers, and any decision waiting
 * for the user. Only what the runtime reports is shown.
 */
import type { TFunction } from "i18next";
import type { ConfirmSpec, StateSnapshot, StepView, TurnView } from "../../ipc/generated";
import {
  IslandActions,
  IslandApp,
  IslandChip,
  IslandDot,
  IslandOk,
  IslandRisk,
  IslandSpin,
  type IslandModel,
} from "./Island";

/** What the Island's buttons do (each goes to the runtime through `island_request`). */
export interface IslandHandlers {
  stop: () => void;
  answer: (callId: string, allow: boolean, always: boolean) => void;
  openControlCenter: () => void;
}

/** The card width for text content (UX §2: up to 520 px). */
const CARD = 480;

/** A leading square for the app KIVO is acting on (UX §8.1 target-app icon). */
function appIcon(name: string | null) {
  if (!name) return undefined;
  const initials = name
    .split(/\s+/)
    .filter(Boolean)
    .slice(-2)
    .map((w) => w[0]?.toUpperCase() ?? "")
    .join("");
  return <IslandApp text={initials || "•"} bg="var(--acc)" />;
}

function StepIcon({ step }: { step: StepView }) {
  if (step.status === "done") return <IslandOk />;
  if (step.status === "failed") return <IslandDot color="#FF5147" />;
  return <IslandSpin />;
}

function Steps({ steps }: { steps: StepView[] }) {
  return (
    <div className="k-island__steps" role="list">
      {steps.map((step) => (
        <div key={step.id} role="listitem">
          <StepIcon step={step} />
          <b>{step.title}</b>
          {step.detail && step.status !== "running" ? ` ${step.detail}` : null}
        </div>
      ))}
    </div>
  );
}

/** What the user said, as the Island quotes it (grey while it may still change). */
function Heard({ turn }: { turn: TurnView }) {
  if (!turn.transcript) return null;
  return (
    <div className={turn.transcriptFinal ? "k-island__quote" : "k-island__quote k-island__quote--live"}>
      {turn.transcript}
    </div>
  );
}

function confirmCard(confirm: ConfirmSpec, turn: TurnView, t: TFunction, on: IslandHandlers): IslandModel {
  const high = confirm.risk === "high";
  const actions = [
    {
      label: confirm.plan ? t("island.approvePlan") : t("island.allowOnce"),
      kind: "primary" as const,
      onClick: () => on.answer(confirm.callId, true, false),
    },
    ...(confirm.allowAlways
      ? [{ label: t("island.allowAlways"), onClick: () => on.answer(confirm.callId, true, true) }]
      : []),
    { label: t("island.deny"), kind: "danger" as const, onClick: () => on.answer(confirm.callId, false, false) },
  ];
  return {
    state: `confirm-${confirm.callId}`,
    width: 490,
    label: high ? t("island.confirmHigh") : t("island.needsOk"),
    lead: <IslandDot color={high ? "#FF5147" : "#FFC857"} />,
    body: (
      <>
        <Heard turn={turn} />
        {confirm.risk !== "safe" && confirm.risk !== "low" && <IslandRisk level={high ? "high" : "medium"} />}
        <div className="k-island__action">
          {confirm.plan ? t("island.planTitle", { action: confirm.action }) : confirm.action}
        </div>
        <div className="k-island__meta">
          {t("island.why", { why: confirm.why })} · {confirm.provenance}
        </div>
        {/* Voice approval arrives with the local decision grammar (M2, CONV-27): buttons only. */}
        <IslandActions actions={actions} hint={false} />
      </>
    ),
  };
}

/** The footer every card has while KIVO works (UX-09): Stop and Open in Control Center. */
function Footer({ t, on, stop }: { t: TFunction; on: IslandHandlers; stop: boolean }) {
  return (
    <IslandActions
      hint={false}
      actions={[
        ...(stop ? [{ label: t("island.stop"), kind: "danger" as const, onClick: on.stop }] : []),
        { label: t("island.openControlCenter"), onClick: on.openControlCenter },
      ]}
    />
  );
}

/** The Island for the runtime's state, or `null` when there is nothing to show. */
export function islandForTurn(snapshot: StateSnapshot, t: TFunction, on: IslandHandlers): IslandModel | null {
  if (snapshot.islandHidden) return null;
  const { session, turn } = snapshot;

  if (turn?.confirm) return confirmCard(turn.confirm, turn, t, on);

  if (turn?.error) {
    return {
      state: `error-${turn.id}`,
      width: CARD,
      label: turn.transcript ? t("island.error") : t("island.cantHear"),
      lead: <IslandDot color="#FF5147" />,
      body: (
        <>
          <Heard turn={turn} />
          {turn.steps.length > 0 && <Steps steps={turn.steps} />}
          <div className="k-island__answer">{turn.error}</div>
          <Footer t={t} on={on} stop={false} />
        </>
      ),
    };
  }

  switch (session) {
    case "listening":
    case "followUp": {
      const heard = turn?.transcript ?? "";
      return {
        state: session,
        width: heard ? CARD : 236,
        label: t(session === "followUp" ? "island.followUp" : "island.listening"),
        sub: session === "followUp" ? t("island.followUpSub") : undefined,
        wave: true,
        body: heard && turn ? <Heard turn={turn} /> : undefined,
      };
    }
    case "thinking":
      return {
        state: "thinking",
        width: turn?.transcript ? CARD : 196,
        label: t("island.thinking"),
        trail: <IslandSpin />,
        body: turn?.transcript ? <Heard turn={turn} /> : undefined,
      };
    case "acting":
      return {
        state: "acting",
        width: CARD,
        label: t("island.working"),
        sub: turn && turn.steps.length > 1 ? t("island.steps", { count: turn.steps.length }) : undefined,
        lead: appIcon(turn?.targetApp ?? null),
        trail: <IslandSpin />,
        body: turn ? (
          <>
            <Heard turn={turn} />
            <Steps steps={turn.steps} />
            <Footer t={t} on={on} stop />
          </>
        ) : undefined,
      };
    case "speaking":
      return {
        state: "speaking",
        width: CARD,
        label: t("island.kivo"),
        lead: appIcon(turn?.targetApp ?? null),
        wave: true,
        voice: "kivo",
        body: turn ? (
          <>
            <Heard turn={turn} />
            {turn.answer && <div className="k-island__answer">{turn.answer}</div>}
            <Footer t={t} on={on} stop />
          </>
        ) : undefined,
      };
    case "interrupted":
      return { state: "interrupted", width: 196, label: t("island.stopping"), trail: <IslandSpin /> };
    case "awaitingConfirmation":
      return { state: "awaiting", width: 300, label: t("island.needsOk"), lead: <IslandDot color="#FFC857" /> };
    case "error":
      return { state: "error", width: 300, label: t("island.error"), lead: <IslandDot color="#FF5147" /> };
    case "idle":
    case "paused":
      // The answer stays up for a moment after the turn ends, then the Island collapses (UX-10).
      if (turn?.answer) {
        return {
          state: `done-${turn.id}`,
          width: CARD,
          label: t("island.kivo"),
          lead: appIcon(turn.targetApp) ?? <IslandOk />,
          trail: <IslandChip>{t("island.stepDone").toUpperCase()}</IslandChip>,
          body: (
            <>
              <Heard turn={turn} />
              {turn.steps.length > 0 && <Steps steps={turn.steps} />}
              <div className="k-island__answer">{turn.answer}</div>
            </>
          ),
        };
      }
      return null;
  }
}

/** True when the Island shows buttons, so its window must take clicks (UX §2). */
export function hasButtons(model: IslandModel | null): boolean {
  if (!model) return false;
  return (
    model.state.startsWith("confirm-") ||
    model.state.startsWith("error-") ||
    model.state === "acting" ||
    model.state === "speaking"
  );
}
