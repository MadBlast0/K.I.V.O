/**
 * The Island for what KIVO is doing now (UX §2, UX-07/09): the runtime's session state plus its
 * live turn — what KIVO heard, the steps it is taking, what it answers, and any decision waiting
 * for the user. Only what the runtime reports is shown.
 */
import type { TFunction } from "i18next";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  Capability,
  ConfirmSpec,
  ControlView,
  DraftView,
  GrantDuration,
  LiveActivity,
  Offer,
  OverlayStyle,
  PermissionMode,
  PointTarget,
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
import { Icon, type IconName } from "../../icons";

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
  /** "Remember this" on an answer (MEM-05): the runtime keeps the answer as a memory. */
  remember: (turnId: string) => Promise<unknown>;
  /** The Draft card's Edit (CONV-15): opens the text field with the prompt in it. */
  editDraft: (callId: string, text: string) => void;
  /** Answers an offer the Island makes outside a turn (CONV-10). */
  offer: (id: string, accept: boolean) => void;
  /** Pauses or resumes computer use (CAP-12). */
  computerPause?: () => void;
  /** "Talk live": starts a realtime conversation (BRAIN-33). */
  live?: () => void;
  /** Opens a task on the Tasks page (UX-24). */
  openTask: (id: string) => void;
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

/** A realtime conversation's chip (BRAIN-33): LIVE with its running time. */
function liveChip(turn: TurnView | null | undefined, t: TFunction) {
  const live = turn?.live;
  return live ? <LiveClock started={live.started} maxMinutes={live.maxMinutes ?? null} t={t} /> : null;
}

/** mm:ss since `started`, ticking once a second (only while a live conversation is open). */
export function LiveClock({ started, maxMinutes, t }: { started: number; maxMinutes: number | null; t: TFunction }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, []);
  const seconds = Math.max(0, Math.floor((now - started) / 1000));
  const time = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
  const label =
    maxMinutes != null
      ? t("island.liveOf", { clock: time, minutes: maxMinutes })
      : t("island.liveFor", { clock: time });
  return (
    <span className="k-island__chip k-island__chip--live" aria-label={label} title={label}>
      <span className="k-island__live-dot" aria-hidden />
      {`${t("island.live").toUpperCase()} · ${time}`}
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
          label: confirm.watch
            ? t("island.watchAllow")
            : confirm.plan
              ? t("island.approvePlan")
              : t("island.allowOnce"),
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
        {
          label: confirm.watch ? t("island.watchSkip") : t("island.deny"),
          kind: confirm.watch ? undefined : ("danger" as const),
          onClick: () => on.answer(confirm.callId, false, false),
        },
      ]}
    />
  );
}

/** The Draft card (CONV-15): the prompt KIVO will type into another AI, where it goes, and Send ·
 * Edit · Cancel. Voice edits it live ("add …", "remove the last sentence", "read it back") and
 * "send" sends it. */
