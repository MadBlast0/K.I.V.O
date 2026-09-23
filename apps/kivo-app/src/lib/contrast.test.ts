/**
 * The design tokens meet the contrast rule (UX-54, DS-16): every text colour, the status colours as
 * text and every accent's text colour read at ≥ 4.5:1 on each surface text sits on, in both themes;
 * the accents themselves (focus rings, switches) at ≥ 3:1; the Island's text on black. Read from
 * tokens.css itself, so a changed token is checked.
 */
import { describe, expect, it } from "vitest";
import css from "../styles/tokens.css?raw";
import { ratio, SURFACES, TEXT_RATIO, textSafe } from "./contrast";

/** The declarations of the first rule whose selector list is exactly `selector`. */
function block(selector: string): Record<string, string> {
  const rules = [...css.matchAll(/([^{}]+)\{([^}]*)\}/g)];
  const rule = rules.find((r) =>
    r[1]
      .split(",")
      .map((s) => s.replace(/\/\*[\s\S]*?\*\//g, "").trim())
      .includes(selector),
  );
  if (!rule) throw new Error(`no rule for ${selector}`);
  return Object.fromEntries([...rule[2].matchAll(/(--[\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
}

const THEMES = {
  light: block('[data-theme="light"]'),
  dark: block('[data-theme="dark"]'),
} as const;
const ACCENTS = ["violet", "teal", "green", "amber", "coral", "graphite"] as const;

function accent(name: string, theme: "light" | "dark"): Record<string, string> {
  if (name === "blue")
    return theme === "light" ? block('[data-accent="blue"]') : block('[data-theme="dark"][data-accent="blue"]');
  return theme === "light" ? block(`[data-accent="${name}"]`) : block(`[data-theme="dark"][data-accent="${name}"]`);
}

describe("contrast (UX-54, DS-16)", () => {
  for (const theme of ["light", "dark"] as const) {
    const t = THEMES[theme];
    it(`${theme}: every text colour reads at 4.5:1 on every surface`, () => {
      for (const token of ["--text", "--text-2", "--text-3", "--green-text", "--orange-text", "--red-text"]) {
        for (const surface of [t["--group"], t["--win"], t["--side"]]) {
          expect(ratio(t[token], surface), `${token} ${t[token]} on ${surface}`).toBeGreaterThanOrEqual(TEXT_RATIO);
        }
      }
      // Buttons: ink with its own text colour.
      expect(ratio(t["--ink"], t["--ink-fg"])).toBeGreaterThanOrEqual(TEXT_RATIO);
    });

    it(`${theme}: every accent is legible as text, visible as a control, and carries its bubble text`, () => {
      for (const name of ["blue", ...ACCENTS]) {
        const a = accent(name, theme);
        for (const surface of SURFACES[theme]) {
          expect(ratio(a["--acc-text"], surface), `${name} text on ${surface}`).toBeGreaterThanOrEqual(TEXT_RATIO);
          expect(ratio(a["--acc"], surface), `${name} control on ${surface}`).toBeGreaterThanOrEqual(3);
        }
        expect(ratio(a["--acc-text"], t["--on-acc"]), `${name} bubble`).toBeGreaterThanOrEqual(TEXT_RATIO);
      }
    });
  }

  it("the Island's text reads on black", () => {
    expect(ratio("#ffffff", "#000000")).toBe(21);
    // --isl-2 is white at 62% on black.
    expect(ratio("#9e9e9e", "#000000")).toBeGreaterThanOrEqual(TEXT_RATIO);
  });

  it("makes any custom accent safe as text", () => {
    for (const color of ["#ffe600", "#00ff66", "#ff00ff", "#0000ff", "#777777", "#2b2b2b"]) {
      for (const theme of ["light", "dark"] as const) {
        const safe = textSafe(color, theme);
        for (const surface of SURFACES[theme]) expect(ratio(safe, surface)).toBeGreaterThanOrEqual(TEXT_RATIO);
      }
    }
    // Already fine: unchanged.
    expect(textSafe("#0963eb", "light")).toBe("#0963eb");
  });
});
