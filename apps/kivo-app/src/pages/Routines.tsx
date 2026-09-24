/**
 * Routines (UX-26, ROUTINES §4): the user's own commands and routines, and the builder.
 *
 * - The list: each routine with its triggers, an AI badge when a step calls a brain (ROUT-08), its
 *   switch, Run and Edit.
 * - The builder (ROUT-09): name → triggers (phrases with `{variables}`, a hotkey) → steps, which
 *   reorder by dragging (or with Move up / Move down), each a tool call with a form generated from
 *   the tool's JSON Schema, something to say, a wait, or a question for a brain → its error policy
 *   (ROUT-04) → a check for clashing phrases and hotkeys (ROUT-07) → save, which shows the full
 *   list of permissions it needs and grants them for exactly those arguments (ROUT-03) → test run.
 *
 * The runtime checks and stores everything; the page only edits a draft.
 */
import { OtherTriggers, describeTrigger, isOther } from "../components/routines/OtherTriggers";
import { useCallback, useEffect, useMemo, useRef, useState, type DragEvent } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import {
  Alert,
  Button,
  Checkbox,
  Dialog,
  EmptyState,
  Group,
  IconButton,
  Meta,
  Note,
  Pill,
  Row,
  Section,
  Select,
  ShortcutRecorder,
  Switch,
  Tag,
  TextArea,
  TextField,
  useToast,
  type Tone,
} from "../components/ui";
import { Icon, type IconName } from "../icons";
import {
  Method,
  type OnError,
  type Risk,
  type Routine,
  type RoutineCheck,
  type RoutineStep,
  type RoutineView,
  type StepAction,
  type ToolItem,
  type Trigger,
} from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { display, fieldsOf, missing, parseInput, type FormField } from "../lib/schemaForm";

const RISK_TONE: Record<Risk, Tone> = { safe: "success", low: "success", medium: "neutral", high: "danger" };

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** A new, empty routine. */
export function blankRoutine(): Routine {
  return {
    id: "",
    name: "",
    description: "",
    enabled: true,
    triggers: [{ type: "phrase", phrases: [], lang: "en" }],
    steps: [],
    variables: [],
    grants: [],
  };
}

/** A copy of a routine to start a new one from (a template). */
function copyOf(routine: Routine): Routine {
  return { ...structuredClone(routine), id: "", starter: undefined, grants: [], enabled: true };
}

let nextStep = 0;
function stepId(): string {
  nextStep += 1;
  return `s${Date.now().toString(36)}${nextStep}`;
}

export function phrasesOf(routine: Routine): string[] {
  return routine.triggers.flatMap((t) => (t.type === "phrase" ? t.phrases : []));
}

export function hotkeyOf(routine: Routine): string | undefined {
  return routine.triggers.find((t): t is Extract<Trigger, { type: "hotkey" }> => t.type === "hotkey")?.chord;
}

/** The draft with `phrases` and `hotkey` as its triggers; its schedules and events are kept
 * unless `others` replaces them. */
export function withTriggers(
  routine: Routine,
  phrases: string[],
  hotkey: string | undefined,
  others: Trigger[] = routine.triggers.filter(isOther),
): Routine {
  const triggers: Trigger[] = [];
  if (phrases.length > 0) triggers.push({ type: "phrase", phrases, lang: "en" });
  if (hotkey) triggers.push({ type: "hotkey", chord: hotkey });
  triggers.push(...others);
  if (triggers.length === 0) triggers.push({ type: "manual" });
  return { ...routine, triggers };
}

/** Moves the step at `from` to `to`. */
export function moveStep(steps: RoutineStep[], from: number, to: number): RoutineStep[] {
  if (from === to || from < 0 || to < 0 || from >= steps.length || to >= steps.length) return steps;
  const next = [...steps];
  const [moved] = next.splice(from, 1);
  if (moved) next.splice(to, 0, moved);
  return next;
}

