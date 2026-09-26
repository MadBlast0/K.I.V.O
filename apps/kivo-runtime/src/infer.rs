//! The inference worker's supervisor (ARCHITECTURE §1, ARCH-09/21): starts `kivo-infer` on demand,
//! keeps the speech models warm per the residency policy (VOICE §8, PLAN-02), restarts the worker
//! if it crashes, and fails the turn in progress with a spoken message instead of taking KIVO down.

use kivo_ipc::infer::{
    GpuBackend, GpuTarget, INFER_PROTOCOL, InferHello, InferSlot, InferWelcome, ModelLoad,
    ModelState, ModelUnload, Residency, SttAudio, SttFinal, SttPartial, SttStart, TtsAudio,
    TtsDone, TtsSpeak, UtteranceId, encode_pcm, method,
};
use kivo_ipc::{Incoming, Peer};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, watch};
use tokio_util::sync::CancellationToken;

/// Backoff between restarts after a crash.
const FIRST_RETRY: Duration = Duration::from_millis(500);
const MAX_RETRY: Duration = Duration::from_secs(30);
/// A worker that stops sooner than this after starting crashed quickly (PLAN-12).
const STABLE_AFTER: Duration = Duration::from_secs(60);
/// Quick crashes in a row before an engine is suspected and its stand-in used.
const SUSPECT_AFTER: u32 = 3;
/// Quick crashes in a row before KIVO stops restarting the worker until the engines change.
const STOP_AFTER: u32 = 6;

/// What KIVO does after the speech worker crashed (plan §114, PLAN-12): restart it where that's
/// safe, and change what's likely to crash it again first.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// Start it again as it was.
    Restart,
    /// It crashed on the graphics card: the recognizer runs on the processor from now on this
    /// session (a driver problem is the likeliest cause).
    Cpu,
    /// It crashed on CUDA: the recognizer uses the same card through Vulkan from now on this
    /// session (VOICE-50).
    Vulkan,
    /// It keeps crashing: this engine is set aside for the session and its stand-in used.
    Fallback(String),
    /// It keeps crashing with nothing left to change: no more restarts until the speech settings
    /// change.
    Stopped,
}

impl Recovery {
    pub fn key(&self) -> &'static str {
        match self {
            Self::Restart => "restart",
            Self::Cpu => "cpu",
            Self::Vulkan => "vulkan",
            Self::Fallback(_) => "fallback",
            Self::Stopped => "stopped",
        }
    }
}

/// The recovery after the `quick`-th quick crash in a row (0 when the worker had run a while).
pub fn recovery(
    quick: u32,
    engines: &Engines,
    failed: &std::collections::HashSet<String>,
) -> Recovery {
    match engines.gpu.as_ref().map(|g| g.backend) {
        Some(GpuBackend::Cuda) => return Recovery::Vulkan,
        Some(_) => return Recovery::Cpu,
        None => {}
    }
    if quick >= STOP_AFTER {
        return Recovery::Stopped;
    }
    if quick >= SUSPECT_AFTER {
        // The recognizer first (it has a stand-in when another is installed), then the voice
        // (Windows voices stand in).
        let stt = engines
            .stt
            .as_ref()
            .filter(|_| engines.stt_fallback.is_some())
            .map(|(id, _)| id.clone());
        let tts = engines
            .tts
            .as_ref()
            .map(|(id, _)| id.clone())
            .filter(|id| id != kivo_voice::system_tts::ENGINE_ID);
        if let Some(id) = [stt, tts]
            .into_iter()
            .flatten()
            .find(|id| !failed.contains(id))
        {
            return Recovery::Fallback(id);
        }
    }
    Recovery::Restart
}

/// What the worker sends back while it works.
#[derive(Clone, Debug, PartialEq)]
pub enum InferEvent {
    Partial {
        id: u64,
        text: String,
        stable: bool,
    },
    /// Synthesized speech for utterance `id`.
    Speech {
        id: u64,
        rate: u32,
        pcm: Vec<f32>,
    },
    /// The worker crashed and what KIVO does about it (PLAN-12). `crashes` counts quick crashes
    /// in a row.
    Crashed {
        crashes: u32,
        recovery: Recovery,
    },
    SpeakDone {
        id: u64,
        error: Option<String>,
        cancelled: bool,
    },
    Residency {
        slot: InferSlot,
        engine: String,
        state: Residency,
    },
    /// A chosen engine failed to load, so the worker uses `to` instead for the rest of the
    /// session (VOICE-47); `None` when nothing could replace it. The saved choice is unchanged.
    Fallback {
        slot: InferSlot,
        from: String,
        to: Option<String>,
        error: String,
    },
    /// The worker stopped; the current turn can't continue.
    Lost,
}

