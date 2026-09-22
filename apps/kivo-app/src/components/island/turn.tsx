/**
 * The Island for what KIVO is doing now (UX §2, UX-07/09): the runtime's session state plus its
 * live turn — what KIVO heard, the steps it is taking, what it answers, and any decision waiting
 * for the user. Only what the runtime reports is shown.
 */
import type { TFunction } from "i18next";
import type { Capability, ConfirmSpec, PermissionMode, StateSnapshot, StepView, TurnView } from "../../ipc/generated";
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
import { Icon } from "../../icons";

/** What the Island's buttons do (each goes to the runtime through `island_request`). */
export interface IslandHandlers {
  stop: () => void;
  answer: (callId: string, allow: boolean, always: boolean) => void;
  openControlCenter: () => void;
  /** Opens where the permission mode is switched (the overlay itself can't switch it, SEC-04). */
  openMode: () => void;
  /** Runs the same request again (PLAN-18). */
  retry: (text: string) => void;
  /** Turns on the capability a request needed (CAP-02): only the one it needed, only on. */
  enable: (capability: Capability) => void;
  /** Opens the text field with `text` in it: fix what KIVO heard and send it again (UX-09). */
  edit: (text: string) => void;
  /** Listens for a new request (the footer's mic). */
  talk: () => void;
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

/** What the user said, as the Island quotes it (grey while it may still change). Once final, a
 * click opens it in the text field to fix and send again (UX-09). */
function Heard({ turn, t, on }: { turn: TurnView; t?: TFunction; on?: IslandHandlers }) {
  if (!turn.transcript) return null;
  if (!turn.transcriptFinal || !on || !t) {
    return (
      <div className={turn.transcriptFinal ? "k-island__quote" : "k-island__quote k-island__quote--live"}>
        {turn.transcript}
      </div>
    );
  }
  return (
    <button
      type="button"
      className="k-island__quote k-island__quote--edit"
      title={t("island.editHint")}
      onClick={() => on.edit(turn.transcript)}
    >
      {turn.transcript}
    </button>
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
      ? [
          {
            // "Always for…" names what the grant covers (SEC-10).
            label: confirm.target ? t("island.allowAlwaysFor", { target: confirm.target }) : t("island.allowAlways"),
            onClick: () => on.answer(confirm.callId, true, true),
          },
        ]
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
        {confirm.target && !confirm.action.includes(confirm.target) && (
          <div className="k-island__meta">{t("island.target", { target: confirm.target })}</div>
        )}
        <div className="k-island__meta">
          {t("island.why", { why: confirm.why })} · {confirm.provenance}
        </div>
        {/* Voice approval arrives with the local decision grammar (M2, CONV-27): buttons only. */}
        <IslandActions actions={actions} hint={false} />
      </>
    ),
  };
}

/** The permission mode while KIVO acts (SEC-04): hidden in Auto; a click opens where it's switched. */
function ModeChip({ mode, t, on }: { mode: PermissionMode; t: TFunction; on: IslandHandlers }) {
  if (mode === "auto") return null;
  return (
    <button
      type="button"
      className={mode === "bypass" ? "k-island__chip k-island__chip--danger" : "k-island__chip"}
      aria-label={t("island.modeChipLabel", { mode: t(`mode.${mode}`) })}
      title={t("island.modeChipLabel", { mode: t(`mode.${mode}`) })}
      onClick={on.openMode}
    >
      {t(`island.modeChip.${mode}`)}
    </button>
  );
}

/** The card's footer (UX-09): a text field and the mic for the next request, Stop while KIVO
 * works, and Open in Control Center. */
function Footer({ t, on, stop }: { t: TFunction; on: IslandHandlers; stop: boolean }) {
  return (
    <>
      <div className="k-island__footer">
        <button type="button" className="k-island__field" onClick={() => on.edit("")}>
          {t("island.typePlaceholder")}
        </button>
        <button
          type="button"
          className="k-island__btn k-island__mic-btn"
          aria-label={t("island.talk")}
          title={t("island.talk")}
          onClick={on.talk}
          disabled={stop}
        >
          <Icon name="mic" />
        </button>
      </div>
      <IslandActions
        hint={false}
        actions={[
          ...(stop ? [{ label: t("island.stop"), kind: "danger" as const, onClick: on.stop }] : []),
          { label: t("island.openControlCenter"), onClick: on.openControlCenter },
        ]}
      />
    </>
  );
}

/** The Island for the runtime's state, or `null` when there is nothing to show. */
export function islandForTurn(snapshot: StateSnapshot, t: TFunction, on: IslandHandlers): IslandModel | null {
  if (snapshot.islandHidden) return null;
  const { session, turn } = snapshot;

  if (turn?.confirm) return confirmCard(turn.confirm, turn, t, on);

  // Over a fullscreen app or during Focus, a dot is all that shows (UX-11; "hidden" never
  // reaches here: the window stays hidden).
  if (turn?.quiet === "tiny") return { state: "quiet", width: 36, label: "" };

  if (turn?.error) {
    const offer = turn.capabilityOff;
    const actions = offer
      ? [
          { label: t("island.turnOn"), kind: "primary" as const, onClick: () => on.enable(offer) },
          { label: t("island.notNow"), onClick: on.stop },
        ]
      : [
          ...(turn.transcript
            ? [{ label: t("island.retry"), kind: "primary" as const, onClick: () => on.retry(turn.transcript) }]
            : []),
          { label: t("island.openControlCenter"), onClick: on.openControlCenter },
        ];
    return {
      state: `error-${turn.id}`,
      width: CARD,
      label: turn.transcript ? t("island.error") : t("island.cantHear"),
      lead: <IslandDot color="#FF5147" />,
      body: (
        <>
          <Heard turn={turn} t={t} on={on} />
          {turn.steps.length > 0 && <Steps steps={turn.steps} />}
          <div className="k-island__answer">{turn.error}</div>
          <IslandActions actions={actions} hint={false} />
        </>
      ),
    };
  }

  // A request that needed a capability that is off (CAP-02): its answer offers to turn it on.
  if (turn?.capabilityOff && turn.answer) {
    const offer = turn.capabilityOff;
    return {
      state: `capability-${turn.id}`,
      width: CARD,
      label: turn.answer,
      lead: <IslandDot color="#FFC857" />,
      body: (
        <IslandActions
          hint={false}
          actions={[
            { label: t("island.turnOn"), kind: "primary", onClick: () => on.enable(offer) },
            { label: t("island.notNow"), onClick: on.stop },
          ]}
        />
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
        trail: (
          <>
            <ModeChip mode={snapshot.mode} t={t} on={on} />
            <IslandSpin />
          </>
        ),
        body: turn ? (
          <>
            <Heard turn={turn} t={t} on={on} />
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
            <Heard turn={turn} t={t} on={on} />
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
              <Heard turn={turn} t={t} on={on} />
              {turn.steps.length > 0 && <Steps steps={turn.steps} />}
              <div className="k-island__answer">{turn.answer}</div>
              <Footer t={t} on={on} stop={false} />
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
    model.state.startsWith("done-") ||
    model.state.startsWith("capability-") ||
    model.state === "acting" ||
    model.state === "speaking"
  );
}
