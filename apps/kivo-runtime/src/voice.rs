//! The listening pipeline (VOICE §1, §4, VOICE-02/03/04/05/19/32/41). Capture runs on the audio
//! thread and writes a lock-free ring buffer; one detection thread reads it, runs the energy
//! gate and Silero VAD, publishes the level for the Island's waveform, and streams 16 kHz mono
//! audio to the speech worker. Silence after speech ends the utterance by itself ("auto-end on
//! silence").
//!
//! The microphone is open only while KIVO listens: during an utterance, or all the time when
//! hands-free listening is on (the "Microphone listening" capability with the keyword model
//! installed). Hands-free, the keyword spotter runs only around speech, primed with the audio
//! just before it, and a wake word starts the utterance on this thread at once, from just before
//! the wake phrase began, so "Hey Kivo, open Chrome" said in one breath is heard whole and the
//! phrase is then removed from the transcript (VOICE-05). While KIVO is busy the same spotter hears "Kivo stop", "stop" and "cancel"
//! (VOICE-19).
//!
//! Everything the microphone hears while KIVO's voice plays first has KIVO's own output removed
//! (echo cancellation, VOICE-30). Hands-free, the user can talk over KIVO: speech over
//! its voice ducks it at once, and speech that lasts 300 ms interrupts it and becomes the next
//! request (barge-in, VOICE-31). After an answer, speech within the follow-up window is a new
//! request without the wake word (UX-45).

use crate::infer::Infer;
use crate::speaker::Speaker;
use kivo_audio::capture::capture_ring;
use kivo_audio::{Chunker, EnergyGate, History};
use kivo_platform::{AudioIo, DeviceId};
use kivo_voice::kws::{Keyword, KeywordSpotter, KwsModel};
use kivo_voice::silero::SileroVad;
use kivo_voice::smart_turn::SmartTurn;
use kivo_voice::traits::{TurnDetector, VadEngine};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

/// Seconds of audio the ring holds (VOICE-03: at least 3 s, also the pre-roll).
const RING_SECONDS: u32 = 5;
/// Silero's frame: 32 ms at 16 kHz.
const VAD_FRAME: usize = 512;
/// Speech is this likely before KIVO counts it as speech.
const SPEECH_PROB: f32 = 0.5;
/// Silence this long after speech ends the utterance (without the end-of-turn model).
const END_SILENCE: Duration = Duration::from_millis(800);
/// VOICE-33: after this pause the end-of-turn model is asked whether the user has finished…
const TURN_CHECK: Duration = Duration::from_millis(250);
/// …and once more after a longer pause if they hadn't…
const TURN_RECHECK: Duration = Duration::from_millis(700);
/// …and a pause this long ends the utterance whatever it says.
const TURN_MAX_PAUSE: Duration = Duration::from_millis(1_600);
/// The end-of-turn model's "finished" probability.
const TURN_DONE: f32 = 0.5;
/// Give up listening if nothing is said at all.
const NO_SPEECH_TIMEOUT: Duration = Duration::from_secs(6);
/// A decision waits this long for a spoken answer (CONV-26); talking extends it.
pub const ANSWER_WAIT: Duration = Duration::from_secs(10);
/// Longest single utterance.
const MAX_UTTERANCE: Duration = Duration::from_secs(45);
/// The Island's waveform is fed at 30 Hz (ARCHITECTURE §3).
const LEVEL_PERIOD: Duration = Duration::from_millis(33);
/// Level scale: −60 dBFS is silence, −10 dBFS is full (matches the waveform's dots-to-bars).
const FLOOR_DB: f32 = -60.0;
const TOP_DB: f32 = -10.0;
const RATE: usize = kivo_audio::capture::RATE as usize;
/// Audio the spotter sees from just before speech starts, so a word's first sound isn't lost.
const SPOTTER_PREROLL: usize = RATE * 600 / 1000;
/// The spotter keeps listening this long after the last speech (sherpa resets after 1.5 s).
const SPOTTER_TAIL: Duration = Duration::from_millis(1_500);
/// Barge-in keeps this much audio from before the user's speech was detected.
const WAKE_PREROLL: usize = RATE * 300 / 1000;
/// VOICE-05: recognition starts this long before the wake phrase begins, so it hears the whole
/// phrase and what follows; the phrase is then removed from the transcript. (Starting just before
/// the wake word ended, the spec's first figure, clipped it: the spotter's word boundaries are
/// approximate, and a clipped "…vo, mute" was often not recognized at all.)
const WAKE_LEAD: usize = RATE * 150 / 1000;
/// VOICE-32: while KIVO is playing audio a wake word must be this sure, so KIVO's own voice
/// (and the room's echo of it) doesn't wake it.
const PLAYING_MIN_SCORE: f32 = 0.6;
/// Echo cancellation keeps running this long after KIVO's voice stops (the room's echo).
const ECHO_TAIL: Duration = Duration::from_millis(500);
/// VOICE-31: over KIVO's voice the VAD is stricter.
const BARGE_PROB: f32 = 0.75;
/// Speech over KIVO this long interrupts it.
const BARGE_AFTER_MS: u32 = 300;
/// Quiet this long after a duck restores KIVO's voice.
const BARGE_RELEASE: Duration = Duration::from_millis(400);
/// Silero's frame, in ms.
const VAD_FRAME_MS: u32 = 32;

