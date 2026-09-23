/**
 * Tasks (UX-24, TOOLS_AND_CONTROL §6): long-running work and watchers — running and finished
 * tasks with their status, current step, elapsed time, each step's activity, a question a task
 * waits on, cancel, pause and resume, and the result. The list reloads when the runtime reports a
 * task event; nothing polls while nothing changes (a running task's elapsed time ticks once a
 * second on screen only).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import {
  Button,
  EmptyState,
  Group,
  Meta,
  Note,
  Row,
  Section,
  Spinner,
  Tag,
  useToast,
  type Tone,
} from "../components/ui";
import { Icon, type IconName } from "../icons";
import { Method, type TaskKind, type TaskStatus, type TaskStepView, type TaskView } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";

const TONE: Record<TaskStatus, Tone> = {
  pending: "neutral",
  running: "accent",
  waiting: "accent",
  needsYou: "warning",
  paused: "warning",
  done: "success",
  failed: "danger",
  cancelled: "neutral",
  interrupted: "warning",
};

const KIND_ICON: Record<TaskKind, IconName> = {
  plan: "list",
  watch: "eye",
  reminder: "clock",
  coding: "code",
  routine: "routine",
};

/** Whether a task counts as running (it can still be stopped). */
export function isActive(task: TaskView): boolean {
  return !["done", "failed", "cancelled", "interrupted"].includes(task.status);
}

/** The step that is running, or the first one still to do. */
export function currentStep(task: TaskView): TaskStepView | undefined {
  return (
    task.steps.find((s) => s.status === "running" || s.status === "needsYou" || s.status === "waiting") ??
    task.steps.find((s) => s.status === "pending")
  );
}