/// Which engines the worker should hold.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Engines {
    /// Speech-to-text engine id and its model folder.
    pub stt: Option<(String, PathBuf)>,
    /// Another installed speech-to-text engine, used if `stt` fails to load (VOICE-47).
    pub stt_fallback: Option<(String, PathBuf)>,
    /// Text-to-speech engine id ("system", "kokoro-82m") and its model folder, if it has one.
    pub tts: Option<(String, Option<PathBuf>)>,
    pub threads: usize,
    /// The language KIVO speaks and hears.
    pub language: String,
    /// Keys and addresses for the cloud engines among these, by engine id (VOICE-10/11).
    pub cloud: std::collections::BTreeMap<String, kivo_ipc::infer::CloudLoad>,
    /// The graphics card and backend the speech recognizer may run on (PLAN-09, VOICE-50); `None`
    /// for the processor.
    pub gpu: Option<GpuTarget>,
}

#[derive(Debug, thiserror::Error)]
pub enum InferError {
    #[error("{0}")]
    Engine(String),
    #[error("KIVO's speech engine is restarting")]
    NotReady,
}

/// A handle to the worker. Cloning is cheap; every call goes to the live connection.
///
/// Residency (VOICE §8, PLAN-02): the engines the settings choose are only *configured* here.
/// They are loaded when a request starts (`warm`: prewarm on turn start) and unloaded after
/// `warm_for` without use; with nothing loaded the worker process exits, so an idle KIVO holds no
/// model at all (plan §128).
#[derive(Clone)]
pub struct Infer {
    peer: Arc<Mutex<Option<Peer>>>,
    ready: watch::Sender<bool>,
    engines: watch::Sender<Engines>,
    configured: Arc<Mutex<Engines>>,
    /// Bumped on every use; the cool-down timer only unloads if nothing used the models since.
    used: watch::Sender<u64>,
    warm_for: Arc<Mutex<Duration>>,
    next_id: Arc<AtomicU64>,
    /// The running worker's process id (0 while none runs).
    pid: Arc<std::sync::atomic::AtomicU32>,
    program: PathBuf,
    /// Engines that failed to load this session; their fallbacks stand in (VOICE-47).
    failed: Arc<Mutex<std::collections::HashSet<String>>>,
    /// Speaking speed in percent (UX-61).
    speed: Arc<Mutex<u16>>,
    /// The worker crashed on the graphics card: the recognizer stays on the processor this session
    /// (PLAN-12).
    gpu_off: Arc<std::sync::atomic::AtomicBool>,
    /// The CUDA worker crashed: CUDA targets use Vulkan this session (VOICE-50).
    cuda_off: Arc<std::sync::atomic::AtomicBool>,
    /// The CUDA worker, once its runtime is installed (VOICE-50).
    cuda: Arc<Mutex<Option<CudaWorker>>>,
}

/// The speech worker built with CUDA and the folder holding NVIDIA's runtime libraries, which go
/// on its `PATH` (VOICE-50).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CudaWorker {
    pub program: PathBuf,
    pub libraries: PathBuf,
}

impl Infer {
    /// Creates the handle and the event stream; nothing starts until `ensure` is called.
    pub fn new(
        program: PathBuf,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<InferEvent>,
        mpsc::UnboundedSender<InferEvent>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();
        (
            Self {
                peer: Arc::default(),
                ready: watch::Sender::new(false),
                engines: watch::Sender::new(Engines::default()),
                configured: Arc::default(),
                used: watch::Sender::new(0),
                warm_for: Arc::new(Mutex::new(Duration::from_secs(10 * 60))),
                next_id: Arc::new(AtomicU64::new(1)),
                pid: Arc::default(),
                program,
                failed: Arc::default(),
                speed: Arc::new(Mutex::new(100)),
                gpu_off: Arc::default(),
                cuda_off: Arc::default(),
                cuda: Arc::default(),
            },
            rx,
            tx,
        )
    }

    /// Whether the recognizer was taken off the graphics card after a crash there (PLAN-12).
    pub fn gpu_off(&self) -> bool {
        self.gpu_off.load(Ordering::Relaxed)
    }

    /// Sets the CUDA worker (once its runtime is installed) or takes it away; a running CUDA worker
    /// is replaced by the other one at once (VOICE-50).
    pub fn set_cuda(&self, cuda: Option<CudaWorker>) {
        let changed = {
            let mut current = lock(&self.cuda);
            let changed = *current != cuda;
            *current = cuda;
            changed
        };
        if changed {
            // Wakes the supervisor, which starts the worker the engines now call for.
            self.engines.send_modify(|_| {});
        }
    }

    pub fn cuda(&self) -> Option<CudaWorker> {
        lock(&self.cuda).clone()
    }

    /// The worker program for `engines` and the folder its libraries come from: the CUDA worker
    /// for a CUDA target when there is one, else the usual worker (which runs a CUDA target on its
    /// own GPU backend).
    fn launch_for(&self, engines: &Engines) -> (PathBuf, Option<PathBuf>) {
        match (engines.gpu.as_ref().map(|g| g.backend), self.cuda()) {
            (Some(GpuBackend::Cuda), Some(cuda)) => (cuda.program, Some(cuda.libraries)),
            _ => (self.program.clone(), None),
        }
    }

