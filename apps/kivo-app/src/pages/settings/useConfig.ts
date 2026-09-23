/** The settings for a Settings tab: read by group and key, saved as a patch (UX-37). */
import { useCallback } from "react";
import { useToast } from "../../components/ui";
import { setting, useSettings, type Settings } from "../../lib/settings";

export function message(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function useConfig() {
  const toast = useToast();
  const [settings, save] = useSettings((e) => toast(message(e)));
  const get = useCallback((group: string, key: string) => setting(settings, group, key), [settings]);
  const set = useCallback((group: string, patch: Record<string, unknown>) => save({ [group]: patch }), [save]);
  return { settings, get, set, save: (patch: Settings) => save(patch) };
}