export function Routines() {
  const { t } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [routines, setRoutines] = useState<RoutineView[]>([]);
  const [tools, setTools] = useState<ToolItem[]>([]);
  const [editing, setEditing] = useState<Routine | null>(null);
  const [choosing, setChoosing] = useState(false);
  const [running, setRunning] = useState<RoutineView | null>(null);

  const load = useCallback(() => {
    if (!connected) return;
    void request<RoutineView[]>(Method.routinesList)
      .then(setRoutines)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  // A routine drafted by voice, chat, "save what you just did" or an import opens for review
  // (ROUT-13, ROUT-14).
  const takeDraft = useCallback(() => {
    if (!connected) return;
    void request<Routine | null>(Method.routinesDraft)
      .then((draft) => {
        if (draft) {
          setEditing(draft);
          toast(t("routines.draftReady"));
        }
      })
      .catch(() => {});
  }, [connected, request, t, toast]);
  useEffect(takeDraft, [takeDraft]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "discoveryChanged" && event.event.section === "routines") {
      takeDraft();
      load();
    }
  });
  const file = useRef<HTMLInputElement>(null);
  const importFile = (chosen: File) =>
    void chosen
      .text()
      .then((content) => request<Routine>(Method.routinesImport, { content }))
      .then((draft) => {
        setEditing(draft);
        toast(t("routines.imported", { name: draft.name }));
      })
      .catch((e: unknown) => toast(message(e)));
  useEffect(() => {
    if (!connected) return;
    void request<ToolItem[]>(Method.routinesTools)
      .then(setTools)
      .catch(() => {});
  }, [connected, request]);

  const enable = (view: RoutineView, on: boolean) => {
    request(Method.routinesEnable, { id: view.routine.id, on })
      .then(load)
      .catch((e: unknown) => {
        toast(message(e));
        // A routine without its permissions opens in the builder to grant them.
        if (on && !view.granted) setEditing(structuredClone(view.routine));
      });
  };
  const run = (view: RoutineView, vars: Record<string, string> = {}) => {
    request<{ task: string }>(Method.routinesRun, { id: view.routine.id, vars })
      .then(() => toast(t("routines.started", { name: view.routine.name })))
      .catch((e: unknown) => toast(message(e)));
  };
  const templates = routines.filter((r) => r.routine.starter);

  if (!connected) {
    return (
      <>
        <PageHeader title={t("nav.routines")} subtitle={t("routines.subtitle")} />
        <Note>{t("voice.notConnected")}</Note>
      </>
    );
  }

  return (
    <>
      <PageHeader
        title={t("nav.routines")}
        subtitle={t("routines.subtitle")}
        actions={
          <>
            <Button icon="download" onClick={() => file.current?.click()}>
              {t("routines.import")}
            </Button>
            <input
              ref={file}
              type="file"
              accept=".json,application/json"
              hidden
              aria-label={t("routines.importFile")}
              onChange={(e) => {
                const chosen = e.target.files?.[0];
                e.target.value = "";
                if (chosen) importFile(chosen);
              }}
            />
            <Button variant="primary" icon="add" onClick={() => setChoosing(true)}>
              {t("routines.new")}
            </Button>
          </>
        }
      />
      <div className="k-routines">
        <div>
          {routines.length === 0 ? (
            <EmptyState icon="routine" title={t("routines.none")}>
              {t("routines.noneDetail")}
            </EmptyState>
          ) : (
            <Group>
              {routines.map((view) => (
                <RoutineRow
                  key={view.routine.id}
                  view={view}
                  selected={editing?.id === view.routine.id}
                  onEdit={() => setEditing(structuredClone(view.routine))}
                  onEnable={(on) => enable(view, on)}
                  onRun={() => (view.routine.variables.length > 0 ? setRunning(view) : run(view))}
                />
              ))}
            </Group>
          )}
          <Note>{t("routines.note")}</Note>
        </div>
        {editing && (
          <Builder
            key={editing.id || "new"}
            initial={editing}
            tools={tools}
            all={routines.map((r) => ({ id: r.routine.id, name: r.routine.name }))}
            saved={routines.find((r) => r.routine.id === editing.id)}
            onSaved={(view) => {
              load();
              setEditing(structuredClone(view.routine));
            }}
            onDeleted={() => {
              load();
              setEditing(null);
            }}
            onClose={() => setEditing(null)}
            onRun={run}
          />
        )}
      </div>
      <Dialog
        open={choosing}
        onOpenChange={setChoosing}
        title={t("routines.new")}
        description={t("routines.newDetail")}
        footer={
          <Button
            variant="primary"
            onClick={() => {
              setChoosing(false);
              setEditing(blankRoutine());
            }}
          >
            {t("routines.blank")}
          </Button>
        }
      >
        <Section title={t("routines.templates")} />
        <Group>
          {templates.map((view) => (
            <Row
              key={view.routine.id}
              icon="routine"
              title={view.routine.name}
              subtitle={view.routine.description}
              onClick={() => {
                setChoosing(false);
                setEditing(copyOf(view.routine));
              }}
            />
          ))}
        </Group>
      </Dialog>
      {running && (
        <RunDialog
          view={running}
          onClose={() => setRunning(null)}
          onRun={(vars) => {
            run(running, vars);
            setRunning(null);
          }}
        />
      )}
    </>
  );
}

