/**
 * Usage (UX-30, BRAIN-37, BRAINS §9): the estimated cost of cloud AI and speech — work done on
 * this PC is free. Totals, spend by day (AI and speech), by brain provider, feature and routine,
 * the most expensive tasks and requests, a CSV export, and the limits: a monthly budget with its
 * warnings and what happens at the limit, per-task caps, and the card's live estimate.
 *
 * Every figure is an estimate from the price table; requests whose model has no known price are
 * counted and said so.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import {
  BudgetMeter,
  Button,
  Dialog,
  Group,
  Meta,
  Meter,
  Note,
  Row,
  Section,
  Segmented,
  Stat,
  Switch,
  TextField,
  Tile,
  useToast,
} from "../components/ui";
import { Method } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { bool, setting, useSettings } from "../lib/settings";

type AtLimit = "ask" | "cheaperProfile" | "localOnly" | "block";

/** A spending limit as the runtime keeps it (`kivo_brain::cost::Limit`). */
export interface Limit {
  scope: { kind: "overall" | "provider" | "profile" | "feature"; id?: string };
  period: "daily" | "weekly" | { monthly: { reset_day: number } };
  amount: number;
  warnings: number[];
  atLimit: AtLimit;
}

interface LimitState {
  spent: number;
  amount: number;
  warning: number | null;
  reached: boolean;
}

interface Caps {
  computerUse: number | null;
  agents: number | null;
  realtimeMinutes: number | null;
}

export interface UsageSummary {
  total: number;
  today: number;
  requests: number;
  unpriced: number;
  turns: number;
  freeTurns: number;
  byDay: { day: number; cost: number; ai: number; speech: number }[];
  byProvider: { provider: string; cost: number; tokens: number }[];
  byFeature: { feature: string; cost: number }[];
  byRoutine: { routine: string; name: string; cost: number }[];
  top: { kind: "task" | "turn"; id: string; title: string; cost: number }[];
  limits: { limit: Limit; state: LimitState | null }[];
  caps: Caps;
}

type Range = "week" | "month" | "all";
const DAYS: Record<Range, number> = { week: 7, month: 30, all: 366 };
const WARNINGS = ["50", "80", "95"] as const;
const AT_LIMIT: ReadonlyArray<AtLimit> = ["ask", "cheaperProfile", "localOnly", "block"];

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** A dollar amount typed into a field, or `null` for none (empty or not a number). */
function parseAmount(v: string): number | null {
  const n = Number.parseFloat(v);
  return v.trim() === "" || !Number.isFinite(n) || n < 0 ? null : n;
}

function isOverallMonthly(l: Limit): boolean {
  return l.scope.kind === "overall" && typeof l.period === "object";
}

