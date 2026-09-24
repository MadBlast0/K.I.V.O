/**
 * The overlay window's content: the Island, driven by the runtime's state and live turn (UX §2).
 * The window is transparent and never takes focus. It lets clicks through except while the Island
 * shows buttons, and takes focus only when asked with Ctrl+Shift+Space: to type to KIVO (UX-09,
 * UX-41) or, when the Island shows buttons, to use them from the keyboard (UX-52). The app shows
 * it only while the Island has something to show and hides it (WebView2 invisible) otherwise.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { MotionConfig } from "motion/react";
import { useCallback, useEffect, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";
import { Icon } from "../icons";
import { Island, IslandActions, IslandKeys, type IslandModel } from "../components/island/Island";
import { islandForMode } from "../components/island/session";
import { hasButtons, islandForTurn, type IslandHandlers } from "../components/island/turn";
import { NOTHING, announcements, heardFrom } from "../components/island/announce";
import type { Link, MotionPref, PermissionMode, StateSnapshot } from "../ipc/generated";
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

/** Settings → Appearance "Animations" for Motion (`user` follows Windows). */
const MOTION: Record<MotionPref, "user" | "never" | "always"> = { system: "user", full: "never", reduced: "always" };

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
  // The Draft card's Edit (CONV-15): the text field edits that prompt instead of sending a request.
  const [editingDraft, setEditingDraft] = useState<string | null>(null);
  // Keyboard focus is on the Island's buttons (Tab / arrows move, Enter presses, Esc leaves): the
  // Island it was asked for, so it ends by itself when that Island changes.
  const [keyboardFor, setKeyboardFor] = useState<string | null>(null);
  // The Island showing buttons right now (its key), read when the hotkey arrives.
  const buttons = useRef<string | null>(null);
  const root = useRef<HTMLDivElement>(null);

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
          if (buttons.current) {
            setKeyboardFor(buttons.current);
          } else {
            setDraft("");
            setTyping(true);
          }
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

  // A timer's countdown in the collapsed Island (UX-15) ticks once a second, only while one shows.
  const [, setTick] = useState(0);
  const counting = !!snapshot?.activities?.some((a) => a.until != null);
  useEffect(() => {
    if (!counting) return;
    const id = window.setInterval(() => setTick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [counting]);

  // Screen readers hear state changes, what KIVO heard and its answers (UX-53).
  const heard = useRef(NOTHING);
  useEffect(() => {
    const now = heardFrom(snapshot);
    const said = announcements(heard.current, now, t);
    heard.current = now;
    // Only when the user wants them (Settings → Accessibility).
    const wanted = snapshot?.island?.announcements ?? true;
    if (isTauri() && wanted) for (const text of said) void invoke("announce", { text });
  }, [snapshot, t]);

  const handlers = useMemo<IslandHandlers>(
    () => ({
      stop: () => request(Method.sessionCancel),
      answer: (callId, allow, always, extra) =>
        request(Method.permissionsAnswer, {
          callId,
          allow,
          always,
          ...(extra?.duration ? { duration: extra.duration } : {}),
          ...(extra?.hello ? { hello: true } : {}),
        }),
      undo: () => request(Method.sessionUndo),
      openControlCenter: () => request("island.openControlCenter"),
      openMode: () => request("island.openMode"),
      retry: (text) => request(Method.sessionSay, { text }),
      enable: (capability) => request(Method.capabilitiesSet, { capability, on: true }),
      edit: (text) => {
        setDraft(text);
        setTyping(true);
        void invoke("overlay_focus");
      },
      talk: () => request(Method.sessionTalk),
      misroute: (turnId) => request(Method.chatMisroute, { turnId, note: "" }),
      live: () => request(Method.sessionLive),
      remember: (turnId) => invoke("island_request", { method: Method.memoryRememberTurn, params: { turnId } }),
      editDraft: (callId, text) => {
        setEditingDraft(callId);
        setDraft(text);
        setTyping(true);
        void invoke("overlay_focus");
      },
      offer: (id, accept) => request(Method.offerAnswer, { id, accept }),
      computerPause: () => request(Method.sessionComputerPause),
      openTask: (id) => request("island.openTask", { id }),
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
    setEditingDraft(null);
    setDraft("");
    void invoke("overlay_typing_done");
  }, []);

  const send = useCallback(() => {
    const text = draft.trim();
    if (!text) return;
    if (editingDraft) request(Method.draftAnswer, { callId: editingDraft, text });
    else request(Method.sessionSay, { text });
    closeTyping();
  }, [draft, editingDraft, closeTyping]);

  // Keyboard (UX-12): Esc cancels, Enter or Ctrl+Enter sends.
  const onKey = (e: KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "F1") {
      // "What can I say?" (UX-44): the same request as saying it.
      e.preventDefault();
      request(Method.sessionSay, { text: t("island.helpPhrase") });
      closeTyping();
    } else if (e.key === "Escape") {
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
        label: editingDraft ? t("island.draft.editTitle") : t("island.typeTitle"),
        trail: <IslandKeys keys={["Esc"]} />,
        body: (
          <>
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
            {!editingDraft && snapshot?.hasSelection && (
              // The selection shortcut (UX-42): quick actions on the text selected in the app in front.
              <IslandActions
                hint={false}
                actions={(["explain", "rewrite", "translate"] as const).map((action) => ({
                  label: t(`island.selection.${action}`),
                  onClick: () => {
                    request(Method.selectionAction, { action });
                    closeTyping();
                  },
                }))}
              />
            )}
          </>
        ),
      }
    : null;

  // Typing wins; then what KIVO is doing; then a recent mode change for a moment; then, while the
  // pointer is over it, the card the user is reading (UX-10: it doesn't collapse under the mouse).
  const [held, setHeld] = useState<IslandModel | null>(null);
  const live =
    typingModel ?? (snapshot ? islandForTurn(snapshot, t, handlers) : null) ?? (notice ? islandForMode(notice) : null);
  const model = live ?? held;
  const hover = (on: boolean) => {
    setHeld(on ? live : null);
    if (isTauri()) void invoke("overlay_hover", { hovering: on });
  };

  // Clicks reach the window only while it has something to click (UX §2).
  const withButtons = !typing && hasButtons(model);
  const interactive = typing || withButtons;
  const islandKey = withButtons && model ? `${snapshot?.turn?.id ?? ""}:${model.state}` : null;
  const keyboard = keyboardFor !== null && keyboardFor === islandKey;
  useEffect(() => {
    buttons.current = islandKey;
  }, [islandKey]);

  // Keyboard mode: focus the first button; once that Island is gone, give focus back.
  useEffect(() => {
    if (keyboard) {
      root.current?.querySelector<HTMLButtonElement>(".k-island button")?.focus();
    } else if (keyboardFor !== null && !typing) {
      void invoke("overlay_typing_done");
    }
  }, [keyboard, keyboardFor, typing]);

  const onIslandKey = (e: KeyboardEvent<HTMLDivElement>) => {
    if (!keyboard) return;
    if (e.key === "F1") {
      e.preventDefault();
      request(Method.sessionSay, { text: t("island.helpPhrase") });
      return;
    }
    if (e.key === "Escape") {
      e.preventDefault();
      setKeyboardFor(null);
      void invoke("overlay_typing_done");
      return;
    }
    if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
    const all = [...(root.current?.querySelectorAll<HTMLButtonElement>(".k-island button") ?? [])];
    const at = all.findIndex((b) => b === document.activeElement);
    const rtl = document.documentElement.dir === "rtl";
    const step = (e.key === "ArrowRight") !== rtl ? 1 : -1;
    const next = all[(at + step + all.length) % all.length];
    if (next) {
      e.preventDefault();
      next.focus();
    }
  };
  useEffect(() => {
    if (isTauri()) void invoke("overlay_interactive", { interactive });
  }, [interactive]);

  // Keep the window as small as the Island (plus its shadow): a large transparent window costs
  // GPU and power every frame. The Island's content box is measured, not its animated outline,
  // so the window changes size once per state rather than on every frame of the spring.
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
    // "user" follows Windows' "Animation effects" setting (DESIGN_SYSTEM §5), unless the user
    // chose Full or Reduced (Settings → Appearance).
    <MotionConfig reducedMotion={MOTION[snapshot?.island?.motion ?? "system"]}>
      {/* The keys only act while the user asked for keyboard focus (Ctrl+Shift+Space). */}
      {/* oxlint-disable-next-line jsx-a11y/no-static-element-interactions */}
      <div
        className="k-overlay"
        data-size={snapshot?.island?.size ?? "standard"}
        data-large-text={snapshot?.island?.largeText ? "" : undefined}
        ref={root}
        // Half the screen, less the Island's row and shadow: the card scrolls beyond it (UX-09).
        style={{
          ["--island-body-max" as string]: `${Math.max(160, Math.round(window.screen.availHeight / 2) - 90)}px`,
        }}
        onKeyDown={onIslandKey}
        onMouseEnter={() => hover(true)}
        onMouseLeave={() => hover(false)}
      >
        <Island
          model={model}
          level={readLevel}
          companion={
            snapshot?.island?.companion === "orb" || snapshot?.island?.companion === "character"
              ? snapshot.island.companion
              : "pill"
          }
          onDrag={() => {
            if (isTauri()) void invoke("overlay_drag").catch(() => {});
          }}
          aria-label={typeof model?.label === "string" ? model.label : undefined}
        />
      </div>
    </MotionConfig>
  );
}
