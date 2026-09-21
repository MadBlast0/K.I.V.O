/**
 * The UI's view of KIVO's runtime (ARCHITECTURE §1, §3). The Tauri side owns the connection; this
 * keeps the latest `Link` (status + state) in React, and sends requests through it. Outside the
 * KIVO app (the UI running in a plain browser) there is no runtime, and `link` is null.
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { createContext, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import i18n from "../i18n";
import type { Event, Link, Method } from "./generated";

interface Boot {
  link: Link;
  page: string | null;
}

export interface RuntimeValue {
  /** null outside the KIVO app. */
  link: Link | null;
  /** Sends a request; rejects with a message that can be shown as is. `T` is the method's result
   * type (the runtime's Rust types, generated into `generated.ts`). */
  request: <T = unknown>(method: Method, params?: unknown) => Promise<T>;
  /** Starts the runtime when it isn't running. */
  start: () => Promise<void>;
}

/** The error a request gets outside the KIVO app (a plain browser during UI work). */
const notInApp = () => new Error(i18n.t("link.notInApp"));

const RuntimeContext = createContext<RuntimeValue>({
  link: null,
  request: () => Promise.reject(notInApp()),
  start: () => Promise.reject(notInApp()),
});

/** Tauri rejects commands with the Rust error string; make it an Error. */
function asError(e: unknown): Error {
  return e instanceof Error ? e : new Error(String(e));
}

export function RuntimeProvider({
  onNavigate,
  children,
}: {
  /** The runtime or a launch flag asked for a page (tray "Settings", `--page`). Keep it stable. */
  onNavigate: (page: string) => void;
  children: ReactNode;
}) {
  const [link, setLink] = useState<Link | null>(null);

  useEffect(() => {
    if (!isTauri()) return;
    let cancelled = false;
    const unlisten: UnlistenFn[] = [];
    void (async () => {
      // Listen first, then ask for the current state, so no update falls in between.
      const subscriptions = await Promise.all([
        listen<Link>("runtime://link", (e) => setLink(e.payload)),
        listen<string>("runtime://navigate", (e) => onNavigate(e.payload)),
      ]);
      if (cancelled) {
        // Unmounted while subscribing (React StrictMode does this on purpose).
        subscriptions.forEach((u) => u());
        return;
      }
      unlisten.push(...subscriptions);
      const boot = await invoke<Boot>("ui_ready");
      if (cancelled) return;
      setLink((current) => current ?? boot.link);
      if (boot.page) onNavigate(boot.page);
    })();
    return () => {
      cancelled = true;
      unlisten.forEach((u) => u());
    };
  }, [onNavigate]);

  const value = useMemo<RuntimeValue>(
    () => ({
      link,
      request: <T,>(method: Method, params?: unknown) =>
        isTauri()
          ? invoke<T>("runtime_request", { method, params: params ?? null }).catch((e: unknown) => {
              throw asError(e);
            })
          : Promise.reject(notInApp()),
      start: () =>
        isTauri()
          ? invoke<void>("runtime_start").catch((e: unknown) => {
              throw asError(e);
            })
          : Promise.reject(notInApp()),
    }),
    [link],
  );

  return <RuntimeContext.Provider value={value}>{children}</RuntimeContext.Provider>;
}

export function useRuntime(): RuntimeValue {
  return useContext(RuntimeContext);
}

/**
 * Calls `handler` whenever the runtime reports an event (a turn finished, a setting changed), so
 * pages refresh when something happens rather than polling (DISC-13).
 */
export function useRuntimeEvents(handler: (event: Event) => void) {
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });
  useEffect(() => {
    if (!isTauri()) return;
    let stop: UnlistenFn | undefined;
    let cancelled = false;
    void listen<Event>("runtime://event", (e) => latest.current(e.payload)).then((u) => {
      if (cancelled) u();
      else stop = u;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, []);
}
