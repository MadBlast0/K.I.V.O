/**
 * The overlay window's content: the Island, driven by the runtime's session state (UX §2). The
 * window itself is transparent, click-through and never takes focus; the app shows it only while
 * the Island has something to show, and hides it (WebView2 invisible, zero frames) otherwise.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MotionConfig } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { Island } from "../components/island/Island";
import { islandForSession } from "../components/island/session";
import type { Link, SessionState } from "../ipc/generated";

/** Room below the Island for its shadow (0 16px 36px -14px → ~38 px). */
const SHADOW = 40;
/** `.k-overlay`'s top padding. */
const TOP_PADDING = 1;

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

  // Keep the window as small as the Island (plus its shadow): a large transparent window costs
  // GPU and power every frame. The Island's content box is measured, not its animated outline,
  // so the window changes size once per state rather than on every frame of the spring.
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = root.current;
    if (!el || !isTauri()) return;
    let last = 0;
    let content: Element | null = null;
    const fit = () => {
      if (!content) return;
      const height = Math.ceil(TOP_PADDING + content.getBoundingClientRect().height + SHADOW);
      if (height !== last) {
        last = height;
        void invoke("overlay_fit", { height });
      }
    };
    const sizes = new ResizeObserver(fit);
    const follow = () => {
      const next = el.querySelector(".k-island > div");
      if (next === content) return;
      sizes.disconnect();
      content = next;
      if (content) sizes.observe(content);
    };
    const mounts = new MutationObserver(follow);
    mounts.observe(el, { childList: true, subtree: true });
    follow();
    return () => {
      mounts.disconnect();
      sizes.disconnect();
    };
  }, []);

  const model = session ? islandForSession(session) : null;
  return (
    // "user" follows Windows' "Animation effects" setting (DESIGN_SYSTEM §5).
    <MotionConfig reducedMotion="user">
      <div className="k-overlay" ref={root}>
        <Island model={model} aria-label={typeof model?.label === "string" ? model.label : undefined} />
      </div>
    </MotionConfig>
  );
}
