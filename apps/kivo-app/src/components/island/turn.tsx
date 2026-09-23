/**
 * The Island for what KIVO is doing now (UX §2, UX-07/09): the runtime's session state plus its
 * live turn — what KIVO heard, the steps it is taking, what it answers, and any decision waiting
 * for the user. Only what the runtime reports is shown.
 */
import type { TFunction } from "i18next";
import { useState } from "react";
import type {
  Capability,
  ConfirmSpec,
  GrantDuration,
  PermissionMode,
  StateSnapshot,
  StepView,
  TurnView,
} from "../../ipc/generated";
import {
  IslandActions,
  IslandApp,
  IslandChip,
  IslandCountdown,
  IslandDot,
  IslandOk,
  IslandRisk,
  IslandSpin,
  VoiceHint,
  type IslandModel,
} from "./Island";
import { Icon } from "../../icons";

/** What the Island's buttons do (each goes to the runtime through `island_request`). */
export interface IslandHandlers {
  stop: () => void;
  /** Allow or deny; `always` is a grant ("Always for…", with how long, SEC-08); `hello` asks
   * Windows Hello to confirm (High risk, SEC-11). */
  answer: (
    callId: string,
    allow: boolean,
    always: boolean,
    extra?: { duration?: GrantDuration; hello?: boolean },
  ) => void;
  /** Takes back the last change (UX-43). */
  undo?: () => void;
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
  /** "That's not what I meant" on a brain's answer (BRAIN-06). */
  misroute: (turnId: string) => void;
}

/** The card width for text content (UX §2: up to 520 px). */
const CARD = 480;

/** A leading square for the app KIVO is acting on (UX-46): the app's own icon when Windows has
 * one, else its initials. */
