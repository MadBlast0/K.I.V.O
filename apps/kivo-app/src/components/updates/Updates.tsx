/**
 * KIVO's own updates (DIST-07/08, UX-59): the Updates section of Settings → About — the version and
 * where the update stands, Check now / Install now, the channel and when to install — and the
 * "What's new" dialog shown once after an update. The runtime checks, verifies and installs
 * (`updater.rs`); this only shows and asks.
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button, Dialog, Group, Row, Section, Segmented, Switch, useToast } from "../ui";
import { Method, type UpdateView } from "../../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../../ipc/runtime";
import { bool, oneOf, setting, useSettings } from "../../lib/settings";

const RELEASES = "https://github.com/MadBlast0/K.I.V.O/releases";
const CHANNELS = ["stable", "beta", "experimental"] as const;
const INSTALL = ["ask", "when-idle"] as const;

function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** The runtime's update status, kept current by its `updateChanged` events. */
export function useUpdates(): [UpdateView | null, () => void] {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [view, setView] = useState<UpdateView | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<UpdateView>(Method.updatesStatus)
      .then(setView)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && (event.event.type === "updateChanged" || event.event.type === "configChanged"))
      load();
  });
  return [view, load];
}

/** The release notes as lines: "- item" bullets become a list, the rest stays text. */
export function noteLines(notes: string): string[] {
  return notes
    .split(/\r?\n/)
    .map((l) => l.replace(/^\s*[-*•]\s+/, "").trim())
    .filter(Boolean);
}

function percent(received: number, total: number | null): string | null {
  return total && total > 0 ? `${Math.min(100, Math.round((received / total) * 100))}%` : null;
}

export function UpdatesSection() {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const toast = useToast();
  const [view, reload] = useUpdates();
  const [settings, save] = useSettings((e) => toast(message(e)));
  const [busy, setBusy] = useState(false);
  const updates = setting(settings, "updates", "channel");
  const channel = oneOf(updates, CHANNELS) ?? view?.channel ?? "stable";
  const install = oneOf(setting(settings, "updates", "install"), INSTALL) ?? "ask";
  const check = bool(setting(settings, "updates", "check"), true);

  const run = (method: Method, params?: unknown) => {
    setBusy(true);
    void request(method, params)
      .then(reload)
      .catch((e: unknown) => toast(message(e)))
      .finally(() => setBusy(false));
  };

  const state = view?.state;
  let status = t("updates.upToDate");
  let action = (
    <Button size="sm" disabled={busy || !view?.enabled} onClick={() => run(Method.updatesCheck)}>
      {t("updates.checkNow")}
    </Button>
  );
  if (view && !view.enabled) status = t("updates.notThisBuild");
  else if (state?.kind === "checking") status = t("updates.checking");
  else if (state?.kind === "downloading") {
    const p = percent(state.received, state.total);
    status = p
      ? t("updates.downloadingPercent", { version: state.version, percent: p })
      : t("updates.downloading", { version: state.version });
  } else if (state?.kind === "ready") {
    status = state.whenIdle
      ? t("updates.readyWhenIdle", { version: state.version })
      : t("updates.ready", { version: state.version });
    action = (
      <Button
        size="sm"
        variant="primary"
        disabled={busy}
        onClick={() => run(Method.updatesInstall, { whenIdle: false })}
      >
        {t("updates.installNow")}
      </Button>
    );
  } else if (state?.kind === "installing") status = t("updates.installing", { version: state.version });
  else if (state?.kind === "failed") status = state.message;
  else if (state?.kind === "idle" || !state) status = t("updates.notChecked");

  return (
    <>
      <Section title={t("updates.title")} />
      <Group>
        <Row icon="refresh" title={`KIVO ${view?.current ?? "—"}`} subtitle={status} end={action} />
        <Row
          icon="refresh"
          title={t("updates.channel")}
          subtitle={t("updates.channelHint")}
          end={
            <Segmented<(typeof CHANNELS)[number]>
              label={t("updates.channel")}
              value={channel}
              onChange={(v) => save({ updates: { channel: v } })}
              options={CHANNELS.map((c) => ({ value: c, label: t(`updates.channels.${c}`) }))}
            />
          }
        />
        <Row
          icon="download"
          title={t("updates.install")}
          end={
            <Segmented<(typeof INSTALL)[number]>
              label={t("updates.install")}
              value={install}
              onChange={(v) => save({ updates: { install: v } })}
              options={[
                { value: "ask", label: t("updates.askMe") },
                { value: "when-idle", label: t("updates.whenIdle") },
              ]}
            />
          }
        />
        <Row
          icon="clock"
          title={t("updates.daily")}
          subtitle={t("updates.dailyHint")}
          end={<Switch label={t("updates.daily")} checked={check} onChange={(v) => save({ updates: { check: v } })} />}
        />
      </Group>
    </>
  );
}

/** "What's new in KIVO x.y", once after an update (UX-59). */
export function WhatsNewDialog() {
  const { t } = useTranslation();
  const { request } = useRuntime();
  const [view] = useUpdates();
  const [closed, setClosed] = useState(false);
  const news = view?.whatsNew;
  if (!news || closed) return null;
  const done = () => {
    setClosed(true);
    void request(Method.updatesSeen).catch(() => {});
  };
  const lines = noteLines(news.notes);
  return (
    <Dialog
      open
      onOpenChange={(o) => !o && done()}
      title={t("updates.whatsNew", { version: news.version })}
      description={t("updates.installedNow")}
      footer={
        <>
          <Button
            onClick={() =>
              void request(Method.systemOpenUrl, { url: `${RELEASES}/tag/v${news.version}` }).catch(() => {})
            }
          >
            {t("updates.releaseNotes")}
          </Button>
          <Button variant="primary" onClick={done}>
            {t("updates.gotIt")}
          </Button>
        </>
      }
    >
      {lines.length > 0 ? (
        <ul className="k-whatsnew">
          {lines.map((l) => (
            <li key={l}>{l}</li>
          ))}
        </ul>
      ) : (
        <p className="k-note">{t("updates.noNotes", { from: news.from })}</p>
      )}
    </Dialog>
  );
}