/** "1 min 05 s", "2 h 03 min". */
export function elapsed(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000));
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${String(s % 60).padStart(2, "0")} s`;
  return `${Math.floor(m / 60)} h ${String(m % 60).padStart(2, "0")} min`;
}

/** Running and finished tasks, reloaded on every task event. */
export function useTasks(): { active: TaskView[]; finished: TaskView[]; reload: () => void } {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [active, setActive] = useState<TaskView[]>([]);
  const [finished, setFinished] = useState<TaskView[]>([]);
  const reload = useCallback(() => {
    if (!connected) return;
    void request<TaskView[]>(Method.tasksList, { finished: false })
      .then(setActive)
      .catch(() => {});
    void request<TaskView[]>(Method.tasksList, { finished: true, limit: 50 })
      .then(setFinished)
      .catch(() => {});
  }, [connected, request]);
  useEffect(reload, [reload]);
  useRuntimeEvents((event) => {
    if (event.group === "task") reload();
  });
  return { active, finished, reload };
}

/** Re-renders once a second while `on`, for elapsed times. */
function useTick(on: boolean): number {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!on) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [on]);
  return now;
}

export function Tasks({ focus }: { focus?: string }) {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const { active, finished, reload } = useTasks();
  const now = useTick(active.some((task) => task.status === "running" || task.status === "waiting"));
  const [open, setOpen] = useState<Record<string, boolean>>(focus ? { [focus]: true } : {});

  const act = (method: Method, params: Record<string, unknown>) => {
    request(method, params)
      .then(reload)
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
  };
  const stopAll = () => {
    Promise.all(active.map((task) => request(Method.tasksCancel, { id: task.id })))
      .then(reload)
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));
  };

  const day = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium", timeStyle: "short" }),
    [i18n.language],
  );

  if (link?.status !== "connected") {
    return (
      <>
        <PageHeader title={t("nav.tasks")} subtitle={t("tasks.subtitle")} />
        <Note>{t("voice.notConnected")}</Note>
      </>
    );
  }

  return (
    <>
      <PageHeader
        title={t("nav.tasks")}
        subtitle={t("tasks.subtitle")}
        actions={
          active.length > 0 && (
            <Button variant="destructive" icon="stop" onClick={stopAll}>
              {t("tasks.stopAll")}
            </Button>
          )
        }
      />
      <Section title={t("tasks.running")} />
      {active.length === 0 ? (
        <EmptyState icon="tasks" title={t("tasks.noneRunning")}>
          {t("tasks.noneRunningDetail")}
        </EmptyState>
      ) : (
        <Group>
          {active.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              now={now}
              open={open[task.id] ?? true}
              onToggle={() => setOpen((o) => ({ ...o, [task.id]: !(o[task.id] ?? true) }))}
              onAct={act}
              when={day.format(new Date(task.createdAt))}
            />
          ))}
        </Group>
      )}
      <Section
        title={t("tasks.finished")}
        aside={
          finished.length > 0 && (
            <Button size="sm" variant="plain" onClick={() => act(Method.tasksClear, {})}>
              {t("tasks.clear")}
            </Button>
          )
        }
      />
      {finished.length === 0 ? (
        <Note>{t("tasks.noneFinished")}</Note>
      ) : (
        <Group>
          {finished.map((task) => (
            <TaskRow
              key={task.id}
              task={task}
              now={now}
              open={open[task.id] ?? false}
              onToggle={() => setOpen((o) => ({ ...o, [task.id]: !o[task.id] }))}
              onAct={act}
              when={day.format(new Date(task.finishedAt ?? task.updatedAt))}
            />
          ))}
        </Group>
      )}
    </>
  );
}

function TaskRow({
  task,
  now,
  open,
  onToggle,
  onAct,
  when,
}: {
  task: TaskView;
  now: number;
  open: boolean;
  onToggle: () => void;
  onAct: (method: Method, params: Record<string, unknown>) => void;
  when: string;
}) {
  const { t } = useTranslation();
  const running = isActive(task);
  const step = currentStep(task);
  const index = step ? task.steps.indexOf(step) + 1 : task.steps.length;
  const took = (task.finishedAt ?? now) - task.createdAt;
  const subtitle = [
    task.owner === "you" ? t("tasks.byYou") : task.owner,
    running
      ? task.steps.length > 1
        ? t("tasks.stepOf", { n: index, count: task.steps.length })
        : step?.title
      : (task.error ?? task.result ?? task.summary),
    running ? elapsed(took) : when,
    task.usesAi ? t("tasks.usesAi") : t("tasks.noAi"),
  ]
    .filter(Boolean)
    .join(" · ");
  return (
    <div className="k-task" data-status={task.status}>
      <Row
        lead={
          task.status === "running" ? (
            <Spinner label={t("tasks.status.running")} />
          ) : (
            <Icon name={KIND_ICON[task.kind]} />
          )
        }
        title={task.title}
        subtitle={subtitle}
        end={
          <>
            <Tag tone={TONE[task.status]}>{t(`tasks.status.${task.status}`)}</Tag>
            {task.steps.length > 0 && (
              <Button size="sm" variant="plain" aria-expanded={open} onClick={onToggle}>
                {open ? t("tasks.hideSteps") : t("tasks.showSteps")}
              </Button>
            )}
            {task.status === "paused" && (
              <Button size="sm" icon="play" onClick={() => onAct(Method.tasksResume, { id: task.id })}>
                {t("tasks.resume")}
              </Button>
            )}
            {running && task.status !== "paused" && (
              <Button size="sm" icon="pause" onClick={() => onAct(Method.tasksPause, { id: task.id })}>
                {t("tasks.pause")}
              </Button>
            )}
            {running ? (
              <Button
                size="sm"
                variant="destructive"
                icon="stop"
                onClick={() => onAct(Method.tasksCancel, { id: task.id })}
              >
                {t("tasks.stop")}
              </Button>
            ) : (
              <Button
                size="sm"
                variant="plain"
                icon="delete"
                aria-label={t("tasks.delete", { title: task.title })}
                onClick={() => onAct(Method.tasksDelete, { id: task.id })}
              />
            )}
          </>
        }
      />
      {task.question && (
        <div className="k-task__question" role="group" aria-label={task.question.text}>
          <span>{task.question.text}</span>
          {task.question.choices.map((choice) => (
            <Button
              key={choice}
              size="sm"
              variant={choice === "stop" || choice === "deny" ? "destructive" : "secondary"}
              onClick={() => onAct(Method.tasksAnswer, { id: task.id, choice })}
            >
              {t(`tasks.choice.${choice}`)}
            </Button>
          ))}
        </div>
      )}
      {open && task.steps.length > 0 && (
        <ol className="k-task__steps">
          {task.steps.map((s) => (
            <li key={s.id} className="k-task__step" data-status={s.status}>
              <StepMark status={s.status} />
              <span className="k-task__step-title">{s.title}</span>
              {s.detail && <Meta>{s.detail}</Meta>}
              {s.attempts > 1 && <Meta>{t("tasks.attempts", { count: s.attempts })}</Meta>}
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

function StepMark({ status }: { status: string }) {
  const { t } = useTranslation();
  const label = t(`tasks.step.${status}`, { defaultValue: status });
  if (status === "running") return <Spinner label={label} />;
  const icon = STEP_ICON[status];
  return (
    <span className={icon ? "k-task__mark" : "k-task__dot"} role="img" aria-label={label}>
      {icon && <Icon name={icon} />}
    </span>
  );
}

const STEP_ICON: Record<string, IconName | undefined> = {
  done: "check",
  failed: "warning",
  waiting: "clock",
  needsYou: "hand",
  skipped: "close",
};
