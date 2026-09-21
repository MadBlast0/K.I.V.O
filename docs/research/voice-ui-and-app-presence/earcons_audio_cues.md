# Earcons and Audio Cues for Voice Assistants (KIVO)

Research date: 2026-09-21. Scope: start/stop listening tones, error/confirmation, "thinking" cues, barge-in, design properties, self-trigger/echo, latency, settings, accessibility, and assets.

## 1. What do Siri, Google Assistant/Gemini, Alexa, Cortana, Windows voice typing, and Wispr Flow play, and can users turn the sounds off?

### Takeaway
Alexa is the best-documented example. It plays a Start of Request sound as soon as it detects the wake word or a button press, and an End of Request sound when the mic closes. Both are optional, and users switch them on in settings. Cortana played an "EarCon" only when the user said the wake word alone. Wispr Flow plays start and stop sounds by default and lets users turn them off, except on Android. The sounds used by Siri, Gemini, and Windows voice typing are poorly documented, and user reports disagree about them.

### Cited Findings
**Amazon Alexa**
- Alexa Auto guidance says: "Play the Start of Request sound immediately after the wake word is detected" and immediately after a push-to-talk or tap-to-talk press (file `med_ui_wakesound_hybrid`). It is "required to play when visual cues display the Listening state", so the sound and the visual cue appear together. — [Alexa Auto: Invoking Alexa](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html)
- The same page says: "Play the End of Request sound at the end of speech input" (`med_ui_endpointing`). It tells the customer that the assistant heard the request "without looking at the screen," and it must play when the visual cues leave the Listening state. — [Alexa Auto](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html)
- Multi-turn: "the Start of Request sound must play each time the mic opens during the interaction. The End of Request sound must play each time the mic closes." — [Alexa Auto](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html)
- Settings: "Allow customers to turn off the Start and End of Request sounds under the Settings menu." Alexa's sounds may only be used for Alexa features. — [Alexa Auto](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html)
- On Echo devices these request sounds are off by default. Users turn them on in the Alexa app under Device Settings > Sounds > Request Sounds (Start of Request / End of Request). — [Amazon: How to enable a Request Sound](https://www.amazon.com/b?ie=UTF8&node=21341310011); [groovyPost](https://www.groovypost.com/howto/amazon-echo-tip-enable-wake-up-sound/)
- Echo sound design (Chris Seifert, Amazon): most earcons come from 3 rising notes meant to suggest "A-ma-zon." The wake sound uses 2 of those notes. The third note plays as the end-pointing tone after the user finishes speaking. Amazon modeled the wake sound on a human "uh-huh" by studying how often it occurs, how long it lasts, and how loud it is compared with normal speech. **Users can talk over it without waiting**, so the conversation keeps flowing between the wake word and the command. — [Engadget: The secret behind Amazon Echo's alert sounds](https://www.engadget.com/amazon-echo-sounds-explained-160055039.html)
- Seifert says personalization is still unsolved because Echo is usually a shared device. — [Engadget](https://www.engadget.com/amazon-echo-sounds-explained-160055039.html)

**Cortana (Windows 10, now retired)**
- Microsoft's driver docs say: "*Keyword only* activation occurs when only the Cortana keyword is said, Cortana starts and plays the EarCon sound to indicate that it has entered listening mode." A "staged command" means: "Hey Cortana <pause, wait for EarCon sound> What's the weather?" A "chained command" ("Hey Cortana what's the weather?") needs no wait. — [Microsoft Learn: Voice Activation](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/voice-activation)
- Cortana has reached end of support. That doc covers Windows 10 1909 and earlier. — [Microsoft Learn: Voice Activation](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/voice-activation)

**Wispr Flow**
- iOS: start and stop sounds have their own switch under Wispr Flow > Settings > Interaction Sounds (Audio section). — [Wispr Flow Help: Manage notifications on iOS](https://docs.wisprflow.ai/articles/9454889914-how-to-disable-wispr-flow-notifications-on-ios)
- A search-result summary of Wispr help pages said the sounds are on by default and can be turned off on desktop (Mac/Windows) under Settings > System > Sound. On Android they are "always on and cannot be disabled." I could not open the source page directly. — [Wispr Flow Help Center](https://docs.wisprflow.ai/articles/6409258247-starting-your-first-dictation) (unverified; low confidence)
- Wispr also has an auto-mute feature that silences music while the user dictates on macOS. — [Wispr Flow Help: Auto-mute music](https://docs.wisprflow.ai/articles/7231650589-auto-mute-music-while-dictating-on-macos-how-audio-detection-works)

**Siri (iOS 18+)**
- Siri Responses settings (Settings > Apple Intelligence & Siri > Siri Responses) offer "Prefer Silent Responses" and "Always Silent." "Automatic" stays silent while the user is looking at the screen. — [Proton summary / Apple Support](https://support.apple.com/guide/iphone/change-siri-settings-iphc28624b81abc/ios) (the Apple page did not render fully when fetched)
- Apple Community users report that the activation beep depends on the Ring/Silent switch, and that the iOS 18 "intelligence" chime changed. These are anecdotal reports. — [Apple Community: Siri activation beep removed](https://discussions.apple.com/thread/255139748); [Apple Community: Siri intelligence chime](https://discussions.apple.com/thread/255929864)

**Google Assistant / Gemini (Android)**
- Many users report that the "Hey Google" chime or beep no longer plays when Gemini launches from the wake word or the side button. The cause is unclear (a bug or a removed feature). No setting for the activation chime was found. — [Samsung Community: Gemini activation sound](https://eu.community.samsung.com/t5/galaxy-s24-series/gemini-activation-sound/td-p/12224732); [Samsung Community: Hey Google chime disappeared](https://eu.community.samsung.com/t5/galaxy-s25-series/hey-google-chime-disappeared/m-p/11619079)
- "Hey Google" itself can be turned off in Gemini > Settings. — [Gemini Apps Help](https://support.google.com/gemini/answer/16938321?hl=en)

### Inferences
- The industry norm is a pair of sounds: one when the mic opens and one when it closes. Each sound is tied to a matching visual state and plays on every mic open or close in a multi-turn session. Alexa writes these rules down most clearly.
- "Optional, off by default on a smart speaker" (Echo) and "on by default, user can turn off" (Wispr) are both established patterns. KIVO is a desktop companion where the user may not be looking at the screen, so on by default with an easy toggle fits best. The toggle should stay available on every platform, because Wispr's Android lock-in draws complaints.
- Siri and Gemini seem to have been reducing activation sounds on phones, where the screen already shows state. That makes sense only when a visual indicator is certain to be seen. A desktop app in a tray or overlay cannot count on that.

### Gaps
- I could not find official documentation of the sound Windows 11 voice typing (Win+H) plays or whether it can be disabled. Search results had no dedicated setting.
- I found no official Google or Apple page that lists the exact listening, stop, and error earcons for Gemini Live or Siri in 2026.
- I did not verify Wispr Flow's desktop setting path from a page I could load.

## 2. Published guidance and HCI research (Alexa, Google, Apple, Material, Brewster, meta-analyses)

### Takeaway
Guidance agrees: use few sounds, keep them short, keep them distinct, and use them the same way every time. Don't use an earcon whose meaning users must be taught. Brewster's guidelines give concrete numbers for pitch range, note length, loudness, and gaps. A 2023 meta-analysis found abstract earcons less accurate and slower to recognize than auditory icons or speech. That is acceptable for a few well-learned state cues, but it argues against a large set of abstract sounds.

### Cited Findings
**Google Conversation Design (Actions on Google)**
- "Limit use to just a few sounds that are easily distinguishable so that users don't have to learn too many"; use them in moderation; "Use them consistently so that users learn to associate the sound with its context"; generally keep them extremely brief (greetings may run longer); match the brand and persona. — [Google Conversation Design: Earcons](https://conversation-design.web.app/conversational-components/earcons/); [developers.google.com](https://developers.google.com/assistant/conversation-design/earcons)
- "If you feel like you have to teach users what an earcon means, don't use an earcon." Earcons often add cognitive load instead of value. — [Google Conversation Design](https://conversation-design.web.app/conversational-components/earcons/)
- Google offers the Actions on Google Sound Library, hosted sounds referenced through SSML `<audio>`. — [Google Sound Library](https://developers.google.com/assistant/tools/sound-library)

**Apple HIG**
- Use system sound services for short sounds. People silence devices partly to avoid nonessential sounds such as keyboard sounds and other audible feedback. The system volume should govern final output. — [Apple HIG: Playing audio](https://developers.apple.com/design/human-interface-guidelines/patterns/playing-audio/)
- Feedback principle: combine color, text, sound, and haptics so people get the feedback "whether they silence their device, look away from the screen, or use VoiceOver." — [Apple HIG: Feedback](https://developers.apple.com/design/human-interface-guidelines/patterns/feedback/)

**Material Design (Google) sound guidelines**
- Material covers earcons (sounds for information, actions, or events), sound attributes, and sound choreography, and it publishes downloadable sound resources. The docs are CC BY 4.0. — [Material: Applying sound to UI](https://m2.material.io/design/sound/applying-sound-to-ui.html); [Material: Sound attributes](https://m2.material.io/design/sound/sound-attributes.html); [Internet Archive: Material Design sound resources](https://archive.org/details/material-design-sound-resources) (the pages loaded without body text, so I could not extract details)

**Brewster, Wright & Edwards, "Guidelines for the creation of earcons" (Glasgow)**
- Timbre: use musical instrument timbres that are easy to tell apart. An early experiment found musical timbres worked better than simple tones, and structured earcons worked better than unstructured bursts of sound. — [Brewster earcon guidelines](https://www.dcs.gla.ac.uk/~stephen/earcon_guidelines.shtml); [ResearchGate](https://www.researchgate.net/publication/284229636_Guidelines_for_the_creation_of_earcons)
- Pitch: maximum about 5 kHz, minimum about 125–150 Hz. If users must judge register in absolute terms, differences of 2–3 octaves work better. — [Brewster guidelines](https://www.dcs.gla.ac.uk/~stephen/earcon_guidelines.shtml)
- Rhythm: make rhythms as different as possible. Using a different number of notes in each is very effective. — [Brewster guidelines](https://www.dcs.gla.ac.uk/~stephen/earcon_guidelines.shtml)
- Note length: at least 0.0825 s. Notes as short as 0.03 s work for simple earcons of 1–2 notes. Up to 6 notes in 1 second has been usable. — [Brewster guidelines](https://www.dcs.gla.ac.uk/~stephen/earcon_guidelines.shtml)
- Loudness: between 10 dB and 20 dB above the background threshold. — [Brewster guidelines](https://www.dcs.gla.ac.uk/~stephen/earcon_guidelines.shtml)
- Leave a gap of about 0.1 s between earcons played in sequence. — [Brewster guidelines](https://www.dcs.gla.ac.uk/~stephen/earcon_guidelines.shtml)
- Blattner et al.: a motive should have no more than 4 notes, to keep compound earcons short. — [Sonification Handbook ch. 14 (via search summary)](https://sonification.de/handbook/download/TheSonificationHandbook-chapter14.pdf)

**Earcons vs auditory icons vs speech**
- A 2023 systematic review and meta-analysis (Auditory Perception & Cognition) found that for accuracy and reaction time, speech, spearcons, and hybrids beat auditory icons, and auditory icons beat earcons. Earcons also had the lowest subjective ratings. Heterogeneity was high. — [T&F: Auditory Icons, Earcons, Spearcons, and Speech meta-analysis](https://www.tandfonline.com/doi/abs/10.1080/25742442.2023.2219201)
- Definitions: auditory icons are real-world sounds with a natural link to what they represent. Earcons are abstract sounds with no such link. — [same](https://www.tandfonline.com/doi/abs/10.1080/25742442.2023.2219201)

### Inferences
- Suggested KIVO set of 3–5 sounds: listen-start, listen-stop/heard, error or cancel, and possibly confirmation (task done) and optionally a quiet "thinking" loop. Beyond that, the meta-analysis and Google both point to spoken or text feedback instead.
- Tell sounds apart by contour and note count, not by absolute pitch. Example: rising 2-note for start, falling or resolving single note for stop, low descending or dissonant 2-note for error. Brewster's rhythm and register guidance supports this.
- Brewster's numbers suggest roughly 80–150 ms per note, about 150–350 ms per cue, fundamentals between about 150 Hz and 5 kHz, and playback about 10–20 dB above ambient. These are my synthesis for design, not a standard.
- The Echo "uh-huh" approach (a short cue shaped like a natural backchannel that users can talk over) is a good model for the start sound.

### Gaps
- No authoritative source found giving a recommended duration in milliseconds for voice-assistant start or stop earcons. Commercial durations were not published.
- No published guidance found on "thinking" or processing loops in voice assistants (for example, whether Alexa uses a sound while processing). The best-documented processing indicators are visual (light rings).
- I found no published guidance on error earcons specifically for voice assistants. Google and Alexa lean on spoken error prompts.

## 3. How should earcons interact with the wake-word/VAD/STT pipeline? (self-trigger, echo, AEC, latency, barge-in)

### Takeaway
Play the start sound right away on wake, in sync with the visual listening state, and keep the mic streaming. Users will talk over it (chained commands). Keep the earcon out of the recognizer in two ways: send it through the render path that AEC uses as its reference, and gate or mark the earcon's time window in the VAD/STT audio. On Windows 11, KIVO can open a communications-mode capture stream and point the AEC reference at the render endpoint it plays on. AEC is also what makes barge-in over TTS possible.

### Cited Findings
- Alexa: start sound "immediately after the wake word is detected." End sound at end of speech, tied to visual state changes. — [Alexa Auto](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html)
- Echo's wake sound lets users "speak over it" without waiting. — [Engadget](https://www.engadget.com/amazon-echo-sounds-explained-160055039.html)
- Windows voice activation must support both staged commands (wait for the EarCon) and chained commands (command right after the keyword). — [Microsoft Learn: Voice Activation](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/voice-activation)
- Windows keyword pipeline: keep a circular "burst buffer" so all the audio that triggered the keyword is kept. Buffer the whole keyword plus 250 ms before it, and provide timestamps for where it starts and ends. Keyword-detector pins should buffer at least 5000 ms. Capture continues after detection so "no data after the keyword is lost." — [Microsoft Learn: Voice Activation](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/voice-activation)
- Windows speech capture APOs must provide AEC, AGC, and NS (16 kHz mono float output). HW keyword spotting in the working state needs AEC (since 1803). — [Microsoft Learn: Voice Activation](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/voice-activation)
- Windows 11 (build 22540+) AEC API: `IAcousticEchoCancellationControl::SetEchoCancellationRenderEndpoint` sets which render endpoint serves as the AEC reference for communications capture streams. NULL lets Windows choose, and the default is the default render device. — [Microsoft Learn: SetEchoCancellationRenderEndpoint](https://learn.microsoft.com/en-us/windows/win32/api/audioclient/nf-audioclient-iacousticechocancellationcontrol-setechocancellationrenderendpoint); [Windows-classic-samples AEC sample](https://github.com/microsoft/Windows-classic-samples/blob/main/Samples/AcousticEchoCancellation/README.md)
- WinRT equivalent: `AcousticEchoCancellationConfiguration.SetEchoCancellationRenderEndpoint`. — [Microsoft Learn (UWP)](https://learn.microsoft.com/en-us/uwp/api/windows.media.effects.acousticechocancellationconfiguration.setechocancellationrenderendpoint?view=winrt-26100)
- WASAPI loopback mode exists "primarily to support acoustic echo cancellation." — [Microsoft Learn: Loopback Recording](https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording)
- AEC is required for voice barge-in. Smart speakers cancel their own music and TTS while listening for the wake word or commands. — [VOCAL: AEC Barge-In](https://vocal.com/echo-cancellation/aec-barge-in/)
- Amazon patent literature: if a wake word is detected while only the device is playing ("single-talk"), that may mean the wake word came from residual echo, not from the user. AEC statistics can therefore suppress false self-triggers. — [USPTO 10586534: Voice-controlled device control using AEC statistics](https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/10586534)
- An open-source voice project's spec says: show activation feedback within 150 ms of wake acceptance, "Do not let playback feed back into wake recognition; serialize or suppress capture as required," and make volume and sound configurable. This is one project's requirement, not an industry standard. — [GitHub jerryfane/voice issue #2](https://github.com/jerryfane/voice/issues/2)
- Microsoft's Modern Standby Wake-on-Voice test requires system resume within 1 second of the voice command. That covers waking from low power, not earcon latency. — [Microsoft Learn: Voice Activation](https://learn.microsoft.com/en-us/windows-hardware/drivers/audio/voice-activation)

### Inferences (recommended KIVO pipeline behavior)
- **Don't pause capture to play the start earcon.** Keep a ring buffer (Windows-style 250 ms pre-roll plus continuous capture) so a chained command spoken over the tone is not lost. Pausing capture breaks "Hey KIVO, open ...".
- **Keep the earcon out of STT:** (a) play the earcon and TTS through the same render endpoint that AEC uses as its reference. On Windows 11, use a communications-category capture stream and `SetEchoCancellationRenderEndpoint`, or run an in-app AEC such as the WebRTC APM fed with the app's own output. (b) Also timestamp the earcon's playback window and have the VAD ignore speech onsets during that window, or trim that span from the audio sent to STT. This covers devices with weak or missing AEC (speakers plus a laptop mic, Bluetooth). (c) Keep the earcon short, well under 300 ms, and in a pitch band that endpointing won't mistake for speech. Tonal sounds with little broadband energy are easier to reject.
- **Self-triggering on the wake word:** suppress wake detection while KIVO's own TTS or earcon is playing unless AEC is confirmed active. Alternatively, raise the threshold during single-talk, following the Amazon patent idea.
- **Headphones vs speakers:** with headphones, echo is minimal and the earcon can play freely. With speakers, rely on AEC plus gating.
- **Latency targets (synthesis):** the start earcon and the visual listening state should appear "immediately" (Alexa's word), in practice within about 100–150 ms of wake or hotkey. Pre-load and decode the sounds, and use a low-latency shared-mode stream so no device-open delay applies. The 150 ms figure comes from one open-source spec. I found no official number.
- **Stop earcon:** play it at VAD endpoint or hotkey release, when the mic actually closes. In multi-turn follow-up mode, repeat start and stop on every mic open and close (Alexa rule).
- **Barge-in:** when the user interrupts TTS (detected through AEC plus VAD, or the hotkey), stop TTS at once and duck or fade over about 50–100 ms. Don't play a full start earcon over the user's speech. At most, use a very quiet short tick, or change the visual state only. (Design inference. No official source found on barge-in earcons.)
- **"Thinking" cue:** if used, make it very quiet, loopable, and non-rhythmic, and start it only after a delay (for example, if the response has not begun within about 1 s). Stop it the moment TTS starts. Keep it off by default or provide a separate toggle. (Inference. No authoritative source found.)

### Gaps
- No official Alexa, Google, or Apple number for wake-to-earcon latency.
- No official documentation on how commercial assistants remove their own earcon from the ASR stream (AEC vs gating vs trimming). The only direct evidence is patents and one open-source spec.
- I did not verify whether Windows 11 voice typing uses communications-mode AEC.

## 4. User settings, accessibility, and sound assets (free/licensable or custom)

### Takeaway
Give separate toggles for start, stop, and other cues, plus volume. Always pair sounds with visual state so deaf or muted users are covered, and pair visual state with sound for users who can't see the screen. CC0 packs from Kenney and Google's Material and Actions sound libraries are usable starting points. A custom 2–3-note motif gives KIVO a recognizable identity, as Amazon's "A-ma-zon" notes do for Echo.

### Cited Findings
- Alexa: "Allow customers to turn off the Start and End of Request sounds under the Settings menu." The two sounds are toggled separately in the Alexa app. — [Alexa Auto](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html); [Amazon Request Sounds](https://www.amazon.com/b?ie=UTF8&node=21341310011)
- The Wispr Flow toggle for interaction sounds is separate from its other audio settings. — [Wispr Flow Help](https://docs.wisprflow.ai/articles/9454889914-how-to-disable-wispr-flow-notifications-on-ios)
- Apple: respect silent mode and system volume. Multimodal feedback (color, text, sound, haptics) reaches people who have muted the device, are looking away, or use VoiceOver. — [Apple HIG: Feedback](https://developers.apple.com/design/human-interface-guidelines/patterns/feedback/); [Apple HIG: Playing audio](https://developers.apple.com/design/human-interface-guidelines/patterns/playing-audio/)
- End of Request sound: lets the user know they were heard "without looking at the screen," which is an accessibility and eyes-free benefit. — [Alexa Auto](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html)
- Assets: Kenney "UI Audio" (50 sounds) and "Interface Sounds" (100 sounds, including confirmations), CC0, free for commercial use. — [Kenney UI Audio](https://kenney.nl/assets/ui-audio); [Kenney Interface Sounds](https://kenney.nl/assets/interface-sounds)
- Material Design sound resources are downloadable, archived on Internet Archive. The docs are CC BY 4.0, so check the license on the audio files themselves. — [Internet Archive](https://archive.org/details/material-design-sound-resources); [Material: About sound](https://m2.material.io/design/sound/about-sound.html)
- Google Actions Sound Library: hosted sounds for Actions. Its terms were written for Actions, which have been deprecated, so check reuse rights. — [Google Sound Library](https://developers.google.com/assistant/tools/sound-library)
- Brand motif approach: the Echo earcons come from a 3-note motif, and the wake sound was modeled on the human "uh-huh." — [Engadget](https://www.engadget.com/amazon-echo-sounds-explained-160055039.html)
- Don't reuse another assistant's sounds. Alexa's terms limit its sounds to Alexa features. — [Alexa Auto](https://developer.amazon.com/en-US/docs/alexa/alexa-auto/invoking-alexa.html)

### Inferences (recommended KIVO settings)
- Settings > Sounds: master "Listening sounds" toggle (on by default for voice-first use), separate toggles for start, stop, error, and confirmation, a "thinking" sound toggle (off by default), a sound volume slider set relative to system volume, and a sound-set picker (for example "Subtle" and "Classic").
- Honor Windows Focus / Do Not Disturb and system mute. Consider automatically ducking the start earcon when headphones are not detected and AEC is unavailable.
- Accessibility: never convey state by sound alone. Pair every sound with an overlay, tray icon, or orb state, and add an optional screen-reader announcement (UIA live region) for listening started and stopped. Keep sounds out of harsh ranges: avoid more than about 5 kHz fundamentals and sharp transients, which also helps users with hyperacusis. Offer a "visual only" mode.
- For custom design: synthesize 2–3 short tonal notes (sine or soft mallet timbre with a short attack and about 50–150 ms decay). Use a rising contour for start, a resolving or falling contour for stop, and a lower two-note descending figure for error. Export at 48 kHz, peak-normalized well below 0 dBFS, with separate loudness-matched files. (Design inference drawing on Brewster.)

### Gaps
- I did not verify the exact license text on the Material Design audio files or the Google Sound Library.
- I found no WCAG success criterion specific to earcons. WCAG 1.4.2 Audio Control covers audio longer than 3 s, which could apply to a "thinking" loop, but I did not fetch it to confirm.