    /// A second handle for a worker that tries an engine out (VOICE-45), with this one's CUDA
    /// worker.
    pub fn probe(
        &self,
    ) -> (
        Self,
        mpsc::UnboundedReceiver<InferEvent>,
        mpsc::UnboundedSender<InferEvent>,
    ) {
        let (probe, events, sender) = Self::new(self.program.clone());
        probe.set_cuda(self.cuda());
        (probe, events, sender)
    }

    /// Where the recognizer runs now (the Performance page): the configured target, after any
    /// crash moved it (off CUDA, or off the card).
    pub fn effective_gpu(&self) -> Option<GpuTarget> {
        self.safe(self.configured()).gpu
    }

    /// The engines as they may run now: off the graphics card after a crash there.
    fn safe(&self, mut engines: Engines) -> Engines {
        if self.gpu_off() {
            engines.gpu = None;
        }
        if self.cuda_off.load(Ordering::Relaxed)
            && let Some(gpu) = &mut engines.gpu
            && gpu.backend == GpuBackend::Cuda
        {
            gpu.backend = GpuBackend::Vulkan;
        }
        engines
    }

    /// Lets an engine that failed earlier in the session be tried again (it was reinstalled, or
    /// passed a test in another worker).
    pub fn forgive(&self, engine: &str) {
        lock(&self.failed).remove(engine);
    }

    pub fn is_ready(&self) -> bool {
        *self.ready.borrow()
    }

    /// Waits until the worker is connected and the models are loaded.
    pub async fn wait_ready(&self, within: Duration) -> bool {
        if self.is_ready() {
            return true;
        }
        let mut ready = self.ready.subscribe();
        tokio::time::timeout(within, async move {
            while !*ready.borrow_and_update() {
                if ready.changed().await.is_err() {
                    return false;
                }
            }
            true
        })
        .await
        .unwrap_or(false)
    }

    /// Sets which engines to keep loaded now (the supervisor loads them, and reloads after a
    /// restart). Most callers use `configure` and `warm` instead.
    pub fn set_engines(&self, engines: Engines) {
        self.engines.send_replace(engines);
    }

    /// Loads the engines again if they are loaded: a model's files changed under them (an update
    /// that brought new voices).
    pub fn reload(&self) {
        self.engines.send_modify(|_| {});
    }

    /// The engines the settings choose. If they are loaded, they are reloaded as configured.
    /// The engines configured now (diagnostics and tests).
    pub fn configured(&self) -> Engines {
        lock(&self.configured).clone()
    }

    pub fn configure(&self, engines: Engines, warm_for: Duration) {
        *lock(&self.warm_for) = warm_for;
        *lock(&self.configured) = engines.clone();
        let loaded = {
            let current = self.engines.borrow();
            current.stt.is_some() || current.tts.is_some()
        };
        if loaded {
            self.engines.send_replace(engines);
        }
    }

    /// A request is starting: load the configured engines now if they aren't (prewarm on turn
    /// start, VOICE-34) and restart the cool-down.
    pub fn warm(&self) {
        let configured = lock(&self.configured).clone();
        self.engines.send_if_modified(|current| {
            let changed = *current != configured;
            if changed {
                current.clone_from(&configured);
            }
            changed
        });
        self.used.send_modify(|n| *n += 1);
    }

    /// Keeps the models warm (called when a request finishes).
    pub fn touch(&self) {
        self.used.send_modify(|n| *n += 1);
    }

    /// The residency of each slot, as the worker last reported it.
    pub fn loaded(&self) -> bool {
        let e = self.engines.borrow();
        e.stt.is_some() || e.tts.is_some()
    }

    /// The worker's process id, if one is running (diagnostics and tests).
    pub fn worker_pid(&self) -> Option<u32> {
        Some(self.pid.load(Ordering::Relaxed)).filter(|&p| p != 0)
    }