function RoutineRow({
  view,
  selected,
  onEdit,
  onEnable,
  onRun,
}: {
  view: RoutineView;
  selected: boolean;
  onEdit: () => void;
  onEnable: (on: boolean) => void;
  onRun: () => void;
}) {
  const { t } = useTranslation();
  const r = view.routine;
  const phrases = phrasesOf(r);
  const hotkey = hotkeyOf(r);
  const others = r.triggers.filter(isOther);
  const subtitle = [
    phrases.length > 0 ? phrases.map((p) => `“${p}”`).join(", ") : undefined,
    hotkey,
    ...others.map((o) => describeTrigger(o, t, (id) => id)),
    phrases.length === 0 && !hotkey && others.length === 0 ? t("routines.manual") : undefined,
    view.containsAi ? t("routines.usesAi") : undefined,
    view.customCommand ? t("routines.customCommand") : undefined,
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <div className="k-routine-row" data-selected={selected || undefined}>
      <Row
        icon={view.containsAi ? "ai" : "routine"}
        title={r.name}
        subtitle={subtitle}
        end={
          <>
            {!view.granted && <Tag tone="warning">{t("routines.needsPermission")}</Tag>}
            <IconButton icon="play" size="sm" label={t("routines.runNamed", { name: r.name })} onClick={onRun} />
            <IconButton icon="edit" size="sm" label={t("routines.editNamed", { name: r.name })} onClick={onEdit} />
            <Switch label={t("routines.enabledNamed", { name: r.name })} checked={r.enabled} onChange={onEnable} />
          </>
        }
      />
    </div>
  );
}

function RunDialog({
  view,
  onClose,
  onRun,
}: {
  view: RoutineView;
  onClose: () => void;
  onRun: (vars: Record<string, string>) => void;
}) {
  const { t } = useTranslation();
  const [vars, setVars] = useState<Record<string, string>>({});
  const ready = view.routine.variables.every((v) => (vars[v.name] ?? "").trim() !== "");
  return (
    <Dialog
      open
      onOpenChange={(open) => !open && onClose()}
      title={t("routines.runNamed", { name: view.routine.name })}
      footer={
        <Button variant="primary" icon="play" disabled={!ready} onClick={() => onRun(vars)}>
          {t("routines.run")}
        </Button>
      }
    >
      {view.routine.variables.map((v) => (
        <label key={v.name} className="k-builder__field">
          <span>{v.name}</span>
          <TextField
            aria-label={v.name}
            inputMode={v.kind === "number" ? "numeric" : undefined}
            value={vars[v.name] ?? ""}
            onChange={(e) => setVars((cur) => ({ ...cur, [v.name]: e.target.value }))}
          />
        </label>
      ))}
    </Dialog>
  );
}

/* ───────────────────────────── The builder ───────────────────────────── */