function appIcon(name: string | null, icon?: string | null) {
  if (icon) return <img className="k-island__app k-island__app--icon" src={icon} alt={name ?? ""} />;
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

/** The brain answering and why (UX-09, PLAN-17): "Coding · Claude Code", the routing reason on
 * hover, and a cost estimate when the user shows costs (BRAINS §9). */
function brainChip(turn: TurnView | null | undefined, t: TFunction) {
  const chip = turn?.brain;
  if (!chip) return null;
  const cost = chip.cost != null ? ` · ≈ $${chip.cost.toFixed(chip.cost < 0.01 ? 4 : 2)}` : "";
  return (
    <span className="k-island__chip" title={chip.reason} aria-label={t("island.brainChip", { reason: chip.reason })}>
      {`${chip.profile} · ${chip.name}${cost}`}
    </span>
  );
}

/** A guest's turn (VOICE-22, UX-08): a neutral "Guest" chip, so it's clear KIVO didn't
 * recognize the owner's voice. */
function guestChip(turn: TurnView | null | undefined, t: TFunction) {
  return turn?.guest ? <IslandChip>{t("island.guest").toUpperCase()}</IslandChip> : null;
}

/** "Always for…": how long the grant lasts (SEC-08). */
const DURATIONS: ReadonlyArray<GrantDuration> = ["session", "day", "always"];

/** The decision's buttons: Allow once (or Approve plan), Always for… with how long, Windows
 * Hello for High risk, and Deny. */
function ConfirmButtons({ confirm, t, on }: { confirm: ConfirmSpec; t: TFunction; on: IslandHandlers }) {
  const [choosing, setChoosing] = useState(false);
  if (choosing) {
    return (
      <IslandActions
        hint={false}
        actions={[
          ...DURATIONS.map((duration) => ({
            label: t(`island.duration.${duration}`),
            kind: duration === "always" ? ("primary" as const) : undefined,
            onClick: () => on.answer(confirm.callId, true, true, { duration }),
          })),
          { label: t("island.back"), onClick: () => setChoosing(false) },
        ]}
      />
    );
  }
  return (
    <IslandActions
      hint={false}
      actions={[
        ...(confirm.hello
          ? [
              {
                label: t("island.hello"),
                kind: "primary" as const,
                onClick: () => on.answer(confirm.callId, true, false, { hello: true }),
              },
            ]
          : []),
        {
          label: confirm.plan ? t("island.approvePlan") : t("island.allowOnce"),
          kind: confirm.hello ? undefined : ("primary" as const),
          onClick: () => on.answer(confirm.callId, true, false),
        },
        ...(confirm.allowAlways
          ? [
              {
                // "Always for…" names what the grant covers (SEC-10), then asks how long (SEC-08).
                label: confirm.target
                  ? t("island.allowAlwaysFor", { target: confirm.target })
                  : t("island.allowAlways"),
                onClick: () => setChoosing(true),
              },
            ]
          : []),
        { label: t("island.deny"), kind: "danger" as const, onClick: () => on.answer(confirm.callId, false, false) },
      ]}
    />
  );
}

function confirmCard(confirm: ConfirmSpec, turn: TurnView, t: TFunction, on: IslandHandlers): IslandModel {
  const high = confirm.risk === "high";
  // A plan from the runtime (Plan first, SEC-02) arrives as its steps, one per line.
  const steps = confirm.tool === "plan";
  // KIVO listens for a spoken answer (CONV-26): the mic ring and the buttons' own words, which the
  // decision grammar understands (CONV-27). High risk needs a click, so no hint. "Wait" keeps
  // the card with no timeout (UX-08).
  const hint =
    turn.answering && !high ? (
      <VoiceHint
        words={[
          t("island.voice.allow"),
          ...(confirm.allowAlways ? [t("island.voice.always")] : []),
          t("island.voice.deny"),
          t("island.voice.wait"),
        ]}
      />
    ) : null;
  return {
    state: `confirm-${confirm.callId}${turn.waiting ? "-waiting" : ""}`,
    width: 490,
    label: turn.waiting ? t("island.waiting") : high ? t("island.confirmHigh") : t("island.needsOk"),
    sub: turn.waiting ? t("island.waitingSub") : undefined,
    lead: <IslandDot color={high ? "#FF5147" : "#FFC857"} />,
    trail: guestChip(turn, t),
    body: (
      <>
        <Heard turn={turn} />
        {confirm.risk !== "safe" && confirm.risk !== "low" && <IslandRisk level={high ? "high" : "medium"} />}
        <div className={steps ? "k-island__action k-island__action--plan" : "k-island__action"}>
          {confirm.plan && !steps ? t("island.planTitle", { action: confirm.action }) : confirm.action}
        </div>
        {confirm.target && !confirm.action.includes(confirm.target) && (
          <div className="k-island__meta">{t("island.target", { target: confirm.target })}</div>
        )}
        <div className="k-island__meta">
          {t("island.why", { why: confirm.why })} · {confirm.provenance}
        </div>
        <ConfirmButtons key={confirm.callId} confirm={confirm} t={t} on={on} />
        {hint}
      </>
    ),
  };
}

/** Sensitive capabilities in use right now (CAP-06): an eye for the screen, a hand for input
 * control, a terminal for the shell. */
const IN_USE_ICON = { screen: "eye", input: "hand", shell: "terminal" } as const;

function InUse({ kinds, t }: { kinds: string[] | undefined; t: TFunction }) {
  if (!kinds?.length) return null;
  return (
    <span className="k-island__inuse">
      {kinds.map((k) => {
        const kind = k === "screen" || k === "input" || k === "shell" ? k : null;
        return kind ? (
          <span key={k} role="img" aria-label={t(`island.inUse.${kind}`)} title={t(`island.inUse.${kind}`)}>
            <Icon name={IN_USE_ICON[kind]} />
          </span>
        ) : null;
      })}
    </span>
  );
}

/** What KIVO did with the user's data, when the card should say so (CAP-08). */
function Note({ turn }: { turn: TurnView }) {
  return turn.note ? <div className="k-island__meta k-island__note">{turn.note}</div> : null;
}

/** Undo with a countdown ring while it is offered (UX-43); irreversible actions never offer it. */
function undoOffer(turn: TurnView, t: TFunction, on: IslandHandlers) {
  const offer = turn.undo;
  if (!offer || !on.undo) return null;
  const left = Math.ceil((offer.until - Date.now()) / 1000);
  if (left <= 0) return null;
  return (
    <span className="k-island__undo">
      <IslandCountdown seconds={left} label={t("island.undoLeft", { seconds: left })} />
      <button type="button" className="k-island__btn k-island__btn--primary" onClick={on.undo}>
        {t("island.undo")}
      </button>
    </span>
  );
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
      // A follow-up needs no wake word for a few seconds: the ring counts them down (UX-45).
      const seconds = session === "followUp" ? turn?.followUp : undefined;
      return {
        state: seconds ? `followUp-${turn?.id ?? ""}` : session,
        width: heard ? CARD : 256,
        label: t(session === "followUp" ? "island.followUp" : "island.listening"),
        sub: session === "followUp" ? t("island.followUpSub") : undefined,
        trail: (
          <>
            {guestChip(turn, t)}
            {seconds ? <IslandCountdown seconds={seconds} label={t("island.followUpLeft", { seconds })} /> : null}
          </>
        ),
        wave: true,
        body: heard && turn ? <Heard turn={turn} /> : undefined,
      };
    }
    case "thinking":
      return {
        state: "thinking",
        width: turn?.transcript ? CARD : 196,
        label: t("island.thinking"),
        trail: (
          <>
            {guestChip(turn, t)}
            {brainChip(turn, t)}
            <IslandSpin />
          </>
        ),
        body: turn?.transcript ? <Heard turn={turn} /> : undefined,
      };
    case "acting":
      return {
        state: "acting",
        width: CARD,
        label: t("island.working"),
        sub: turn && turn.steps.length > 1 ? t("island.steps", { count: turn.steps.length }) : undefined,
        lead: appIcon(turn?.targetApp ?? null, turn?.targetIcon),
        trail: (
          <>
            {guestChip(turn, t)}
            {brainChip(turn, t)}
            <InUse kinds={snapshot.inUse} t={t} />
            <ModeChip mode={snapshot.mode} t={t} on={on} />
            <IslandSpin />
          </>
        ),
        body: turn ? (
          <>
            <Heard turn={turn} t={t} on={on} />
            <Steps steps={turn.steps} />
            <Note turn={turn} />
            <Footer t={t} on={on} stop />
          </>
        ) : undefined,
      };
    case "speaking":
      return {
        state: "speaking",
        width: CARD,
        label: t("island.kivo"),
        lead: appIcon(turn?.targetApp ?? null, turn?.targetIcon),
        trail: (
          <>
            {guestChip(turn, t)}
            {brainChip(turn, t)}
          </>
        ),
        wave: true,
        voice: "kivo",
        body: turn ? (
          <>
            <Heard turn={turn} t={t} on={on} />
            {turn.answer && <div className="k-island__answer">{turn.answer}</div>}
            <Note turn={turn} />
            {undoOffer(turn, t, on)}
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
          lead: appIcon(turn.targetApp, turn.targetIcon) ?? <IslandOk />,
          trail: turn.brain ? brainChip(turn, t) : <IslandChip>{t("island.stepDone").toUpperCase()}</IslandChip>,
          body: (
            <>
              <Heard turn={turn} t={t} on={on} />
              {turn.steps.length > 0 && <Steps steps={turn.steps} />}
              <div className="k-island__answer">{turn.answer}</div>
              <Note turn={turn} />
              {undoOffer(turn, t, on)}
              {turn.brain && (
                <button type="button" className="k-island__misroute" onClick={() => on.misroute(turn.id)}>
                  {t("island.misroute")}
                </button>
              )}
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