    pub fn next_utterance(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::Relaxed)
    }

    /// Unloads the models once they have gone `warm_for` without use (VOICE §8). Runs until
    /// shutdown.
    pub async fn cool_down(self, shutdown: CancellationToken) {
        let mut used = self.used.subscribe();
        loop {
            let seen = *used.borrow_and_update();
            let warm_for = *lock(&self.warm_for);
            tokio::select! {
                () = tokio::time::sleep(warm_for) => {
                    if *self.used.borrow() == seen && self.loaded() {
                        tracing::info!(minutes = warm_for.as_secs() / 60, "speech models unloaded after going unused");
                        self.engines.send_replace(Engines::default());
                    }
                    // Cold now: wait for the next use (or shutdown) before timing again.
                    tokio::select! {
                        changed = used.changed() => if changed.is_err() { return },
                        () = shutdown.cancelled() => return,
                    }
                }
                changed = used.changed() => if changed.is_err() { return },
                () = shutdown.cancelled() => return,
            }
        }
    }

    fn peer(&self) -> Result<Peer, InferError> {
        self.peer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
            .filter(|p| !p.is_closed())
            .ok_or(InferError::NotReady)
    }

    async fn request(&self, method: &str, params: Value) -> Result<Value, InferError> {
        self.peer()?
            .request(method, params)
            .await
            .map_err(|e| InferError::Engine(e.to_string()))
    }

    fn notify(&self, method: &str, params: Value) -> Result<(), InferError> {
        self.peer()?
            .notify(method, params)
            .map_err(|_| InferError::NotReady)
    }

    /// Starts transcribing utterance `id`.
    pub async fn start_stt(
        &self,
        id: u64,
        language: &str,
        vocabulary: Vec<String>,
    ) -> Result<(), InferError> {
        let params = json!(SttStart {
            id,
            language: language.to_owned(),
            vocabulary
        });
        self.request(method::STT_START, params).await.map(|_| ())
    }

    /// Sends 16 kHz mono audio for an utterance.
    pub fn send_audio(&self, id: u64, pcm: &[f32]) -> Result<(), InferError> {
        self.notify(
            method::STT_AUDIO,
            json!(SttAudio {
                id,
                pcm: encode_pcm(pcm)
            }),
        )
    }

    /// Ends the utterance and returns the final transcript.
    pub async fn finish_stt(&self, id: u64) -> Result<SttFinal, InferError> {
        let value = self
            .request(method::STT_FINISH, json!(UtteranceId { id }))
            .await?;
        serde_json::from_value(value).map_err(|e| InferError::Engine(e.to_string()))
    }

    pub fn cancel_stt(&self, id: u64) -> Result<(), InferError> {
        self.notify(method::STT_CANCEL, json!(UtteranceId { id }))
    }

    /// The speaking speed for replies, in percent (UX-61).
    pub fn set_speed(&self, percent: u16) {
        *lock(&self.speed) = percent.clamp(50, 200);
    }

    /// Speaks `text`; the audio arrives as `InferEvent::Speech`, then `SpeakDone`.
    pub async fn speak(&self, id: u64, text: &str, voice: Option<&str>) -> Result<(), InferError> {
        let speed = f32::from(*lock(&self.speed)) / 100.0;
        let params = json!(TtsSpeak {
            id,
            text: text.to_owned(),
            voice: voice.map(str::to_owned),
            speed: ((speed - 1.0).abs() > f32::EPSILON).then_some(speed),
        });
        self.request(method::TTS_SPEAK, params).await.map(|_| ())
    }

    pub fn cancel_speech(&self, id: u64) -> Result<(), InferError> {
        self.notify(method::TTS_CANCEL, json!(UtteranceId { id }))
    }

    /// Asks the worker to stop (shutdown order, plan §127).
    pub async fn shutdown(&self) {
        if let Ok(peer) = self.peer() {
            let _ = ask_to_stop(&peer).await;
        }
    }
}

/// Asks the worker to stop, waiting briefly for its answer: a worker that doesn't answer is
/// killed by the caller anyway, instead of the wait holding the kill back for ever.
async fn ask_to_stop(peer: &Peer) {
    let _ = tokio::time::timeout(STOP_WAIT, peer.request(method::SHUTDOWN, Value::Null)).await;
}

/// How long a stopping worker may take to answer.
const STOP_WAIT: Duration = Duration::from_secs(2);

/// Starts `kivo-infer` and keeps it running until shutdown.
pub async fn supervise(
    infer: Infer,
    events: mpsc::UnboundedSender<InferEvent>,
    shutdown: CancellationToken,
) {
    let mut backoff = FIRST_RETRY;
    let mut quick = 0_u32;
    let mut engines_rx = infer.engines.subscribe();
    loop {
        if shutdown.is_cancelled() {
            return;
        }
        // Nothing to run until an engine is wanted (no models loaded at idle, plan §128).
        while {
            let engines = engines_rx.borrow_and_update();
            engines.stt.is_none() && engines.tts.is_none()
        } {
            tokio::select! {
                changed = engines_rx.changed() => if changed.is_err() { return },
                () = shutdown.cancelled() => return,
            }
        }
        let started = tokio::time::Instant::now();
        let running = infer.safe(engines_rx.borrow().clone());
        match run_worker(&infer, &events, &shutdown, &mut engines_rx).await {
            Ok(()) => {
                backoff = FIRST_RETRY;
                quick = 0;
            }
            Err(e) => {
                tracing::error!(%e, "the speech worker stopped");
                infer.ready.send_replace(false);
                let _ = events.send(InferEvent::Lost);
                // Restart where it's safe (PLAN-12): change what likely crashed it first.
                quick = if started.elapsed() < STABLE_AFTER {
                    quick + 1
                } else {
                    1
                };
                let then = recovery(quick, &running, &lock(&infer.failed));
                tracing::warn!(
                    crashes = quick,
                    recovery = then.key(),
                    "speech worker recovery"
                );
                let _ = events.send(InferEvent::Crashed {
                    crashes: quick,
                    recovery: then.clone(),
                });
                match &then {
                    Recovery::Cpu => infer.gpu_off.store(true, Ordering::Relaxed),
                    Recovery::Vulkan => infer.cuda_off.store(true, Ordering::Relaxed),
                    Recovery::Fallback(id) => {
                        lock(&infer.failed).insert(id.clone());
                    }
                    Recovery::Stopped => {
                        // Wait until the speech settings change (or KIVO quits), then try again.
                        engines_rx.borrow_and_update();
                        tokio::select! {
                            changed = engines_rx.changed() => if changed.is_err() { return },
                            () = shutdown.cancelled() => return,
                        }
                        quick = 0;
                        backoff = FIRST_RETRY;
                        continue;
                    }
                    Recovery::Restart => {}
                }
                tokio::select! {
                    () = tokio::time::sleep(backoff) => {}
                    () = shutdown.cancelled() => return,
                }
                backoff = (backoff * 2).min(MAX_RETRY);
            }
        }
        if shutdown.is_cancelled() {
            return;
        }
    }
}