type StepKind = "tool" | "say" | "delay" | "ask";

function kindOf(action: StepAction): StepKind {
  switch (action.type) {
    case "say":
      return "say";
    case "delay":
      return "delay";
    case "ask":
    case "decide":
      return "ask";
    default:
      return "tool";
  }
}

function newAction(kind: StepKind, tools: ToolItem[]): StepAction {
  switch (kind) {
    case "say":
      return { type: "say", text: "" };
    case "delay":
      return { type: "delay", ms: 5000 };
    case "ask":
      return { type: "ask", prompt: "", profile: null };
    case "tool":
      return { type: "tool", tool: tools[0]?.id ?? "", args: {} };
  }
}

function argsOf(action: StepAction): Record<string, unknown> {
  if (action.type !== "tool" || typeof action.args !== "object" || action.args === null) return {};
  return Object.fromEntries(Object.entries(action.args));
}

function Builder({
  initial,
  tools,
  all,
  saved,
  onSaved,
  onDeleted,
  onClose,
  onRun,
}: {
  initial: Routine;
  tools: ToolItem[];
  /** Every routine, for "After another routine". */
  all: { id: string; name: string }[];
  saved: RoutineView | undefined;
  onSaved: (view: RoutineView) => void;
  onDeleted: () => void;
  onClose: () => void;
  onRun: (view: RoutineView) => void;
}) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [draft, setDraft] = useState<Routine>(initial);
  const [check, setCheck] = useState<RoutineCheck | null>(null);
  const [phrase, setPhrase] = useState("");
  const [reviewing, setReviewing] = useState(false);
  const [open, setOpen] = useState<string | null>(null);
  const dragged = useRef<number | null>(null);

  const phrases = phrasesOf(draft);
  const hotkey = hotkeyOf(draft);
  const changed = !saved || JSON.stringify(saved.routine) !== JSON.stringify(draft);

  // Checks the draft as it changes (ROUT-07, ROUT-03): clashes, problems and the grants it needs.
  useEffect(() => {
    const id = window.setTimeout(() => {
      request<RoutineCheck>(Method.routinesCheck, { routine: draft })
        .then(setCheck)
        .catch(() => {});
    }, 250);
    return () => window.clearTimeout(id);
  }, [draft, request]);

  const setStep = (index: number, step: RoutineStep) =>
    setDraft((d) => ({ ...d, steps: d.steps.map((s, i) => (i === index ? step : s)) }));
  const addStep = (kind: StepKind) => {
    const step: RoutineStep = { id: stepId(), action: newAction(kind, tools), onError: { policy: "stop" } };
    setDraft((d) => ({ ...d, steps: [...d.steps, step] }));
    setOpen(step.id);
  };
  const move = (from: number, to: number) => setDraft((d) => ({ ...d, steps: moveStep(d.steps, from, to) }));
  const addPhrase = () => {
    const p = phrase.trim();
    if (!p || phrases.includes(p)) return;
    setDraft((d) => withTriggers(d, [...phrasesOf(d), p], hotkeyOf(d)));
    setPhrase("");
  };

  const incomplete = draft.steps.some((s) => {
    if (s.action.type === "tool") {
      const tool = tools.find((x) => x.id === (s.action.type === "tool" ? s.action.tool : ""));
      return !tool || missing(fieldsOf(tool.params), argsOf(s.action)).length > 0;
    }
    if (s.action.type === "say") return s.action.text.trim() === "";
    if (s.action.type === "ask") return s.action.prompt.trim() === "";
    return false;
  });
  const blocked =
    !check || check.problems.length > 0 || check.collisions.some((c) => c.blocking) || draft.steps.length === 0;

  const save = (grant: boolean) => {
    request<RoutineView>(Method.routinesSave, { routine: draft, grant })
      .then((view) => {
        setReviewing(false);
        toast(t("routines.saved", { name: view.routine.name }));
        onSaved(view);
      })
      .catch((e: unknown) => toast(message(e)));
  };
  const remove = () => {
    request(Method.routinesDelete, { id: draft.id })
      .then(onDeleted)
      .catch((e: unknown) => toast(message(e)));
  };

  const onDrop = (to: number) => (e: DragEvent) => {
    e.preventDefault();
    if (dragged.current !== null) move(dragged.current, to);
    dragged.current = null;
  };

  return (
    <section className="k-builder" aria-label={t("routines.builder")}>
      <div className="k-builder__head">
        <TextField
          aria-label={t("routines.name")}
          placeholder={t("routines.namePlaceholder")}
          value={draft.name}
          onChange={(e) => setDraft((d) => ({ ...d, name: e.target.value }))}
        />
        <IconButton icon="close" size="sm" label={t("shell.close")} onClick={onClose} />
      </div>

      <Section title={t("routines.startsWhen")} />
      <div className="k-builder__chips">
        {phrases.map((p) => (
          <span key={p} className="k-builder__chip">
            <Pill>“{p}”</Pill>
            <IconButton
              icon="close"
              size="sm"
              variant="plain"
              label={t("routines.removePhrase", { phrase: p })}
              onClick={() =>
                setDraft((d) =>
                  withTriggers(
                    d,
                    phrasesOf(d).filter((x) => x !== p),
                    hotkeyOf(d),
                  ),
                )
              }
            />
          </span>
        ))}
        <TextField
          aria-label={t("routines.addPhrase")}
          placeholder={t("routines.phrasePlaceholder")}
          value={phrase}
          onChange={(e) => setPhrase(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              addPhrase();
            }
          }}
        />
        <Button size="sm" icon="add" onClick={addPhrase} disabled={phrase.trim() === ""}>
          {t("routines.addPhrase")}
        </Button>
      </div>
      <div className="k-builder__hotkey">
        <span>{t("routines.hotkey")}</span>
        <ShortcutRecorder
          value={hotkey ? hotkey.split("+") : []}
          onChange={(keys) => setDraft((d) => withTriggers(d, phrasesOf(d), keys.join("+")))}
        />
        {hotkey && (
          <Button size="sm" variant="plain" onClick={() => setDraft((d) => withTriggers(d, phrasesOf(d), undefined))}>
            {t("routines.removeHotkey")}
          </Button>
        )}
      </div>
      <Section title={t("routines.when.title")} />
      <OtherTriggers
        triggers={draft.triggers.filter(isOther)}
        routines={all}
        selfId={draft.id}
        onChange={(next) => setDraft((d) => withTriggers(d, phrasesOf(d), hotkeyOf(d), next))}
      />
      {check && check.collisions.length > 0 && (
        <ul className="k-builder__issues">
          {check.collisions.map((c) => (
            <li key={`${c.phrase}-${c.kind}-${c.with}`}>
              <Tag tone={c.blocking ? "danger" : "warning"}>
                {t(c.blocking ? "routines.clash" : "routines.soundsLike", { phrase: c.phrase, with: c.with })}
              </Tag>
            </li>
          ))}
        </ul>
      )}

      <Section title={t("routines.steps")} />
      {draft.steps.length === 0 ? (
        <Note>{t("routines.noSteps")}</Note>
      ) : (
        <ol className="k-builder__steps">
          {draft.steps.map((step, i) => (
            // Dragging is for the pointer; Move up / Move down do the same from the keyboard.
            // oxlint-disable-next-line jsx-a11y/no-noninteractive-element-interactions
            <li
              key={step.id}
              className="k-builder__step"
              draggable
              onDragStart={() => (dragged.current = i)}
              onDragOver={(e) => e.preventDefault()}
              onDrop={onDrop(i)}
            >
              <div className="k-builder__step-head">
                <span className="k-builder__grip" aria-hidden>
                  <Icon name="grip" />
                </span>
                <span className="k-builder__n">{i + 1}</span>
                <StepSummary step={step} tools={tools} />
                {step.parallelGroup && <Pill>{t("routines.together")}</Pill>}
                <IconButton
                  icon="edit"
                  size="sm"
                  variant="plain"
                  label={t("routines.editStep", { n: i + 1 })}
                  aria-expanded={open === step.id}
                  onClick={() => setOpen(open === step.id ? null : step.id)}
                />
                <IconButton
                  icon="up"
                  size="sm"
                  variant="plain"
                  label={t("routines.moveUp", { n: i + 1 })}
                  disabled={i === 0}
                  onClick={() => move(i, i - 1)}
                />
                <IconButton
                  icon="down"
                  size="sm"
                  variant="plain"
                  label={t("routines.moveDown", { n: i + 1 })}
                  disabled={i === draft.steps.length - 1}
                  onClick={() => move(i, i + 1)}
                />
                <IconButton
                  icon="delete"
                  size="sm"
                  variant="plain"
                  label={t("routines.removeStep", { n: i + 1 })}
                  onClick={() => setDraft((d) => ({ ...d, steps: d.steps.filter((s) => s.id !== step.id) }))}
                />
              </div>
              {open === step.id && (
                <StepEditor step={step} tools={tools} previous={draft.steps[i - 1]} onChange={(s) => setStep(i, s)} />
              )}
            </li>
          ))}
        </ol>
      )}
      <div className="k-builder__add">
        <Button size="sm" icon="add" onClick={() => addStep("tool")} disabled={tools.length === 0}>
          {t("routines.addAction")}
        </Button>
        <Button size="sm" icon="wave" onClick={() => addStep("say")}>
          {t("routines.addSay")}
        </Button>
        <Button size="sm" icon="clock" onClick={() => addStep("delay")}>
          {t("routines.addDelay")}
        </Button>
        <Button size="sm" icon="ai" onClick={() => addStep("ask")}>
          {t("routines.addAsk")}
        </Button>
      </div>

      {check && check.problems.length > 0 && (
        <Alert kind="warning" title={t("routines.cantSave")}>
          {check.problems.join(" ")}
        </Alert>
      )}
      {check && check.grants.length > 0 && (
        <p className="k-note">{t("routines.allowed", { list: check.grants.map((g) => g.title).join(", ") })}</p>
      )}

      <div className="k-builder__foot">
        {saved && (
          <Button variant="destructive" icon="delete" onClick={remove}>
            {t("routines.delete")}
          </Button>
        )}
        {saved && (
          <Button
            icon="upload"
            onClick={() =>
              void request<{ file: string }>(Method.routinesExport, { id: saved.routine.id })
                .then((r) => toast(t("routines.exported", { file: r.file })))
                .catch((e: unknown) => toast(message(e)))
            }
          >
            {t("routines.export")}
          </Button>
        )}
        {saved && (
          <Button
            icon="skill"
            onClick={() =>
              void request<{ skill: string }>(Method.routinesToSkill, { id: saved.routine.id })
                .then(() => toast(t("routines.skillMade", { name: saved.routine.name })))
                .catch((e: unknown) => toast(message(e)))
            }
          >
            {t("routines.makeSkill")}
          </Button>
        )}
        <span className="k-builder__spacer" />
        <Button
          icon="play"
          disabled={!saved || changed || !saved.granted}
          title={changed ? t("routines.saveFirst") : undefined}
          onClick={() => saved && onRun(saved)}
        >
          {t("routines.test")}
        </Button>
        <Button variant="primary" disabled={blocked || incomplete} onClick={() => setReviewing(true)}>
          {t("routines.save")}
        </Button>
      </div>

      <Dialog
        open={reviewing}
        onOpenChange={setReviewing}
        title={t("routines.reviewTitle", { name: draft.name })}
        description={t("routines.reviewDetail")}
        footer={
          <>
            <Button onClick={() => save(false)}>{t("routines.saveWithout")}</Button>
            <Button variant="primary" onClick={() => save(true)}>
              {t("routines.allowAndSave")}
            </Button>
          </>
        }
      >
        {check && check.grants.length === 0 ? (
          <Note>{t("routines.nothingNeeded")}</Note>
        ) : (
          <Group>
            {check?.grants.map((g, i) => (
              <Row
                key={`${g.tool}-${i}`}
                icon="permissions"
                title={g.title}
                subtitle={g.capabilityOff ? t("routines.capabilityOff") : g.tool}
                end={<Tag tone={RISK_TONE[g.risk]}>{t(`risk.${g.risk}`)}</Tag>}
              />
            ))}
          </Group>
        )}
        {draft.steps.some((s) => s.action.type === "ask" || s.action.type === "decide") && (
          <Note>{t("routines.aiCosts")}</Note>
        )}
      </Dialog>
    </section>
  );
}

