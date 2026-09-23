/**
 * Home (UX §3, UX-19, plan §81): KIVO's status at a glance, the controls that act on it (Talk,
 * Pause listening, the permission mode) and what it did recently. Everything shown comes from the
 * runtime and refreshes when the runtime reports a change. Running lists the tasks at work (UX-19).
 */
import { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  Alert,
  Button,
  Group,
  Keys,
  Note,
  Row,
  Section,
  Select,
  ShortcutRecorder,
  Tag,
  useToast,
} from "../components/ui";
import { Method, type ActivityItem, type PermissionMode, type SpeechStatus } from "../ipc/generated";
import { withNodes } from "../i18n/nodes";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { entries } from "../lib/activity";
import { cn } from "../lib/cn";
import { modes, viewLink } from "../lib/session";
import { currentStep, useTasks } from "./Tasks";

interface Shortcuts {
  pushToTalk: string[];
  typeToKivo: string[];
}

const DEFAULT_KEYS: Shortcuts = { pushToTalk: ["Ctrl", "Space"], typeToKivo: ["Ctrl", "Shift", "Space"] };

/** The shortcuts from the settings' `voice` section. */
function shortcutsOf(voice: Record<string, unknown>): Shortcuts {
  return {
    pushToTalk: keysOr(voice["push-to-talk"], DEFAULT_KEYS.pushToTalk),
    typeToKivo: keysOr(voice["type-to-kivo"], DEFAULT_KEYS.typeToKivo),
  };
}

function keysOr(value: unknown, fallback: string[]): string[] {
  return Array.isArray(value) && value.every((k) => typeof k === "string") ? value : fallback;
}

/** How many recent requests Home shows. */
const RECENT = 4;

function SpeechNote({
  speech,
  onRetry,
  onChoose,
}: {
  speech: SpeechStatus;
  onRetry: () => void;
  onChoose: () => void;
}) {
  const { t } = useTranslation();
  switch (speech.state) {
    case "ready":
      return null;
    case "downloading":
      return <Note>{t("home.downloading", { percent: speech.percent })}</Note>;
    case "missing":
      return (
        <Alert kind="info" title={t("home.missing")}>
          <Button onClick={onChoose}>{t("home.chooseModel")}</Button>
        </Alert>
      );
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
  onOpenVoice,
  onOpenTasks,
}: {
  onOpenPermissions: () => void;
  onOpenActivity: () => void;
  onOpenVoice: () => void;
  onOpenTasks: () => void;
}) {
  const { t, i18n } = useTranslation();
  const { link, request, start } = useRuntime();
  const view = viewLink(link);
  const toast = useToast();
  const [busy, setBusy] = useState(false);
  const [recent, setRecent] = useState<ActivityItem[]>([]);
  const { active: running } = useTasks();

  const run = (action: () => Promise<unknown>) => {
    setBusy(true);
    action()
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)))
      .finally(() => setBusy(false));
  };

  const snapshot = link?.status === "connected" ? link.snapshot : null;
  const [keys, setKeys] = useState<Shortcuts>(DEFAULT_KEYS);
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

  // The shortcuts as set, for the hint and the rebind prompt.
  useEffect(() => {
    if (!connected) return;
    void request<{ voice: Record<string, unknown> }>(Method.settingsGet)
      .then((settings) => setKeys(shortcutsOf(settings.voice)))
      .catch(() => {});
  }, [connected, request]);

  const rebind = (next: string[]) =>
    run(async () => {
      await request(Method.settingsSet, { voice: { "push-to-talk": next } });
      setKeys((current) => ({ ...current, pushToTalk: next }));
    });
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
              ptt: <Keys keys={keys.pushToTalk} />,
              type: <Keys keys={keys.typeToKivo} />,
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
        {session !== null && session !== "idle" && session !== "paused" && (
          // The emergency stop (SEC-25): the turn, KIVO's voice and (from M5) tasks.
          <Button variant="stop" icon="stop" onClick={() => run(() => request(Method.sessionStopEverything))}>
            {t("home.stopEverything")}
          </Button>
        )}
        {mode && (
          <Select label={t("home.mode")} icon="permissions" items={modes()} value={mode} onChange={chooseMode} />
        )}
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
            onChoose={onOpenVoice}
          />
          {snapshot.hotkeyConflict && (
            // The rebind prompt (VOICE-41): the keys still work, but another app uses them too.
            <Alert kind="warning" title={t("home.hotkeyTaken", { keys: snapshot.hotkeyConflict })}>
              <div className="k-home__rebind">
                {t("home.hotkeyRebind")}
                <ShortcutRecorder value={keys.pushToTalk} onChange={rebind} />
              </div>
            </Alert>
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

      {connected && running.length > 0 && (
        <div className="k-home__lists">
          <Section
            title={t("home.running")}
            aside={
              <Button size="sm" variant="plain" onClick={onOpenTasks}>
                {t("home.allTasks")}
              </Button>
            }
          />
          <Group>
            {running.slice(0, RECENT).map((task) => (
              <Row
                key={task.id}
                icon="tasks"
                title={task.title}
                subtitle={currentStep(task)?.title}
                onClick={onOpenTasks}
                end={
                  <Tag tone={task.status === "needsYou" ? "warning" : "accent"}>{t(`tasks.status.${task.status}`)}</Tag>
                }
              />
            ))}
          </Group>
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