/// One worker lifetime: start, handshake, load models, forward notifications.
async fn run_worker(
    infer: &Infer,
    events: &mpsc::UnboundedSender<InferEvent>,
    shutdown: &CancellationToken,
    engines_rx: &mut watch::Receiver<Engines>,
) -> Result<(), String> {
    let wanted = infer.safe(engines_rx.borrow_and_update().clone());
    let launch = infer.launch_for(&wanted);
    let mut child = spawn(&launch.0, launch.1.as_deref())?;
    let stdin = child.stdin.take().ok_or("no stdin")?;
    let stdout = child.stdout.take().ok_or("no stdout")?;
    if let Some(stderr) = child.stderr.take() {
        // The worker's log lines join the runtime's log.
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!(target: "kivo_infer", "{line}");
            }
        });
    }
    let (peer, mut incoming) = Peer::spawn(tokio::io::join(stdout, stdin));
    let welcome: InferWelcome = serde_json::from_value(
        peer.request(
            method::HELLO,
            json!(InferHello {
                protocol: INFER_PROTOCOL
            }),
        )
        .await
        .map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    tracing::info!(
        version = welcome.version,
        pid = welcome.pid,
        backend = ?welcome.backend,
        "speech worker started"
    );
    if launch.1.is_some() && welcome.backend != Some(GpuBackend::Cuda) {
        // A CUDA worker that isn't one (a development build made without `--features cuda`).
        tracing::warn!(backend = ?welcome.backend, "the CUDA worker wasn't built with CUDA");
    }
    infer.pid.store(welcome.pid, Ordering::Relaxed);
    *infer
        .peer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(peer.clone());
    load_engines(&peer, &wanted, &infer.failed, events).await?;
    infer.ready.send_replace(true);

    let result = loop {
        tokio::select! {
            message = incoming.recv() => match message {
                Some(Incoming::Notification(n)) => forward(&n.method, n.params, events),
                Some(Incoming::Request(_)) => {}
                None => break Err("the speech worker closed the connection".to_owned()),
            },
            changed = engines_rx.changed() => {
                if changed.is_err() {
                    break Ok(());
                }
                let engines = infer.safe(engines_rx.borrow_and_update().clone());
                if engines.stt.is_none() && engines.tts.is_none() {
                    // Nothing to hold: the worker process goes away until it is needed again.
                    let _ = ask_to_stop(&peer).await;
                    break Ok(());
                }
                if infer.launch_for(&engines) != launch {
                    // Another worker program (CUDA in or out): the supervisor starts it next.
                    let _ = ask_to_stop(&peer).await;
                    break Ok(());
                }
                if let Err(e) = load_engines(&peer, &engines, &infer.failed, events).await {
                    break Err(e);
                }
            }
            status = child.wait() => break Err(format!("the speech worker exited: {status:?}")),
            () = shutdown.cancelled() => {
                let _ = ask_to_stop(&peer).await;
                break Ok(());
            }
        }
    };
    infer.ready.send_replace(false);
    infer.pid.store(0, Ordering::Relaxed);
    *infer
        .peer
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
    let _ = child.start_kill();
    result
}

/// Starts `program`; `libraries` (NVIDIA's CUDA runtime) goes first on its `PATH`, where Windows
/// looks for the DLLs it loads.
fn spawn(program: &std::path::Path, libraries: Option<&std::path::Path>) -> Result<Child, String> {
    let mut command = Command::new(program);
    if let Some(libraries) = libraries {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let joined = std::env::join_paths(
            std::iter::once(libraries.to_path_buf()).chain(std::env::split_paths(&path)),
        )
        .map_err(|e| e.to_string())?;
        command.env("PATH", joined);
    }
    command
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // CREATE_NO_WINDOW: no console flashes when the worker starts.
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    command
        .spawn()
        .map_err(|e| format!("couldn't start {}: {e}", program.display()))
}

