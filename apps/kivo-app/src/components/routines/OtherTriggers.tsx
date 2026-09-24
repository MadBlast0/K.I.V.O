/**
 * Schedules and events in the routine builder (ROUT-11): each shown in plain words with a remove
 * button, and "Add a trigger" to start the routine at a time, on a schedule, when an app starts,
 * a USB device is plugged in, the PC joins a network, the user is away or back, the battery runs
 * low, or another routine finishes. Such runs are unattended, so the builder says High-risk steps
 * ask on screen first (ROUT-12).
 */
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { RoutineEvent, Trigger } from "../../ipc/generated";
import { Button, Dialog, IconButton, Note, Pill, Select, TextField } from "../ui";

type Other = Extract<Trigger, { type: "schedule" | "event" }>;
type Kind = "time" | "schedule" | Exclude<RoutineEvent["kind"], "timeOfDay">;

const KINDS: Kind[] = [
  "time",
  "schedule",
  "appLaunched",
  "usbDevice",
  "network",
  "idle",
  "return",
  "batteryLow",
  "afterRoutine",
];

/** An event as a trigger. */
function event(e: RoutineEvent): Other {
  return { type: "event", event: e };
}

export function isOther(t: Trigger): t is Other {
  return t.type === "schedule" || t.type === "event";
}

/** A trigger in the user's words ("Weekdays at 07:30", "When Spotify starts"). */
export function describeTrigger(
  trigger: Other,
  t: (key: string, values?: Record<string, unknown>) => string,
  routineName: (id: string) => string,
): string {
  if (trigger.type === "schedule") {
    return trigger.tz
      ? t("routines.when.scheduleTz", { cron: trigger.cron, tz: trigger.tz })
      : t("routines.when.schedule", { cron: trigger.cron });
  }
  const e = trigger.event;
  switch (e.kind) {
    case "timeOfDay": {
      const days = e.days.length === 0 || e.days.length === 7 ? t("routines.when.everyDay") : dayNames(e.days, t);
      return t("routines.when.timeOfDay", { time: e.time, days });
    }
    case "appLaunched":
      return t("routines.when.appLaunched", { app: e.app });
    case "usbDevice":
      return e.name ? t("routines.when.usbNamed", { name: e.name }) : t("routines.when.usbAny");
    case "network":
      return t("routines.when.network", { name: e.name });
    case "idle":
      return t("routines.when.idle", { minutes: e.minutes });
    case "return":
      return t("routines.when.return", { minutes: e.awayMinutes });
    case "batteryLow":
      return t("routines.when.batteryLow", { percent: e.percent });
    case "afterRoutine":
      return t("routines.when.afterRoutine", { name: routineName(e.routine) });
  }
}

function dayNames(days: number[], t: (key: string) => string): string {
  const weekdays = [1, 2, 3, 4, 5];
  if (days.length === 5 && weekdays.every((d) => days.includes(d))) return t("routines.when.weekdays");
  if (days.length === 2 && days.includes(0) && days.includes(6)) return t("routines.when.weekends");
  return days
    .toSorted((a, b) => ((a + 6) % 7) - ((b + 6) % 7))
    .map((d) => t(`routines.day.${d}`))
    .join(", ");
}

export function OtherTriggers({
  triggers,
  onChange,
  routines,
  selfId,
}: {
  triggers: Other[];
  onChange: (next: Other[]) => void;
  /** Other routines, for "After another routine". */
  routines: { id: string; name: string }[];
  selfId: string;
}) {
  const { t } = useTranslation();
  const [adding, setAdding] = useState(false);
  const name = (id: string) => routines.find((r) => r.id === id)?.name ?? id;
  return (
    <>
      <div className="k-builder__chips">
        {triggers.map((trigger, i) => {
          const label = describeTrigger(trigger, t, name);
          return (
            <span key={`${i}-${label}`} className="k-builder__chip">
              <Pill>{label}</Pill>
              <IconButton
                icon="close"
                size="sm"
                variant="plain"
                label={t("routines.when.remove", { trigger: label })}
                onClick={() => onChange(triggers.filter((_, j) => j !== i))}
              />
            </span>
          );
        })}
        <Button size="sm" icon="add" onClick={() => setAdding(true)}>
          {t("routines.when.add")}
        </Button>
      </div>
      {triggers.length > 0 && <Note>{t("routines.when.unattended")}</Note>}
      {adding && (
        <AddTrigger
          routines={routines.filter((r) => r.id !== selfId && r.id !== "")}
          onCancel={() => setAdding(false)}
          onAdd={(trigger) => {
            onChange([...triggers, trigger]);
            setAdding(false);
          }}
        />
      )}
    </>
  );
}

