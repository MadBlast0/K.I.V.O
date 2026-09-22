//! The listening pipeline (VOICE §1, VOICE-02/03/04/41). The microphone is opened only while KIVO
//! listens. Capture runs on the audio thread and writes a lock-free ring buffer; one detection
//! thread reads it, runs the energy gate and Silero VAD, publishes the level for the Island's
//! waveform, and streams 16 kHz mono audio to the speech worker. Silence after speech ends the
//! utterance by itself ("auto-end on silence").

use crate::infer::Infer;
use crate::speaker::Speaker;
use kivo_audio::capture::capture_ring;
use kivo_audio::{Chunker, EnergyGate, History};
use kivo_platform::{AudioIo, DeviceId};
use kivo_voice::silero::SileroVad;
use kivo_voice::traits::VadEngine;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

/// Seconds of audio the ring holds (VOICE-03: at least 3 s, also the pre-roll for M2).
const RING_SECONDS: u32 = 5;
/// Silero's frame: 32 ms at 16 kHz.
const VAD_FRAME: usize = 512;
/// Speech is this likely before KIVO counts it as speech.
const SPEECH_PROB: f32 = 0.5;
const SILENCE_PROB: f32 = 0.35;
/// Silence this long after speech ends the utterance.
const END_SILENCE: Duration = Duration::from_millis(800);
/// Give up listening if nothing is said at all.
const NO_SPEECH_TIMEOUT: Duration = Duration::from_secs(6);
/// Longest single utterance.
const MAX_UTTERANCE: Duration = Duration::from_secs(45);
/// The Island's waveform is fed at 30 Hz (ARCHITECTURE §3).
const LEVEL_PERIOD: Duration = Duration::from_millis(33);
/// Level scale: −60 dBFS is silence, −10 dBFS is full (matches the waveform's dots-to-bars).
const FLOOR_DB: f32 = -60.0;
const TOP_DB: f32 = -10.0;

/// What the detection thread is doing.
enum Command {
    /// Start an utterance: open the mic and stream to the worker under this id.
    Listen {
        utterance: u64,
        auto_end: bool,
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
    Quit,
}

/// What the pipeline tells the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceSignal {
    /// The user started speaking (T2).
    SpeechStarted { utterance: u64 },
    /// Silence ended the utterance (VOICE-41 auto-end), or it ran too long.
    EndOfSpeech { utterance: u64 },
    /// Nothing was said.
    NoSpeech { utterance: u64 },
    /// The microphone couldn't be opened (blocked in Windows privacy settings, UX-57).
    MicrophoneUnavailable,
}

