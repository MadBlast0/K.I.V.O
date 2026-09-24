# KIVO design system

Status: v1, 2026-09-21. **Reference mockup:** [mockups/kivo-app.html](mockups/kivo-app.html)
(open it in a browser; it has the Island, every Control Center page, onboarding, system surfaces
and the component library). **Implementation:** `apps/kivo-app/src/styles/` (tokens and CSS) and
`apps/kivo-app/src/components/` (React), shown together on the in-app **Components** gallery page.
Behaviour (what each surface does) is specified in [../architecture/UX.md](../architecture/UX.md);
this file covers how it looks and moves.

## 1. Direction

- **Island theme.** KIVO's identity is the Island: a true-black capsule (`#000`) at the top of the
  screen that morphs with springs. The same black material is used for the command palette, toasts
  and the onboarding welcome.
- **Control Center:** calm and native-feeling. Grouped lists in the macOS style (white groups on a
  light window), the system font, no decorative gradients, no "AI dashboard" look.
- **Ink buttons.** Primary buttons are ink (near-black in light, near-white in dark), echoing the
  Island. **The accent is for selection, focus and status only**, never for buttons.
- **Light theme by default**, with Light / Dark / System. One comfortable density (no compact
  mode).
- **Custom title bar.** The native title bar is off (`decorations: false`). The sidebar brand row
  and a strip over the page form one bar with Windows-style caption buttons (Segoe Fluent Icons).
  Windows 11 keeps the rounded corners and shadow.

## 2. Tokens

The single source of truth is `apps/kivo-app/src/styles/tokens.css`.