function StepSummary({ step, tools }: { step: RoutineStep; tools: ToolItem[] }) {
  const { t } = useTranslation();
  const a = step.action;
  let icon: IconName = "play";
  let text: string;
  switch (a.type) {
    case "tool": {
      const tool = tools.find((x) => x.id === a.tool);
      const args = Object.values(argsOf(a))
        .filter((v) => typeof v === "string" || typeof v === "number")
        .join(", ");
      text = `${tool?.title ?? a.tool}${args ? ` · ${args}` : ""}`;
      break;
    }
    case "say":
      icon = "wave";
      text = t("routines.sayStep", { text: a.text });
      break;
    case "delay":
      icon = "clock";
      text = t("routines.delayStep", { seconds: Math.round(a.ms / 1000) });
      break;
    case "ask":
      icon = "ai";
      text = t("routines.askStep", { prompt: a.prompt });
      break;
    case "decide":
      icon = "ai";
      text = a.question;
      break;
    default:
      text = a.type;
  }
  return (
    <span className="k-builder__summary">
      <Icon name={icon} />
      <span>{text}</span>
      {step.confirm && <Meta>{t("routines.asksFirst")}</Meta>}
    </span>
  );
}

const POLICIES = ["stop", "continue", "retry", "ask"] as const;
type Policy = (typeof POLICIES)[number];