function draftCard(draft: DraftView, confirm: ConfirmSpec, turn: TurnView, t: TFunction, on: IslandHandlers) {
  const hint = turn.answering ? (
    <VoiceHint
      words={[t("island.voice.send"), t("island.voice.addTo"), t("island.voice.readBack"), t("island.voice.cancel")]}
    />
  ) : null;
  return {
    state: `draft-${confirm.callId}-${draft.text.length}`,
    width: 490,
    label: t("island.draft.title"),
    sub: draft.target,
    lead: <IslandDot color="#3B8BFF" />,
    trail: guestChip(turn, t),
    body: (
      <>
        <Heard turn={turn} />
        <div className="k-island__draft" aria-label={t("island.draft.text")}>
          {draft.text}
        </div>
        <IslandActions
          hint={false}
          actions={[
            {
              label: t("island.draft.send"),
              kind: "primary",
              onClick: () => on.answer(confirm.callId, true, false),
            },
            { label: t("island.draft.edit"), onClick: () => on.editDraft(confirm.callId, draft.text) },
            { label: t("island.draft.cancel"), kind: "danger", onClick: () => on.answer(confirm.callId, false, false) },
          ]}
        />
        {hint}
      </>
    ),
  } satisfies IslandModel;
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

/** "What can I say?" (UX-44): examples for the app in front, then general ones. */
function Help({ turn }: { turn: TurnView }) {
  if (!turn.help?.length) return null;
  return (
    <ul className="k-island__help">
      {turn.help.map((example) => (
        <li key={example}>“{example}”</li>
      ))}
    </ul>
  );
}

/** The task a request started ("tell me when …"): Open in Tasks (UX-24). */
function TaskLink({ turn, t, on }: { turn: TurnView; t: TFunction; on: IslandHandlers }) {
  const id = turn.taskId;
  if (!id) return null;
  return (
    <button type="button" className="k-island__misroute" onClick={() => on.openTask(id)}>
      {t("island.openTask")}
    </button>
  );
}

/** Seconds left until `until` (epoch ms). */
function secondsLeft(until: number): number {
  return Math.max(0, Math.ceil((until - Date.now()) / 1000));
}

const two = (n: number) => String(n).padStart(2, "0");

/** "4:05", "1:02:03". */
export function clock(seconds: number): string {
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  const s = seconds % 60;
  return h > 0 ? `${h}:${two(m)}:${two(s)}` : `${m}:${two(s)}`;
}

const ACTIVITY_ICON: Record<string, IconName | undefined> = {
  timer: "clock",
  download: "download",
  agent: "code",
  media: "music",
  task: "tasks",
};

/** Live activities in the collapsed Island (UX-15): the first one, and how many more. */
function activityIsland(a: LiveActivity, more: number, t: TFunction, on: IslandHandlers): IslandModel {
  const icon = ACTIVITY_ICON[a.kind] ?? "tasks";
  const left = a.until != null ? secondsLeft(a.until) : null;
  const progress = a.progress != null ? `${Math.round(a.progress * 100)}%` : null;
  return {
    state: `activity-${a.id}`,
    width: 300,
    label: a.title,
    sub: [a.detail, left != null ? clock(left) : progress].filter(Boolean).join(" · ") || undefined,
    lead: <Icon name={icon} />,
    trail: (
      <>
        {more > 0 && <IslandChip>{t("island.moreActivities", { count: more })}</IslandChip>}
        {a.taskId && (
          <button
            type="button"
            className="k-island__btn"
            aria-label={t("island.openTask")}
            title={t("island.openTask")}
            onClick={() => a.taskId && on.openTask(a.taskId)}
          >
            <Icon name="external" />
          </button>
        )}
      </>
    ),
  };
}

/** Computer use's controller (CAP-12): what KIVO controls, its progress, Pause / Resume, Stop. */
function controllerIsland(cu: ControlView, t: TFunction, on: IslandHandlers): IslandModel {
  return {
    state: `controller-${cu.paused ? "paused" : "running"}`,
    width: 360,
    label: t("island.controlling", { app: cu.app }),
    sub: cu.paused
      ? t("island.controlPaused")
      : t("island.controlProgress", { step: cu.step, max: cu.maxSteps, cost: (cu.costCents / 100).toFixed(2) }),
    lead: <Icon name="cursor" />,
    trail: (
      <>
        {on.computerPause && (
          <button
            type="button"
            className="k-island__btn"
            aria-label={cu.paused ? t("island.resume") : t("island.pause")}
            title={cu.paused ? t("island.resume") : t("island.pause")}
            onClick={on.computerPause}
          >
            <Icon name={cu.paused ? "play" : "pause"} />
          </button>
        )}
        <button
          type="button"
          className="k-island__btn"
          aria-label={t("island.stop")}
          title={t("island.stop")}
          onClick={on.stop}
        >
          <Icon name="stop" />
        </button>
      </>
    ),
  };
}

/** A question the Island asks outside a turn ("Remember kivo as a workspace?", CONV-10). */
function offerIsland(offer: Offer, on: IslandHandlers): IslandModel {
  return {
    state: `offer-${offer.id}`,
    width: 420,
    label: offer.text,
    lead: <IslandDot color="#3B8BFF" />,
    body: (
      <IslandActions
        hint={false}
        actions={[
          { label: offer.accept, kind: "primary", onClick: () => on.offer(offer.id, true) },
          { label: offer.decline, onClick: () => on.offer(offer.id, false) },
        ]}
      />
    ),
  };
}

/** What KIVO did with the user's data, when the card should say so (CAP-08). */
function Note({ turn }: { turn: TurnView }) {
  return turn.note ? <div className="k-island__meta k-island__note">{turn.note}</div> : null;
}

/** Undo with a countdown ring while it is offered (UX-43); irreversible actions never offer it. */
/** "Remember this" (MEM-05): once kept, it says so. */
function RememberThis({ turnId, on }: { turnId: string; on: IslandHandlers }) {
  const { t } = useTranslation();
  const [state, setState] = useState<"idle" | "saving" | "kept" | "failed">("idle");
  if (state === "kept") return <span className="k-island__misroute">{t("island.remembered")}</span>;
  return (
    <button
      type="button"
      className="k-island__misroute"
      disabled={state === "saving"}
      onClick={() => {
        setState("saving");
        on.remember(turnId).then(
          () => setState("kept"),
          () => setState("failed"),
        );
      }}
    >
      {state === "failed" ? t("island.rememberFailed") : t("island.rememberThis")}
    </button>
  );
}

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

/** How the Island shows things (Settings → Island, Accessibility), from the runtime. */
export interface IslandPrefs {
  showTranscript: boolean;
  showUndo: boolean;
  voiceHints: boolean;
  captions: boolean;
}

export function islandPrefs(snapshot: StateSnapshot | null): IslandPrefs {
  const island = snapshot?.island;
  return {
    showTranscript: island?.showTranscript ?? true,
    showUndo: island?.showUndo ?? true,
    voiceHints: island?.voiceHints ?? true,
    captions: island?.captions ?? true,
  };
}

/** The Island for the runtime's state, as the overlay style shows it (UX-17), or `null` when
 * there is nothing to show. */
export function islandForTurn(snapshot: StateSnapshot, t: TFunction, on: IslandHandlers): IslandModel | null {
  return withStyle(islandModel(snapshot, t, on), snapshot.island?.style ?? "pill-and-card");
}

/** Decisions always show: KIVO can't go on without an answer. */
function isDecision(model: IslandModel): boolean {
  return model.state.startsWith("confirm-") || model.state.startsWith("draft-");
}

/** Width of the pill without its card. */
const PILL = 300;

/** Applies the overlay style (UX-17): pill only drops the card (and so the text), card only shows
 * the Island only when there's something to read, off shows decisions alone (sounds carry the
 * rest). */
export function withStyle(model: IslandModel | null, style: OverlayStyle): IslandModel | null {
  if (!model || style === "pill-and-card" || isDecision(model)) return model;
  switch (style) {
    case "pill-only":
      return model.body ? { ...model, body: undefined, width: Math.min(model.width, PILL) } : model;
    case "card-only":
      return model.body ? model : null;
    case "off":
      return null;
  }
}

function islandModel(snapshot: StateSnapshot, t: TFunction, on: IslandHandlers): IslandModel | null {
  if (snapshot.islandHidden) return null;
  const prefs = islandPrefs(snapshot);
  const { session } = snapshot;
  // The user's words while they speak, and Undo, only when they're wanted.
  const raw = snapshot.turn;
  const turn = raw
    ? {
        ...raw,
        transcript: !prefs.showTranscript && !raw.transcriptFinal ? "" : raw.transcript,
        undo: prefs.showUndo ? raw.undo : null,
        answering: prefs.voiceHints ? raw.answering : false,
      }
    : raw;
  // What KIVO says, shown as text too unless captions are off.
  const caption = (text: string | null | undefined) =>
    prefs.captions && text ? <div className="k-island__answer">{text}</div> : null;

  if (turn?.confirm && turn.draft) return draftCard(turn.draft, turn.confirm, turn, t, on);
  if (turn?.confirm) return confirmCard(turn.confirm, turn, t, on);
  // Hidden (Settings → Island): sounds only. A question that needs an answer still shows.
  if (snapshot.island?.companion === "hidden") return null;

  // Computer use (CAP-12): the controller, with the step, the cost so far, Pause and Stop.
  const cu = snapshot.controlling;
  if (cu && !turn?.confirm) return controllerIsland(cu, t, on);

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
            {liveChip(turn, t)}
            {seconds ? <IslandCountdown seconds={seconds} label={t("island.followUpLeft", { seconds })} /> : null}
          </>
        ),
        wave: true,
        body:
          turn?.live && turn ? (
            <>
              {heard && <Heard turn={turn} />}
              {caption(turn.answer)}
              <Footer t={t} on={on} stop />
            </>
          ) : heard && turn ? (
            <Heard turn={turn} />
          ) : undefined,
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
            {liveChip(turn, t)}
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
        lead: turn?.point ? (
          <PointArrow point={turn.point} t={t} />
        ) : (
          appIcon(turn?.targetApp ?? null, turn?.targetIcon)
        ),
        trail: (
          <>
            {guestChip(turn, t)}
            {liveChip(turn, t)}
            {brainChip(turn, t)}
          </>
        ),
        wave: true,
        voice: "kivo",
        body: turn ? (
          <>
            <Heard turn={turn} t={t} on={on} />
            {caption(turn.answer)}
            <Help turn={turn} />
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
          lead: turn.point ? (
            <PointArrow point={turn.point} t={t} />
          ) : (
            (appIcon(turn.targetApp, turn.targetIcon) ?? <IslandOk />)
          ),
          trail: turn.brain ? brainChip(turn, t) : <IslandChip>{t("island.stepDone").toUpperCase()}</IslandChip>,
          body: (
            <>
              <Heard turn={turn} t={t} on={on} />
              {turn.steps.length > 0 && <Steps steps={turn.steps} />}
              {caption(turn.answer)}
              <Help turn={turn} />
              <Note turn={turn} />
              {undoOffer(turn, t, on)}
              <TaskLink turn={turn} t={t} on={on} />
              {turn.brain && (
                <span className="k-island__links">
                  {turn.liveOffer && on.live && (
                    <button type="button" className="k-island__misroute" onClick={on.live}>
                      {t("island.talkLive")}
                    </button>
                  )}
                  <RememberThis turnId={turn.id} on={on} />
                  <button type="button" className="k-island__misroute" onClick={() => on.misroute(turn.id)}>
                    {t("island.misroute")}
                  </button>
                </span>
              )}
              <Footer t={t} on={on} stop={false} />
            </>
          ),
        };
      }
      // Outside a turn: an offer, then live activities (UX-15).
      if (snapshot.offer) return offerIsland(snapshot.offer, on);
      const [first, ...more] = snapshot.activities ?? [];
      if (session === "idle" && first) return activityIsland(first, more.length, t, on);
      return null;
  }
}