function AddTrigger({
  routines,
  onAdd,
  onCancel,
}: {
  routines: { id: string; name: string }[];
  onAdd: (trigger: Other) => void;
  onCancel: () => void;
}) {
  const { t } = useTranslation();
  const [kind, setKind] = useState<Kind>("time");
  const [time, setTime] = useState("07:30");
  const [days, setDays] = useState<number[]>([1, 2, 3, 4, 5]);
  const [text, setText] = useState("");
  const [tz, setTz] = useState("");
  const [amount, setAmount] = useState("");
  const [routine, setRoutine] = useState(routines[0]?.id ?? "");

  const number = (fallback: number) => {
    const n = Number.parseInt(amount, 10);
    return Number.isFinite(n) && n > 0 ? n : fallback;
  };
  const build = (): Other | null => {
    switch (kind) {
      case "time":
        return /^\d{1,2}:\d{2}$/.test(time) ? event({ kind: "timeOfDay", time, days }) : null;
      case "schedule":
        return text.trim() ? { type: "schedule", cron: text.trim(), tz: tz.trim() || null } : null;
      case "appLaunched":
        return text.trim() ? event({ kind: "appLaunched", app: text.trim() }) : null;
      case "usbDevice":
        return event({ kind: "usbDevice", name: text.trim() || null });
      case "network":
        return text.trim() ? event({ kind: "network", name: text.trim() }) : null;
      case "idle":
        return event({ kind: "idle", minutes: number(10) });
      case "return":
        return event({ kind: "return", awayMinutes: number(10) });
      case "batteryLow":
        return event({ kind: "batteryLow", percent: Math.min(number(20), 99) });
      case "afterRoutine":
        return routine ? event({ kind: "afterRoutine", routine }) : null;
    }
  };
  const ready = build();

  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onCancel()}
      title={t("routines.when.add")}
      footer={
        <>
          <Button variant="plain" onClick={onCancel}>
            {t("ui.cancel")}
          </Button>
          <Button variant="primary" disabled={!ready} onClick={() => ready && onAdd(ready)}>
            {t("routines.when.addButton")}
          </Button>
        </>
      }
    >
      <div className="k-form">
        <Select<Kind>
          label={t("routines.when.kind")}
          value={kind}
          onChange={(k) => {
            setKind(k);
            setText("");
            setAmount("");
          }}
          items={KINDS.filter((k) => k !== "afterRoutine" || routines.length > 0).map((k) => ({
            value: k,
            label: t(`routines.when.kinds.${k}`),
          }))}
        />
        {kind === "time" && (
          <>
            <TextField
              aria-label={t("routines.when.time")}
              type="time"
              value={time}
              onChange={(e) => setTime(e.target.value)}
            />
            <div className="k-chips" role="group" aria-label={t("routines.when.days")}>
              {[1, 2, 3, 4, 5, 6, 0].map((d) => (
                <button
                  key={d}
                  type="button"
                  aria-pressed={days.includes(d)}
                  onClick={() => setDays((x) => (x.includes(d) ? x.filter((y) => y !== d) : [...x, d]))}
                >
                  {t(`routines.day.${d}`)}
                </button>
              ))}
            </div>
          </>
        )}
        {kind === "schedule" && (
          <>
            <TextField
              aria-label={t("routines.when.cron")}
              placeholder="30 7 * * mon-fri"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
            <TextField
              aria-label={t("routines.when.tz")}
              placeholder={t("routines.when.tzPlaceholder")}
              value={tz}
              onChange={(e) => setTz(e.target.value)}
            />
            <Note>{t("routines.when.cronHelp")}</Note>
          </>
        )}
        {(kind === "appLaunched" || kind === "usbDevice" || kind === "network") && (
          <TextField
            aria-label={t(`routines.when.field.${kind}`)}
            placeholder={t(`routines.when.placeholder.${kind}`)}
            value={text}
            onChange={(e) => setText(e.target.value)}
          />
        )}
        {(kind === "idle" || kind === "return" || kind === "batteryLow") && (
          <TextField
            aria-label={t(`routines.when.field.${kind}`)}
            inputMode="numeric"
            placeholder={kind === "batteryLow" ? "20" : "10"}
            value={amount}
            onChange={(e) => setAmount(e.target.value.replaceAll(/\D/g, ""))}
          />
        )}
        {kind === "afterRoutine" && (
          <Select
            label={t("routines.when.field.afterRoutine")}
            value={routine}
            onChange={setRoutine}
            items={routines.map((r) => ({ value: r.id, label: r.name }))}
          />
        )}
      </div>
    </Dialog>
  );
}
