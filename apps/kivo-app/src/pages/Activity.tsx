/**
 * Activity (UX §3, UX-20, plan §83): what KIVO did, and why — every request with what was heard,
 * what ran and what KIVO said — and the audit log of every permission decision (SECURITY §7). It
 * refreshes when the runtime reports something new; nothing polls (DISC-13).
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "../components/layout/Shell";
import { Button, Group, Note, Popover, Row, Segmented, Tag, useToast, type Tone } from "../components/ui";
import type { IconName } from "../icons";
import { Method, type ActivityItem, type AuditItem } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { entries, type Entry, type Outcome } from "../lib/activity";

type Filter = "all" | "voice" | "ai" | "tools" | "audit";
const PAGE = 100;

const TONE: Record<Outcome, Tone> = {
  done: "success",
  failed: "danger",
  cancelled: "neutral",
  denied: "danger",
  unhandled: "warning",
};

const ICON: Record<Entry["kind"], IconName> = { voice: "mic", tool: "play", setting: "settings" };

type Used = { path: string; title: string; exists: boolean };

/** "Why did you say that?" (MEM-10): the memories a request's answer was given. */
function Why({ turn }: { turn: string }) {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const [used, setUsed] = useState<Used[] | null>(null);
  return (
    <Popover
      title={t("activity.whyTitle")}
      trigger={
        <Button
          size="sm"
          variant="plain"
          onClick={() =>
            void request<Used[]>(Method.memoryWhy, { turn })
              .then(setUsed)
              .catch(() => setUsed([]))
          }
        >
          {t("activity.why")}
        </Button>
      }
    >
      {used === null ? (
        t("ui.working")
      ) : used.length === 0 ? (
        t("activity.whyNone")
      ) : (
        <>
          {t("activity.whyUsed", { count: used.length })}
          <ul className="k-why">
            {used.map((u) => (
              <li key={u.path}>
                {u.title}
                {!u.exists && ` (${t("activity.whyGone")})`}
              </li>
            ))}
          </ul>
        </>
      )}
    </Popover>
  );
}

export function Activity() {
  const { t, i18n } = useTranslation();
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const toast = useToast();
  const [filter, setFilter] = useState<Filter>("all");
  const [items, setItems] = useState<ActivityItem[]>([]);
  const [audit, setAudit] = useState<AuditItem[]>([]);
  const [limit, setLimit] = useState(PAGE);

  const load = useCallback(() => {
    if (!connected) return;
    void request<ActivityItem[]>(Method.activityList, { limit })
      .then(setItems)
      .catch(() => {});
    void request<AuditItem[]>(Method.auditList, { limit })
      .then(setAudit)
      .catch(() => {});
  }, [connected, limit, request]);

  useEffect(load, [load]);
  // A turn ended or a setting changed: read again.
  useRuntimeEvents((event) => {
    if (event.group === "turn" || event.group === "tool") load();
  });

  const time = useMemo(
    () => new Intl.DateTimeFormat(i18n.language, { hour: "2-digit", minute: "2-digit" }),
    [i18n.language],
  );
  const day = useMemo(() => new Intl.DateTimeFormat(i18n.language, { dateStyle: "medium" }), [i18n.language]);
  const when = (ts: number) => {
    const d = new Date(ts);
    const today = new Date().toDateString() === d.toDateString();
    return today ? time.format(d) : `${day.format(d)} ${time.format(d)}`;
  };

  const list = entries(items).filter((e) =>
    filter === "all" ? true : filter === "voice" ? e.kind === "voice" : filter === "ai" ? e.ai : e.kind === "tool",
  );
  const exportCsv = () =>
    void request<{ file: string }>(Method.activityExport)
      .then((r) => toast(t("activity.exported", { file: r.file })))
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)));

  return (
    <>
      <PageHeader
        title={t("activity.title")}
        subtitle={t("activity.subtitle")}
        actions={
          <>
            <Segmented<Filter>
              label={t("activity.filter")}
              value={filter}
              onChange={setFilter}
              options={[
                { value: "all", label: t("activity.all") },
                { value: "voice", label: t("activity.voice") },
                { value: "ai", label: t("activity.ai") },
                { value: "tools", label: t("activity.tools") },
                { value: "audit", label: t("activity.audit") },
              ]}
            />
            <Button icon="download" disabled={!connected} onClick={exportCsv}>
              {t("activity.export")}
            </Button>
          </>
        }
      />
      {!connected ? (
        <Note>{t("activity.offline")}</Note>
      ) : filter === "audit" ? (
        audit.length === 0 ? (
          <Note>{t("activity.empty")}</Note>
        ) : (
          <Group>
            {audit.map((a, i) => (
              <Row
                key={`${a.ts}-${i}`}
                icon="permissions"
                title={a.argsSummary ? `${a.tool} · ${a.argsSummary}` : a.tool}
                subtitle={[a.decision + (a.confirmedBy ? ` · ${a.confirmedBy}` : ""), a.error]
                  .filter(Boolean)
                  .join(" — ")}
                end={<span className="k-meta">{when(a.ts)}</span>}
              />
            ))}
          </Group>
        )
      ) : list.length === 0 ? (
        <Note>{t("activity.empty")}</Note>
      ) : (
        <Group>
          {list.map((e) => (
            <Row
              key={e.key}
              icon={ICON[e.kind]}
              title={e.title}
              subtitle={e.detail ?? undefined}
              end={
                <>
                  {e.turn !== null && e.answered && <Why turn={e.turn} />}
                  <span className="k-meta">{when(e.ts)}</span>
                  <Tag tone={TONE[e.outcome]}>{t(`activity.outcome.${e.outcome}`)}</Tag>
                </>
              }
            />
          ))}
        </Group>
      )}
      {connected && items.length >= limit && (
        <div className="k-page__more">
          <Button onClick={() => setLimit((n) => n + PAGE)}>{t("activity.more")}</Button>
        </div>
      )}
    </>
  );
}