/// Controls the detection thread.
pub struct Listener {
    commands: Sender<Command>,
    listening: Arc<Mutex<Option<u64>>>,
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
    let flag = Arc::clone(&listening);
    std::thread::Builder::new()
        .name("kivo-detect".into())
        .spawn(move || run(pipeline, &rx, &flag))
        .expect("thread spawn");
    Listener {
        commands: tx,
        listening,
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
}

/// Audio goes to the speech worker in 80 ms batches: 10 ms frames inside, fewer messages out
/// (VOICE-02).
const MODEL_BATCH: usize = kivo_audio::capture::RATE as usize * 80 / 1000;

/// The most audio held while the speech worker starts (one long utterance).
const PENDING_LIMIT: usize = kivo_audio::capture::RATE as usize * 45;

#[allow(
    clippy::too_many_lines,
    reason = "one loop: mic, level, gate, VAD, streaming"
)]
fn run(pipeline: Pipeline, commands: &Receiver<Command>, listening: &Arc<Mutex<Option<u64>>>) {
    let Pipeline {
        audio,
        device,
        vad_model,
        infer,
        speaker,
        levels,
        signals,
        qos,
    } = pipeline;
    // Efficient while waiting, full speed while listening (VOICE-03).
    let mut eco = false;
    let mut vad = match SileroVad::load(&vad_model) {
        Ok(vad) => Some(vad),
        Err(e) => {
            // Without the VAD, push-to-talk still works: the key decides when speech ends.
            tracing::warn!(
                detail = e.detail(),
                "voice activity detection is unavailable"
            );
            None
        }
    };
    let mut gate = EnergyGate::new();
    let mut chunker = Chunker::new(VAD_FRAME);
    let mut history = History::new(kivo_audio::capture::RATE as usize * RING_SECONDS as usize);
    let mut current: Option<Utterance> = None;
    let mut mic = None;
    // Replaced whenever the microphone is opened; the writer moves into the audio callback.
    #[allow(
        unused_assignments,
        reason = "the first pair is replaced when the mic opens"
    )]
    let (mut writer, mut reader) = capture_ring(1);
    let mut audio_buf: Vec<f32> = Vec::with_capacity(16_000);
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
                }) => {
                    if eco {
                        qos.efficiency_mode(false);
                        eco = false;
                    }
                    if mic.is_none() {
                        let (w, r) = capture_ring(RING_SECONDS);
                        writer = w;
                        reader = r;
                        mic = open_microphone(&audio, device.as_ref(), &mut writer, &signals);
                    }
                    if mic.is_some() {
                        reader.discard();
                        chunker.clear();
                        history.clear();
                        if let Some(vad) = vad.as_mut() {
                            vad.reset();
                        }
                        current = Some(Utterance {
                            id: utterance,
                            auto_end,
                            started: Instant::now(),
                            heard_speech: false,
                            last_speech: None,
                            ending: false,
                            ready: false,
                            pending: Vec::new(),
                            outbound: Vec::with_capacity(MODEL_BATCH * 2),
                        });
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
                    mic = None;
                    let _ = levels.send_replace(0.0);
                }
                Ok(Command::Wake) => {}
                Ok(Command::Quit) | Err(TryRecvError::Disconnected) => {
                    return;
                }
                Err(TryRecvError::Empty) => break,
            }
        }

        // 2. Audio: gate → VAD → the speech worker.
        audio_buf.clear();
        reader.read(&mut audio_buf);
        if !audio_buf.is_empty() {
            history.push(&audio_buf);
            if let Some(u) = current.as_mut() {
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
                // KIVO's own cue must not count as the user speaking (VOICE-25).
                let muted = speaker.muting_microphone();
                let (vad_ref, gate_ref) = (&mut vad, &mut gate);
                let mut speech_frames = 0;
                let mut silent_frames = 0;
                chunker.push(&audio_buf, &mut |frame| {
                    let probability = if muted {
                        0.0
                    } else if gate_ref.open(frame) {
                        vad_ref
                            .as_mut()
                            .and_then(|v| v.process(frame).ok())
                            .unwrap_or(1.0)
                    } else {
                        0.0
                    };
                    if probability > SPEECH_PROB {
                        speech_frames += 1;
                    } else if probability < SILENCE_PROB {
                        silent_frames += 1;
                    }
                });
                if speech_frames > 0 {
                    if !u.heard_speech {
                        u.heard_speech = true;
                        let _ = signals.send(VoiceSignal::SpeechStarted { utterance: u.id });
                    }
                    u.last_speech = Some(Instant::now());
                }
                let _ = silent_frames;
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

        // 4. Has the utterance ended? The microphone is released before anyone hears so.
        let ended = current.as_ref().and_then(|u| {
            let silent_long_enough = u.auto_end
                && u.last_speech
                    .is_some_and(|since| since.elapsed() > END_SILENCE);
            let too_long = u.started.elapsed() > MAX_UTTERANCE;
            let nothing_said = !u.heard_speech && u.started.elapsed() > NO_SPEECH_TIMEOUT;
            if u.ending || silent_long_enough || too_long {
                Some(if u.heard_speech || u.ending {
                    VoiceSignal::EndOfSpeech { utterance: u.id }
                } else {
                    VoiceSignal::NoSpeech { utterance: u.id }
                })
            } else if nothing_said {
                let _ = infer.cancel_stt(u.id);
                Some(VoiceSignal::NoSpeech { utterance: u.id })
            } else {
                None
            }
        });
        if let Some(signal) = ended {
            // The last partial batch goes before the worker hears the utterance is over.
            if let Some(u) = current.as_mut().filter(|u| u.ready) {
                send_batch(&infer, u);
            }
            finish(&mut current, &mut mic, listening, &levels);
            let _ = signals.send(signal);
        }

        // 5. Sleep until audio arrives. Idle, the thread sleeps until a command comes (DISC-19:
        // no work at idle), waking only to pulse the Island while KIVO speaks or, once a
        // second, to close a speaker that has gone quiet.
        if current.is_some() {
            if eco {
                qos.efficiency_mode(false);
                eco = false;
            }
            reader.wait(Duration::from_millis(20));
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

/// Ends the utterance: the mic is released at once, so nothing is recorded after it.
fn finish(
    current: &mut Option<Utterance>,
    mic: &mut Option<Box<dyn kivo_platform::AudioStream>>,
    listening: &Arc<Mutex<Option<u64>>>,
    levels: &watch::Sender<f32>,
) {
    *current = None;
    *mic = None;
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
}