export function Usage() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const toast = useToast();
  const connected = link?.status === "connected";
  const [range, setRange] = useState<Range>("month");
  const [usage, setUsage] = useState<UsageSummary | null>(null);
  const [settings, save] = useSettings((e) => toast(message(e)));
  const [caps, setCaps] = useState<Caps | null>(null);

  const load = useCallback(() => {
    if (!connected) return;
    void request<UsageSummary>(Method.usageSummary, { days: DAYS[range] })
      .then(setUsage)
      .catch(() => {});
  }, [connected, range, request]);
  useEffect(load, [load]);
  // Each metered request updates the page (DISCOVERY §3: pushed, never polled).
  useRuntimeEvents((event) => {
    if (event.group === "provider" && event.event.type === "usageRecorded") load();
  });

  const money = (v: number) =>
    new Intl.NumberFormat(i18n.language, {
      style: "currency",
      currency: "USD",
      // A total can come back as -0: never show "-$0.00".
      signDisplay: "negative",
      minimumFractionDigits: 2,
      maximumFractionDigits: v !== 0 && Math.abs(v) < 0.01 ? 4 : 2,
    }).format(v);

  if (!connected) {
    return (
      <>
        <PageHeader title={t("nav.usage")} subtitle={t("usage.subtitle")} />
        <Note>{t("voice.notConnected")}</Note>
      </>
    );
  }

  const budget = usage?.limits.find((l) => isOverallMonthly(l.limit));
  const others = usage?.limits.filter((l) => l !== budget) ?? [];
  const setLimits = (limits: Limit[]) =>
    void request<UsageSummary>(Method.usageSetLimits, { limits })
      .then(setUsage)
      .catch((e: unknown) => toast(message(e)));
  const withBudget = (change: (l: Limit) => Limit | null) => {
    const current: Limit = budget?.limit ?? {
      scope: { kind: "overall" },
      period: { monthly: { reset_day: 1 } },
      amount: 10,
      warnings: [80],
      atLimit: "ask",
    };
    const next = change(current);
    setLimits([...others.map((l) => l.limit), ...(next ? [next] : [])]);
  };
  const max = Math.max(0.0001, ...(usage?.byDay.map((d) => d.cost) ?? []));
  const freeShare = usage && usage.turns > 0 ? Math.round((usage.freeTurns / usage.turns) * 100) : null;
  const providerTotal = Math.max(0.0001, ...(usage?.byProvider.map((p) => p.cost) ?? []));
  const date = new Intl.DateTimeFormat(i18n.language, { day: "numeric" });
  const longDate = new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" });

  return (
    <>
      <PageHeader
        title={t("nav.usage")}
        subtitle={t("usage.subtitle")}
        actions={
          <>
            <Segmented<Range>
              label={t("usage.range")}
              value={range}
              onChange={setRange}
              options={[
                { value: "week", label: t("usage.week") },
                { value: "month", label: t("usage.month") },
                { value: "all", label: t("usage.all") },
              ]}
            />
            <Button
              icon="download"
              onClick={() =>
                void request<{ file?: string }>(Method.usageExport, { days: DAYS[range], save: true })
                  .then((r) => toast(r.file ? t("usage.exported", { file: r.file }) : t("usage.exportedNoFile")))
                  .catch((e: unknown) => toast(message(e)))
              }
            >
              {t("usage.csv")}
            </Button>
          </>
        }
      />
      {usage && (
        <>
          <div className="k-grid k-grid--4">
            <Stat value={money(usage.total)} label={t(`usage.totalFor.${range}`)} />
            <Stat value={money(usage.today)} label={t("usage.today")} />
            <Stat value={freeShare === null ? "—" : `${freeShare}%`} label={t("usage.free")} />
            <Stat
              value={budget ? money(budget.limit.amount) : t("usage.noLimit")}
              label={budget?.state ? t("usage.budgetSpent", { spent: money(budget.state.spent) }) : t("usage.budget")}
            />
          </div>
          <div className="k-grid k-grid--2" style={{ marginBlockStart: "var(--gap)" }}>
            <Tile>
              <div className="k-chart__head">
                <b>{t("usage.byDay")}</b>
                <span>
                  {t("usage.ai")} · <span className="k-chart__speech-key">{t("usage.speech")}</span>
                </span>
              </div>
              {usage.byDay.length === 0 ? (
                <Note>{t("usage.nothingYet")}</Note>
              ) : (
                <div className="k-chart" role="img" aria-label={t("usage.chartLabel")}>
                  {usage.byDay.slice(-14).map((d) => (
                    <div
                      className="k-chart__bar"
                      key={d.day}
                      title={`${longDate.format(new Date(d.day))}: ${money(d.cost)}`}
                    >
                      <i className="k-chart__speech" style={{ height: `${(d.speech / max) * 85}%` }} />
                      <i style={{ height: `${(d.ai / max) * 85}%` }} />
                      <span>{date.format(new Date(d.day))}</span>
                    </div>
                  ))}
                </div>
              )}
            </Tile>
            <Group>
              {usage.byProvider.length === 0 ? (
                <Row title={t("usage.noProviders")} />
              ) : (
                usage.byProvider
                  .toSorted((a, b) => b.cost - a.cost)
                  .map((p) => (
                    <Row
                      key={p.provider}
                      title={p.provider}
                      subtitle={
                        <Meter
                          value={Math.round((p.cost / providerTotal) * 100)}
                          label={t("usage.shareOf", { name: p.provider })}
                        />
                      }
                      end={<Meta>{money(p.cost)}</Meta>}
                    />
                  ))
              )}
            </Group>
          </div>
          {usage.unpriced > 0 && <Note>{t("usage.unpriced", { count: usage.unpriced })}</Note>}

          <div className="k-grid k-grid--2" style={{ marginBlockStart: "var(--gap)" }}>
            <div>
              <Section title={t("usage.byFeature")} />
              <Group>
                {usage.byFeature.length === 0 ? (
                  <Row title={t("usage.nothingYet")} />
                ) : (
                  usage.byFeature.map((f) => (
                    <Row
                      key={f.feature}
                      title={t(`usage.feature.${f.feature}`, { defaultValue: f.feature })}
                      end={<Meta>{money(f.cost)}</Meta>}
                    />
                  ))
                )}
              </Group>
            </div>
            <div>
              <Section title={t("usage.byRoutine")} />
              <Group>
                {usage.byRoutine.length === 0 ? (
                  <Row title={t("usage.noRoutines")} />
                ) : (
                  usage.byRoutine.map((r) => (
                    <Row
                      key={r.routine}
                      icon="routine"
                      title={r.name || r.routine}
                      end={<Meta>{money(r.cost)}</Meta>}
                    />
                  ))
                )}
              </Group>
            </div>
          </div>

          <Section title={t("usage.top")} />
          <Group>
            {usage.top.length === 0 ? (
              <Row title={t("usage.nothingYet")} />
            ) : (
              usage.top.map((x) => (
                <Row
                  key={`${x.kind}-${x.id}`}
                  icon={x.kind === "task" ? "tasks" : "chat"}
                  title={x.title || t("usage.untitled")}
                  subtitle={t(`usage.kind.${x.kind}`)}
                  end={<Meta>{money(x.cost)}</Meta>}
                />
              ))
            )}
          </Group>

          <Section title={t("usage.limits")} />
          <Group>
            <Row
              icon="coin"
              title={t("usage.monthlyBudget")}
              subtitle={
                budget?.state ? (
                  <BudgetMeter
                    value={budget.state.amount > 0 ? (budget.state.spent / budget.state.amount) * 100 : 0}
                    thresholds={budget.limit.warnings}
                    label={t("usage.monthlyBudget")}
                  />
                ) : (
                  t("usage.monthlyBudgetDetail")
                )
              }
              end={
                <>
                  {budget && (
                    <TextField
                      prefix="$"
                      style={{ width: 96 }}
                      aria-label={t("usage.amount")}
                      inputMode="decimal"
                      defaultValue={budget.limit.amount.toFixed(2)}
                      key={budget.limit.amount}
                      onBlur={(e) => {
                        const amount = Number.parseFloat(e.target.value);
                        if (Number.isFinite(amount) && amount >= 0 && amount !== budget.limit.amount) {
                          withBudget((l) => ({ ...l, amount }));
                        }
                      }}
                    />
                  )}
                  <Switch
                    label={t("usage.monthlyBudget")}
                    checked={budget !== undefined}
                    onChange={(on) => withBudget((l) => (on ? l : null))}
                  />
                </>
              }
            />
            {budget && (
              <>
                <Row
                  icon="bell"
                  title={t("usage.warnAt")}
                  end={
                    <Segmented
                      label={t("usage.warnAt")}
                      value={String(budget.limit.warnings[0] ?? 80)}
                      onChange={(v) => withBudget((l) => ({ ...l, warnings: [Number(v)] }))}
                      options={WARNINGS.map((w) => ({ value: w, label: `${w}%` }))}
                    />
                  }
                />
                <Row
                  icon="warning"
                  title={t("usage.atLimit")}
                  end={
                    <Segmented<AtLimit>
                      label={t("usage.atLimit")}
                      value={budget.limit.atLimit}
                      onChange={(v) => withBudget((l) => ({ ...l, atLimit: v }))}
                      options={AT_LIMIT.map((a) => ({ value: a, label: t(`usage.at.${a}`) }))}
                    />
                  }
                />
              </>
            )}
            <Row
              icon="coin"
              title={t("usage.caps")}
              subtitle={t("usage.capsLine", {
                computer: usage.caps.computerUse === null ? t("usage.none") : money(usage.caps.computerUse),
                agents: usage.caps.agents === null ? t("usage.none") : money(usage.caps.agents),
                minutes: usage.caps.realtimeMinutes ?? "—",
              })}
              onClick={() => setCaps(usage.caps)}
            />
            <Row
              icon="usage"
              title={t("usage.showCost")}
              subtitle={t("usage.showCostDetail")}
              end={
                <Switch
                  label={t("usage.showCost")}
                  checked={bool(setting(settings, "brains", "show-cost"))}
                  onChange={(on) => save({ brains: { "show-cost": on } })}
                />
              }
            />
          </Group>
          <Note>{t("usage.note")}</Note>
        </>
      )}
      {caps && (
        <CapsDialog
          caps={caps}
          onClose={() => setCaps(null)}
          onSave={(next) =>
            void request<UsageSummary>(Method.usageSetCaps, next)
              .then((u) => {
                setUsage(u);
                setCaps(null);
              })
              .catch((e: unknown) => toast(message(e)))
          }
        />
      )}
    </>
  );
}

