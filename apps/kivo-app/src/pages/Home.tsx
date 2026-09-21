/**
 * Home (UX §3, plan §81): KIVO's status at a glance and the controls that act on it. Everything
 * shown comes from the runtime; the Running and Recent lists join as tasks and Activity land.
 */
import { useState } from "react";
import { Alert, Button, useToast } from "../components/ui";
import { Method } from "../ipc/generated";
import { useRuntime } from "../ipc/runtime";
import { cn } from "../lib/cn";
import { viewLink } from "../lib/session";

export function Home() {
  const { link, request, start } = useRuntime();
  const view = viewLink(link);
  const toast = useToast();
  const [busy, setBusy] = useState(false);

  const run = (action: () => Promise<unknown>) => {
    setBusy(true);
    action()
      .catch((e: unknown) => toast(e instanceof Error ? e.message : String(e)))
      .finally(() => setBusy(false));
  };

  const session = view.session;
  const animated = session === "listening" || session === "speaking" || session === "followUp";

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
      <p className="k-home__detail">{view.detail}</p>

      <div className="k-home__actions">
        {session === "paused" ? (
          <Button
            variant="primary"
            icon="play"
            disabled={busy}
            onClick={() => run(() => request(Method.sessionResume))}
          >
            Resume listening
          </Button>
        ) : session !== null ? (
          <Button
            icon="pause"
            disabled={busy || !(session === "idle" || session === "followUp")}
            onClick={() => run(() => request(Method.sessionPause))}
          >
            Pause listening
          </Button>
        ) : null}
        {link?.status === "reconnecting" && (
          <Button variant="primary" icon="power" disabled={busy} onClick={() => run(start)}>
            Start KIVO
          </Button>
        )}
      </div>

      {link?.status === "incompatible" && (
        <div className="k-home__note">
          <Alert kind="warning" title="Quit KIVO from the tray, then open it again.">
            {link.message}
          </Alert>
        </div>
      )}
      {link?.status === "connected" && link.runtimeVersion && (
        <p className="k-home__meta">KIVO {link.runtimeVersion}</p>
      )}
    </div>
  );
}
