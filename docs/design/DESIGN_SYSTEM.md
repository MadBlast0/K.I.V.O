# KIVO design system (draft)

Status: exploration, 2026-09-21. Mockups: [mockups/kivo-mockups.html](mockups/kivo-mockups.html)
(open it in a browser, and switch directions at the top right). The UX behavior is specified in
[../architecture/UX.md](../architecture/UX.md). This file covers the *look*.

## 0. Round 2 (current): wake-up concepts

Round 1 (below, and `mockups/kivo-mockups.html`) was **rejected** because it looked too generic.
Round 2 starts from the wake-up moment instead of color themes, and shows it over a realistic
Windows 11 desktop: [mockups/kivo-wake-concepts.html](mockups/kivo-wake-concepts.html).

| # | Concept | Idea |
|---|---|---|
| 01 | **Native** | A Fluent-style acrylic flyout above the taskbar, in the Windows accent color and system font. Feels built into Windows |
| 02 | **Line** | No container. The voice is a single luminous line at the bottom, with film-style captions above it |
| 03 | **Island** | A black capsule at the top center that morphs with springs into a card, then into a small live activity |
| 04 | **Halo** | Soft light blooms up from the bottom edge and breathes with the voice, with a frosted caption card |

- **Font:** round 2 uses the **Windows system font (Segoe UI Variable)**, which ships with Windows,
  so there is nothing to bundle and no license issue. Other platforms use their system fonts.
- **Process:** after the owner picks a concept (or a mix), the Control Center, onboarding and
  brand get designed in that language.

**Chosen: 03 Island** (2026-09-21). The design language that follows from it:

- **Material:** true black capsules and cards (`#000`) with 1 px inner highlights. Continuous
  corner radii (capsule, then 26–28 px for cards).
- **Motion:** springs (`cubic-bezier(.32,1.28,.54,1)`) that morph size and shape to express
  state. Content cross-fades inside the morphing shape.
- **Accent:** a small luminous orb (blue by default) plus a white or tinted waveform.
- **Type:** system font; white text on black, with secondary text at 60% opacity.
- **Next:** Control Center, onboarding and live-activity mockups in this language (D0).

## 0.1 Full app mockup (v1, for finalization)

[mockups/kivo-app.html](mockups/kivo-app.html) is one file with five sections:

- **Island overlay:** 9 states and 4 live activities, top or bottom position, wake glow, and the
  computer-use frame.
- **Control Center:** 19 screens.
- **Onboarding:** 12 steps.
- **System surfaces:** tray menu, toasts, Windows Hello, capability-off prompt.
- **Component library:** tokens, type and every control, with its implementation source.

**Options to decide**, all switchable at the top of the mockup:

| Option | Choices |
|---|---|
| Navigation | A · sidebar with groups / B · icon rail / C · top tabs |
| Home | A · overview dashboard / B · minimal |
| Accent | Blue / Green / Coral / Mono |
| Density | Comfortable / Compact |
| Default theme | Follow system (both themes are designed) |

**Proposed structure changes from the plan** (all reflected in the mockup):

- **Tools merged into Capabilities**, so there is one place for "what KIVO may do".
- **Integrations renamed "Connectors"**, with "Works without connecting" shown first.
- **Usage and Routines** added to the navigation.
- **Primary buttons use "ink"** (black in light mode, white in dark mode), echoing the Island. The
  accent color is reserved for status, focus and selection.

## 1. Round 1 directions (rejected)

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
