/**
 * Home (UX §3, UX-19, plan §81): KIVO's status at a glance, the controls that act on it (Talk,
 * Pause listening, the permission mode) and what it did recently. Everything shown comes from the
 * runtime and refreshes when the runtime reports a change. The Running list joins with tasks (M5).
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Alert, Button, Group, Keys, Note, Row, Section, Select, Tag, useToast } from "../components/ui";
import { Method, type ActivityItem, type PermissionMode, type SpeechStatus } from "../ipc/generated";
import { withNodes } from "../i18n/nodes";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { entries } from "../lib/activity";
import { cn } from "../lib/cn";
import { MODES, viewLink } from "../lib/session";

/** How many recent requests Home shows. */
const RECENT = 4;

function SpeechNote({ speech, onRetry }: { speech: SpeechStatus; onRetry: () => void }) {
  const { t } = useTranslation();
  switch (speech.state) {
    case "ready":
      return null;
    case "downloading":
      return <Note>{t("home.downloading", { percent: speech.percent })}</Note>;
    case "missing":
      return <Note>{t("home.missing")}</Note>;
    case "failed":
      return (
        <Alert kind="warning" title={t("home.failed", { message: speech.message })}>
          <Button onClick={onRetry}>{t("home.retry")}</Button>
        </Alert>
      );
  }
}

export function Home({
  onOpenPermissions,
  onOpenActivity,
}: {
  onOpenPermissions: () => void;
  onOpenActivity: () => void;
}) {
  const { t, i18n } = useTranslation();
  const { link, request, start } = useRuntime();
  const view = viewLink(link);
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [recent, setRecent] = useState<ActivityItem[]>([]);

  const run = (action: () => Promise<unknown>) => {
    setBusy(true);
    action()
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)))
      .finally(() => setBusy(false));
  };

  const snapshot = link?.status === "connected" ? link.snapshot : null;
  const session = view.session;
  const mode = snapshot?.mode;
  const connected = !!snapshot;

  const loadRecent = useCallback(() => {
    if (!connected) return;
    void request<ActivityItem[]>(Method.activityList, { limit: 40 })
      .then(setRecent)
      .catch(() => {});
  }, [connected, request]);
  useEffect(loadRecent, [loadRecent]);
  useRuntimeEvents((event) => {
    if (event.group === "turn") loadRecent();
  });

  const chooseMode = (next: PermissionMode) => {
    // Bypass is switched on with its confirmation step on the Permissions page (SEC-03).
    if (next === "bypass") onOpenPermissions();
    else run(() => request(Method.permissionsSetMode, { mode: next }));
  };
  const animated = session === "listening" || session === "speaking" || session === "followUp";
  const time = new Intl.DateTimeFormat(i18n.language, { hour: "2-digit", minute: "2-digit" });
  const shown = entries(recent).slice(0, RECENT);

  return (
    <div className="k-home">
      <div
        className={cn("k-home__orb", animated && "is-live", (view.tone !== "ok" || session === "paused") && "is-dim")}
        aria-hidden
      >
        <span />
      </div>
      <h2 className="k-home__title" aria-live="polite">
        {view.title}
      </h2>
      <p className="k-home__detail">
        {connected ? (
          <span className="k-home__hint">
            {withNodes(t, "home.hint", {
              ptt: <Keys keys={["Ctrl", "Space"]} />,
              type: <Keys keys={["Ctrl", "Shift", "Space"]} />,
            })}
          </span>
        ) : (
          view.detail
        )}
      </p>

      <div className="k-home__actions">
        {connected && (
          <Button
            variant="primary"
            icon="mic"
            disabled={busy || session !== "idle" || snapshot.speech.state !== "ready"}
            onClick={() => run(() => request(Method.sessionTalk))}
          >
            {t("home.talk")}
          </Button>
        )}
        {session === "paused" ? (
          <Button icon="play" disabled={busy} onClick={() => run(() => request(Method.sessionResume))}>
            {t("home.resume")}
          </Button>
        ) : session !== null ? (
          <Button
            icon="pause"
            disabled={busy || !(session === "idle" || session === "followUp")}
            onClick={() => run(() => request(Method.sessionPause))}
          >
            {t("home.pause")}
          </Button>
        ) : null}
        {mode && <Select label={t("home.mode")} icon="permissions" items={MODES} value={mode} onChange={chooseMode} />}
        {link?.status === "reconnecting" && (
          <Button variant="primary" icon="power" disabled={busy} onClick={() => run(start)}>
            {t("home.start")}
          </Button>
        )}
      </div>

      {snapshot && (
        <div className="k-home__note">
          <SpeechNote
            speech={snapshot.speech}
            onRetry={() => run(() => request(Method.modelsInstall, { id: "moonshine-base-en" }))}
          />
          {snapshot.hotkeyConflict && (
            <Alert kind="warning" title={t("home.hotkeyTaken", { keys: snapshot.hotkeyConflict })} />
          )}
        </div>
      )}
      {link?.status === "incompatible" && (
        <div className="k-home__note">
          <Alert kind="warning" title={t("home.incompatible")}>
            {link.message}
          </Alert>
        </div>
      )}

      {connected && (
        <div className="k-home__lists">
          <Section
            title={t("home.recent")}
            aside={
              <Button size="sm" variant="plain" onClick={onOpenActivity}>
                {t("home.activity")}
              </Button>
            }
          />
          {shown.length === 0 ? (
            <Note>{withNodes(t, "home.nothingYet", { ptt: <Keys keys={["Ctrl", "Space"]} /> })}</Note>
          ) : (
            <Group>
              {shown.map((e) => (
                <Row
                  key={e.key}
                  icon={e.kind === "setting" ? "settings" : e.kind === "tool" ? "play" : "mic"}
                  title={e.title}
                  subtitle={e.detail ?? undefined}
                  end={
                    <>
                      <span className="k-meta">{time.format(new Date(e.ts))}</span>
                      {e.outcome !== "done" && <Tag tone="warning">{t(`activity.outcome.${e.outcome}`)}</Tag>}
                    </>
                  }
                />
              ))}
            </Group>
          )}
        </div>
      )}

      {link?.status === "connected" && link.runtimeVersion && (
        <p className="k-home__meta">{t("home.version", { version: link.runtimeVersion })}</p>
      )}
    </div>
  );
}
