# Voice UI Component Libraries, Animation Tech, and Design-System Bases for KIVO (Tauri v2 / WebView2)

Research date: 2026-09-21. GitHub star counts, licenses and last-push dates were pulled live via the GitHub API (`gh api repos/<owner>/<repo>`), and npm versions and licenses via `npm view` on the same date. Repo URLs are cited as sources for those numbers.

## Voice UI components: what ElevenLabs UI, LiveKit Agents UI, Pipecat, Vapi, siriwave, orb-ui and similar kits provide

### Takeaway
The two strongest sources are ElevenLabs UI (MIT) and LiveKit Agents UI (Apache-2.0). Both are shadcn-style copy-paste registries for React 19 and Tailwind v4, so their visual components can be borrowed and rewired to KIVO's own audio pipeline. ElevenLabs' components (orb, bar visualizer, waveform, transcript) are loosely coupled: they take plain volume getters or a MediaStream. LiveKit's are tied to `livekit-client` track types. Pipecat's kit is a full npm dependency tied to Pipecat transports, and siriwave is effectively unmaintained. None of these should be adopted as a runtime dependency. Borrow and adapt instead.

### Cited Findings
**ElevenLabs UI**
- Repo `elevenlabs/ui`: 2,392 stars, MIT, last push 2026-09-14, not archived. — [GitHub elevenlabs/ui](https://github.com/elevenlabs/ui)
- It describes itself as "a component library built on top of shadcn/ui" for "agent & audio applications, including orbs, waveforms, voice agents, audio players". It installs through `npx @elevenlabs/cli components add <name>` or `npx shadcn@latest add https://ui.elevenlabs.io/r/orb.json`. Prerequisites are shadcn/ui initialised and Tailwind configured. The README frames it around Next.js. — [elevenlabs/ui README](https://github.com/elevenlabs/ui)
- Audio and agent components in the registry: `orb`, `bar-visualizer`, `live-waveform`, `conversation`, `conversation-bar`, `message`, `response`, `transcript-viewer`, `speech-input`, `mic-selector`, `audio-player`, `scrub-bar`, `shimmering-text`, `matrix`. The registry also re-ships the standard shadcn primitives. — [registry directory](https://github.com/elevenlabs/ui/tree/main/apps/www/registry/elevenlabs-ui/ui)
- **Orb:** built on WebGL through `three`, `@react-three/fiber` (`Canvas`, `useFrame`) and `@react-three/drei` (`useTexture`) with a custom `shaderMaterial`. Its props are `colors`, `seed`, `agentState`, `getInputVolume()` and `getOutputVolume()`, so it is provider-agnostic: any volume source works. `useFrame` means it renders on every frame while mounted. — [orb.tsx source](https://raw.githubusercontent.com/elevenlabs/ui/main/apps/www/registry/elevenlabs-ui/ui/orb.tsx); [Orb docs](https://ui.elevenlabs.io/docs/components/orb)
- **BarVisualizer:** plain React/DOM driven by a `requestAnimationFrame` loop. It only updates state when volume changes significantly, and it has built-in state animations for `connecting`, `initializing`, `listening`, `thinking` and `speaking`, plus a fake-data demo mode. — [bar-visualizer.tsx](https://raw.githubusercontent.com/elevenlabs/ui/main/apps/www/registry/elevenlabs-ui/ui/bar-visualizer.tsx)
- **LiveWaveform:** Canvas 2D with DPR-aware resizing. No WebGL. — [live-waveform.tsx](https://raw.githubusercontent.com/elevenlabs/ui/main/apps/www/registry/elevenlabs-ui/ui/live-waveform.tsx)
- The companion SDK `@elevenlabs/react` is at v1.15.2 (MIT). It is not needed to use the UI components. — [npm @elevenlabs/react](https://www.npmjs.com/package/@elevenlabs/react)

**LiveKit Agents UI / components-react**
- Repo `livekit/components-js`: 462 stars, Apache-2.0, pushed 2026-09-21. It contains the packages `core`, `react`, `shadcn` and `styles`. `@livekit/components-react` is at v2.9.24 (Apache-2.0). — [GitHub livekit/components-js](https://github.com/livekit/components-js); [npm](https://www.npmjs.com/package/@livekit/components-react)
- **Agents UI** (`packages/shadcn`) is "a component library built on top of shadcn/ui", "built targeting React 19 (no forwardRef usage) and Tailwind CSS 4". Its components are `AgentSessionProvider`, `AgentControlBar`, `AgentTrackToggle`, `AgentTrackControl`, `AgentDisconnectButton`, `AgentChatTranscript`, `AgentChatIndicator` (thinking state), `AgentAudioVisualizerBar`, `AgentAudioVisualizerRadial`, `AgentAudioVisualizerWave`, `AgentAudioVisualizerAura` and `StartAudioButton`, plus a Grid visualizer and the blocks `agent-popup-01` and `agent-session-view-01`. — [Agents UI README](https://github.com/livekit/components-js/tree/main/packages/shadcn)
- The Aura visualizer renders through a bundled `ReactShaderToy` component (raw WebGL, no three.js). It imports `LocalAudioTrack`/`RemoteAudioTrack` from `livekit-client` and `AgentState` from `@livekit/components-react`. — [agent-audio-visualizer-aura.tsx](https://github.com/livekit/components-js/blob/main/packages/shadcn/components/agents-ui/agent-audio-visualizer-aura.tsx)

**Pipecat Voice UI Kit**
- Repo `pipecat-ai/voice-ui-kit`: 417 stars, **BSD-2-Clause**, pushed 2026-09-14. The npm package `@pipecat-ai/voice-ui-kit` is at v0.14.0 (pre-1.0). — [GitHub](https://github.com/pipecat-ai/voice-ui-kit); [npm](https://www.npmjs.com/package/@pipecat-ai/voice-ui-kit)
- It provides "Components, hooks and template apps for building React voice AI applications", a debug console, drop-in templates, Tailwind 4 and CSS-variable theming, and it recommends the Geist fonts. It requires `@pipecat-ai/client-js`, `@pipecat-ai/client-react` and a transport (Daily or SmallWebRTC). — [voice-ui-kit README](https://github.com/pipecat-ai/voice-ui-kit); [docs](https://voiceuikit.pipecat.ai)

**Vapi**
- Vapi does not ship an official UI kit. The web SDK repo (`VapiAI/client-sdk-web`, 68 stars, no SPDX license detected) is a client SDK. — [GitHub VapiAI/client-sdk-web](https://github.com/VapiAI/client-sdk-web)
- The community `VapiBlocks` (cameronking4) is MIT, has 85 stars, and was last pushed 2024-12-20, so it is stale. It offers copy-paste React/Tailwind components. — [GitHub VapiBlocks](https://github.com/cameronking4/VapiBlocks)
- **orb-ui** (`exprmntl/orb-ui`, MIT, 67 stars, pushed 2026-09-17, npm v0.8.1) is a React voice-orb library with adapters for Vapi, ElevenLabs, LiveKit, Pipecat, OpenAI Realtime, Gemini Live and custom stacks. Its themes are circle, bars, cloud, radial and debug. Its npm peer dependencies include `livekit-client`. — [GitHub orb-ui](https://github.com/alexanderqchen/orb-ui); [orb-ui.com](https://orb-ui.com/); [npm orb-ui](https://www.npmjs.com/package/orb-ui)

**siriwave / react-siriwave**
- `kopiro/siriwave`: 1,731 stars, MIT, last push **2024-06-18**. The npm package `siriwave` is at v2.4.0, last published 2023-10. It draws a pure-JS Canvas 2D Siri wave in "ios" (classic) and "ios9" styles. — [GitHub siriwave](https://github.com/kopiro/siriwave)
- `react-siriwave` v3.1.0 (MIT) was last published 2023-03. — [npm react-siriwave](https://www.npmjs.com/package/react-siriwave)

**Other kits**
- 21st.dev hosts community "voice powered orb" components. — [21st.dev](https://21st.dev/community/components/isaiahbjork/voice-powered-orb)

### Inferences
- ElevenLabs components suit KIVO best, because the orb and visualizers accept `getInputVolume`/`getOutputVolume` functions or a stream rather than a vendor session. KIVO can feed them from its own Rust or Web Audio analyser, emitted through Tauri events.
- The LiveKit visualizers need their `livekit-client` track types stripped out. This is feasible, since the shader code in ReactShaderToy is self-contained, but it takes more work.
- The Pipecat kit and orb-ui add transport or SDK peer dependencies that KIVO does not need.
- Apache-2.0 (LiveKit) and BSD-2 (Pipecat) both allow commercial use, but copied code must keep its license and NOTICE text.

### Gaps
- Exact gzipped bundle cost of the ElevenLabs Orb subtree (three + r3f + drei). I did not measure it. three.js alone is large, and the unpacked npm size of about 20 MB is not a bundle size.
- The ElevenLabs UI docs site (ui.elevenlabs.io) returned HTTP 403 to fetches, so I verified component details from source only.
- I found no official Vapi UI kit in 2026.

## Animation tech: Motion, Rive, Lottie, CSS/SVG, Canvas 2D, WebGL, and the idle cost of an always-available overlay

### Takeaway
For an always-on overlay, the dominant cost is any continuous render loop: `requestAnimationFrame`, `useFrame`, a CSS infinite animation, or a WebGL shader. In a Tauri/WebView2 window this loop can keep a discrete GPU at full power even when the window looks hidden. The pill or orb should therefore stop rendering entirely when idle and only run a loop while listening or speaking. Motion (MIT) is the right tool for state transitions. Canvas 2D or a small raw-WebGL shader suits the audio-reactive visual. Rive is a strong option if a designer authors the orb, but it costs a WASM download. Lottie is weaker for audio-reactive work.

### Cited Findings
**Motion (formerly Framer Motion)**
- `motiondivision/motion`: 33,672 stars, MIT, pushed 2026-09-16. `motion` is at npm v13.4.0. — [GitHub motion](https://github.com/motiondivision/motion); [npm motion](https://www.npmjs.com/package/motion)
- Bundle sizes:
  - Full `motion` component: 34 kb.
  - `m` component with `LazyMotion`: 4.6 kb initial.
  - `domAnimation`: +15 kb.
  - `domMax`: +25 kb.
  - `useAnimate` mini: 2.3 kb.
  - `useAnimate` hybrid: 17 kb.

  — [motion.dev: reduce bundle size](https://motion.dev/docs/react-reduce-bundle-size)

**Rive**
- `rive-app/rive-wasm`: 968 stars, MIT, pushed 2026-09-21. `@rive-app/canvas-lite` is at v2.42.2 and `@rive-app/react-webgl2` at v4.34.3, both MIT. — [GitHub rive-wasm](https://github.com/rive-app/rive-wasm); [npm canvas-lite](https://www.npmjs.com/package/@rive-app/canvas-lite)
- `@rive-app/canvas-lite` has "the same API and similar rendering capabilities" as `@rive-app/canvas` but a smaller package. It excludes text, audio, layouts and scripting. — [Rive web FAQ](https://rive.app/docs/runtimes/web/faq)
- Rive offers both Canvas and WebGL renderer variants. — [Rive: Canvas vs WebGL](https://help.rive.app/runtimes/overview/web-js/canvas-vs-webgl)
- One third-party estimate puts the web runtime at about 200 KB gzipped including WASM, against about 60 KB for lottie-web. It says Rive renders through Canvas or WebGL and bypasses browser layout. — [Unicorn Icons: Lottie vs Rive](https://unicornicons.com/blog/lottie-vs-rive-performance). This is a secondary source, and its lottie-web figure conflicts with the 84 KB gzipped figure below.
- **Data binding and state machines:** View Models expose typed properties (boolean, number, string, color, enum, trigger, and others) that the app sets at runtime and that can drive state-machine transitions. A mic-level number property is the natural way to make a Rive orb audio-reactive. — [Rive blog: Data Binding](https://rive.app/blog/data-binding-in-rive-a-shared-language-for-designers-and-developers); [Rive: getting started with data binding](https://rive.app/blog/getting-started-with-data-binding)
- React-specific Rive optimisation guidance, such as pausing off-screen instances, is covered by Pixel Point. — [Pixel Point: Rive React optimizations](https://pixelpoint.io/blog/rive-react-optimizations/)

**Lottie**
- `airbnb/lottie-web`: 32,115 stars, MIT, last push **2025-09-01**. npm v5.13.0 was last published 2025-05. — [GitHub lottie-web](https://github.com/airbnb/lottie-web)
- `LottieFiles/dotlottie-web`: 881 stars, MIT, pushed 2026-09-16, npm v0.80.0 (pre-1.0). It is a "High-performance Lottie & dotLottie web player powered by Rust + WASM, with React, Vue, Svelte, Solid & Web Components", built on ThorVG. — [GitHub dotlottie-web](https://github.com/lottiefiles/dotlottie-web); [How the dotLottie web player works](https://docs.lottiefiles.com/en/runtimes/distributions/js/v0.x/core-concepts)
- Size claims: the dotLottie WASM is about 500 KB compressed (roughly 700 KB gzipped .wasm per another report) plus about 57 KB of JS. It loads from a CDN by default, and self-hosting is discussed in the repo. lottie-web is 334 KB (84 KB gzipped). — [dotlottie-web discussion #198 (self-host WASM)](https://github.com/LottieFiles/dotlottie-web/discussions/198); [lottie-player issue #166](https://github.com/LottieFiles/lottie-player/issues/166). These figures come from issues and discussions and are only approximate.
- Lottie's SVG renderer triggers layout and paint on every frame, which is expensive for complex animations. Its Canvas renderer avoids that but loses SVG resolution independence. — [Unicorn Icons](https://unicornicons.com/blog/lottie-vs-rive-performance)

**Idle cost in WebView2 and Tauri**
- A Tauri app report: the WebView2 GPU process kept an NVIDIA dGPU at P0 (about 13 W, 1380 MHz) instead of P8 (about 3.8 W, 300 MHz) while the app was visually idle. It showed sustained `engtype_3d` usage, and the likely cause was a continuous rAF, CSS-animation or compositor loop. — [tablo issue #79](https://github.com/unravel-team/tablo/issues/79)
- Chromium in WebView2 only throttles animations once the controller's `IsVisible` is false. Tauri's `hide()` hides the OS window, but the WebView2 controller stays marked visible. The fix was to pair window visibility with WebView visibility at every show and hide site. — [tablo PR #80](https://github.com/unravel-team/tablo/pull/80)
- A known Tauri issue reports cases where canvas and CSS are not hardware-accelerated. — [tauri issue #4891](https://github.com/tauri-apps/tauri/issues/4891)

**three.js and react-three-fiber**
- `three` is at v0.186.0 and `@react-three/fiber` at v9.7.0, both MIT. — [npm three](https://www.npmjs.com/package/three); [npm @react-three/fiber](https://www.npmjs.com/package/@react-three/fiber)

### Inferences
- Recommended stack by layer:
  - **Idle pill:** static CSS with no infinite animation. It should cost zero frames at rest.
  - **Listening waveform or pill:** Canvas 2D, adapted from ElevenLabs LiveWaveform or BarVisualizer, driven by analyser data and running only while active.
  - **Premium orb:** either a single fragment shader in raw WebGL (the LiveKit ReactShaderToy approach, far lighter than three + r3f + drei) or a Rive file with a data-bound volume input.
  - **Transitions between states and in the Control Center:** Motion with `LazyMotion` and `m`.
- Avoid three.js plus r3f for the overlay. It pulls in a large dependency, and `useFrame` renders every frame unless `frameloop="demand"` is configured. That is acceptable for a demo but wasteful for an always-available overlay.
- Whichever tech is chosen, KIVO must explicitly pause the loop and set WebView2 visibility to false when the overlay is hidden, per the tablo findings.
- Lottie suits pre-baked icon or micro-animations (such as a thinking shimmer), not continuous audio-driven visuals. If KIVO uses it, dotLottie is the maintained path, but its WASM should be self-hosted for offline desktop use.

### Gaps
- I found no rigorous benchmark of idle CPU and GPU for Rive, Lottie, Canvas and WebGL specifically inside WebView2. This should be measured in a KIVO prototype, for example with Task Manager GPU engine columns or PresentMon.
- Rive audio-reactive input: data binding clearly supports runtime numeric inputs, but I found no official Rive example that is specifically audio-reactive.
- Exact current gzipped sizes of the Rive canvas-lite WASM are not published in the docs I fetched.

## Design-system bases, macOS-look kits, and fonts

### Takeaway
Use shadcn/ui (MIT, now defaulting to Base UI) on Tailwind v4 as the base for the Control Center. Both voice registries (ElevenLabs, LiveKit) are shadcn-native, so they drop in cleanly. Magic UI (MIT) is fine for borrowing effects. Treat Aceternity with care: its free components are reportedly MIT, but Pro items have redistribution restrictions. For fonts, use Inter or Geist (both OFL-1.1). SF Pro cannot legally be shipped in a Windows app.

### Cited Findings
**Component bases**
- `shadcn-ui/ui`: 124,292 stars, MIT, pushed 2026-09-21. — [GitHub shadcn-ui/ui](https://github.com/shadcn-ui/ui)
- shadcn timeline:
  - December 2025: `npx shadcn create` added a choice between Radix and Base UI.
  - January 2026: full Base UI documentation.
  - February 2026: all blocks available for both libraries.
  - July 2026: new projects default to **Base UI**. Radix remains fully supported and is not deprecated.

  — [shadcn changelog: Base UI default (July 2026)](https://ui.shadcn.com/docs/changelog/2026-07-base-ui-default); [Jan 2026](https://ui.shadcn.com/docs/changelog/2026-01-base-ui); [Feb 2026](https://ui.shadcn.com/docs/changelog/2026-02-blocks)
- Base UI: `mui/base-ui`, 10,960 stars, MIT, pushed 2026-09-21. The stable package is `@base-ui/react` v1.8.0. The older `@base-ui-components/react` stopped at 1.0.0-rc.0. — [GitHub base-ui](https://github.com/mui/base-ui); [npm @base-ui/react](https://www.npmjs.com/package/@base-ui/react)
- Radix Primitives: `radix-ui/primitives`, 19,308 stars, MIT, last push 2026-08-08. `@radix-ui/react-dialog` is at v1.1.23. — [GitHub radix primitives](https://github.com/radix-ui/primitives)
- React Aria: `adobe/react-spectrum`, 15,884 stars, **Apache-2.0**, pushed 2026-09-21. `react-aria-components` is at v1.21.1. — [GitHub react-spectrum](https://github.com/adobe/react-spectrum)
- Tailwind CSS: `tailwindlabs/tailwindcss`, 97,631 stars, MIT, pushed 2026-09-08. Both ElevenLabs UI and LiveKit Agents UI target Tailwind 4. — [GitHub tailwindcss](https://github.com/tailwindlabs/tailwindcss); [Agents UI README](https://github.com/livekit/components-js/tree/main/packages/shadcn)

**Effects libraries**
- Magic UI: `magicuidesign/magicui`, 22,346 stars, MIT, pushed 2026-09-20. — [GitHub magicui](https://github.com/magicuidesign/magicui)
- Aceternity UI: 200+ copy-paste React, Tailwind and Motion components. Its Pro licence bans redistributing source files, reselling, or building derivative templates for marketplaces. A secondary source says the free core is MIT. — [Aceternity licence](https://ui.aceternity.com/licence); [Aceternity components](https://ui.aceternity.com/components)

**macOS-look kits**
- Puppertino (`codedgar/Puppertino`): CSS framework following Apple HIG, 1,157 stars, MIT, pushed 2026-09-17. It is CSS-only and modular, with dark mode. — [GitHub Puppertino](https://github.com/codedgar/Puppertino)
- `react-desktop` has 9,482 stars (MIT) but was last pushed 2023-07. It is effectively dead. — [GitHub react-desktop](https://github.com/gabrielbull/react-desktop)

**Fonts**
- Inter (`rsms/inter`): OFL-1.1, 19,899 stars. — [GitHub inter](https://github.com/rsms/inter)
- Geist (`vercel/geist-font`): OFL-1.1, 3,627 stars, pushed 2026-07-14. It is recommended by the Pipecat kit. — [GitHub geist-font](https://github.com/vercel/geist-font); [Pipecat README](https://github.com/pipecat-ai/voice-ui-kit)
- SF Pro: Apple's font licence limits use to registered Apple developers building for Apple platforms (iOS, iPadOS, macOS, tvOS), including mock-ups for those platforms. It is not usable in a Windows app or as a web font. — [Apple Fonts](https://developer.apple.com/fonts/); [Apple Developer Forums thread](https://developer.apple.com/forums/thread/127350)

### Inferences
- The macOS feel is better achieved with shadcn/Base UI plus custom tokens (translucency via Windows Mica/Acrylic window effects, 12–16 px radii, subtle shadows, Inter or Geist) than with a dedicated macOS kit. Puppertino is useful only as visual reference.
- Base UI (MIT) is the current shadcn default. React Aria (Apache-2.0) is the strongest on accessibility but is not the shadcn base.
- OFL fonts can be bundled with the app. The copyright notice and licence must be retained, and the font may not be sold on its own.

### Gaps
- I did not verify Aceternity's free-tier license from a primary repo, since no public GitHub repo was found. This needs confirmation before copying any Aceternity component.
- I did not fetch the full text of the SF Pro licence, only Apple's fonts page and forum and community summaries.

## Framework choice for Tauri (React vs Svelte vs Solid) and voice-component availability; licenses summary

### Takeaway
React has essentially all of the voice-agent UI ecosystem: ElevenLabs UI, LiveKit Agents UI, Pipecat, orb-ui, and the Motion, Magic UI and Aceternity effects. Svelte and Solid have shadcn ports but only scattered, stale audio-visualizer code. Given KIVO's reliance on copy-paste voice components, React is the pragmatic choice. The overlay can still be a tiny separate React entry, or vanilla Canvas, to keep it light. All the main candidates are permissively licensed (MIT, Apache-2.0, BSD-2, OFL). Nothing blocks commercial redistribution except Aceternity Pro terms and SF Pro.

### Cited Findings
- `svelte` is at v5.57.1 and `solid-js` at v1.9.15, both MIT. — [npm svelte](https://www.npmjs.com/package/svelte); [npm solid-js](https://www.npmjs.com/package/solid-js)
- shadcn-svelte: 9,138 stars, MIT, pushed 2026-09-17. shadcn-solid (`hngngn/shadcn-solid`): 769 stars, MIT, pushed 2026-06-17. — [GitHub shadcn-svelte](https://github.com/huntabyte/shadcn-svelte); [GitHub shadcn-solid](https://github.com/hngngn/shadcn-solid)
- Svelte voice visuals:
  - `flo-bit/svelte-audio-visualizations`: 94 stars, MIT, last push 2024-12. It offers zero-dependency voice input and output visualizations.
  - A voice-orb pull request exists in `juspay/svelte-ui-components`.

  — [svelte-audio-visualizations](https://github.com/flo-bit/svelte-audio-visualizations); [juspay PR #602](https://github.com/juspay/svelte-ui-components/pull/602)
- dotLottie ships React, Vue, Svelte, Solid and Web Component wrappers, and Rive and Motion have vanilla JS APIs. The animation layer is therefore not React-locked. — [GitHub dotlottie-web](https://github.com/lottiefiles/dotlottie-web); [npm motion](https://www.npmjs.com/package/motion)
- ElevenLabs UI, LiveKit Agents UI, Pipecat and orb-ui are all React-only. LiveKit targets React 19 specifically. — [elevenlabs/ui](https://github.com/elevenlabs/ui); [Agents UI README](https://github.com/livekit/components-js/tree/main/packages/shadcn); [voice-ui-kit](https://github.com/pipecat-ai/voice-ui-kit); [npm orb-ui](https://www.npmjs.com/package/orb-ui)

**License summary**

| Project | License |
|---|---|
| ElevenLabs UI | MIT |
| shadcn/ui | MIT |
| Base UI | MIT |
| Radix | MIT |
| Magic UI | MIT |
| Motion | MIT |
| Rive runtimes | MIT |
| lottie-web | MIT |
| dotLottie | MIT |
| three.js | MIT |
| react-three-fiber | MIT |
| siriwave | MIT |
| orb-ui | MIT |
| Puppertino | MIT |
| Tailwind | MIT |
| LiveKit components | Apache-2.0 |
| React Aria | Apache-2.0 |
| Pipecat voice-ui-kit | BSD-2-Clause |
| Inter | OFL-1.1 |
| Geist | OFL-1.1 |
| Aceternity Pro | Proprietary, no redistribution of source |
| SF Pro | Apple-platforms only |

Sources: the repo and npm links cited above.

### Inferences
- **Recommendation matrix**
  - **Adopt as dependencies:** Tailwind v4, shadcn/ui on Base UI (CLI copy-in), Motion (use `LazyMotion`), and Inter or Geist fonts. Optionally add Rive `canvas-lite` or `webgl2` if a designer creates the orb in Rive.
  - **Borrow and adapt (copy-paste):**
    - ElevenLabs `bar-visualizer`, `live-waveform`, `conversation`, `message` and `transcript-viewer`, and `shimmering-text` for the "thinking" state.
    - The shader from the LiveKit Aura/ReactShaderToy for an orb, with `livekit-client` types removed. Keep the Apache-2.0 notice.
    - Optionally the ElevenLabs Orb shader, ported off three/r3f into raw WebGL.
  - **Build custom:** the always-on pill/orb overlay shell (idle = no render loop), the state machine (idle → listening → thinking → speaking), the Tauri-event-to-analyser bridge, and the visibility and throttle handling for WebView2.
  - **Avoid:** siriwave and react-siriwave (unmaintained since 2023/24), react-desktop (dead), Pipecat kit and orb-ui as dependencies (transport coupling), three.js plus r3f in the overlay, SF Pro, and Aceternity Pro code.
- Svelte or Solid would give a lighter runtime, but KIVO would lose the ready voice components and have to hand-port them. The overlay's performance is dominated by render-loop discipline, not framework overhead.

### Gaps
- I found no direct benchmark of React vs Svelte vs Solid memory and CPU inside a Tauri/WebView2 window.
- Apache-2.0 attribution mechanics for copy-pasted shadcn-registry code (per-file header vs a NOTICE file) should be confirmed by whoever owns legal and compliance.