/** Which way the target lies from the Island's window (UX-39): the runtime places the Island
 * below it when it can, else above, else beside. Screen positions are physical pixels. */
export function pointDirection(
  p: PointTarget,
  win: { x: number; y: number; width: number; height: number },
): "up" | "down" | "left" | "right" {
  if (p.y + p.height <= win.y) return "up";
  if (p.y >= win.y + win.height) return "down";
  if (p.x + p.width <= win.x) return "left";
  return "right";
}

/** An arrow from the Island toward the control KIVO is pointing at. */
function PointArrow({ point, t }: { point: PointTarget; t: TFunction }) {
  const ratio = window.devicePixelRatio || 1;
  const dir = pointDirection(point, {
    x: window.screenX * ratio,
    y: window.screenY * ratio,
    width: window.outerWidth * ratio,
    height: window.outerHeight * ratio,
  });
  return (
    <span
      className="k-island__point"
      data-dir={dir}
      role="img"
      aria-label={t("island.pointing", { name: point.label })}
    >
      <Icon name="up" size={16} />
    </span>
  );
}

/** True when the Island shows buttons, so its window must take clicks (UX §2). */
export function hasButtons(model: IslandModel | null): boolean {
  if (!model) return false;
  return (
    model.state.startsWith("confirm-") ||
    model.state.startsWith("error-") ||
    model.state.startsWith("done-") ||
    model.state.startsWith("capability-") ||
    model.state.startsWith("draft-") ||
    model.state.startsWith("offer-") ||
    model.state.startsWith("activity-") ||
    model.state.startsWith("controller-") ||
    model.state === "acting" ||
    model.state === "speaking"
  );
}
