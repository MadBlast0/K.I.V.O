# Visual Feedback Patterns for Voice Assistants and Dictation Tools (for KIVO, Windows-first)

Research date: 2026-09-21. Every claim below has an inline source. Where a detail comes from a secondary or unofficial source, the note says so.

## 1. Catalog of existing patterns (Apple, Google, Amazon, Microsoft, OpenAI, Wispr Flow, Superwhisper)

### Takeaway
The industry has settled on three visual families: (a) an **ambient edge glow** that wraps the display edges (Siri on iOS 18 to 26, and Gemini's newer fullscreen overlay); (b) a **compact pill or capsule**, usually bottom-center or growing out of a hardware or system anchor (Wispr Flow, the Gemini overlay pill, the Copilot Voice floating UI, Siri growing out of the Dynamic Island in iOS 27); (c) a **docked bar or small window** that shows commands or transcript text (Windows Voice Access bar, Win+H voice typing panel, Superwhisper's recording and mini windows). The trend for 2025 to 2026 is toward a hybrid: a small anchor that expands into a card or conversation surface, with the glow used as decoration on top.

### Cited Findings

**Apple Siri (iOS 18 to 26, macOS)**
- Apple's official description of the Apple Intelligence Siri: "an elegant glowing light that wraps around the edge of the screen when active on iPhone, iPad, or CarPlay". On Mac, "users can place Siri anywhere on their desktop". Users can type to Siri at any time and switch between text and voice. — [Apple Newsroom, June 2024](https://www.apple.com/newsroom/2024/06/introducing-apple-intelligence-for-iphone-ipad-and-mac/)
- The edge glow replaced the old circular orb at the bottom of the screen with colorful light around the device edges. — [PhoneArena](https://www.phonearena.com/news/with-ios-18-siri-learns-about-apple-devices-and-becomes-more-useful_id159252); [Pocket-lint](https://www.pocket-lint.com/how-to-get-new-siri-look-glowing-border/)
- Note for KIVO: on Mac, Apple did not use a full-screen edge glow as the main placement. It said users can put Siri anywhere on the desktop, meaning a movable floating element. — [Apple Newsroom, June 2024](https://www.apple.com/newsroom/2024/06/introducing-apple-intelligence-for-iphone-ipad-and-mac/)

**Apple Siri (iOS 27, 2026) and the Dynamic Island**
- In iOS 27, "a swirling Siri orb expands from the Dynamic Island". It is pill-shaped to hide the real camera cutouts. On iPad, Mac, and Vision Pro, Apple shows Siri as a circular orb. With the narrower Dynamic Island rumored for iPhone 18 Pro, it could become "a perfect circle". — [MacRumors, 2026-06-16](https://www.macrumors.com/2026/06/16/iphone-18-could-make-siri-a-circle/)
- Pre-WWDC reporting (Gurman, a rumor at the time): Siri "emerges from the Dynamic Island" and expands into a "Search or Ask" prompt with a glowing cursor. A standalone Siri app lists past conversations. This is described as a move away from the iOS 18 rainbow edge lighting. — [9to5Mac, 2026-04-19](https://9to5mac.com/2026/04/19/apple-has-already-teased-siris-new-design-coming-in-ios-27/)
- Apple's WWDC26 press release confirms a dedicated Siri app for revisiting past conversations, synced over iCloud, and on-screen awareness. It gives no visual specifics. — [Apple Newsroom, June 2026](https://www.apple.com/newsroom/2026/06/apple-unveils-next-generation-of-apple-intelligence-siri-ai-and-more/)

**macOS Dictation indicator**
- When dictation starts, a blue microphone icon appears above or near the text cursor, with animated sound-level bars that react to the voice. — [AbilityNet My Computer My Way (Sequoia)](https://mcmw.abilitynet.org.uk/how-to-use-dictation-in-macos-15-sequoia); [Spokenly guide](https://spokenly.app/blog/how-to-use-dictation-on-mac) (the second is a third-party blog)

**Google Gemini overlay (Android)**
- August 2025 redesign: the overlay "starts as a circle that quickly expands from the center of your screen into a wide pill". It holds a plus menu, a voice-input mic, a Gemini Live shortcut, and a drag handle to open the full app. It is triggered by "Hey Google", a long press on the power button, or a corner swipe. — [9to5Google, 2025-08-25](https://9to5google.com/2025/08/25/gemini-overlay-pill/)
- The glow around the overlay went from blue-purple to a four-color red, yellow, green, and blue gradient. It shows all four colors at launch and then settles to blue. — [9to5Google, 2025-08-06](https://9to5google.com/2025/08/06/new-gemini-overlay-glow-colors/)
- In November 2025, Google tested a fullscreen perimeter glow: the pill slides up from the bottom and colored "waves" run along the screen perimeter and reach toward the center. Coverage explicitly compares it to Apple Intelligence. Circle to Search, by contrast, "bathes the entire screen". — [9to5Google, 2025-11-24](https://9to5google.com/2025/11/24/gemini-overlay-fullscreen/)
- Voice input mode shows a waveform-like animation in the center. — [Neowin](https://www.neowin.net/news/google-rolls-out-a-new-glowing-gemini-overlay-for-android-devices/); [Android Authority teardown](https://www.androidauthority.com/gemini-overlay-live-neural-design-apk-teardown-3690991/)

**Amazon Alexa (Echo light ring and Echo Show light bar)**
- A solid blue ring with a cyan spotlight means listening, and the spot points toward the speaker. Blue spinning around the ring means processing. Slowly spinning teal and blue means starting up. Orange means setup mode. On Echo Show, the same meanings appear as a thin light bar along the top edge of the screen. — [How-To Geek](https://www.howtogeek.com/what-do-alexas-light-colors-mean/); [Android Central](https://www.androidcentral.com/what-all-color-rings-mean-your-amazon-echo); official Amazon page (returned HTTP 503 during this research): [Amazon Help](https://www.amazon.com/gp/help/customer/display.html?nodeId=GKLDRFT7FP4FZE56)
- The Echo Show top light bar is an early example, on a screen device, of a thin edge strip used as the listening indicator. — [How-To Geek](https://www.howtogeek.com/what-do-alexas-light-colors-mean/)

**Microsoft Copilot Voice on Windows ("Hey, Copilot")**
- When the wake word is detected, "the Copilot Voice Floating UI [appears] on the bottom of your screen" together with a chime, or with a voice greeting or response. To end: tap the X on the "Floating Call UI", or stay silent for a few seconds and it ends automatically, with a hang-up chime. "If you see the Copilot Voice interface, you are in a conversation." Windows also shows that Copilot is using the microphone in the system tray. The feature is opt-in. — [Windows Insider Blog, 2025-05-14](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)
- The October 2025 wave rolled out Copilot Voice, Vision, and Actions more broadly. Saying "Hey Copilot" produces a chime and a floating microphone UI, and an on-device wake-word spotter runs while the PC is unlocked. — [Windows Forum summary](https://windowsforum.com/threads/windows-11-copilot-goes-system-level-with-voice-vision-and-actions.385428/) (a community forum, so treat as secondary)

**Windows Voice Access bar**
- The Voice Access UI is a bar docked at the top of the screen. It shows mic status (listening, sleeping, or off), feedback on what it heard and whether it understood the command, and buttons for settings and help. In sleep mode it only reacts to "Voice access wake up". — [Microsoft Support: Get started with voice access](https://support.microsoft.com/en-US/accessibility/windows/voice-access/get-started-with-voice-access)
- The bar is fixed to the top of the main display and cannot be dragged. — [Spokenly blog](https://spokenly.app/blog/voice-access-windows-11) (third party; not verified against Microsoft docs)

**Windows 11 voice typing (Win+H)**
- Win+H opens a small floating panel with a mic button, a settings gear (auto punctuation, voice typing launcher, profanity filter), and a status line. Text is typed straight into the focused field, so the panel itself does not show a transcript. — [Microsoft Support: Use voice typing](https://support.microsoft.com/en-us/windows/use-voice-typing-to-talk-instead-of-type-on-your-pc-fec94565-c4bd-329d-e59a-af033fa5689f); [WindowsForum](https://windowsforum.com/threads/windows-voice-typing-fast-free-dictation-across-apps-with-win-h.380428/)

**ChatGPT Voice (orb, then an integrated mode)**
- The old Advanced Voice Mode opened a separate full-screen orb interface. It showed no live transcript during the conversation and had only three controls: mute, end, and interrupt. — [MacRumors, 2025-11-26](https://www.macrumors.com/2025/11/26/chatgpt-voice-mode-update-seamless-chat/); [Engadget](https://www.engadget.com/ai/now-you-can-use-chatgpt-voice-without-leaving-your-chat-195000538.html)
- On 2025-11-25, OpenAI moved voice into the chat itself. Responses now appear as live text, along with images and maps, while you talk. The old orb is still available under Settings > Voice Mode > Separate mode. — [MacRumors](https://www.macrumors.com/2025/11/26/chatgpt-voice-mode-update-seamless-chat/); [OpenAI Voice FAQ](https://help.openai.com/en/articles/8400625-voice-mode-faq)

**Wispr Flow (Flow Bar / pill)**
- Wispr Flow's desktop indicator is called the "Flow Bar", and its Android counterpart is the "Flow Bubble". The Flow Bar includes a mic switcher. — [Wispr Flow Help: Troubleshooting the Flow Bar](https://docs.wisprflow.ai/articles/5002934560-why-is-the-wispr-bar-is-not-appearing-or-disappearing)
- The pill is locked to bottom-center by default. A third-party macOS utility, PillFloat, exists only to let users move it, which shows that some users want to reposition it. — [PillFloat GitHub](https://github.com/OrangeAKA/pillfloat); [Product Hunt](https://www.producthunt.com/products/pillfloat)
- A detailed "Wispr-Flow-grade" pill spec from an open-source project (elizaOS). It describes their own design modeled on Wispr, not Wispr's official spec:
  - Size and placement: a 64x32 pill in a 96x56 window, bottom-center, always on top.
  - States: dim pulse while booting, solid white when idle, red with a live waveform while listening, and a warm glow that "breathes" while responding.
  - Behavior: "red + animated bars = mic is hot"; a flat waveform honestly signals a dead mic; hold a hotkey to talk and release to send; no focus stealing; a start "ping" sound; Esc cancels.
  - A tier model: glance, then talk, then summon a 560x640 panel that grows upward from the pill.
  - [elizaOS issue #20483](https://github.com/elizaOS/eliza/issues/20483)

**Superwhisper**
- Full recording window:
  - A live waveform confirms the mic is capturing audio, and a mode label can be switched with a shortcut.
  - Stop uses the same shortcut that started recording. Esc cancels; recordings over 30 seconds ask for confirmation first.
  - Optional live transcription with realtime models, and an indicator when clipboard or selected text is being captured as context.
- Mini window: the same core controls in a smaller space. It can stay visible even when idle, as a persistent indicator. Hovering shows the mode, record, and expand options, and right-clicking opens settings and history.
- Source: [Superwhisper docs: Recording Window](https://superwhisper.com/docs/get-started/interface-rec-window)

### Inferences
- The pill or capsule is currently the dominant pattern for desktop dictation and voice tools (Wispr, Superwhisper mini, Copilot Voice, Win+H). The edge glow mostly lives on phones, where the screen is small and the whole device is "the assistant". Even Apple chose "place Siri anywhere on the desktop" for Mac, and by iOS 27 was moving iPhone toward a Dynamic Island origin.
- Almost every product pairs the visual cue with an audio cue (Copilot's chime, Wispr's ping). Audio start and stop cues are standard practice.

### Gaps
- I found no official pixel dimensions for the Copilot Voice floating UI, the Win+H panel, or Wispr's real pill. The elizaOS numbers are a clone's spec.
- I did not verify Pixel Assistant's legacy UI or Alexa+ on Echo Show screen UI in detail. The only screen-UI detail I confirmed is the top-edge light bar.
- I found no primary source for how the macOS Siri glow behaves on multiple displays.

## 2. How assistants show states, partial transcripts, and response text

### Takeaway
State is carried by color plus motion plus sound, not by text labels. Listening uses a live, audio-reactive waveform or glow. Thinking uses a looping or spinning motion (Alexa's spinning blue, the "breathing" glow). Speaking uses a pulse synced to the voice. Showing what the system heard is the single most important feedback for trust: NN/g found that users repeat themselves when no transcript is shown. Answers are increasingly shown as live text beside the voice (ChatGPT, November 2025).

### Cited Findings
- State palettes:
  - Alexa: cyan spot on blue means listening, spinning blue means processing, orange means setup. — [How-To Geek](https://www.howtogeek.com/what-do-alexas-light-colors-mean/)
  - Wispr-style pill: dim pulse, white idle, red with waveform while listening, warm breathing glow while responding. — [elizaOS #20483](https://github.com/elizaOS/eliza/issues/20483)
  - Voice Access: listening, sleeping, or off, plus command feedback and execution status shown in the bar. — [Microsoft Support](https://support.microsoft.com/en-US/accessibility/windows/voice-access/get-started-with-voice-access)
- Conversation design practice separates a "leading cue", which shows the device started listening (for example waves pulsing with the voice), from an "ending cue", which shows it stopped listening and is processing. Both can be visual, auditory, haptic, or motion cues. — [Medium, Shimokobe, on conversation design](https://medium.com/@takashimokobe/the-evolution-of-conversation-design-chatbots-voice-user-interfaces-and-google-duplex-815d7bee2233) (secondary; it summarizes Google's conversation design material)
- Google's conversation design guidance: prompts and chips should exist for every dialog turn. Cards and carousels are for detail and are not needed on every turn. — [Google Conversation Design](https://developers.google.com/assistant/conversation-design/scale-your-design)
- NN/g, on transcripts: one participant repeated herself because she "didn't see transcription on screen" and thought Siri hadn't heard her. Participants were unsure whether they were still inside a skill, which is a system-status failure. — [NN/g: Intelligent Assistants Have Poor Usability](https://www.nngroup.com/articles/intelligent-assistant-usability/)
- NN/g, on how answers are delivered:
  - Users liked getting a spoken answer together with on-screen confirmation, and one called that combination magical.
  - Users disliked being handed links instead of an answer.
  - Long spoken answers overloaded working memory, and there was no repeat option.
  - Confirming every single item became tedious.
  - Source: [NN/g](https://www.nngroup.com/articles/intelligent-assistant-usability/)
- NN/g's overall finding: assistants work well only for simple queries with short answers. — [NN/g](https://www.nngroup.com/articles/intelligent-assistant-usability/)
- Live partial transcript examples:
  - Superwhisper shows live transcription with realtime models. — [Superwhisper docs](https://superwhisper.com/docs/get-started/interface-rec-window)
  - ChatGPT's integrated voice shows a live transcript of the assistant's reply. — [MacRumors](https://www.macrumors.com/2025/11/26/chatgpt-voice-mode-update-seamless-chat/)
  - Voice Access shows the recognized command in its bar. — [Microsoft Support](https://support.microsoft.com/en-US/accessibility/windows/voice-access/get-started-with-voice-access)
- Silence between "I stopped talking" and "the agent starts talking" is ambiguous. One practitioner maps Nielsen's response-time limits onto voice agents: about 100 ms feels instant, about 300 ms feels alive, and about 800 ms feels dead. — [DEV Community](https://dev.to/kenimo49/nielsens-3-ux-cliffs-mapped-to-voice-ai-100ms-feels-instant-300ms-alive-800ms-dead-3l6m) (a practitioner blog, not NN/g itself)
- A US patent (assignee not verified) describes showing the speech-recognition indicator only during active listening and hiding it during passive (wake-word) listening, so it does not pull focus from content. — [USPTO patent 9721587](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9721587)
- Dismissal patterns:
  - Copilot: X button, or auto-end after a few seconds of silence with a hang-up chime. — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)
  - Superwhisper: Esc to cancel, with a confirmation for recordings over 30 seconds. — [Superwhisper docs](https://superwhisper.com/docs/get-started/interface-rec-window)
  - Gemini: the glow fades to blue and then disappears. — [9to5Google](https://9to5google.com/2025/11/24/gemini-overlay-fullscreen/)

### Inferences
- Suggested state map for KIVO:
  - Idle: hidden, or a tiny dot.
  - Wake detected or listening: accent color with an audio-reactive waveform, plus a start chime.
  - Thinking: a slow looping shimmer or "breathing", plus an ending cue sound.
  - Acting (running tools): a distinct color or icon and a one-line step label, for example "Opening Spotify…".
  - Speaking: a pulse synced to the TTS amplitude, with captions.
  - Needs confirmation: amber, with explicit Yes/No buttons and a spoken prompt. It stays until answered and never auto-dismisses.
  - Error: red or amber, with a text reason. A flat waveform is itself an honest "mic dead" signal.
- Show the final recognized text ("what I heard") every turn. This is NN/g's main fix and is cheap to do.

### Gaps
- I found no published official state spec (colors and timings) for Copilot Voice or Siri's "thinking" versus "listening" glow.
- I did not locate the specific Google Material or Android guidance on the voice-input visual beyond the conversation design pages. That page was not fetched in full.

## 3. Desktop trade-offs: edge glow vs pill vs corner card

### Takeaway
On a Windows desktop, a full-screen edge glow is the most eye-catching option but the worst fit. It is peripheral motion across the whole screen, it is ambiguous on multiple monitors, it clashes with fullscreen games, and it cannot hold text. A bottom-center pill is small, anchored, and easy to recognize, and it is the pattern desktop users already know from Copilot, Win+H, and Wispr. A corner card is needed whenever text (transcript, answer, confirmation) must be shown. The best practice is a pill that expands into a card.

### Cited Findings
- WCAG 2.3.3 (AAA): motion triggered by an interaction must be able to be turned off unless it is essential. Motion can trigger vestibular symptoms such as dizziness and nausea. Honor the OS reduced-motion setting and also offer an in-app toggle. — [W3C Understanding 2.3.3](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html)
- Precedent for OS-level voice UI on Windows:
  - The Copilot floating UI sits at the bottom of the screen. — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)
  - The Voice Access bar is docked at the top. — [Microsoft Support](https://support.microsoft.com/en-US/accessibility/windows/voice-access/get-started-with-voice-access)
  - Win+H is a small floating panel. — [Microsoft Support](https://support.microsoft.com/en-us/windows/use-voice-typing-to-talk-instead-of-type-on-your-pc-fec94565-c4bd-329d-e59a-af033fa5689f)
  - None of the three uses a screen-edge glow.
- Users want control over where the pill sits: a whole third-party tool exists just to move Wispr's fixed bottom-center pill. — [PillFloat](https://github.com/OrangeAKA/pillfloat)
- "No focus steal" is a core requirement for a desktop voice pill. The cursor and keyboard focus must stay in the user's app. — [elizaOS #20483](https://github.com/elizaOS/eliza/issues/20483)
- Voice Access fixes its bar to the main display only, which avoids ambiguity but ignores where the user is working. — [Spokenly](https://spokenly.app/blog/voice-access-windows-11) (third party)
- Apple Mac precedent: Siri can be placed anywhere on the desktop, which is a movable element, not a glow. — [Apple Newsroom](https://www.apple.com/newsroom/2024/06/introducing-apple-intelligence-for-iphone-ipad-and-mac/)
- Privacy and trust: Windows shows mic-in-use in the system tray whenever an assistant listens. This is a system cue KIVO gets for free but should not rely on alone. — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)

### Inferences (trade-off analysis; my reasoning, not sourced claims)

**Edge glow**
- Pros: unmistakable; costs no screen area; reads as "the whole PC is listening", which suits a companion's personality.
- Cons:
  - Peripheral motion across a whole large or high-DPI monitor is very distracting.
  - It carries no text, so NN/g's transcript need goes unmet.
  - On multiple monitors you must pick a screen or glow on all of them.
  - Over exclusive-fullscreen games (DirectX exclusive mode) an overlay window may not render at all. Borderless games are covered, which risks breaking immersion or being mistaken for game UI.
  - Colored glows can clash with content, and the glow has no contrast guarantee.
  - Screen readers get nothing from it.
- Use it at most as an optional short "wake flash" of about 300 to 600 ms, only on the active monitor.

**Pill or capsule**
- Pros:
  - Small, and bottom-center matches Copilot, Win+H, and Wispr.
  - Can carry a waveform and state color.
  - Can be made keyboard- and UIA-accessible.
  - Easy to render at every DPI (vector).
  - Easy to exclude from screen capture and to hide in fullscreen.
- Cons:
  - Can overlap the taskbar, video captions, or game HUDs.
  - Too small to hold transcript or answer text.

**Corner card with live transcript**
- Pros: holds partial transcript, "what I heard", the answer, and confirmation buttons; reads well with screen readers through live regions.
- Cons: covers content; a fixed corner can collide with notifications (Windows toasts appear bottom-right); more visual weight.

**Multi-monitor**
- Show on the monitor holding the foreground window, or the monitor under the cursor, not always the primary. Allow a pinned-monitor option.

**Fullscreen and games**
- Detect fullscreen or D3D exclusive mode and the Windows "do not disturb" / "Focus" state. In those cases switch to audio-only cues plus the tray icon, or at most a tiny top-center pill.

**Accessibility**
- Never rely on color alone: pair each color with a shape or icon and short text.
- Meet 3:1 contrast for graphical indicators (WCAG 1.4.11; I did not fetch the source for this, so it is my knowledge).
- Announce state changes through UI Automation live-region notifications.
- Offer sound cues for blind users and captions for deaf users.
- Honor the Windows "Animation effects" setting, which is the reduced-motion equivalent.

**Discoverability**
- A persistent tiny idle dot or tray icon (like Superwhisper's always-on mini window) helps first-time users. It should be hideable.

### Gaps
- I found no controlled HCI study that compares edge glow, pill, and card on desktop directly. The trade-offs above are inferred from product precedent and general accessibility guidance.
- I did not verify how Windows' "Animation effects" setting maps into WinUI, Electron, or Tauri apps. That needs a check of platform docs.
- I did not confirm overlay behavior over exclusive-fullscreen games in any source.

## 4. Recommendation for KIVO (Windows desktop)

### Takeaway
Use a hybrid **"pill that expands into a card"** as the primary design. Keep an optional, brief, subtle edge glow on the active monitor only as a personality accent that can be turned off. Do not use a full-time glow as the main indicator.

### Cited Findings
- Hybrid precedents:
  - Wispr-style tiers: glance at the pill, talk, then summon a panel that grows upward from it. — [elizaOS #20483](https://github.com/elizaOS/eliza/issues/20483)
  - Siri iOS 27: an orb expands from the Dynamic Island into a prompt. — [MacRumors](https://www.macrumors.com/2026/06/16/iphone-18-could-make-siri-a-circle/)
  - Gemini: a circle expands into a pill, with the glow as an accent. — [9to5Google](https://9to5google.com/2025/08/25/gemini-overlay-pill/)
  - ChatGPT: text inline with the voice. — [MacRumors](https://www.macrumors.com/2025/11/26/chatgpt-voice-mode-update-seamless-chat/)
- Always show what was heard, and keep spoken answers short with on-screen detail. — [NN/g](https://www.nngroup.com/articles/intelligent-assistant-usability/)
- Pair the visual cue with a start chime and an end chime, and auto-end after silence. — [Windows Insider Blog](https://blogs.windows.com/windows-insider/2025/05/14/copilot-on-windows-hey-copilot-begins-rolling-out-to-windows-insiders/)
- Motion must be able to be turned off. — [W3C 2.3.3](https://www.w3.org/WAI/WCAG22/Understanding/animation-from-interactions.html)

### Inferences (proposed spec; design judgment)
1. **Anchor**
   - Place a capsule of about 120 to 160 x 36 to 40 effective px, bottom-center, just above the taskbar, on the monitor of the foreground window.
   - It is draggable, and it remembers position per monitor.
   - It is a non-activating, topmost, click-through-when-idle window, so it never steals focus.
   - When idle it is hidden, or shows an optional 8 to 12 px dot.
2. **States inside the capsule**
   - Each state is shown by icon, color, motion, and a short label or tooltip together.
   - Listening: an audio-reactive waveform in the accent color.
   - Thinking: a slow shimmer.
   - Acting: a tool icon and a one-line step.
   - Speaking: a pulse synced to TTS amplitude.
   - Needs confirmation: amber.
   - Error: red, with a reason.
   - Muted or push-to-talk-off: a slashed mic.
3. **Expansion card**
   - The card grows upward from the capsule, to about 360 to 420 px wide.
   - It shows the live partial transcript (in gray) that locks in as the final "You said…" text (in solid).
   - It then streams the answer text and shows confirmation buttons (Yes / No / Edit).
   - It auto-collapses a few seconds after the answer. It does not collapse while a confirmation is pending, or while the mouse is over it.
   - Esc cancels, and the global hotkey toggles it.
4. **Optional wake glow**
   - A thin (2 to 4 px) gradient along the bottom edge, or all edges, of the active monitor only.
   - It lasts about 400 ms on wake and then hands off to the capsule.
   - It is off automatically under reduced motion, Focus/DND, or fullscreen apps and games.
5. **Fullscreen and games mode**: audio cues only, plus a tray icon state, with an optional tiny top-center pill.
6. **Accessibility**
   - Send UIA notifications for state changes and final transcripts.
   - Make every control reachable by keyboard.
   - Meet 3:1 contrast for indicators and 4.5:1 for text.
   - Support Windows high-contrast themes.
   - Offer a sound-cue toggle and caption text for all TTS.
7. **Privacy**: keep the mic-hot state (red or accent waveform) distinct from wake-word standby, and never animate during passive wake-word listening. The latter follows the active versus passive distinction in [USPTO 9721587](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/9721587).

### Gaps
- This recommendation should be checked with quick user tests (glow on versus off, and bottom-center versus corner) on real multi-monitor and gaming setups. I found no public study that settles it.