function StepEditor({
  step,
  tools,
  previous,
  onChange,
}: {
  step: RoutineStep;
  tools: ToolItem[];
  previous: RoutineStep | undefined;
  onChange: (step: RoutineStep) => void;
}) {
  const { t } = useTranslation();
  const a = step.action;
  const kind = kindOf(a);
  const setAction = (action: StepAction) => onChange({ ...step, action });
  const policy: Policy = step.onError.policy;
  const setPolicy = (p: Policy) => {
    const onError: OnError = p === "retry" ? { policy: "retry", times: 2 } : { policy: p };
    onChange({ ...step, onError });
  };
  const together = !!previous && !!step.parallelGroup && step.parallelGroup === previous.parallelGroup;

  return (
    <div className="k-builder__editor">
      <Select<StepKind>
        label={t("routines.stepKind")}
        value={kind}
        onChange={(k) => setAction(newAction(k, tools))}
        items={[
          { value: "tool", label: t("routines.kind.tool") },
          { value: "say", label: t("routines.kind.say") },
          { value: "delay", label: t("routines.kind.delay") },
          { value: "ask", label: t("routines.kind.ask") },
        ]}
      />
      {a.type === "tool" && <ToolForm action={a} tools={tools} onChange={setAction} />}
      {a.type === "say" && (
        <TextField
          aria-label={t("routines.kind.say")}
          value={a.text}
          onChange={(e) => setAction({ ...a, text: e.target.value })}
        />
      )}
      {a.type === "delay" && (
        <label className="k-builder__field">
          <span>{t("routines.seconds")}</span>
          <TextField
            aria-label={t("routines.seconds")}
            inputMode="numeric"
            value={String(Math.round(a.ms / 1000))}
            onChange={(e) => setAction({ ...a, ms: Math.max(0, Number(e.target.value) || 0) * 1000 })}
          />
        </label>
      )}
      {a.type === "ask" && (
        <TextArea
          aria-label={t("routines.kind.ask")}
          rows={3}
          value={a.prompt}
          onChange={(e) => setAction({ ...a, prompt: e.target.value })}
        />
      )}
      <div className="k-builder__options">
        <Select<Policy>
          label={t("routines.onError")}
          value={policy}
          onChange={setPolicy}
          items={POLICIES.map((p) => ({ value: p, label: t(`routines.policy.${p}`) }))}
        />
        {step.onError.policy === "retry" && (
          <TextField
            aria-label={t("routines.retries")}
            inputMode="numeric"
            value={String(step.onError.times)}
            onChange={(e) =>
              onChange({
                ...step,
                onError: { policy: "retry", times: Math.min(5, Math.max(1, Number(e.target.value) || 1)) },
              })
            }
          />
        )}
        <Checkbox checked={step.confirm ?? false} onChange={(confirm) => onChange({ ...step, confirm })}>
          {t("routines.confirm")}
        </Checkbox>
        {previous && (
          <Checkbox
            checked={together}
            onChange={(on) =>
              onChange({ ...step, parallelGroup: on ? (previous.parallelGroup ?? previous.id) : undefined })
            }
          >
            {t("routines.withPrevious")}
          </Checkbox>
        )}
      </div>
    </div>
  );
}

