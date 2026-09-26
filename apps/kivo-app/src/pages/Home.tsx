/**
 * Home (UX §3, UX-19, plan §81): KIVO's status at a glance, kept calm. The orb breathes softly
 * while KIVO waits and comes alive while it listens (following the voice), thinks and speaks;
 * the line under it says how to call KIVO, as the user set it up ("Hey Kivo", the keys, or both).
 * Below: the permission mode, Resume when listening is paused, Stop everything while KIVO works,
 * the tasks at work (UX-19) and what it did recently. Everything comes from the runtime and
 * refreshes when it reports a change.
 */
import { useCallback, useEffect, useRef, useState } from "react";
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
import { useCallWays } from "../components/voice/CallKivo";
import { useMicLevel } from "../components/voice/MicCheck";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";
import { entries } from "../lib/activity";
import { cn } from "../lib/cn";
import { modes, viewLink } from "../lib/session";
import { currentStep, useTasks } from "./Tasks";

const DEFAULT_TYPE_KEYS = ["Ctrl", "Shift", "Space"];

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
  const ways = useCallWays();
  const [typeKeys, setTypeKeys] = useState<string[]>(DEFAULT_TYPE_KEYS);
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

  // The type-to-KIVO keys, for the hint.
  useEffect(() => {
    if (!connected) return;
    void request<{ voice: Record<string, unknown> }>(Method.settingsGet)
      .then((settings) => setTypeKeys(keysOr(settings.voice["type-to-kivo"], DEFAULT_TYPE_KEYS)))
      .catch(() => {});
  }, [connected, request]);
  useRuntimeEvents((event) => {
    if (event.group === "turn") loadRecent();
  });

  const chooseMode = (next: PermissionMode) => {
    // Bypass is switched on with its confirmation step on the Permissions page (SEC-03).
    if (next === "bypass") onOpenPermissions();
    else run(() => request(Method.permissionsSetMode, { mode: next }));
  };
  // The orb's state: listening follows the voice's level; thinking and speaking have their own
  // motion; otherwise it breathes softly (paused with the window hidden, off with reduced motion).
  const listening = session === "listening" || session === "followUp";
  const level = useMicLevel(listening);
  // The voice's level reaches the orb's CSS (--level) without re-rendering the page's styles.
  const orbRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    orbRef.current?.style.setProperty("--level", String(Math.min(1, level * 1.6)));
  }, [level]);
  const orb = listening
    ? "is-listening"
    : session === "speaking"
      ? "is-speaking"
      : session === "thinking" || session === "acting"
        ? "is-thinking"
        : "is-idle";
  const ptt = <Keys keys={ways.keys} />;
  const hint = ways.wake && ways.ptt ? "home.hintBoth" : ways.wake ? "home.hintWake" : "home.hintKeys";
  const time = new Intl.DateTimeFormat(i18n.language, { hour: "2-digit", minute: "2-digit" });
  const shown = entries(recent).slice(0, RECENT);

  return (
    <div className="k-home">
      <div
        className={cn("k-home__orb", orb, (view.tone !== "ok" || session === "paused") && "is-dim")}
        ref={orbRef}
        aria-hidden
      >
        <span />
      </div>
      <h2 className="k-home__title" aria-live="polite">
        {view.title}
      </h2>
      <p className="k-home__detail">
        {connected ? (
          <span className="k-home__hint">{withNodes(t, hint, { ptt, type: <Keys keys={typeKeys} /> })}</span>
        ) : (
          view.detail
        )}
      </p>

      <div className="k-home__actions">
        {session === "paused" && (
          <Button icon="play" disabled={busy} onClick={() => run(() => request(Method.sessionResume))}>
            {t("home.resume")}
          </Button>
        )}
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
                <ShortcutRecorder value={ways.keys} onChange={ways.setKeys} />
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
            <Note>{withNodes(t, ways.ptt ? "home.nothingYet" : "home.nothingYetWake", { ptt })}</Note>
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