/// Loads the wanted engines. An engine that fails to load doesn't take the worker down: the STT
/// fallback or the Windows voices stand in for the session, and the runtime is told so it can
/// say so (VOICE-47). Only a broken connection is an error.
async fn load_engines(
    peer: &Peer,
    engines: &Engines,
    failed: &Mutex<std::collections::HashSet<String>>,
    events: &mpsc::UnboundedSender<InferEvent>,
) -> Result<(), String> {
    let threads = engines.threads.max(1);
    let language = &engines.language;
    let load = |slot: InferSlot, engine: &str, dir: Option<&std::path::Path>| {
        let params = json!(ModelLoad {
            slot,
            engine: engine.to_owned(),
            dir: dir.map(|d| d.to_string_lossy().into_owned()),
            threads,
            language: Some(language.clone()),
            cloud: engines.cloud.get(engine).cloned(),
            gpu: (slot == InferSlot::Stt)
                .then(|| engines.gpu.clone())
                .flatten(),
        });
        async move { peer.request(method::MODEL_LOAD, params).await }
    };
    let unload = |slot: InferSlot| async move {
        let _ = peer
            .request(method::MODEL_UNLOAD, json!(ModelUnload { slot }))
            .await;
    };
    // Tries `candidates` in order, skipping ones that failed before; reports a stand-in.
    let fill = |slot: InferSlot, candidates: Vec<(String, Option<PathBuf>)>| async move {
        let Some(chosen) = candidates.first().map(|(id, _)| id.clone()) else {
            unload(slot).await;
            return Ok(());
        };
        let mut error = None;
        for (id, dir) in &candidates {
            if lock(failed).contains(id) {
                continue;
            }
            match load(slot, id, dir.as_deref()).await {
                Ok(_) => {
                    if *id != chosen {
                        let _ = events.send(InferEvent::Fallback {
                            slot,
                            from: chosen.clone(),
                            to: Some(id.clone()),
                            error: error.clone().unwrap_or_default(),
                        });
                    }
                    return Ok(());
                }
                Err(_) if peer.is_closed() => {
                    return Err("the speech worker closed the connection".to_owned());
                }
                Err(e) => {
                    tracing::error!(%e, engine = id, ?slot, "a speech engine failed to load");
                    lock(failed).insert(id.clone());
                    error = Some(e.to_string());
                }
            }
        }
        unload(slot).await;
        let _ = events.send(InferEvent::Fallback {
            slot,
            from: chosen,
            to: None,
            error: error.unwrap_or_default(),
        });
        Ok(())
    };
    let stt: Vec<(String, Option<PathBuf>)> = engines
        .stt
        .iter()
        .chain(engines.stt_fallback.iter())
        .map(|(id, dir)| (id.clone(), Some(dir.clone())))
        .collect();
    fill(InferSlot::Stt, stt).await?;
    let mut tts: Vec<(String, Option<PathBuf>)> = engines.tts.iter().cloned().collect();
    if tts
        .first()
        .is_some_and(|(id, _)| id != kivo_voice::system_tts::ENGINE_ID)
    {
        tts.push((kivo_voice::system_tts::ENGINE_ID.to_owned(), None));
    }
    fill(InferSlot::Tts, tts).await
}