function ToolForm({
  action,
  tools,
  onChange,
}: {
  action: Extract<StepAction, { type: "tool" }>;
  tools: ToolItem[];
  onChange: (action: StepAction) => void;
}) {
  const { t } = useTranslation();
  const tool = tools.find((x) => x.id === action.tool);
  const fields = useMemo(() => fieldsOf(tool?.params), [tool]);
  const args = argsOf(action);
  const set = (name: string, value: unknown) => {
    const next = { ...args };
    if (value === undefined) delete next[name];
    else next[name] = value;
    onChange({ ...action, args: next });
  };
  return (
    <div className="k-builder__form">
      <Select<string>
        label={t("routines.tool")}
        value={action.tool}
        onChange={(id) => onChange({ type: "tool", tool: id, args: {} })}
        items={tools.map((x) => ({ value: x.id, label: x.title }))}
      />
      {tool && (
        <p className="k-note">
          {tool.description} <Tag tone={RISK_TONE[tool.risk]}>{t(`risk.${tool.risk}`)}</Tag>
        </p>
      )}
      {fields.map((f) => (
        <SchemaField key={`${action.tool}-${f.name}`} field={f} value={args[f.name]} onChange={(v) => set(f.name, v)} />
      ))}
      {tool && fields.length === 0 && <Note>{t("routines.noArgs")}</Note>}
    </div>
  );
}

