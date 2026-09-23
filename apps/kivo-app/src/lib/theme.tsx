/**
 * Appearance: theme (Light default / Dark / System), accent colour (seven presets or a custom
 * colour, DS-03), text size, motion and transparency (UX-32). The runtime keeps them in
 * `kivo.toml` (UX-37) and is authoritative; `<ThemeSync>` (inside the runtime provider) applies
 * what it holds and saves changes back. A copy in local storage paints the first frame before the
 * runtime answers, and is the only store when the UI runs without a runtime (the browser preview).
 */
import { invoke, isTauri } from "@tauri-apps/api/core";
import { MotionConfig } from "motion/react";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { textSafe } from "./contrast";

export type ThemePref = "light" | "dark" | "system";
export type Preset = "blue" | "violet" | "teal" | "green" | "amber" | "coral" | "graphite";
/** A preset, or a custom colour as `#rrggbb`. */
export type Accent = Preset | `#${string}`;
export type TextSize = "normal" | "large";
/** "system" follows Windows "Animation effects"; "full" and "reduced" override it in KIVO. */
export type MotionPref = "system" | "full" | "reduced";

/** Accent presets; their names are `accent.<id>` in the translations. */
export const ACCENTS: Array<{ id: Preset; swatch: string }> = [
  { id: "blue", swatch: "#0A6CFF" },
  { id: "violet", swatch: "#7C5CFF" },
  { id: "teal", swatch: "#0E9494" },
  { id: "green", swatch: "#1F9D55" },
  { id: "amber", swatch: "#BF7C00" },
  { id: "coral", swatch: "#E5533D" },
  { id: "graphite", swatch: "#6E6E73" },
];

export function isCustomAccent(a: string): a is `#${string}` {
  return /^#[0-9a-f]{6}$/i.test(a);
}

function isPreset(a: string): a is Preset {
  return ACCENTS.some((x) => x.id === a);
}

/** The appearance as the runtime keeps it (`[appearance]` in kivo.toml). */
export interface Appearance {
  theme: ThemePref;
  accent: Accent;
  textSize: TextSize;
  motion: MotionPref;
  transparency: boolean;
}

const DEFAULTS: Appearance = {
  theme: "light",
  accent: "blue",
  textSize: "normal",
  motion: "system",
  transparency: true,
};

interface ThemeState extends Appearance {
  resolved: "light" | "dark";
  setTheme: (t: ThemePref) => void;
  setAccent: (a: Accent) => void;
  setTextSize: (s: TextSize) => void;
  setMotion: (m: MotionPref) => void;
  setTransparency: (on: boolean) => void;
  /** Applies what the runtime holds (no save back). */
  apply: (a: Partial<Appearance>) => void;
  /** Where changes are saved (the runtime, once connected). */
  setSaver: (save: ((patch: Record<string, Record<string, unknown>>) => void) | null) => void;
}

const ThemeContext = createContext<ThemeState | null>(null);
const KEY = "kivo.appearance";

function load(): Partial<Appearance> {
  try {
    const v: unknown = JSON.parse(localStorage.getItem(KEY) ?? "{}");
    return typeof v === "object" && v !== null ? parse(v) : {};
  } catch {
    return {};
  }
}

function entry(v: object, key: string): unknown {
  return Object.entries(v).find(([k]) => k === key)?.[1];
}

/** The appearance from the runtime's kebab-case settings or the stored copy, keeping only known values. */
export function parse(v: object): Partial<Appearance> {
  const out: Partial<Appearance> = {};
  const theme = entry(v, "theme");
  if (theme === "light" || theme === "dark" || theme === "system") out.theme = theme;
  const accent = entry(v, "accent");
  if (typeof accent === "string" && (isPreset(accent) || isCustomAccent(accent))) out.accent = accent;
  const size = entry(v, "text-size") ?? entry(v, "textSize");
  if (size === "normal" || size === "large") out.textSize = size;
  // (Stored before M7: "reduced" or "system" only; both still read.)
  const motion = entry(v, "motion");
  if (motion === "system" || motion === "full" || motion === "reduced") out.motion = motion;
  const transparency = entry(v, "transparency");
  if (typeof transparency === "boolean") out.transparency = transparency;
  return out;
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<Appearance>(() => ({ ...DEFAULTS, ...load() }));
  const [systemDark, setSystemDark] = useState(() => matchMedia("(prefers-color-scheme: dark)").matches);
  const saver = useRef<((patch: Record<string, Record<string, unknown>>) => void) | null>(null);

  useEffect(() => {
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const resolved = state.theme === "system" ? (systemDark ? "dark" : "light") : state.theme;

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolved;
    if (isCustomAccent(state.accent)) {
      root.dataset.accent = "custom";
      root.style.setProperty("--acc", state.accent);
      // Where the accent is text, a version that reads at 4.5:1 (UX-54).
      root.style.setProperty("--acc-text", textSafe(state.accent, resolved));
    } else {
      root.dataset.accent = state.accent;
      root.style.removeProperty("--acc");
      root.style.removeProperty("--acc-text");
    }
    root.dataset.textsize = state.textSize;
    if (state.motion === "reduced") root.dataset.motion = "reduced";
    else delete root.dataset.motion;
    root.dataset.transparency = state.transparency ? "on" : "off";
    try {
      localStorage.setItem(KEY, JSON.stringify(state));
    } catch {
      /* storage unavailable */
    }
  }, [resolved, state]);

  // Mica behind the window on Windows 11 (UX-32); the page paints its own background otherwise.
  useEffect(() => {
    const root = document.documentElement;
    if (!isTauri()) {
      root.dataset.mica = "off";
      return;
    }
    invoke<boolean>("window_effects", { transparency: state.transparency })
      .then((on) => {
        root.dataset.mica = on ? "on" : "off";
      })
      .catch(() => {
        root.dataset.mica = "off";
      });
  }, [state.transparency]);

  // A change the user made: shown at once, saved to the runtime.
  const change = useCallback((patch: Partial<Appearance>, runtime: Record<string, unknown>) => {
    setState((s) => ({ ...s, ...patch }));
    saver.current?.({ appearance: runtime });
  }, []);

  const value = useMemo<ThemeState>(
    () => ({
      ...state,
      resolved,
      setTheme: (theme) => change({ theme }, { theme }),
      setAccent: (accent) => change({ accent }, { accent }),
      setTextSize: (textSize) => change({ textSize }, { "text-size": textSize }),
      setMotion: (motion) => change({ motion }, { motion }),
      setTransparency: (transparency) => change({ transparency }, { transparency }),
      apply: (a) => setState((s) => ({ ...s, ...a })),
      setSaver: (save) => {
        saver.current = save;
      },
    }),
    [state, resolved, change],
  );
  const reduced = state.motion === "reduced" ? "always" : state.motion === "full" ? "never" : "user";
  return (
    <ThemeContext.Provider value={value}>
      {/* "user" follows Windows; KIVO's own Full or Reduced overrides it. */}
      <MotionConfig reducedMotion={reduced}>{children}</MotionConfig>
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeState {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside <ThemeProvider>");
  return ctx;
}