/// Hands-free listening: the keyword model and what to listen for (VOICE §4).
#[derive(Clone, Debug)]
pub struct HandsFree {
    pub model_dir: PathBuf,
    /// Wake words: start a request (ids are the wake words' ids).
    pub wake: Vec<Keyword>,
    /// "Kivo stop", "stop", "cancel": honoured only while KIVO is busy (VOICE-19).
    pub stop: Vec<Keyword>,
}

/// What the detection thread is doing.
enum Command {
    /// Start an utterance: open the mic and stream to the worker under this id. `wait` is how
    /// long to wait for speech; `keep` sends the audio back as `Heard` (a spoken answer, for
    /// the owner's-voice check).
    Listen {
        utterance: u64,
        auto_end: bool,
        wait: Duration,
        keep: bool,
    },
    /// The user let go of push-to-talk: finish the utterance.
    Stop,
    /// The speech worker has started this utterance: send it the audio (buffered until now).
    Ready {
        utterance: u64,
    },
    /// Drop the utterance without a result.
    Cancel,
    /// KIVO started making sound: wake up to pulse the Island.
    Wake,
    /// Turn hands-free listening on (with its model and words) or off.
    HandsFree(Option<Box<HandsFree>>),
    /// Record one spoken clip for voice enrollment (VOICE-20): it ends on silence and comes
    /// back as `Recorded`; nothing goes to the speech worker.
    Record {
        id: u64,
    },
    /// Listen for a follow-up without the wake word until then (UX-45), or stop.
    FollowUp(Option<Instant>),
    Quit,
}

/// What the pipeline tells the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum VoiceSignal {
    /// The user started speaking (T2).
    SpeechStarted { utterance: u64 },
    /// Silence ended the utterance (VOICE-41 auto-end), or it ran too long.
    EndOfSpeech { utterance: u64 },
    /// Nothing was said.
    NoSpeech { utterance: u64 },
    /// The microphone couldn't be opened (blocked in Windows privacy settings, UX-57).
    MicrophoneUnavailable,
    /// A wake word: the utterance has already started (T0). `clip` is the wake word's audio,
    /// for the speaker check.
    Wake {
        utterance: u64,
        word: String,
        phrase: String,
        score: f32,
        clip: Vec<f32>,
    },
    /// "Kivo stop" / "stop" / "cancel" while KIVO was busy (VOICE-19).
    StopHeard { word: String },
    /// Hands-free listening couldn't start (the keyword model failed to load).
    HandsFreeFailed { message: String },
    /// An enrollment clip (empty if nothing was said).
    Recorded { id: u64, audio: Vec<f32> },
    /// The user talked over KIVO long enough to interrupt it; the utterance has started with
    /// what they said (VOICE-31).
    BargeIn { utterance: u64 },
    /// Speech in the follow-up window; the utterance has started (UX-45).
    FollowUpSpeech { utterance: u64 },
    /// The audio of an utterance started with `keep` (sent just before it ends).
    Heard { utterance: u64, audio: Vec<f32> },
}

/// Controls the detection thread.
pub struct Listener {
    commands: Sender<Command>,
    listening: Arc<Mutex<Option<u64>>>,
    busy: Arc<AtomicBool>,
    hands_free: Arc<AtomicBool>,
    keep_audio: Arc<AtomicBool>,
}

impl Listener {
    /// Starts listening for a new utterance.
    pub fn listen(&self, utterance: u64, auto_end: bool) {
        *self
            .listening
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(utterance);
        let _ = self.commands.send(Command::Listen {
            utterance,
            auto_end,
            wait: NO_SPEECH_TIMEOUT,
            keep: false,
        });
    }

    /// Listens for the answer to a decision (CONV-26): up to 10 s for speech, and its audio
    /// comes back as `Heard` before the utterance ends.
    pub fn listen_for_answer(&self, utterance: u64) {
        *self
            .listening
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(utterance);
        let _ = self.commands.send(Command::Listen {
            utterance,
            auto_end: true,
            wait: ANSWER_WAIT,
            keep: true,
        });
    }

    /// The speech worker is ready for this utterance: the audio heard so far is sent at once.
    pub fn stt_ready(&self, utterance: u64) {
        let _ = self.commands.send(Command::Ready { utterance });
    }

    /// Ends the utterance (push-to-talk released).
    pub fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    pub fn cancel(&self) {
        *self
            .listening
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
        let _ = self.commands.send(Command::Cancel);
    }

    pub fn is_listening(&self) -> bool {
        self.listening
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .is_some()
    }

    /// Hands-free listening on (with what to listen for) or off.
    pub fn set_hands_free(&self, hands_free: Option<HandsFree>) {
        let _ = self
            .commands
            .send(Command::HandsFree(hands_free.map(Box::new)));
    }

    /// Records one clip for voice enrollment; it arrives as `VoiceSignal::Recorded`.
    pub fn record(&self, id: u64) {
        let _ = self.commands.send(Command::Record { id });
    }

    /// Listen for a follow-up until `until` (UX-45), or stop listening for one.
    pub fn follow_up(&self, until: Option<Instant>) {
        let _ = self.commands.send(Command::FollowUp(until));
    }

    /// Whether hands-free listening is running (the microphone is open for wake words).
    pub fn hands_free_on(&self) -> bool {
        self.hands_free.load(Ordering::Relaxed)
    }

    /// Keep every request's audio and send it as `Heard` before it ends (speaker recognition
    /// needs the whole utterance, VOICE-22).
    pub fn set_keep_audio(&self, keep: bool) {
        self.keep_audio.store(keep, Ordering::Relaxed);
    }