function SchemaField({
  field,
  value,
  onChange,
}: {
  field: FormField;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  const { t } = useTranslation();
  const [text, setText] = useState(() => display(field, value));
  const [error, setError] = useState<string | null>(null);
  const label = `${field.label}${field.required ? " *" : ""}`;
  const commit = (next: string) => {
    setText(next);
    try {
      onChange(parseInput(field, next));
      setError(null);
    } catch (e) {
      setError(message(e));
    }
  };
  if (field.kind === "boolean") {
    return (
      <Checkbox checked={value === true} onChange={(on) => onChange(on)}>
        {label}
      </Checkbox>
    );
  }
  if (field.kind === "choice") {
    return (
      <Select<string>
        label={label}
        value={typeof value === "string" ? value : ""}
        onChange={onChange}
        items={(field.choices ?? []).map((c) => ({ value: c, label: c }))}
      />
    );
  }
  return (
    <label className="k-builder__field">
      <span>{label}</span>
      {field.kind === "json" ? (
        <TextArea aria-label={field.label} rows={3} value={text} onChange={(e) => commit(e.target.value)} />
      ) : (
        <TextField
          aria-label={field.label}
          inputMode={field.kind === "number" ? "decimal" : undefined}
          value={text}
          onChange={(e) => commit(e.target.value)}
        />
      )}
      {field.description && <Meta>{field.description}</Meta>}
      {error && <Meta>{t("routines.badJson", { error })}</Meta>}
    </label>
  );
}
