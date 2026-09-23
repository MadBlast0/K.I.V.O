/**
 * The settings as the runtime keeps them (`kivo.toml`, kebab-case keys), for the pages that change
 * them (UX-37). The runtime is authoritative: every change is a partial patch it merges, checks and
 * saves, and the page shows what it sends back.
 */
import { useCallback, useEffect, useState } from "react";
import { Method } from "../ipc/generated";
import { useRuntime, useRuntimeEvents } from "../ipc/runtime";

export type Settings = Record<string, Record<string, unknown>>;

/** Loads the settings, reloads them when the runtime says they changed, and saves patches. */
export function useSettings(onError?: (e: unknown) => void): [Settings | null, (patch: Settings) => void] {
  const { link, request } = useRuntime();
  const connected = link?.status === "connected";
  const [settings, setSettings] = useState<Settings | null>(null);
  const load = useCallback(() => {
    if (!connected) return;
    void request<Settings>(Method.settingsGet)
      .then(setSettings)
      .catch(() => {});
  }, [connected, request]);
  useEffect(load, [load]);
  useRuntimeEvents((event) => {
    if (event.group === "system" && event.event.type === "configChanged") load();
  });
  const save = useCallback(
    (patch: Settings) => {
      request<Settings>(Method.settingsSet, patch)
        .then(setSettings)
        .catch((e: unknown) => onError?.(e));
    },
    [request, onError],
  );
  return [settings, save];
}

/** `settings[group][key]`. */
export function setting(settings: Settings | null, group: string, key: string): unknown {
  const g = settings ? Object.entries(settings).find(([k]) => k === group)?.[1] : undefined;
  return field(g, key);
}

export function field(v: unknown, key: string): unknown {
  if (typeof v !== "object" || v === null) return undefined;
  return Object.entries(v).find(([k]) => k === key)?.[1];
}

export function bool(v: unknown, fallback = false): boolean {
  return typeof v === "boolean" ? v : fallback;
}

export function num(v: unknown, fallback = 0): number {
  return typeof v === "number" ? v : fallback;
}

export function str(v: unknown, fallback = ""): string {
  return typeof v === "string" ? v : fallback;
}

export function strings(v: unknown): string[] {
  return Array.isArray(v) ? v.filter((x): x is string => typeof x === "string") : [];
}

/** A value from the settings when it is one of `allowed`. */
export function oneOf<T extends string>(v: unknown, allowed: ReadonlyArray<T>): T | undefined {
  return allowed.find((a) => a === v);
}
