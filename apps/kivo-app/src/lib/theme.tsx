/**
 * Theme state: Light (default) / Dark / System, accent colour and text size.
 * Applied as data attributes on <html>; persisted locally.
 */
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

export type ThemePref = "light" | "dark" | "system";
export type Accent = "blue" | "violet" | "teal" | "green" | "amber" | "coral" | "graphite";
export type TextSize = "normal" | "large";

export const ACCENTS: Array<{ id: Accent; label: string; swatch: string }> = [
  { id: "blue", label: "Blue", swatch: "#0A6CFF" },
  { id: "violet", label: "Violet", swatch: "#7C5CFF" },
  { id: "teal", label: "Teal", swatch: "#0E9494" },
  { id: "green", label: "Green", swatch: "#1F9D55" },
  { id: "amber", label: "Amber", swatch: "#C98300" },
  { id: "coral", label: "Coral", swatch: "#E5533D" },
  { id: "graphite", label: "Graphite", swatch: "#6E6E73" },
];

interface ThemeState {
  theme: ThemePref;
  resolved: "light" | "dark";
  accent: Accent;
  textSize: TextSize;
  setTheme: (t: ThemePref) => void;
  setAccent: (a: Accent) => void;
  setTextSize: (s: TextSize) => void;
}

const ThemeContext = createContext<ThemeState | null>(null);
const KEY = "kivo.appearance";

function load(): Partial<Pick<ThemeState, "theme" | "accent" | "textSize">> {
  try { return JSON.parse(localStorage.getItem(KEY) ?? "{}"); } catch { return {}; }
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const saved = load();
  const [theme, setTheme] = useState<ThemePref>(saved.theme ?? "light");
  const [accent, setAccent] = useState<Accent>(saved.accent ?? "blue");
  const [textSize, setTextSize] = useState<TextSize>(saved.textSize ?? "normal");
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
    try { localStorage.setItem(KEY, JSON.stringify({ theme, accent, textSize })); } catch { /* storage unavailable */ }
  }, [resolved, theme, accent, textSize]);

  const value = useMemo(() => ({ theme, resolved, accent, textSize, setTheme, setAccent, setTextSize }), [theme, resolved, accent, textSize]);
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeState {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error("useTheme must be used inside <ThemeProvider>");
  return ctx;
}