    /// KIVO is thinking, acting or speaking: the stop words count (VOICE-19).
    pub fn set_busy(&self, busy: bool) {
        self.busy.store(busy, Ordering::Relaxed);
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        let _ = self.commands.send(Command::Quit);
    }
}

/// Everything the detection thread needs.
pub struct Pipeline {
    pub audio: Arc<dyn AudioIo>,
    pub device: Option<DeviceId>,
    pub vad_model: PathBuf,
    /// The end-of-turn model's folder (VOICE-33); a download with the speech model.
    pub turn_model: PathBuf,
    pub infer: Infer,
    pub speaker: Arc<Speaker>,
    /// The Island's level (0–1).
    pub levels: watch::Sender<f32>,
    pub signals: mpsc::UnboundedSender<VoiceSignal>,
    /// Efficiency mode for the detection thread while it only waits (VOICE-03).
    pub qos: Arc<dyn kivo_platform::ThreadQos>,
}

/// Starts the detection thread and returns its controller.
pub fn start(pipeline: Pipeline) -> Listener {
    let (tx, rx) = channel();
    let wake = tx.clone();
    pipeline.speaker.on_sound(move || {
        let _ = wake.send(Command::Wake);
    });
    let listening = Arc::new(Mutex::new(None));
    let busy = Arc::new(AtomicBool::new(false));
    let hands_free = Arc::new(AtomicBool::new(false));
    let keep_audio = Arc::new(AtomicBool::new(false));
    let flags = Flags {
        listening: Arc::clone(&listening),
        busy: Arc::clone(&busy),
        hands_free: Arc::clone(&hands_free),
        keep_audio: Arc::clone(&keep_audio),
    };
    std::thread::Builder::new()
        .name("kivo-detect".into())
        .spawn(move || run(pipeline, &rx, &flags))
        .expect("thread spawn");
    Listener {
        commands: tx,
        listening,
        busy,
        hands_free,
        keep_audio,
    }
}

/// One utterance in progress.
struct Utterance {
    id: u64,
    auto_end: bool,
    started: Instant,
    heard_speech: bool,
    /// When speech was last heard; silence is measured from here, so a microphone that stops
    /// delivering audio ends the utterance instead of hanging it.
    last_speech: Option<Instant>,
    /// The user let go; finish as soon as the audio is sent.
    ending: bool,
    /// The speech worker has started the utterance. Until then the audio waits in `pending`
    /// (the models load when the request starts, so the first words are never lost).
    ready: bool,
    pending: Vec<f32>,
    /// Audio on its way to the worker, sent in 80 ms batches (VOICE-02).
    outbound: Vec<f32>,
    /// An enrollment recording: kept here, never sent to the worker.
    recording: bool,
    /// End-of-turn checks made in the current pause, and whether the model said "finished".
    turn_checks: u8,
    turn_done: bool,
    /// How long to wait for any speech.
    no_speech: Duration,
    /// A copy of the audio, when the caller wants it back.
    kept: Option<Vec<f32>>,
}

impl Utterance {
    fn new(id: u64, auto_end: bool) -> Self {
        Self {
            id,
            auto_end,
            started: Instant::now(),
            heard_speech: false,
            last_speech: None,
            ending: false,
            ready: false,
            pending: Vec::new(),
            outbound: Vec::with_capacity(MODEL_BATCH * 2),
            recording: false,
            turn_checks: 0,
            turn_done: false,
            no_speech: NO_SPEECH_TIMEOUT,
            kept: None,
        }
    }
}

/// State shared between the listener and its thread.
struct Flags {
    listening: Arc<Mutex<Option<u64>>>,
    busy: Arc<AtomicBool>,
    hands_free: Arc<AtomicBool>,
    keep_audio: Arc<AtomicBool>,
}

/// The user started talking over KIVO (VOICE-31).
struct Barge {
    /// Where their speech (with a little before it) starts, on the absolute clock.
    from: u64,
    speech_ms: u32,
    last_speech: Instant,
}

/// The keyword spotter while hands-free listening is on.
struct Hands {
    spotter: KeywordSpotter,
    /// Stop words, and each wake word's phrase.
    stop: std::collections::HashSet<String>,
    phrases: HashMap<String, String>,
    /// The spotter is fed until this moment (speech plus a tail); `None` while it rests.
    feeding_until: Option<Instant>,
    /// The absolute sample index where the spotter's audio starts.
    origin: u64,
}

impl Hands {
    fn load(config: &HandsFree) -> Result<Self, String> {
        let model = KwsModel::load(&config.model_dir).map_err(|e| e.detail())?;
        let mut keywords = config.wake.clone();
        keywords.extend(config.stop.iter().cloned());
        let (spotter, refused) = KeywordSpotter::new(model, keywords);
        for k in refused {
            tracing::warn!(
                word = k.id,
                "the keyword model can't spell this word; skipped"
            );
        }
        Ok(Self {
            spotter,
            stop: config.stop.iter().map(|k| k.id.clone()).collect(),
            // A word's first spelling is its phrase (the others are pronunciation variants).
            phrases: config.wake.iter().fold(HashMap::new(), |mut m, k| {
                m.entry(k.id.clone()).or_insert_with(|| k.phrase.clone());
                m
            }),
            feeding_until: None,
            origin: 0,
        })
    }
}

/// Audio goes to the speech worker in 80 ms batches: 10 ms frames inside, fewer messages out
/// (VOICE-02).
const MODEL_BATCH: usize = RATE * 80 / 1000;