| Group | Tokens |
|---|---|
| Type | `--font-sys` (Segoe UI Variable Text → system-ui), `--font-disp`, `--font-mono` (Cascadia Mono). Body 13.5 px; large text size 14.5 px (`data-textsize="large"`). Nothing is bundled; Noto fallbacks come with language packs |
| Shape | `--r-s` 6 · `--r-m` 10 (controls) · `--r-l` 14 (cards, groups) · `--r-island` 20 · row height `--row` 44 · `--gap` 12 |
| Surfaces | `--page`, `--win`, `--side`, `--group`, `--fill`, `--fill-2`, `--sep`, `--border`, `--shadow`, `--win-shadow` |
| Text | `--text`, `--text-2`, `--text-3`, each ≥ 4.5:1 on `--group`, `--win` and `--side` in both themes (M7: `--text-3` darkened from the mockup's light `#9C9CA1` to `#6E6E73`, dark `#8E8E93`); `--ink` / `--ink-fg` for primary buttons |
| Status | `--green`, `--orange`, `--red` for fills and dots; `--green-text`, `--orange-text`, `--red-text` where they are text (≥ 4.5:1). Each is always paired with an icon or label, never color alone |
| Accent | `--acc` and `--acc-soft`, per theme, for 7 presets: Blue (default), Violet, Teal, Green, Amber, Coral, Graphite (≥ 3:1 as a control; light Amber is `#BF7C00` for that). `--acc-text` is the same hue where the accent is text (links, the active page, accent pills) at ≥ 4.5:1, and `--on-acc` the text on a solid accent; a custom accent's `--acc-text` is computed (`lib/contrast.ts`). `src/lib/contrast.test.ts` checks every pair from `tokens.css` |
| Island | `--isl` `#000`, `--isl-fg`, `--isl-2` (60% text), `--isl-3` (chips) — identical in both themes |
| Motion | `--t-fast` 150 ms · `--t-base` 250 ms · `--t-slow` 400 ms · `--ease-out` `cubic-bezier(.2,.8,.2,1)` · `--ease-spring` `cubic-bezier(.32,1.28,.54,1)` |

Themes switch with `data-theme` on `<html>`; the accent with `data-accent`. Appearance is set in
Settings → Appearance and applied by `lib/theme.tsx`.

## 3. Components

All classes use the `k-` prefix. Interactive primitives are **Base UI** (`@base-ui/react`), which
supplies accessibility and keyboard behaviour; KIVO styles them through their data attributes.

| Area | Components (`components/…`) |
|---|---|
| Buttons | `Button` (secondary, primary/ink, plain, link, destructive, stop; md/sm), `IconButton` (label required) |
| Controls | `Switch`, `Checkbox`, `RadioGroup` + `Radio`, `OptionCard`, `Segmented`, `Slider` |
| Fields | `TextField`, `SearchField`, `TextArea`, `Select` |
| Lists | `Section`, `Group`, `Row` (lead icon/node, title, subtitle, end, chevron), `Note`, `Meta` |
| Status | `Tag`, `Pill`, `Keys`, `NewDot`, `Spinner`, `Done`, `Meter`, `BudgetMeter`, `LevelMeter`, `Orb`, `Mark`, `Monogram` |
| Feedback | `Alert` (info/success/warning/danger), `EmptyState`, `Tile`, `Stat` |
| Overlays | `PageTabs` (sliding indicator), `Tooltip`, `Dialog` + `DialogClose`, `Sheet`, `Popover` |
| Menus | `DropdownMenu` and `ContextMenu` from one data model (`MenuEntry`): items, icons, shortcuts, checks, labels, separators, danger items, **nested submenus** |
| Toasts | `ToastProvider`, `useToast()` with optional Undo |
| Palette | `CommandPalette` (black, Ctrl+K) |
| Pickers | `AccentPicker`, `ShortcutRecorder` (conflict warning) |
| Island geometry | The collapsed pill is 36 px tall, so its ends are 18 px circles. The leading item (18 px) sits 9 px in, concentric with the left end; trailing round items are nudged so their centre is also 18 px from the right end (a 14 px spinner 11 px in, a 16 px ring 10 px, a 24 px pill button 6 px). Expanded content starts on the label's text column (9 + 18 + 9 = 36 px) with 16 px on the right. The waveform is five bars in 44 × 18 px |
| Island | `Island` (spring morph, size-to-content), `Waveform` (runs only while audible), building blocks (`IslandActions` with matching voice hints, `IslandRisk`, `IslandApp`, `IslandRing`, `IslandProgress`, …) and `islandPreset()` for all 22 states plus 4 notices (Bypass on, capability off, in a call, offline) |
| System previews | `NativeMenuPreview` (tray menu, jump list, Explorer menu; submenus), `TrayTooltipPreview`, `WindowsToastPreview` (actions or reply), `HelloPreview` |
| Layout | `AppWindow`, `Sidebar` (13 items in groups), `PageHeader`, `TitleBar` + `WindowControls` |
| Icons | `Icon` with semantic names (`icons/index.tsx`), Lucide (ISC) at 1.6 stroke |

## 4. Rules

**Icons: one meaning per icon, one icon per meaning.** Components use semantic names, never raw
Lucide names. `ai` (sparkle) means AI-generated or an AI step only; `eye` means seeing or
visibility only; `refresh` means update, retry or retrain only; Island settings → `island`, money
→ `coin`, documents → `file`, agents → `agent`, condensing → `compress`. 16 px in lists, 13 px
inside the Island.

**Voice hints list the words on the buttons**, in the same order (Approve / Edit / Cancel → *Say
"approve", "edit" or "cancel"*). "Wait" and "why?" work everywhere and are explained in Voice
settings instead. `IslandActions` generates the hint from the buttons, so they cannot drift.

**Copy:** plain words, sentence case, no jargon on the surface. "Open KIVO when Windows starts",
not "Start when I sign in".

## 5. Motion

Production uses **Motion** (`motion/react`) and CSS with the token values above.

| Element | Behavior |
|---|---|
| Island | Width and height morph with the spring; content cross-fades 120 ms after the shape; the waveform animates only while audible (zero frames at rest) |
| Page / tab change | Content rises 8 px and fades in, staggered 30 ms per block (max 12); the tab underline slides |
| Onboarding | Steps slide 18 px in the direction of travel; progress dots stretch; the welcome Island demonstrates itself |
| Dialogs | Scrim fades in; the dialog scales 0.96 → 1 with the spring |
| Command palette | Drops 12 px from the top with the spring |
| Side sheet | Slides in from the right (350 ms), then its content staggers in |
| Menus, toasts, notifications | Scale 0.96 → 1 from the anchor; submenus slide 6 px after a 120 ms delay |
| Buttons | Press scale 0.97; hover background 150 ms |
| Reduced motion | Everything becomes instant; follows Windows "Animation effects" and the in-app Motion setting |

## 6. Accessibility

Contrast ≥ 4.5:1 for text; visible focus rings; state never shown by color alone; Windows high
contrast respected; every icon-only button has a label. See UX §10.

## 7. History

Round 1 (three color directions) was rejected as too generic. Round 2 compared four wake-up
concepts (Native, Line, Island, Halo) and the owner chose **Island** (2026-09-21). The full app
mockup then went through v1 → v3.6, settling the navigation, onboarding, Extensions page and
system surfaces. Those earlier mockup files are in the git history; decisions are in
[DECISIONS.md](../DECISIONS.md).

## 8. Open items

1. The companion **Character** design (KIVO's SVG drawing today; a Rive design can replace it) and the **Orb** shader (M8).
2. The custom **earcon motif**, designed with the brand before beta (VOICE-29).
3. High-contrast pass (M7).
4. A trademark registry check for "KIVO" before the first public release.

## Build checklist

Status marks and the build protocol: [docs/README.md](../README.md).

- [x] **DS-01** · D0 · Tokens for type, shape, surfaces, text, status, accents (7 presets × light/dark), Island and motion in `styles/tokens.css` (§2) → done: `apps/kivo-app/src/styles/tokens.css` · verified: gallery in light and dark with accent switching (2026-09-21)
- [x] **DS-02** · D0 · Theme provider: Light (default) / Dark / System (follows Windows live), accent and text size applied as `data-*` attributes and persisted (§2) → done: `src/lib/theme.tsx` · verified: switched in the gallery, `pnpm typecheck`
- [x] **DS-03** · M7 · Custom accent color (the 8th swatch in Appearance) and the transparency-effects setting (§2, DECISIONS "Appearance settings") → done: Appearance has the seven presets plus a custom colour (the 8th swatch opens a colour picker; its text-safe variant is computed, `lib/contrast.ts`) and the transparency-effects switch (Mica on Windows 11) · verified: `Settings.test.tsx` Appearance, `Pickers.test.tsx`, `contrast.test.ts` custom accents (2026-09-24)
- [x] **DS-04** · D0 · Semantic icon set with the one-meaning rule (§4) → done: `src/icons/index.tsx` · verified: all names render on the gallery Icons section
- [x] **DS-05** · D0 · Buttons, controls, fields, lists, status and feedback components (§3) → done: `src/components/ui/{Button,Controls,Fields,List,Status,Feedback}.tsx` · verified: gallery, no console errors, `tsc` and `vite build` clean
- [x] **DS-06** · D0 · Overlays: tabs with sliding indicator, tooltip, dialog, sheet, popover (§3) → done: `src/components/ui/Overlays.tsx` · verified: opened each in the gallery
- [x] **DS-07** · D0 · Dropdown and context menus with nested submenus, checks, shortcuts, labels and danger items (§3) → done: `src/components/ui/Menu.tsx` · verified: two submenu levels by hover in the gallery
- [x] **DS-08** · D0 · Toasts with Undo, command palette, accent picker, shortcut recorder (§3) → done: `src/components/ui/{Toast,CommandPalette,Pickers}.tsx` · verified: toast + Undo, Ctrl+K filtering in the gallery
- [x] **DS-09** · D0 · Island component with all states (22 + 4 notices), spring morph, 120 ms content cross-fade and audible-only waveform (§3, §5) → done: `src/components/island/{Island,presets}.tsx` · verified: opacity sampled in the browser (content holds at 0, fades in from ~120 ms after mount over ~180 ms), height equals content after the morph, one row after exit, no console errors across a state cycle (2026-09-21); geometry measured across all 26 states after the owner review: row items centred to 0.0 px, leading and round trailing items 18 px from their ends, body on the label column, waveform 44 × 18
- [x] **DS-10** · D0 · System-surface previews: tray menu with the permission-mode submenu, tray tooltip, jump list, notifications (first close, task finished with reply, budget, update, microphone blocked), Windows Hello, File Explorer menu; Island notices as Island states (§3) → done: `src/components/system/SystemSurfaces.tsx`, notices in `island/presets.tsx`, shown in the gallery · verified: side by side with the mockup's System surfaces section in the browser pane
- [x] **DS-11** · D0 · App shell: sidebar with 13 grouped items, page header, custom title bar with caption buttons and drag regions, single scroll area under the title bar (§1, §3) → done: `src/components/layout/{Shell,TitleBar}.tsx`, `src-tauri/tauri.conf.json` · verified: running Tauri app on Windows 11 (drag, maximize/restore, close; rounded corners kept)
- [x] **DS-12** · D0 · Motion per §5 with reduced-motion support (§5) → done: page stagger 30 ms per block capped at 12 (`styles/base.css`), dialogs scale .96 spring, palette drop, sheet 350 ms + content stagger, menus/toasts scale from anchor, submenus slide 6 px after 120 ms, button press .97, Island spring + cross-fade; in-app Motion setting (`lib/theme.tsx`, `data-motion` + `MotionConfig`) alongside Windows "Animation effects" · verified: computed delays and transforms in the browser, Island instant with Reduced (opacity 1 and final width after two frames) · note: onboarding step motion ships with the onboarding screens (UX-33)
- [x] **DS-13** · D0 · Components gallery page showing every component in both themes (§3) → done: `src/pages/Gallery.tsx` · verified: `pnpm dev` and the browser pane
- [x] **DS-14** · D0 · App logo as SVG and PNG, Windows icon set generated from it (§1) → done: `assets/icons/kivo.svg`, `kivo.png` (1024), `kivo-256.png`, `apps/kivo-app/src-tauri/icons/` · verified: shown in the window, taskbar and favicon
- [~] **DS-15** · M7 · Every Control Center screen matches the mockup, rebuilt from these components (per-page items are UX-19 to UX-31) (§1) → partial: every Control Center page and Settings tab is built from the design-system components and spot-checked in the dev app against the mockup (Brains → Context, Privacy, Appearance, Chat screenshots over CDP; fixes: tab overflow fade, theme-tile radio, empty-list lines) · missing: the full side-by-side review with the owner, light and dark (2026-09-24)
- [x] **DS-16** · M7 · Accessibility pass: contrast ≥ 4.5:1 in both themes and all accents, high-contrast mode, focus order (§6) → done: `--text-3` darkened and text-safe tokens added (`--acc-text`, `--green/orange/red-text`, `--on-acc`) so all text is ≥ 4.5:1 on every surface in both themes and all accents (controls ≥ 3:1; light Amber moved to `#BF7C00`); Windows contrast themes get system colours, visible edges and Highlight outlines (`@media (forced-colors: active)`); focus order follows the DOM, checked by axe · verified: `src/lib/contrast.test.ts` (6, reads `tokens.css`), the axe audits in the page tests (2026-09-24)
