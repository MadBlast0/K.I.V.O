/**
 * The overlay window's content: the Island, driven by the runtime's session state (UX §2). The
 * window itself is transparent, click-through and never takes focus; the app shows it only while
 * the Island has something to show, and hides it (WebView2 invisible, zero frames) otherwise.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MotionConfig } from "motion/react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Island } from "../components/island/Island";
import { islandForMode, islandForSession } from "../components/island/session";
import type { Link, PermissionMode, SessionState } from "../ipc/generated";

/** Room below the Island for its shadow (0 16px 36px -14px → ~38 px). */
const SHADOW = 40;
/** `.k-overlay`'s top padding. */
const TOP_PADDING = 1;

/** How long the Island shows a mode change (the app hides the window after the same time). */
const NOTICE_MS = 1600;

function sessionOf(link: Link): SessionState | null {
  return link.status === "connected" ? (link.snapshot?.session ?? null) : null;
}

function modeOf(link: Link): PermissionMode | null {
  return link.status === "connected" ? (link.snapshot?.mode ?? null) : null;
}

export function Overlay() {
  const [session, setSession] = useState<SessionState | null>(null);
  // The live mic level, read by the waveform on each frame (no re-render per level).
  const level = useRef(0);
  const readLevel = useCallback(() => level.current, []);
  const [mode, setMode] = useState<PermissionMode | null>(null);
  const [notice, setNotice] = useState<PermissionMode | null>(null);
  // A change of mode (not the first one seen) shows a notice for a moment.
  const lastMode = useRef<PermissionMode | null>(null);
  useEffect(() => {
    if (mode === null) return;
    const previous = lastMode.current;
    lastMode.current = mode;
    if (previous === null || previous === mode) return;
    setNotice(mode);
    const timer = window.setTimeout(() => setNotice(null), NOTICE_MS);
    return () => window.clearTimeout(timer);
  }, [mode]);

  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const stops = await Promise.all([
        listen<Link>("runtime://link", (e) => {
          setSession(sessionOf(e.payload));
          setMode(modeOf(e.payload));
        }),
        listen<number>("runtime://level", (e) => {
          level.current = e.payload;
        }),
      ]);
      const stop = () => stops.forEach((s) => s());
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
      const boot = await invoke<{ link: Link }>("ui_ready");
      if (!cancelled) {
        setSession((current) => current ?? sessionOf(boot.link));
        setMode((current) => current ?? modeOf(boot.link));
      }
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

  // A request in progress wins; otherwise a recent mode change shows briefly.
  const model = (session ? islandForSession(session) : null) ?? (notice ? islandForMode(notice) : null);
  return (
    // "user" follows Windows' "Animation effects" setting (DESIGN_SYSTEM §5).
    <MotionConfig reducedMotion="user">
      <div className="k-overlay" ref={root}>
        <Island
          model={model}
          level={readLevel}
          aria-label={typeof model?.label === "string" ? model.label : undefined}
        />
      </div>
    </MotionConfig>
  );
}