/// The most audio held while the speech worker starts (one long utterance).
const PENDING_LIMIT: usize = RATE * 45;

/// The speech probability of each VAD frame completed by `audio` (the energy gate first, then
/// the VAD; 0 while KIVO's own cue plays).
fn speech_probabilities(
    audio: &[f32],
    muted: bool,
    chunker: &mut Chunker,
    gate: &mut EnergyGate,
    vad: &mut Option<SileroVad>,
) -> Vec<f32> {
    let mut out = Vec::new();
    chunker.push(audio, &mut |frame| {
        out.push(if muted {
            0.0
        } else if gate.open(frame) {
            vad.as_mut()
                .and_then(|v| v.process(frame).ok())
                .unwrap_or(1.0)
        } else {
            0.0
        });
    });
    out
}

fn count_above(probabilities: &[f32], threshold: f32) -> usize {
    probabilities.iter().filter(|p| **p > threshold).count()
}

#[allow(
    clippy::too_many_lines,
    clippy::cognitive_complexity,
    reason = "one loop: mic, level, gate, VAD, spotting, streaming"
)]
fn run(pipeline: Pipeline, commands: &Receiver<Command>, flags: &Flags) {
    let Flags {
        listening,
        busy,
        hands_free,
        keep_audio,
    } = flags;
    let keep = || keep_audio.load(Ordering::Relaxed).then(Vec::new);
    let Pipeline {
        audio,
        device,
        vad_model,
        turn_model,
        infer,
        speaker,
        levels,
        signals,
        qos,
    } = pipeline;
    // Efficient while waiting, full speed while listening (VOICE-03).
    let mut eco = false;
    // Without the VAD, push-to-talk still works: the key decides when speech ends. It is a
    // download the user chooses (with a speech model), so it is loaded again once it's there.
    let load_vad = || match SileroVad::load(&vad_model) {
        Ok(vad) => Some(vad),
        Err(e) => {
            tracing::warn!(
                detail = e.detail(),
                "voice activity detection is unavailable"
            );
            None
        }
    };
    let mut vad = if vad_model.is_file() {
        load_vad()
    } else {
        None
    };
    // The end-of-turn model, loaded once it has been downloaded (VOICE-33).
    let load_turn = || {
        SmartTurn::load(&turn_model)
            .map_err(|e| tracing::debug!(detail = e.detail(), "end-of-turn model unavailable"))
            .ok()
    };
    let mut turn: Option<SmartTurn> = if turn_model.join("smart-turn.onnx").is_file() {
        load_turn()
    } else {
        None
    };
    let mut gate = EnergyGate::new();
    let mut chunker = Chunker::new(VAD_FRAME);
    let mut history = History::new(RATE * RING_SECONDS as usize);
    // Samples heard since the microphone opened (the absolute clock for pre-roll).
    let mut heard: u64 = 0;
    let mut current: Option<Utterance> = None;
    let mut hands: Option<Hands> = None;
    let mut mic = None;
    // Replaced whenever the microphone is opened; the writer moves into the audio callback.
    #[allow(
        unused_assignments,
        reason = "the first pair is replaced when the mic opens"
    )]
    let (mut writer, mut reader) = capture_ring(1);
    let mut audio_buf: Vec<f32> = Vec::with_capacity(16_000);
    // Echo cancellation runs while KIVO's speaker is open (VOICE-30).
    let reference = speaker.reference();
    let mut echo = kivo_audio::echo::EchoCanceller::new();
    let mut echo_active = false;
    let mut voice_until: Option<Instant> = None;
    let mut barge: Option<Barge> = None;
    let mut follow_until: Option<Instant> = None;
    let mut last_level = Instant::now();
    // A command received while waiting idle, handled at the top of the next pass.
    let mut parked: Option<Command> = None;

    loop {
        // 1. Commands (non-blocking; the loop also wakes on audio).
        loop {
            let next = match parked.take() {
                Some(command) => Ok(command),
                None => commands.try_recv(),
            };
            match next {
                Ok(Command::Listen {
                    utterance,
                    auto_end,
                    wait,
                    keep: keep_answer,
                }) => {
                    if eco {
                        qos.efficiency_mode(false);
                        eco = false;
                    }
                    if mic.is_none() {
                        let (w, r) = capture_ring(RING_SECONDS);
                        writer = w;
                        reader = r;
                        heard = 0;
                        mic = open_microphone(&audio, device.as_ref(), &mut writer, &signals);
                    }
                    if vad.is_none() && vad_model.is_file() {
                        vad = load_vad();
                    }
                    if turn.is_none() && turn_model.join("smart-turn.onnx").is_file() {
                        turn = load_turn();
                    }
                    if mic.is_some() {
                        // Push-to-talk: what was said before the key doesn't count.
                        reader.discard();
                        chunker.clear();
                        if let Some(vad) = vad.as_mut() {
                            vad.reset();
                        }
                        if let Some(h) = hands.as_mut() {
                            h.feeding_until = None;
                        }
                        let mut u = Utterance::new(utterance, auto_end);
                        u.no_speech = wait;
                        u.kept = if keep_answer {
                            Some(Vec::new())
                        } else {
                            keep()
                        };
                        current = Some(u);
                    }
                }
                Ok(Command::Ready { utterance }) => {
                    if let Some(u) = current.as_mut().filter(|u| u.id == utterance) {
                        u.ready = true;
                        let pending = std::mem::take(&mut u.pending);
                        if !pending.is_empty()
                            && let Err(e) = infer.send_audio(u.id, &pending)
                        {
                            tracing::debug!(%e, "buffered audio dropped");
                        }
                    }
                }
                Ok(Command::Stop) => {
                    if let Some(u) = current.as_mut() {
                        u.ending = true;
                    }
                }
                Ok(Command::Cancel) => {
                    if let Some(u) = current.take() {
                        let _ = infer.cancel_stt(u.id);
                    }
                    if hands.is_none() {
                        mic = None;
                    }
                    let _ = levels.send_replace(0.0);
                }
                Ok(Command::FollowUp(until)) => follow_until = until,
                Ok(Command::HandsFree(config)) => {
                    hands = config.and_then(|c| match Hands::load(&c) {
                        Ok(h) => {
                            tracing::info!(words = c.wake.len(), "hands-free listening on");
                            Some(h)
                        }
                        Err(message) => {
                            tracing::warn!(message, "hands-free listening couldn't start");
                            let _ = signals.send(VoiceSignal::HandsFreeFailed { message });
                            None
                        }
                    });
                    if hands.is_some() {
                        if vad.is_none() && vad_model.is_file() {
                            vad = load_vad();
                        }
                        if mic.is_none() {
                            let (w, r) = capture_ring(RING_SECONDS);
                            writer = w;
                            reader = r;
                            heard = 0;
                            history.clear();
                            mic = open_microphone(&audio, device.as_ref(), &mut writer, &signals);
                        }
                    } else if current.is_none() {
                        mic = None;
                        tracing::info!("hands-free listening off");
                    }
                    hands_free.store(hands.is_some(), Ordering::Relaxed);
                }
                Ok(Command::Record { id }) => {
                    if current.is_some() {
                        // Busy with a request: the recording can't start now.
                        let _ = signals.send(VoiceSignal::Recorded {
                            id,
                            audio: Vec::new(),
                        });
                        continue;
                    }
                    if eco {
                        qos.efficiency_mode(false);
                        eco = false;
                    }
                    if mic.is_none() {
                        let (w, r) = capture_ring(RING_SECONDS);
                        writer = w;
                        reader = r;
                        heard = 0;
                        mic = open_microphone(&audio, device.as_ref(), &mut writer, &signals);
                    }
                    if vad.is_none() && vad_model.is_file() {
                        vad = load_vad();
                    }
                    if mic.is_some() {
                        reader.discard();
                        chunker.clear();
                        if let Some(vad) = vad.as_mut() {
                            vad.reset();
                        }
                        let mut u = Utterance::new(id, true);
                        u.recording = true;
                        current = Some(u);
                    } else {
                        let _ = signals.send(VoiceSignal::Recorded {
                            id,
                            audio: Vec::new(),
                        });
                    }
                }
                Ok(Command::Wake) => {}
                Ok(Command::Quit) | Err(TryRecvError::Disconnected) => {
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        // 2. Audio: gate → VAD → the speech worker, or → the keyword spotter.
        audio_buf.clear();
        reader.read(&mut audio_buf);
        // Echo cancellation while KIVO's voice plays, and a moment after for the room's echo.
        // Cues are left to the VAD's own gating (VOICE-25): cancelling during a cue would blunt
        // the first sound of what the user says as it plays.
        if speaker.speaking() {
            voice_until = Some(Instant::now() + ECHO_TAIL);
        }
        if voice_until.is_some_and(|until| Instant::now() < until) {
            let (rate, played) = reference.take();
            echo.played(rate, &played);
            echo.clean(&mut audio_buf);
            echo_active = true;
        } else if echo_active {
            // KIVO stopped talking: pass on what the canceller still held, then the rest as is.
            let mut rest = Vec::new();
            echo.flush(&mut rest);
            rest.append(&mut audio_buf);
            audio_buf = rest;
            let _ = reference.take();
            echo_active = false;
            voice_until = None;
        } else {
            // Not cancelling: what was played isn't needed.
            let _ = reference.take();
        }
        if !audio_buf.is_empty() {
            history.push(&audio_buf);
            heard += audio_buf.len() as u64;
            // KIVO's own cue must not count as the user speaking (VOICE-25).
            let muted = speaker.muting_microphone();
            if let Some(u) = current.as_mut() {
                if let Some(kept) = u.kept.as_mut()
                    && kept.len() + audio_buf.len() <= PENDING_LIMIT
                {
                    kept.extend_from_slice(&audio_buf);
                }
                // Every sample goes to recognition: people start talking the moment they press the
                // key, so dropping the chime's length would cut off their first word. Until the
                // worker has started the utterance, the audio waits here.
                if u.ready {
                    u.outbound.extend_from_slice(&audio_buf);
                    if u.outbound.len() >= MODEL_BATCH {
                        send_batch(&infer, u);
                    }
                } else if u.pending.len() + audio_buf.len() <= PENDING_LIMIT {
                    u.pending.extend_from_slice(&audio_buf);
                }
                let probabilities =
                    speech_probabilities(&audio_buf, muted, &mut chunker, &mut gate, &mut vad);
                if count_above(&probabilities, SPEECH_PROB) > 0 {
                    if !u.heard_speech {
                        u.heard_speech = true;
                        let _ = signals.send(VoiceSignal::SpeechStarted { utterance: u.id });
                    }
                    u.last_speech = Some(Instant::now());
                    // Talking again: a new pause will be judged afresh.
                    u.turn_checks = 0;
                    u.turn_done = false;
                }
            } else if let Some(h) = hands.as_mut() {
                let probabilities =
                    speech_probabilities(&audio_buf, muted, &mut chunker, &mut gate, &mut vad);
                let spoke = count_above(&probabilities, SPEECH_PROB) > 0;
                let now = Instant::now();
                let is_busy = busy.load(Ordering::Relaxed);
                // Barge-in (VOICE-31): the user talks over KIVO's voice.
                if speaker.speaking() && is_busy {
                    let strong = count_above(&probabilities, BARGE_PROB);
                    if strong > 0 {
                        let b = barge.get_or_insert_with(|| {
                            speaker.duck(true);
                            let before = (audio_buf.len() + WAKE_PREROLL) as u64;
                            Barge {
                                from: heard.saturating_sub(before),
                                speech_ms: 0,
                                last_speech: now,
                            }
                        });
                        b.speech_ms += u32::try_from(strong).unwrap_or(u32::MAX) * VAD_FRAME_MS;
                        b.last_speech = now;
                    }
                    if let Some(b) = barge.as_ref() {
                        if b.speech_ms >= BARGE_AFTER_MS {
                            let utterance = infer.next_utterance();
                            *listening
                                .lock()
                                .unwrap_or_else(std::sync::PoisonError::into_inner) =
                                Some(utterance);
                            let mut u = Utterance::new(utterance, true);
                            u.heard_speech = true;
                            u.last_speech = Some(now);
                            u.pending = history.last(usize::try_from(heard - b.from).unwrap_or(0));
                            u.kept = keep().map(|_| u.pending.clone());
                            current = Some(u);
                            barge = None;
                            h.feeding_until = None;
                            tracing::info!("barge-in");
                            let _ = signals.send(VoiceSignal::BargeIn { utterance });
                            continue;
                        } else if b.last_speech.elapsed() > BARGE_RELEASE {
                            // A cough, not the user: KIVO's voice comes back.
                            speaker.duck(false);
                            barge = None;
                        }
                    }
                } else if barge.take().is_some() {
                    speaker.duck(false);
                }
                // A follow-up (UX-45): speech after an answer, no wake word needed.
                if follow_until.is_some_and(|until| now < until) && spoke && !speaker.speaking() {
                    let utterance = infer.next_utterance();
                    *listening
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(utterance);
                    let mut u = Utterance::new(utterance, true);
                    u.heard_speech = true;
                    u.last_speech = Some(now);
                    u.pending = history.last(SPOTTER_PREROLL + audio_buf.len());
                    u.kept = keep().map(|_| u.pending.clone());
                    current = Some(u);
                    follow_until = None;
                    h.feeding_until = None;
                    tracing::info!("follow-up");
                    let _ = signals.send(VoiceSignal::FollowUpSpeech { utterance });
                    continue;
                } else if follow_until.is_some_and(|until| now >= until) {
                    follow_until = None;
                }
                let mut fed: Option<Vec<kivo_voice::kws::Detection>> = None;
                if spoke && h.feeding_until.is_none() {
                    // Speech after quiet: a fresh stream, primed with the moment before it.
                    h.spotter.reset();
                    let primed = history.last(SPOTTER_PREROLL + audio_buf.len());
                    h.origin = heard - primed.len() as u64;
                    fed = Some(h.spotter.accept(&primed).unwrap_or_default());
                } else if h.feeding_until.is_some_and(|until| now < until) {
                    fed = Some(h.spotter.accept(&audio_buf).unwrap_or_default());
                } else {
                    h.feeding_until = None;
                }
                if spoke {
                    h.feeding_until = Some(now + SPOTTER_TAIL);
                }
                for hit in fed.unwrap_or_default() {
                    if h.stop.contains(&hit.id) {
                        if is_busy {
                            tracing::info!(word = hit.id, score = hit.score, "stop word");
                            let _ = signals.send(VoiceSignal::StopHeard { word: hit.id });
                        }
                        continue;
                    }
                    if speaker.busy() && hit.score < PLAYING_MIN_SCORE {
                        tracing::debug!(score = hit.score, "wake word ignored while KIVO plays");
                        continue;
                    }
                    let at = |ms: usize| h.origin + (ms * RATE / 1000) as u64;
                    let (start, end) = (at(hit.start_ms), at(hit.end_ms).min(heard));
                    let back = |from: u64| history.last(usize::try_from(heard - from).unwrap_or(0));
                    let mut clip = back(start.min(end));
                    clip.truncate(usize::try_from(end - start.min(end)).unwrap_or(0));
                    let utterance = infer.next_utterance();
                    *listening
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(utterance);
                    // The request itself hasn't been heard yet: a pause after "Hey Kivo" waits for
                    // it (up to the no-speech timeout) instead of ending the utterance.
                    let mut u = Utterance::new(utterance, true);
                    u.pending = back(start.min(end).saturating_sub(WAKE_LEAD as u64));
                    // The spotter confirms a word a few hundred ms after it ends, so the request
                    // may already have begun: listen to what came after the wake word too.
                    let after = back(end);
                    if let Some(vad) = vad.as_mut() {
                        vad.reset();
                    }
                    let mut fresh = Chunker::new(VAD_FRAME);
                    let already = speech_probabilities(
                        &after,
                        false,
                        &mut fresh,
                        &mut EnergyGate::new(),
                        &mut vad,
                    );
                    if count_above(&already, SPEECH_PROB) > 0 {
                        u.heard_speech = true;
                        u.last_speech = Some(now);
                    }
                    // For the voice check: the whole wake word and what follows.
                    u.kept = keep().map(|_| back(start.min(end)));
                    if let Some(vad) = vad.as_mut() {
                        vad.reset();
                    }
                    current = Some(u);
                    if eco {
                        qos.efficiency_mode(false);
                        eco = false;
                    }
                    tracing::info!(word = hit.id, score = hit.score, "wake word");
                    let phrase = h.phrases.get(&hit.id).cloned().unwrap_or_default();
                    let _ = signals.send(VoiceSignal::Wake {
                        utterance,
                        word: hit.id,
                        phrase,
                        score: hit.score,
                        clip,
                    });
                    h.feeding_until = None;
                    break;
                }
            }
        }

        // 3. Level for the Island: the microphone while listening, KIVO's voice while speaking.
        if last_level.elapsed() >= LEVEL_PERIOD {
            last_level = Instant::now();
            let level = if speaker.busy() {
                speaker.take_speaking_level()
            } else if current.is_some() {
                to_level(reader.take_peak())
            } else {
                // Hands-free, the peak still has to be taken so it doesn't pile up.
                let _ = reader.take_peak();
                0.0
            };
            levels.send_if_modified(|current| {
                let changed = (*current - level).abs() > 0.004;
                if changed {
                    *current = level;
                }
                changed
            });
        }

        // 4. Has the utterance ended? In a pause the end-of-turn model judges whether the user
        // has finished (VOICE-33); without it, silence decides. The microphone is released
        // before anyone hears so.
        if let Some(u) = current.as_mut().filter(|u| u.auto_end && !u.recording)
            && let Some(model) = turn.as_mut()
            && let Some(pause) = u.last_speech.map(|t| t.elapsed())
        {
            let due = match u.turn_checks {
                0 => pause >= TURN_CHECK,
                1 => pause >= TURN_RECHECK,
                _ => false,
            };
            if due && !u.turn_done {
                u.turn_checks += 1;
                let tail = history.last(RATE * 8);
                match model.end_probability(&tail, "") {
                    Ok(p) => u.turn_done = p >= TURN_DONE,
                    Err(e) => tracing::debug!(detail = e.detail(), "end-of-turn check failed"),
                }
            }
        }
        let judged = turn.is_some();
        let ended = current.as_ref().and_then(|u| {
            let silent_long_enough = u.auto_end
                && u.last_speech.is_some_and(|since| {
                    let pause = since.elapsed();
                    if judged && !u.recording {
                        (u.turn_done && pause >= TURN_CHECK) || pause > TURN_MAX_PAUSE
                    } else {
                        pause > END_SILENCE
                    }
                });
            let too_long = u.started.elapsed() > MAX_UTTERANCE;
            let nothing_said = !u.heard_speech && u.started.elapsed() > u.no_speech;
            if u.ending || silent_long_enough || too_long {
                Some(if u.heard_speech || u.ending {
                    VoiceSignal::EndOfSpeech { utterance: u.id }
                } else {
                    VoiceSignal::NoSpeech { utterance: u.id }
                })
            } else if nothing_said {
                if !u.recording {
                    let _ = infer.cancel_stt(u.id);
                }
                Some(VoiceSignal::NoSpeech { utterance: u.id })
            } else {
                None
            }
        });
        if let Some(signal) = ended {
            if let Some(u) = current.as_mut()
                && let Some(kept) = u.kept.take()
            {
                let _ = signals.send(VoiceSignal::Heard {
                    utterance: u.id,
                    audio: kept,
                });
            }
            let recorded = current.as_mut().filter(|u| u.recording).map(|u| {
                let heard_something = matches!(signal, VoiceSignal::EndOfSpeech { .. });
                VoiceSignal::Recorded {
                    id: u.id,
                    audio: if heard_something {
                        std::mem::take(&mut u.pending)
                    } else {
                        Vec::new()
                    },
                }
            });
            // The last partial batch goes before the worker hears the utterance is over.
            if let Some(u) = current.as_mut().filter(|u| u.ready) {
                send_batch(&infer, u);
            }
            finish(&mut current, &mut mic, hands.is_some(), listening, &levels);
            let _ = signals.send(recorded.unwrap_or(signal));
        }

        // 5. Sleep until audio arrives. Idle, the thread sleeps until a command comes (DISC-19:
        // no work at idle), waking only to pulse the Island while KIVO speaks or, once a
        // second, to close a speaker that has gone quiet. Hands-free it waits for audio, in
        // efficiency mode.
        if current.is_some() {
            if eco {
                qos.efficiency_mode(false);
                eco = false;
            }
            reader.wait(Duration::from_millis(20));
        } else if hands.is_some() && mic.is_some() {
            if !eco {
                qos.efficiency_mode(true);
                eco = true;
            }
            reader.wait(Duration::from_millis(40));
            speaker.release_if_idle();
        } else {
            if !eco {
                qos.efficiency_mode(true);
                eco = true;
            }
            let wait = if speaker.busy() {
                Some(LEVEL_PERIOD)
            } else if speaker.is_open() {
                Some(Duration::from_secs(1))
            } else {
                None
            };
            let received = match wait {
                Some(wait) => match commands.recv_timeout(wait) {
                    Ok(command) => Some(command),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => return,
                },
                None => match commands.recv() {
                    Ok(command) => Some(command),
                    Err(_) => return,
                },
            };
            speaker.release_if_idle();
            parked = received;
        }
    }
}

/// Sends the batched audio to the speech worker.
fn send_batch(infer: &Infer, u: &mut Utterance) {
    if u.outbound.is_empty() {
        return;
    }
    if let Err(e) = infer.send_audio(u.id, &u.outbound) {
        tracing::debug!(%e, "audio dropped while the speech engine restarts");
    }
    u.outbound.clear();
}

/// Ends the utterance: without hands-free listening the mic is released at once, so nothing is
/// recorded after it.
fn finish(
    current: &mut Option<Utterance>,
    mic: &mut Option<Box<dyn kivo_platform::AudioStream>>,
    hands_free: bool,
    listening: &Arc<Mutex<Option<u64>>>,
    levels: &watch::Sender<f32>,
) {
    *current = None;
    if !hands_free {
        *mic = None;
    }
    *listening
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    levels.send_replace(0.0);
}

fn open_microphone(
    audio: &Arc<dyn AudioIo>,
    device: Option<&DeviceId>,
    writer: &mut kivo_audio::CaptureWriter,
    signals: &mpsc::UnboundedSender<VoiceSignal>,
) -> Option<Box<dyn kivo_platform::AudioStream>> {
    // The writer moves into the audio callback; it is recreated with the ring each time.
    let mut sink = std::mem::replace(writer, capture_ring(1).0);
    match audio.open_capture(
        device,
        Box::new(move |samples, format| sink.write(samples, format.sample_rate, format.channels)),
    ) {
        Ok(stream) => {
            tracing::debug!("microphone opened");
            Some(stream)
        }
        Err(e) => {
            tracing::warn!(%e, "couldn't open the microphone");
            let _ = signals.send(VoiceSignal::MicrophoneUnavailable);
            None
        }
    }
}

/// RMS → 0–1 on a decibel scale (the Island's waveform).
pub fn to_level(rms: f32) -> f32 {
    if rms <= 0.0 {
        return 0.0;
    }
    ((20.0 * rms.log10() - FLOOR_DB) / (TOP_DB - FLOOR_DB)).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_a_decibel_scale() {
        assert_eq!(to_level(0.0), 0.0, "silence");
        assert_eq!(to_level(0.0005), 0.0, "below −60 dBFS is silence");
        assert!((to_level(0.01) - 0.4).abs() < 1e-3, "−40 dBFS");
        assert_eq!(to_level(0.5), 1.0, "loud speech is full scale");
    }

    /// Speech, then silence: the pipeline ends the utterance by itself and releases the mic.
    #[tokio::test]
    async fn silence_after_speech_ends_the_utterance() {
        let speech: Vec<f32> = (0..16_000 * 2)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / 16_000.0;
                if t < 0.7 {
                    (t * 220.0 * std::f32::consts::TAU).sin() * 0.3
                } else {
                    0.0
                }
            })
            .collect();
        let audio = Arc::new(kivo_testkit::FakeAudio::with_clip(speech));
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let (levels, _levels_rx) = watch::channel(0.0);
        let (signals, mut rx) = mpsc::unbounded_channel();
        let qos = Arc::new(kivo_testkit::FakeThreadQos::default());
        let listener = start(Pipeline {
            audio: audio.clone(),
            device: None,
            // Without the model the pipeline still runs; the energy gate decides.
            vad_model: PathBuf::from("missing.onnx"),
            turn_model: PathBuf::from("missing"),
            infer,
            speaker: Arc::new(Speaker::new(audio, None)),
            levels,
            signals,
            qos: qos.clone(),
        });
        // Idle first: the thread waits in efficiency mode.
        let until_switched = |n: usize| {
            let qos = Arc::clone(&qos);
            async move {
                for _ in 0..200 {
                    if qos.switches.lock().unwrap().len() >= n {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
        };
        until_switched(1).await;
        assert_eq!(*qos.switches.lock().unwrap(), [true]);
        listener.listen(7, true);
        listener.stt_ready(7);
        let signal = tokio::time::timeout(Duration::from_secs(10), async {
            loop {
                match rx.recv().await {
                    Some(VoiceSignal::EndOfSpeech { utterance }) => return Some(utterance),
                    Some(_) => {}
                    None => return None,
                }
            }
        })
        .await
        .expect("the utterance ended");
        assert_eq!(signal, Some(7));
        assert!(
            !listener.is_listening(),
            "the microphone is released when the utterance ends"
        );
        // Full speed while listening, efficient again once idle (VOICE-03).
        until_switched(3).await;
        assert_eq!(*qos.switches.lock().unwrap(), [true, false, true]);
    }

    /// Hands-free with a keyword model that can't load: the pipeline says so and stays usable.
    #[tokio::test]
    async fn a_missing_keyword_model_is_reported_not_fatal() {
        let audio = Arc::new(kivo_testkit::FakeAudio::with_clip(vec![0.0; 1_600]));
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let (levels, _levels_rx) = watch::channel(0.0);
        let (signals, mut rx) = mpsc::unbounded_channel();
        let listener = start(Pipeline {
            audio: audio.clone(),
            device: None,
            vad_model: PathBuf::from("missing.onnx"),
            turn_model: PathBuf::from("missing"),
            infer,
            speaker: Arc::new(Speaker::new(audio, None)),
            levels,
            signals,
            qos: Arc::new(kivo_testkit::FakeThreadQos::default()),
        });
        listener.set_hands_free(Some(HandsFree {
            model_dir: PathBuf::from("no-such-model"),
            wake: vec![Keyword::new("hey-kivo", "Hey Kivo")],
            stop: vec![],
        }));
        let signal = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("a signal");
        assert!(matches!(signal, Some(VoiceSignal::HandsFreeFailed { .. })));
    }
}
