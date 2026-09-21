/**
 * The overlay window's content: the Island, driven by the runtime's state and live turn (UX §2).
 * The window is transparent and never takes focus. It lets clicks through except while the Island
 * shows buttons, and takes focus only while the user types to KIVO (UX-09, UX-41). The app shows
 * it only while the Island has something to show and hides it (WebView2 invisible) otherwise.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MotionConfig } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../icons";
import { Island, IslandKeys, type IslandModel } from "../components/island/Island";
import { islandForMode } from "../components/island/session";
import { hasButtons, islandForTurn, type IslandHandlers } from "../components/island/turn";
import type { Link, PermissionMode, StateSnapshot } from "../ipc/generated";
import { Method } from "../ipc/generated";

/** Room below the Island for its shadow (0 16px 36px -14px → ~38 px). */
const SHADOW = 40;
/** `.k-overlay`'s top padding. */
const TOP_PADDING = 1;
/** How long the Island shows a mode change (the app hides the window after the same time). */
const NOTICE_MS = 1600;

function snapshotOf(link: Link): StateSnapshot | null {
  return link.status === "connected" ? (link.snapshot ?? null) : null;
}

/** A request from one of the Island's own buttons (the app allows only these). */
function request(method: string, params?: unknown) {
  void invoke("island_request", { method, params }).catch((e: unknown) => console.warn(e));
}

export function Overlay() {
  const { t } = useTranslation();
  const [snapshot, setSnapshot] = useState<StateSnapshot | null>(null);
  // The live level (mic while listening, KIVO's voice while speaking), read by the waveform on
  // each frame, so it never re-renders the page.
  const level = useRef(0);
  const readLevel = useCallback(() => level.current, []);
  const [notice, setNotice] = useState<PermissionMode | null>(null);
  const [typing, setTyping] = useState(false);
  const [draft, setDraft] = useState("");

  // A change of mode (not the first one seen) shows a notice for a moment.
  const mode = snapshot?.mode ?? null;
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
        listen<Link>("runtime://link", (e) => setSnapshot(snapshotOf(e.payload))),
        listen<number>("runtime://level", (e) => {
          level.current = e.payload;
        }),
        listen("island://type", () => {
          setDraft("");
          setTyping(true);
        }),
      ]);
      const stop = () => stops.forEach((s) => s());
      if (cancelled) {
        stop();
        return;
      }
      unlisten = stop;
      const boot = await invoke<{ link: Link }>("ui_ready");
      if (!cancelled) setSnapshot((current) => current ?? snapshotOf(boot.link));
    })();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const handlers = useMemo<IslandHandlers>(
    () => ({
      stop: () => request(Method.sessionCancel),
      answer: (callId, allow, always) => request(Method.permissionsAnswer, { callId, allow, always }),
      openControlCenter: () => request("island.openControlCenter"),
      retry: (text) => request(Method.sessionSay, { text }),
      enable: (capability) => request(Method.capabilitiesSet, { capability, on: true }),
    }),
    [],
  );

  // The window takes focus only while typing, so the field gets it once it appears.
  const field = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (typing) field.current?.focus();
  }, [typing]);

  const closeTyping = useCallback(() => {
    setTyping(false);
    setDraft("");
    void invoke("overlay_typing_done");
  }, []);

  const send = useCallback(() => {
    const text = draft.trim();
    if (!text) return;
    request(Method.sessionSay, { text });
    closeTyping();
  }, [draft, closeTyping]);

  // Keyboard (UX-12): Esc cancels, Enter or Ctrl+Enter sends.
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.preventDefault();
      closeTyping();
    } else if (e.key === "Enter") {
      e.preventDefault();
      send();
    }
  };

  const typingModel: IslandModel | null = typing
    ? {
        state: "typing",
        width: 480,
        label: t("island.typeTitle"),
        trail: <IslandKeys keys={["Esc"]} />,
        body: (
          <label className="k-island__input">
            <Icon name="chat" />
            <input
              ref={field}
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              onKeyDown={onKey}
              placeholder={t("island.typePlaceholder")}
              aria-label={t("island.typeTitle")}
            />
            <button type="button" className="k-island__btn k-island__btn--primary" onClick={send}>
              {t("island.send")}
            </button>
          </label>
        ),
      }
    : null;

  // Typing wins; then what KIVO is doing; then a recent mode change for a moment.
  const model =
    typingModel ?? (snapshot ? islandForTurn(snapshot, t, handlers) : null) ?? (notice ? islandForMode(notice) : null);

  // Clicks reach the window only while it has something to click (UX §2).
  const interactive = typing || hasButtons(model);
  useEffect(() => {
    if (isTauri()) void invoke("overlay_interactive", { interactive });
  }, [interactive]);

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
