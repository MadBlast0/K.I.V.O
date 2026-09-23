/**
 * Colour contrast (UX-54, DS-16): the WCAG ratio between two colours, and a text-safe version of a
 * custom accent — the same hue, darkened (light theme) or lightened (dark theme) until it reads at
 * 4.5:1 on every surface text sits on.
 */

/** The surfaces text is drawn on, per theme (tokens.css: group, win, side). */
export const SURFACES = {
  light: ["#ffffff", "#fbfbfa", "#f2f2f0"],
  dark: ["#262628", "#1b1b1c", "#222223"],
} as const;

/** The minimum for text (WCAG 1.4.3 AA). */
export const TEXT_RATIO = 4.5;

function rgb(hex: string): [number, number, number] {
  const h = hex.replace("#", "");
  const full = h.length === 3 ? h.replace(/./g, (c) => c + c) : h;
  const n = Number.parseInt(full.slice(0, 6), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function toHex([r, g, b]: [number, number, number]): string {
  return `#${[r, g, b].map((c) => Math.round(c).toString(16).padStart(2, "0")).join("")}`;
}

/** Relative luminance (WCAG). */
export function luminance(hex: string): number {
  const [r, g, b] = rgb(hex).map((c) => {
    const s = c / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

/** The contrast ratio, 1–21. */
export function ratio(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].toSorted((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

/** `color` mixed toward `toward` by `t` (0–1). */
function mix(color: string, toward: string, t: number): string {
  const a = rgb(color);
  const b = rgb(toward);
  return toHex([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]);
}

/** `color` as text: unchanged if it already reads at 4.5:1 on every surface of the theme, else
 * moved toward black (light) or white (dark) just far enough. */
export function textSafe(color: string, theme: "light" | "dark"): string {
  const surfaces = SURFACES[theme];
  const toward = theme === "light" ? "#000000" : "#ffffff";
  for (let step = 0; step <= 50; step++) {
    const c = mix(color, toward, step / 50);
    if (surfaces.every((s) => ratio(c, s) >= TEXT_RATIO)) return c;
  }
  return toward;
}
