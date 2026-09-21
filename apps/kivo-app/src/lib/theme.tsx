/**
 * Appearance state: theme (Light default / Dark / System), accent colour, text size and motion.
 * Applied as data attributes on <html>, and to Motion through <MotionConfig>. Persisted locally
 * until settings move to the runtime's kivo.toml (UX-37).
 */
import { MotionConfig } from "motion/react";
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

export type ThemePref = "light" | "dark" | "system";
export type Accent = "blue" | "violet" | "teal" | "green" | "amber" | "coral" | "graphite";
export type TextSize = "normal" | "large";
/** "system" follows Windows "Animation effects"; "reduced" turns motion off in KIVO only. */
export type MotionPref = "system" | "reduced";

/** Accent presets; their names are `accent.<id>` in the translations. */
export const ACCENTS: Array<{ id: Accent; swatch: string }> = [
  { id: "blue", swatch: "#0A6CFF" },
  { id: "violet", swatch: "#7C5CFF" },
  { id: "teal", swatch: "#0E9494" },
  { id: "green", swatch: "#1F9D55" },
  { id: "amber", swatch: "#C98300" },
  { id: "coral", swatch: "#E5533D" },
  { id: "graphite", swatch: "#6E6E73" },
];

interface ThemeState {
  theme: ThemePref;
  resolved: "light" | "dark";
  accent: Accent;
  textSize: TextSize;
  motion: MotionPref;
  setTheme: (t: ThemePref) => void;
  setAccent: (a: Accent) => void;
  setTextSize: (s: TextSize) => void;
  setMotion: (m: MotionPref) => void;
}

const ThemeContext = createContext<ThemeState | null>(null);
const KEY = "kivo.appearance";

function load(): Partial<Pick<ThemeState, "theme" | "accent" | "textSize" | "motion">> {
  try {
    return JSON.parse(localStorage.getItem(KEY) ?? "{}");
  } catch {
    return {};
  }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const saved = load();
  const [theme, setTheme] = useState<ThemePref>(saved.theme ?? "light");
  const [accent, setAccent] = useState<Accent>(saved.accent ?? "blue");
  const [textSize, setTextSize] = useState<TextSize>(saved.textSize ?? "normal");
  const [motion, setMotion] = useState<MotionPref>(saved.motion ?? "system");
  const [systemDark, setSystemDark] = useState(() => matchMedia("(prefers-color-scheme: dark)").matches);

  useEffect(() => {
    const mq = matchMedia("(prefers-color-scheme: dark)");
    const onChange = () => setSystemDark(mq.matches);
    mq.addEventListener("change", onChange);
    return () => mq.removeEventListener("change", onChange);
  }, []);

  const resolved = theme === "system" ? (systemDark ? "dark" : "light") : theme;

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolved;
    root.dataset.accent = accent;
    root.dataset.textsize = textSize;
    if (motion === "reduced") root.dataset.motion = "reduced";
    else delete root.dataset.motion;
    try {
      localStorage.setItem(KEY, JSON.stringify({ theme, accent, textSize, motion }));
    } catch {
      /* storage unavailable */
    }
  }, [resolved, theme, accent, textSize, motion]);

  const value = useMemo(
    () => ({ theme, resolved, accent, textSize, motion, setTheme, setAccent, setTextSize, setMotion }),
    [theme, resolved, accent, textSize, motion],
  );
  return (
    <ThemeContext.Provider value={value}>
      {/* "user" follows the OS setting; "always" also silences Motion's springs when KIVO's own setting is on. */}
      <MotionConfig reducedMotion={motion === "reduced" ? "always" : "user"}>{children}</MotionConfig>
    </ThemeContext.Provider>
  );
}

export function useTheme(): ThemeState {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside <ThemeProvider>");
  return ctx;
}