fn forward(method_name: &str, params: Value, events: &mpsc::UnboundedSender<InferEvent>) {
    let sent = match method_name {
        method::STT_PARTIAL => {
            serde_json::from_value::<SttPartial>(params).map(|p| InferEvent::Partial {
                id: p.id,
                text: p.text,
                stable: p.stable,
            })
        }
        method::TTS_AUDIO => {
            serde_json::from_value::<TtsAudio>(params).map(|a| InferEvent::Speech {
                id: a.id,
                rate: a.rate,
                pcm: kivo_ipc::infer::decode_pcm(&a.pcm).unwrap_or_default(),
            })
        }
        method::TTS_DONE => {
            serde_json::from_value::<TtsDone>(params).map(|d| InferEvent::SpeakDone {
                id: d.id,
                error: d.error,
                cancelled: d.cancelled,
            })
        }
        method::MODEL_STATE => {
            serde_json::from_value::<ModelState>(params).map(|m| InferEvent::Residency {
                slot: m.slot,
                engine: m.engine,
                state: m.state,
            })
        }
        other => {
            tracing::warn!(method = other, "unknown message from the speech worker");
            return;
        }
    };
    match sent {
        Ok(event) => {
            let _ = events.send(event);
        }
        Err(e) => tracing::warn!(%e, "unreadable message from the speech worker"),
    }
}

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PLAN-12: a crash on the graphics card moves the recognizer to the processor; repeated
    /// quick crashes set the recognizer, then the voice, aside for their stand-ins; then restarts
    /// stop until the settings change.
    #[test]
    fn crashes_are_recovered_where_it_is_safe() {
        let mut engines = Engines {
            stt: Some(("parakeet-tdt-0.6b-v3".into(), PathBuf::from("p"))),
            stt_fallback: Some(("moonshine-base".into(), PathBuf::from("m"))),
            tts: Some(("kokoro-82m".into(), None)),
            gpu: Some(GpuTarget {
                backend: GpuBackend::Cuda,
                device: "RTX".into(),
            }),
            ..Engines::default()
        };
        let mut failed = std::collections::HashSet::new();
        // A crash on CUDA tries Vulkan on the same card; one on Vulkan, the processor.
        assert_eq!(recovery(1, &engines, &failed), Recovery::Vulkan);
        if let Some(gpu) = &mut engines.gpu {
            gpu.backend = GpuBackend::Vulkan;
        }
        assert_eq!(recovery(1, &engines, &failed), Recovery::Cpu);
        engines.gpu = None;
        assert_eq!(recovery(1, &engines, &failed), Recovery::Restart);
        assert_eq!(recovery(2, &engines, &failed), Recovery::Restart);
        assert_eq!(
            recovery(3, &engines, &failed),
            Recovery::Fallback("parakeet-tdt-0.6b-v3".into())
        );
        failed.insert("parakeet-tdt-0.6b-v3".to_owned());
        assert_eq!(
            recovery(4, &engines, &failed),
            Recovery::Fallback("kokoro-82m".into())
        );
        failed.insert("kokoro-82m".to_owned());
        assert_eq!(recovery(5, &engines, &failed), Recovery::Restart);
        assert_eq!(recovery(6, &engines, &failed), Recovery::Stopped);
        // No other recognizer installed: only the voice can be set aside.
        let alone = Engines {
            stt: Some(("moonshine-base".into(), PathBuf::from("m"))),
            tts: Some((kivo_voice::system_tts::ENGINE_ID.into(), None)),
            ..Engines::default()
        };
        assert_eq!(
            recovery(3, &alone, &std::collections::HashSet::new()),
            Recovery::Restart
        );
    }

    /// The CUDA worker runs a CUDA target, with NVIDIA's libraries on its PATH; without one, or
    /// after it crashed, the usual worker runs the card through Vulkan (VOICE-50).
    #[test]
    fn a_cuda_target_starts_the_cuda_worker_when_there_is_one() {
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let cuda = Engines {
            gpu: Some(GpuTarget {
                backend: GpuBackend::Cuda,
                device: "RTX".into(),
            }),
            ..Engines::default()
        };
        assert_eq!(infer.launch_for(&cuda), (PathBuf::from("kivo-infer"), None));
        infer.set_cuda(Some(CudaWorker {
            program: PathBuf::from("pack/kivo-infer-cuda.exe"),
            libraries: PathBuf::from("pack"),
        }));
        assert_eq!(
            infer.launch_for(&cuda),
            (
                PathBuf::from("pack/kivo-infer-cuda.exe"),
                Some(PathBuf::from("pack"))
            )
        );
        let (probe, _e, _s) = infer.probe();
        assert_eq!(probe.cuda(), infer.cuda(), "a test worker uses it too");
        let vulkan = Engines {
            gpu: Some(GpuTarget {
                backend: GpuBackend::Vulkan,
                device: "RTX".into(),
            }),
            ..Engines::default()
        };
        assert_eq!(infer.launch_for(&vulkan).0, PathBuf::from("kivo-infer"));
        // After a crash on CUDA: Vulkan on the same card.
        infer.cuda_off.store(true, Ordering::Relaxed);
        let safe = infer.safe(cuda);
        assert_eq!(
            safe.gpu.as_ref().map(|g| g.backend),
            Some(GpuBackend::Vulkan)
        );
        assert_eq!(infer.launch_for(&safe).0, PathBuf::from("kivo-infer"));
    }

    /// With the CUDA worker built (`pnpm build:cuda`), the CUDA pack installed (`KIVO_CUDA_PACK`,
    /// the folder with NVIDIA's DLLs) and whisper.cpp's base.en model (`KIVO_WHISPER_CPP_DIR`, a
    /// recording in `KIVO_WHISPER_CPP_WAV`): a CUDA target starts the CUDA worker with the pack on
    /// its PATH, and it transcribes on the NVIDIA card (`KIVO_CUDA_DEVICE`, default any "NVIDIA")
    /// (VOICE-50).
    #[tokio::test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    async fn a_cuda_target_transcribes_on_the_cuda_worker_when_it_is_here() {
        let var = |name| std::env::var_os(name).map(PathBuf::from);
        let (Some(pack), Some(models), Some(wav)) = (
            var("KIVO_CUDA_PACK"),
            var("KIVO_WHISPER_CPP_DIR"),
            var("KIVO_WHISPER_CPP_WAV"),
        ) else {
            eprintln!(
                "KIVO_CUDA_PACK / KIVO_WHISPER_CPP_DIR / KIVO_WHISPER_CPP_WAV not set; skipping"
            );
            return;
        };
        // The test runs from target\debug\deps; the worker is in target\debug.
        let program = std::env::current_exe()
            .ok()
            .and_then(|exe| {
                exe.parent()?
                    .parent()
                    .map(|d| d.join("kivo-infer-cuda.exe"))
            })
            .filter(|p| p.is_file());
        let Some(program) = program else {
            eprintln!("kivo-infer-cuda isn't built (pnpm build:cuda); skipping");
            return;
        };
        let device = std::env::var("KIVO_CUDA_DEVICE").unwrap_or_else(|_| "NVIDIA".into());
        let (infer, _events, sender) = Infer::new(PathBuf::from("no-usual-worker.exe"));
        infer.set_cuda(Some(CudaWorker {
            program,
            libraries: pack,
        }));
        let stop = CancellationToken::new();
        let task = tokio::spawn(supervise(infer.clone(), sender, stop.clone()));
        infer.set_engines(Engines {
            stt: Some(("whisper-cpp-base-en".into(), models)),
            threads: 4,
            language: "en".into(),
            gpu: Some(GpuTarget {
                backend: GpuBackend::Cuda,
                device,
            }),
            ..Engines::default()
        });
        assert!(
            infer.wait_ready(Duration::from_secs(120)).await,
            "the CUDA worker started and loaded the model"
        );
        let audio = kivo_voice::utterance::wav_samples(&std::fs::read(wav).unwrap());
        let id = infer.next_utterance();
        infer.start_stt(id, "en", Vec::new()).await.unwrap();
        for piece in audio.chunks(1_280) {
            infer.send_audio(id, piece).unwrap();
        }
        let heard = infer.finish_stt(id).await.unwrap();
        eprintln!("CUDA worker: {:?} in {} ms", heard.text, heard.millis);
        assert!(
            heard.text.to_lowercase().contains("country"),
            "{}",
            heard.text
        );
        stop.cancel();
        let _ = tokio::time::timeout(Duration::from_secs(5), task).await;
    }

    #[tokio::test]
    async fn calls_fail_clearly_while_the_worker_is_down() {
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        assert!(!infer.is_ready());
        let err = infer.start_stt(1, "en", Vec::new()).await.unwrap_err();
        assert_eq!(err.to_string(), "KIVO's speech engine is restarting");
        assert!(infer.send_audio(1, &[0.0]).is_err());
        assert!(!infer.wait_ready(Duration::from_millis(50)).await);
    }

    #[tokio::test]
    async fn utterance_ids_are_unique() {
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let ids: Vec<u64> = (0..3).map(|_| infer.next_utterance()).collect();
        assert_eq!(ids, [1, 2, 3]);
    }

    #[tokio::test(start_paused = true)]
    async fn models_load_on_use_and_unload_after_the_warm_time() {
        let (infer, _events, _tx) = Infer::new(PathBuf::from("kivo-infer"));
        let engines = Engines {
            stt: None,
            stt_fallback: None,
            tts: Some(("system".into(), None)),
            threads: 1,
            language: "en-US".into(),
            cloud: Default::default(),
            gpu: None,
        };
        infer.configure(engines.clone(), Duration::from_secs(600));
        assert!(!infer.loaded(), "configuring loads nothing");
        let shutdown = CancellationToken::new();
        let cooling = tokio::spawn(infer.clone().cool_down(shutdown.clone()));
        infer.warm();
        assert!(infer.loaded(), "a request loads the engines");
        tokio::time::sleep(Duration::from_secs(300)).await;
        infer.touch();
        tokio::time::sleep(Duration::from_secs(400)).await;
        assert!(infer.loaded(), "use restarts the cool-down");
        tokio::time::sleep(Duration::from_secs(300)).await;
        assert!(!infer.loaded(), "unloaded after 10 minutes unused");
        shutdown.cancel();
        cooling.await.unwrap();
    }

    #[test]
    fn worker_messages_become_events() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        forward(
            method::STT_PARTIAL,
            json!({"id": 4, "text": "open chr", "stable": false}),
            &tx,
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            InferEvent::Partial {
                id: 4,
                text: "open chr".into(),
                stable: false
            }
        );
        forward(
            method::TTS_DONE,
            json!({"id": 4, "error": null, "cancelled": true}),
            &tx,
        );
        assert_eq!(
            rx.try_recv().unwrap(),
            InferEvent::SpeakDone {
                id: 4,
                error: None,
                cancelled: true
            }
        );
        forward("nonsense", json!({}), &tx);
        assert!(
            rx.try_recv().is_err(),
            "unknown messages are ignored, not delivered"
        );
    }
}
