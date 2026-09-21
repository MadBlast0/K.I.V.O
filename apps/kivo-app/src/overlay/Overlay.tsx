/**
 * The overlay window's content: the Island, driven by the runtime's session state (UX §2). The
 * window itself is transparent, click-through and never takes focus; the app shows it only while
 * the Island has something to show, and hides it (WebView2 invisible, zero frames) otherwise.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MotionConfig } from "motion/react";
import { useEffect, useState } from "react";
import { Island } from "../components/island/Island";
import { islandForSession } from "../components/island/session";
import type { Link, SessionState } from "../ipc/generated";

function sessionOf(link: Link): SessionState | null {
  return link.status === "connected" ? (link.snapshot?.session ?? null) : null;
}

export function Overlay() {
  const [session, setSession] = useState<SessionState | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const stop = await listen<Link>("runtime://link", (e) => setSession(sessionOf(e.payload)));
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
      const boot = await invoke<{ link: Link }>("ui_ready");
      if (!cancelled) setSession((current) => current ?? sessionOf(boot.link));
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const model = session ? islandForSession(session) : null;
  return (
    // "user" follows Windows' "Animation effects" setting (DESIGN_SYSTEM §5).
    <MotionConfig reducedMotion="user">
      <div className="k-overlay">
        <Island model={model} aria-label={typeof model?.label === "string" ? model.label : undefined} />
      </div>
    </MotionConfig>
  );
}