/** Opened with the current caps; empty fields mean no limit. */
function CapsDialog({ caps, onClose, onSave }: { caps: Caps; onClose: () => void; onSave: (caps: Caps) => void }) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(() => ({
    computer: caps.computerUse === null ? "" : String(caps.computerUse),
    agents: caps.agents === null ? "" : String(caps.agents),
    minutes: caps.realtimeMinutes === null ? "" : String(caps.realtimeMinutes),
  }));
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && onClose()}
      title={t("usage.caps")}
      description={t("usage.capsDetail")}
      footer={
        <>
          <Button variant="plain" onClick={onClose}>
            {t("ui.cancel")}
          </Button>
          <Button
            variant="primary"
            onClick={() => {
              const minutes = parseAmount(draft.minutes);
              onSave({
                computerUse: parseAmount(draft.computer),
                agents: parseAmount(draft.agents),
                realtimeMinutes: minutes === null ? null : Math.round(minutes),
              });
            }}
          >
            {t("ui.save")}
          </Button>
        </>
      }
    >
      <Group>
        <Row
          title={t("usage.capComputer")}
          end={
            <TextField
              prefix="$"
              style={{ width: 96 }}
              aria-label={t("usage.capComputer")}
              inputMode="decimal"
              value={draft.computer}
              onChange={(e) => setDraft({ ...draft, computer: e.target.value })}
            />
          }
        />
        <Row
          title={t("usage.capAgents")}
          end={
            <TextField
              prefix="$"
              style={{ width: 96 }}
              aria-label={t("usage.capAgents")}
              inputMode="decimal"
              value={draft.agents}
              onChange={(e) => setDraft({ ...draft, agents: e.target.value })}
            />
          }
        />
        <Row
          title={t("usage.capRealtime")}
          end={
            <TextField
              style={{ width: 96 }}
              aria-label={t("usage.capRealtime")}
              inputMode="numeric"
              value={draft.minutes}
              onChange={(e) => setDraft({ ...draft, minutes: e.target.value })}
            />
          }
        />
      </Group>
      <Note>{t("usage.capsEmpty")}</Note>
    </Dialog>
  );
}
