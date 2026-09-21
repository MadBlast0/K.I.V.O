# Speaker Enrollment and Voice Personalization for KIVO

Scope: local speaker verification (owner-gating) and STT personalization (accent, vocabulary) for a Rust, voice-first desktop companion. Windows 11 first, then Windows 10, macOS and Linux later. Researched September 2026. Every claim below has a source except items in Inferences and Gaps.

## 1. Which speaker verification models can run locally (size, CPU latency, EER, license, ONNX/Rust)?

### Takeaway
Modern open speaker-embedding models all reach below 1% EER on VoxCeleb1-O. The practical local choice for KIVO is a CAM++ (3D-Speaker) or WeSpeaker ResNet34 ONNX model, run through the sherpa-onnx speaker-identification API. Both are small (about 7M params), permissively licensed (Apache-2.0 code, with weights under the CC-BY-4.0 VoxCeleb data license), and already packaged as ONNX. Picovoice Eagle is a commercial alternative that needs a paid license for any commercial product.

### Cited Findings
- **3D-Speaker (Alibaba/ModelScope) VoxCeleb1-O benchmark:** Res2Net 4.03M params, 1.56% EER; ResNet34 6.34M, 1.05%; ECAPA-TDNN 20.8M, 0.86%; ERes2Net-base 6.61M, 0.84%; **CAM++ 7.2M, 0.65%**; ERes2NetV2 17.8M, 0.61%; ERes2Net-large 22.46M, 0.52%. Repo license is Apache 2.0. ONNX Runtime inference scripts were released in April 2024. — [3D-Speaker GitHub](https://github.com/modelscope/3D-Speaker)
- **SpeechBrain ECAPA-TDNN (spkrec-ecapa-voxceleb):** 0.80% EER on the cleaned VoxCeleb1 test set. Apache 2.0. Trained on VoxCeleb1+2. Verification uses cosine similarity between embeddings, with 16 kHz mono input. — [HF speechbrain/spkrec-ecapa-voxceleb](https://huggingface.co/speechbrain/spkrec-ecapa-voxceleb)
- A separate comparison reports ECAPA-TDNN at 6.19M params with EER 1.03% (Vox1-O), 1.06% (Vox1-E) and 2.05% (Vox1-H). It says TitaNet has the best reported VoxCeleb1 EER but is slower at producing embeddings, and that ECAPA gets similar results with fewer parameters and less training data. — [MDPI Applied Sciences 2024, "Comparison of Modern Deep Learning Models for Speaker Verification"](https://www.mdpi.com/2076-3417/14/4/1329)
  - Note the conflict: parameter counts for "ECAPA-TDNN" range from 6.2M to 20.8M across sources because the channel width varies (512 vs 1024 channels). Always check which variant a number refers to.
- **NVIDIA TitaNet-Large:** 23M params, 0.66% EER on VoxCeleb1, CC-BY-4.0, 16 kHz mono. Diarization DER on AMI MixHeadset is 1.73%. — [HF nvidia/speakerverification_en_titanet_large](https://huggingface.co/nvidia/speakerverification_en_titanet_large)
- **WeSpeaker:** ships pretrained VoxCeleb models (ResNet34/152/221/293, CAM++, ECAPA-TDNN 512/1024, Gemini DFResNet114, W2V-BERT2.0), CNCeleb ResNet34, and multilingual VoxBlink2 SimAM-ResNet models. Most come as both .pt checkpoints and **.onnx runtime models**. The weights inherit the dataset license, so VoxCeleb-trained models are CC BY 4.0. Models with the "_LM" suffix are large-margin fine-tuned and do better on audio longer than about 3 s. — [WeSpeaker pretrained.md](https://github.com/wenet-e2e/wespeaker/blob/master/docs/pretrained.md)
- **sherpa-onnx speaker identification:** runs speaker-embedding models from WeSpeaker (prefix "wespeaker-") and 3D-Speaker (prefix "3dspeaker-"), e.g. `3dspeaker-campplus_sv_en_voxceleb_16k`. Prebuilt models are on the `speaker-recongition-models` release tag. The engine uses onnxruntime, works fully offline, runs on many platforms, and has bindings for 12 programming languages. — [sherpa-onnx speaker identification docs](https://k2-fsa.github.io/sherpa/onnx/speaker-identification/index.html); [sherpa-onnx GitHub](https://github.com/k2-fsa/sherpa-onnx); [python example](https://github.com/k2-fsa/sherpa-onnx/blob/master/python-api-examples/speaker-identification.py)
- **Picovoice Eagle:** on-device streaming speaker recognition that enrolls "in seconds from any natural speech". The Free plan is limited to "personal non-commercial projects" and includes 100 min/month of Eagle. The Foundation plan costs $6,000/yr for 10K min/month, and Enterprise costs $30,000/yr. Every install needs an AccessKey that is checked against account limits. — [Picovoice pricing](https://picovoice.ai/pricing/); [Eagle docs](https://picovoice.ai/docs/eagle/); [Eagle product page](https://picovoice.ai/products/voice/speaker-recognition/)

### Inferences
- For KIVO, **CAM++ (7.2M, 0.65% EER)** has the best accuracy for its size among the models checked. WeSpeaker ResNet34-LM is the fallback. Both are already available as ONNX files that sherpa-onnx loads. KIVO can call sherpa-onnx's C API from Rust, or load the ONNX file directly with the `ort` crate and compute fbank features itself.
- At about 7M params, CPU inference on a 2–3 s utterance should take a few tens of ms on a modern x86 core. No CPU latency figure was found in the sources, so this is an estimate. Measure it on target hardware.
- VoxCeleb EER measures celebrity interview audio. The false-accept and false-reject rates KIVO actually gets on its own mic/room/household will differ. The threshold has to be calibrated on KIVO-like data, not taken from benchmark numbers.
- Eagle's AccessKey and per-minute metering conflict with KIVO's offline-first, always-listening design, and the pricing is steep for a consumer app. Avoid it unless an enterprise tier is negotiated.
- CC-BY-4.0 weights (WeSpeaker, TitaNet) require attribution in the app's credits or licenses screen. That is compatible with commercial use.

### Gaps
- No verified CPU latency or RTF numbers were found for CAM++, ECAPA or TitaNet on consumer CPUs.
- **Resemblyzer** (GE2E d-vector) and **pyannote embedding** models were not checked in this pass. From memory (unverified): Resemblyzer is Apache-2.0 with a 256-dim embedding and older, weaker accuracy, and pyannote's embedding model is a gated HF model that wraps WeSpeaker ResNet34. Verify before relying on either.
- It is unverified whether sherpa-onnx now ships an official Rust crate or whether KIVO must use FFI to its C API.
- TitaNet-Small's size and EER were not found. No official ONNX export was confirmed for TitaNet, though NeMo supports ONNX export in general.

## 2. How Apple, Google and Amazon enroll users (utterances, phrases, adaptation, thresholds, failure)

### Takeaway
All three use a short prompted enrollment: Apple 5 phrases, Google 4, Amazon 10. The phrases mix the bare wake word with wake word + command. All three, or at least Apple and Google, keep updating the profile from accepted real-world utterances (implicit enrollment) or offer an explicit "retrain". Google says outright that a recording or a similar voice may fool Voice Match. The vendors use it to personalize results, not as strong authentication.

### Cited Findings
- **Apple "Personalized Hey Siri" (PHS):** explicit enrollment is 5 utterances: "Hey Siri" x3, "Hey Siri, how is the weather today?", "Hey Siri, it's me." — [Apple ML Research: Personalized Hey Siri](https://machinelearning.apple.com/research/personalized-hey-siri)
- Apple **implicit enrollment:** the profile starts with the 5 speaker vectors from explicit enrollment. The system then adds the latest accepted speaker vector until the profile holds **40 vectors**. It also stores the matching utterance waveforms so profiles can be recomputed when the model is updated over the air. — [Apple ML Research: Personalized Hey Siri](https://machinelearning.apple.com/research/personalized-hey-siri)
- Apple model: a DNN (4x256 sigmoid layers + 100-dim linear) maps each utterance to a 100-dim "speaker vector" and scores it against the enrollment vectors with a threshold λ. The value of λ is not published. Raising it trades fewer false accepts for more false rejects. Weights are 8-bit quantized. Speaker-recognition EER is 4.3%. End to end on 200 production users: about 1 false accept per month, 5.2% false rejects, 3.2% imposter accepts. Reverberant (large room) and noisy (car, wind) environments remain challenging. — [Apple ML Research: Personalized Hey Siri](https://machinelearning.apple.com/research/personalized-hey-siri)
- Apple later used federated learning to improve the speaker-recognition model on-device. Raw Siri audio stays on the device. — [MIT Technology Review, 2019](https://www.technologyreview.com/2019/12/11/131629/apple-ai-personalizes-siri-federated-learning/); related: [Apple ML: Voice Trigger System for Siri](https://machinelearning.apple.com/research/voice-trigger)
- **Google Voice Match:** the 2020 update moved from bare hotwords to **4 full phrases**, e.g. "Hey Google, play my workout playlist", to improve speaker identification. — [9to5Google, Apr 2020](https://9to5google.com/2020/04/23/hey-google-voice-match/). Older setup: "OK Google" x2 + "Hey Google" x2. — [How-To Geek](https://www.howtogeek.com/338071/how-to-retrain-your-google-assistant-voice-model/)
- Google: the voice model is "created on Google's servers and then stored only on the devices where you've turned on Voice Match". Users can retrain via "Teach your Assistant your voice again" > "Retrain", and the new model applies to all their devices. Google warns that a similar voice or **a recording of your voice** may not be told apart. Unrecognized voices are treated as guests and get no personal results. — [Google Assistant Help: Voice Match](https://support.google.com/assistant/answer/9071681?hl=en&co=GENIE.Platform%3DAndroid)
- Google holds a patent on retraining the trigger-phrase voice model from utterances collected during use, i.e. continuous adaptation. — [US10885899B2](https://patents.google.com/patent/US10885899)
- **Amazon Alexa Voice ID:** users speak **ten phrases** shown on screen, including the wake word and routine commands like playing music or asking for the weather. Amazon recommends a short distance and a quiet room. — search-result summary of [Amazon: Create an Alexa Voice ID](https://www.amazon.com/gp/help/customer/display.html?nodeId=GYY4637XC2STFL9R). The Amazon help page returned HTTP 503 when fetched directly, so treat this as moderately confident.
- An Illinois federal court allowed a BIPA class action over Alexa voiceprints to proceed (2023). — [Duane Morris Class Action Defense Blog](https://blogs.duanemorris.com/classactiondefense/2023/11/04/illinois-federal-court-allows-amazon-alexa-privacy-class-action-to-proceed/)

### Inferences
- The industry pattern is 4–10 short prompts, each 1–4 s. The profile is several separate embeddings, not a single average. Apple scores against up to 40 stored vectors and keeps growing the profile from accepted, high-confidence uses. That covers mic, room and time-of-day drift without asking the user to re-enroll.
- Apple keeps enrollment waveforms, not only vectors, so it can re-embed when the model changes. KIVO faces the same question: if it swaps the embedding model later, old vectors become useless. Either keep the encrypted enrollment audio, which is extra biometric data to protect, or force re-enrollment on model upgrade.
- When verification fails, the vendors fall back to guest mode (Google) or simply don't trigger (Apple). None of them locks the user out. That fits a convenience filter.

### Gaps
- Amazon's decision threshold, re-enrollment policy and whether Voice ID adapts over time were not confirmed from a primary source.
- Apple's newer "Siri" (no "Hey") trigger enrollment and any post-2018 changes to the phrase list were not found in primary sources.
- No vendor publishes its numeric thresholds.

## 3. Anti-spoofing and replay: security gate or convenience filter?

### Takeaway
Unprotected speaker verification is highly vulnerable to replay and to modern TTS or voice cloning. Google itself says a recording can fool Voice Match. For KIVO, speaker verification should be a **convenience filter** that decides whether to respond and which profile to use. It must not be the only gate for sensitive actions such as payments, sending messages, file deletion or credentials. Those need an on-screen confirmation or OS authentication (Windows Hello, macOS Touch ID).

### Cited Findings
- Google warns that if someone's voice is similar to yours, or they use a recording of your voice, the device may not distinguish them. — [Google Assistant Help: Voice Match](https://support.google.com/assistant/answer/9071681?hl=en&co=GENIE.Platform%3DAndroid)
- ASVspoof covered TTS and voice conversion (2015) and replay (2017). ASVspoof 2019 covered both logical access (TTS/VC) and physical access (replay). — [ASVspoof 2019, arXiv:1904.05441](https://arxiv.org/pdf/1904.05441)
- Replay studies show false-accept rates on unprotected ASV rising from 1% to 89% (male) and from 5% to 100% (female). One physical-access experiment measured a replay FAR of 93%. — [Introduction to Voice Presentation Attack Detection, arXiv:1901.01085](https://arxiv.org/pdf/1901.01085)
- In "Hello, It's Me", researchers found that "both humans and machines can be reliably fooled by synthetic speech" produced by deep-learning TTS. — [arXiv:2109.09598](https://arxiv.org/abs/2109.09598)
- AASIST is a state-of-the-art spoofing countermeasure. Speaker-aware extensions improved EER by up to 25.1% relative on a custom ASVspoof 2019 protocol. — [Speaker-Aware Anti-spoofing, arXiv:2303.01126](https://arxiv.org/html/2303.01126v2)

### Inferences
- Detecting spoofs from a laptop mic is an open research problem, and countermeasures trained on ASVspoof generalize poorly to new TTS systems. KIVO should not claim that voice ID is secure.
- Practical mitigations: (1) tier the actions. Low-risk actions such as answering or playing music need only speaker verification. High-risk actions need explicit confirmation or Windows Hello. (2) Ignore KIVO's own TTS output with echo cancellation, and optionally fingerprint the audio KIVO itself plays, so it can't trigger itself. (3) Optionally add a small AASIST-class countermeasure later as defense in depth.
- For households, use a "guest mode": respond to non-owners with no personal data, as Google does, rather than refusing outright.

### Gaps
- The exact per-system success rates from "Hello, It's Me" (Azure, WeChat, Alexa) could not be extracted because the PDF did not parse.
- No off-the-shelf, permissively licensed ONNX anti-spoofing model for consumer desktop use was confirmed.

## 4. STT personalization for accent and user speech: what works with modern engines?

### Takeaway
There are three tiers, from cheapest to most expensive. (1) **Contextual biasing / hotwords / prompts**: free, instant, and effective for names and jargon, with little effect on accent. (2) **Engine/model selection per user**, by measuring WER on the enrollment recordings: cheap and helpful. (3) **Per-user LoRA fine-tuning**: papers report roughly 15–37% relative WER reduction for speaker or accent adaptation, but a single user's 1–2 minutes of enrollment audio is far below what those papers used. Treat tier 3 as an optional, later feature that needs more data, gathered with consent from real use.

### Cited Findings
- **sherpa-onnx hotwords:** supported **only for transducer models** (offline and online). Whisper, Paraformer and CTC models are not supported. Decoding must use `modified_beam_search`, not greedy. The file has one phrase per line with an optional per-phrase score (`PHRASE :3.5`). The default `hotwords-score` is 1.5 per matched token. The docs show example corrections but no quantitative gains. — [sherpa-onnx hotwords docs](https://k2-fsa.github.io/sherpa/onnx/hotwords/index.html)
- **NeMo word boosting / GPU-PB:** GPU-accelerated phrase boosting works for CTC, RNN-T/TDT and AED (Canary) models, with greedy and beam search. It needs only a list of key phrases and no retraining. — [NeMo Word Boosting docs](https://docs.nvidia.com/nemo-framework/user-guide/latest/nemotoolkit/asr/asr_customization/word_boosting.html). Word boosting for TDT (Parakeet) arrived in NeMo 2.5.0. — [HF parakeet-tdt-0.6b-v2 discussion](https://huggingface.co/nvidia/parakeet-tdt-0.6b-v2/discussions/34); [NeMo issue #14500](https://github.com/NVIDIA-NeMo/NeMo/issues/14500)
- The TurboBias paper describes a GPU word-boosting tree that works across CTC, transducer and AED models in shallow fusion, with no noticeable speed loss even with up to 20K key phrases. — [arXiv:2508.07014](https://arxiv.org/pdf/2508.07014)
- **Whisper prompt:** whisper-1 prompts have a **224-token limit**. They help with uncommon words and acronyms, for example by passing a list of correct spellings. Whisper "doesn't follow instructions" like an LLM. For long term lists, OpenAI suggests post-processing with a text model instead. — [OpenAI Speech-to-text guide](https://developers.openai.com/api/docs/guides/speech-to-text)
- **LoRA speaker adaptation:** applying LoRA with about 1% extra per-speaker parameters on 4-bit quantized Whisper and Conformer-AED gave 15.1% and 23.3% relative WER reductions vs the full-precision models, on LibriSpeech and TED-LIUM 3. — [Speaker Adaptation for Quantised End-to-End ASR Models, arXiv:2408.03979](https://arxiv.org/pdf/2408.03979)
- **LoRA accent adaptation:** a Whisper-medium LoRA (rank 32, alpha 64) trained on 4,071 Chinese-accented English utterances (about 4.1 h) gave about 37.4% relative WER reduction on held-out accented speech. This is an accent-group adapter, not a per-user one. — [HF Grenmango/whisper-medium-en-chinese-accent](https://huggingface.co/Grenmango/whisper-medium-en-chinese-accent)
- LoRA tooling for Whisper: [Diabolocom Finetune-Whisper-with-LoRA](https://github.com/Diabolocom-Research/Finetune-Whisper-with-LoRA); overview [Diabolocom: fine-tuning ASR](https://www.diabolocom.com/research/fine-tuning-asr-focus-on-whisper/). A search summary also cited "24.2% relative WER reduction on LibriSpeech-SA speaker adaptation sets" for Whisper-LoRA, but its primary source was not identified, so treat it as unverified.
- For accented speech, a mixture of LoRA experts plus LLM generative error correction has been proposed. — [arXiv:2507.09116](https://arxiv.org/pdf/2507.09116)

### Inferences
- **Recommended for KIVO v1:**
  1. Build a per-user **vocabulary list** from the user's name, contacts, app names, project names and KIVO's own name. Feed it as hotwords for a sherpa-onnx transducer (e.g. Zipformer or Parakeet-TDT exported to sherpa-onnx), as a Whisper `initial_prompt` of 224 tokens or fewer, or as NeMo boosting.
  2. **Engine/model auto-selection:** enrollment prompts have known text, so KIVO can compute WER for each candidate local engine or model size on the user's own recordings and pick the best within its latency budget. This costs almost nothing and directly measures accent fit. With about 10 short phrases (about 60–100 words) the WER estimate is noisy. Use it to rank engines, not as a precise figure.
  3. **LLM post-correction** with the user vocabulary. KIVO already has an LLM in the loop, so this is cheap.
- **Per-user LoRA:** about 60 s of enrollment audio is far below the hours used in the accent-adapter results and the per-speaker sets in the SA papers. The gains will likely be small, and it risks overfitting to the prompt texts. If offered, run it later as an opt-in "improve recognition" job. It should train only on utterances the user confirmed or corrected, collected over days, keep a held-out set, and revert if WER gets worse. LoRA training of Whisper-small or -base on minutes of data is feasible on a consumer GPU. No sourced timing figure was found (see Gaps).
- Realistic expectation for accented speakers: large modern models (Whisper large-v3 / turbo, Parakeet v3) are already fairly accent-robust. Choosing the best model and biasing vocabulary probably yields more than per-user fine-tuning in v1.

### Gaps
- No sourced wall-clock figures were found for per-user LoRA training time on a consumer GPU or CPU.
- No quantitative WER gains were found for sherpa-onnx hotwords or NeMo boosting on accented speech; the reported gains focus on rare-word or entity recall.
- No study was found measuring per-user adaptation with only 1–2 minutes of audio on Whisper or Parakeet.

## 5. Privacy: are voiceprints biometric data? Consent, encryption at rest, deletion

### Takeaway
Yes, voiceprints are biometric data. Under GDPR Art. 9, processing them to uniquely identify a person is special-category processing and typically needs explicit consent. Illinois BIPA names "voiceprint" explicitly and requires informed written consent, a public retention and destruction schedule, and allows private lawsuits (Alexa was sued under it). KIVO should keep everything on-device, encrypted, and deletable with one click.

### Cited Findings
- Under GDPR Art. 9, processing biometric data for the purpose of uniquely identifying a person is prohibited unless an exception applies, most commonly **explicit consent**. Voiceprints are considered uniquely identifying biometric data. — [GDPR Local: Biometric data compliance](https://gdprlocal.com/biometric-data-gdpr-compliance-made-simple/); [Didit: Biometric consent guide](https://didit.me/blog/biometric-consent-gdpr-compliance/)
- BIPA's definition of biometric identifier expressly includes "voiceprint". It requires informed written consent before collection and gives a private right of action with statutory damages. — [terms.law Biometric Data Privacy FAQ (2026)](https://terms.law/FAQ/privacy-data/biometric-data-privacy-faq.html)
- Example industry retention policy: voiceprint templates are kept at most 3 years from last interaction, or less, citing BIPA, Texas CUBI and similar laws. — [AI-Media Biometric Data Policy](https://www.ai-media.tv/biometric-data-policy-and-notice/)
- Amazon faced a BIPA class action over Alexa voiceprints that a federal court allowed to proceed in 2023. — [Duane Morris blog](https://blogs.duanemorris.com/classactiondefense/2023/11/04/illinois-federal-court-allows-amazon-alexa-privacy-class-action-to-proceed/)
- Google stores Voice Match models only on the user's enabled devices and keeps audio only if the user enables Web & App Activity audio saving. — [Google Assistant Help](https://support.google.com/assistant/answer/9071681?hl=en&co=GENIE.Platform%3DAndroid)

### Inferences
- Local-only processing does not remove GDPR or BIPA obligations in every case. Whether they apply depends on whether the vendor "collects" or "processes" the data. Legal exposure is still much lower if the voiceprint never leaves the device and the vendor never has access to it. Still, implement explicit opt-in consent, a clear notice (purpose, what's stored, retention) and one-click deletion. Get legal review before shipping in the EU or Illinois.
- Storage design for KIVO:
  - Store embeddings, which are small (e.g. 192–512 floats, a few KB), in a local file encrypted with a key protected by **Windows DPAPI** (`CryptProtectData`, user scope) on Windows, **Keychain** on macOS, and the **Secret Service/libsecret** keyring on Linux. The Rust `keyring` crate abstracts these. These OS-API details come from general platform knowledge, not sources gathered here.
  - Store raw enrollment audio only if it is needed for re-embedding on model upgrades (the Apple pattern). Otherwise delete it right after the embeddings and the STT-engine WER test are computed. Make keeping audio a separate opt-in.
  - Never sync the voiceprint to the cloud by default. Exclude it from logs, crash dumps and telemetry.
  - Provide "Delete my voice profile", which removes the embeddings, any audio and adaptation weights (a LoRA adapter is also derived from biometric audio), and disables owner gating.
  - Delete automatically on uninstall, and after long inactivity (e.g. 3 years) to match BIPA-style schedules.

### Gaps
- No primary legal text (the BIPA statute or the GDPR regulation itself) or EDPB guidance was fetched. The findings above rely on secondary guides. Get counsel review.
- It is unclear whether a per-user LoRA STT adapter legally counts as a "voiceprint" or "biometric template". Treat it as one to be safe.

## Recommended KIVO enrollment flow (synthesis for the report writer)

These are inferences built from the findings above, not vendor-sourced facts.

1. **Consent screen:** explain what is stored (a voice signature on this device only, encrypted), why (respond mainly to you, improve recognition), that it is not a security lock, and how to delete it. Enrollment is optional, and without it KIVO still works in open mode.
2. **Mic check:** measure the level and noise floor first. Ask for a quiet room and normal distance (Amazon asks for this too).
3. **Phrases (8 prompts, about 2–4 s each, about 30–45 s of speech):**
   - 3x the wake word alone, e.g. "Hey KIVO" (Apple uses 3x the wake word).
   - 3x wake word + typical command, e.g. "Hey KIVO, what's on my calendar today?", "Hey KIVO, open my project folder", "Hey KIVO, play some music" (the Google/Amazon pattern).
   - 1x "Hey KIVO, it's me, [user's name]" (Apple uses this). This also captures the name for the hotword list.
   - 1–2 phonetically rich sentences with digits, names and the user's own terms. This improves the STT WER estimate and embedding coverage.
   - Reject and re-prompt a take if VAD finds too little speech, it clips, SNR is low, or its transcript is far from the prompt.
4. **Build the profile:** store each utterance's embedding separately, 8 vectors (Apple uses 5). Check that they agree with each other. If one outlier is far from the rest, re-record it. Set the threshold from a calibration curve built on KIVO-like mic data. Offer "stricter / looser" in settings.
5. **STT fit:** transcribe the enrollment clips with the candidate engines/models, compute WER against the known prompts, and pick the best within the latency budget. Seed the hotword/prompt vocabulary with the user's name, "KIVO" and app/contact names.
6. **Continuous adaptation:** add embeddings from high-confidence accepted utterances up to a cap of about 40 (Apple's scheme), replacing the oldest, and never adding from low-margin accepts. Offer "Retrain my voice" (Google's scheme). Re-enroll automatically if the embedding model changes and raw audio was not kept.
7. **Runtime:** score each command against the profile with max or mean cosine. Owner: full access. Unknown voice: guest mode (no personal data). Sensitive actions always need on-screen or OS confirmation, whatever the speaker score.
