# KIVO design system (draft)

Status: exploration, 2026-09-21. Mockups: [mockups/kivo-mockups.html](mockups/kivo-mockups.html)
(open it in a browser, and switch directions at the top right). The UX behavior is specified in
[../architecture/UX.md](../architecture/UX.md). This file covers the *look*.

## 1. Directions under consideration

| Direction | Feel | Palette (dark) | Type candidates | GPU cost | Risk |
|---|---|---|---|---|---|
| **A. Graphite + electric** | Precise, fast, "pro tool" (Raycast, Linear) | `#0B0C0E` bg · `#1D2026` surface · `#ECEEF2` text · accent **`#4DE8FF`** / `#1FB6FF` | Geist + Geist Mono | Lowest (solid surfaces) | Can feel cold |
| **B. Soft glass + aurora** | Magical, soft, Apple-Intelligence-like | `#0E0B1A` bg · translucent white surfaces · accents **`#9FE7FF`** + **`#FF9BD8`** | Sora (display/body) | Highest (blur, gradients). Needs the M0 overlay power measurement | Blur can lag on low-end GPUs; must fall back to solid surfaces |
| **C. Warm minimal** | Calm, human, crafted | `#17130F` bg · `#2C251E` surface · `#F3EAE0` text · accent **`#FF8A5B`** / `#FFC46B` | Fraunces (display) + Instrument Sans (body) | Low | Serif headlines are unusual for a system utility |

**Owner decision pending:** pick A, B or C, or a hybrid. A common hybrid is **A's surfaces and
restraint with B's aurora only on the pill/orb and wake glow**, which keeps the Control Center fast
and the voice moment magical.

## 2. Shared foundations (all directions)

- **Tokens** are CSS custom properties generated from a single `tokens.json`, which feeds both
  Tailwind v4 `@theme` and the Rust side (for tray icon tints).
  - **Color roles:** `bg`, `bg-2`, `surface`, `surface-2`, `line`, `text`, `text-2`, `text-3`,
    `accent`, `accent-2`, `accent-ink`, `ok`, `warn`, `danger`.
  - **Radius:** control 10, card 16, pill 22, sheet 20.
  - **Spacing:** a 4-px base; the scale is 4, 8, 12, 16, 20, 24, 32, 40, 48.
  - **Elevation:** 3 levels, drawn with CSS shadows.
  - **Motion:** a fast 150 ms / standard 250 ms / emphasis 400 ms scale on
    `cubic-bezier(.2,.8,.2,1)`, plus springs for the card.
- **Themes:** dark first. A light theme is derived per direction in M7. High-contrast mode maps
  onto Windows system colors.
- **State colors:** Listening = accent, Confirm = warn (amber), Error = danger. Each state always
  pairs color with an icon and a label (never color alone).
- **Iconography:** one outline icon set at 1.5 px stroke (Lucide, ISC license) plus a small set of
  custom KIVO glyphs (wake, brain, capability indicators).
- **Fonts:** only fonts with open licenses (OFL), bundled. **SF Pro is never used.**
- **Script coverage:** Noto fallbacks for Devanagari, Gurmukhi, CJK and Arabic, loaded per
  language pack.
- **Logo:** to be designed after the direction is chosen, and after the name question in §3 is
  settled. The mockup's placeholder mark is a conic "voice orb" dot.

## 3. Open items

1. The direction choice (A / B / C / hybrid).
2. The logo and wordmark. This depends on whether "KIVO" stays the public name; see
   [research §8](../research/features-and-extensions/REPORT.md).
3. The companion **character** design (for the Rive character style).
4. The earcon motif, designed together with the brand, before beta.
5. The light theme and high-contrast pass (M7).
