//! Cloud speech engines (VOICE §3, VOICE-10/11): speech-to-text from Deepgram Flux and AssemblyAI
//! (streaming over WebSocket, with partials) and OpenAI (the utterance uploaded when it ends), and
//! text-to-speech from Cartesia Sonic, ElevenLabs Flash, Azure neural voices, OpenAI and Deepgram
//! Aura (raw PCM streamed back over HTTP).
//!
//! They run on the speech worker's threads, so they are blocking. Every one is egress: the runtime
//! only loads one when the privacy mode and the Cloud AI capability allow it (SECURITY §6), and
//! the user's key comes from Credential Manager with the load, never from a file. Each takes a
//! base-URL override so tests talk to a local stand-in.

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::{VoiceError, VoiceResult};
use crate::traits::{
    AudioSink, SAMPLE_RATE, SttEngine, SttEvent, SttOptions, SttStream, TtsEngine, VoiceInfo,
};
use serde_json::{Value, json};
use std::io::Read;
use std::net::TcpStream;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tungstenite::client::IntoClientRequest;
use tungstenite::stream::MaybeTlsStream;
use tungstenite::{Message, WebSocket};

/// What a cloud engine needs to reach its service.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccess {
    /// The user's API key (from Credential Manager; never logged).
    pub key: String,
    /// Another address for the service (tests, a proxy the user set).
    #[serde(default)]
    pub base_url: Option<String>,
    /// The Azure region ("westeurope").
    #[serde(default)]
    pub region: Option<String>,
}

/// A cloud engine KIVO knows.
#[derive(Clone, Copy, Debug)]
pub struct Provider {
    pub id: &'static str,
    pub name: &'static str,
    pub slot: EngineSlot,
    /// Whose key it uses (Credential Manager: `secret://kivo/<vendor>/api-key`).
    pub vendor: &'static str,
    pub streaming: bool,
    /// Languages it hears or speaks well (BCP-47 primary tags); `*` for many.
    pub languages: &'static [&'static str],
}

pub const DEEPGRAM_FLUX: &str = "deepgram-flux";
pub const ASSEMBLYAI: &str = "assemblyai-streaming";
pub const OPENAI_STT: &str = "openai-transcribe";
pub const CARTESIA: &str = "cartesia-sonic";
pub const ELEVENLABS: &str = "elevenlabs-flash";
pub const AZURE: &str = "azure-neural";
pub const OPENAI_TTS: &str = "openai-tts";
pub const DEEPGRAM_AURA: &str = "deepgram-aura";

pub const PROVIDERS: [Provider; 8] = [
    Provider {
        id: DEEPGRAM_FLUX,
        name: "Deepgram Flux",
        slot: EngineSlot::Stt,
        vendor: "deepgram",
        streaming: true,
        languages: &["en"],
    },
    Provider {
        id: ASSEMBLYAI,
        name: "AssemblyAI Streaming",
        slot: EngineSlot::Stt,
        vendor: "assemblyai",
        streaming: true,
        languages: &["en"],
    },
    Provider {
        id: OPENAI_STT,
        name: "OpenAI Transcribe",
        slot: EngineSlot::Stt,
        vendor: "openai",
        streaming: false,
        languages: &["*"],
    },
    Provider {
        id: CARTESIA,
        name: "Cartesia Sonic",
        slot: EngineSlot::Tts,
        vendor: "cartesia",
        streaming: true,
        languages: &["*"],
    },
    Provider {
        id: ELEVENLABS,
        name: "ElevenLabs Flash",
        slot: EngineSlot::Tts,
        vendor: "elevenlabs",
        streaming: true,
        languages: &["*"],
    },
    Provider {
        id: AZURE,
        name: "Azure neural voices",
        slot: EngineSlot::Tts,
        vendor: "azure-speech",
        streaming: true,
        languages: &["*"],
    },
    Provider {
        id: OPENAI_TTS,
        name: "OpenAI voices",
        slot: EngineSlot::Tts,
        vendor: "openai",
        streaming: true,
        languages: &["*"],
    },
    Provider {
        id: DEEPGRAM_AURA,
        name: "Deepgram Aura",
        slot: EngineSlot::Tts,
        vendor: "deepgram",
        streaming: true,
        languages: &["en", "es"],
    },
];

pub fn provider(id: &str) -> Option<&'static Provider> {
    PROVIDERS.iter().find(|p| p.id == id)
}

pub fn info(p: &Provider) -> EngineInfo {
    EngineInfo {
        id: p.id.into(),
        name: p.name.into(),
        slot: p.slot,
        kind: EngineKind::Cloud,
        license: "The provider's terms".into(),
        languages: p.languages.iter().map(|l| (*l).to_owned()).collect(),
        streaming: p.streaming,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 20,
            vram_mb: 0,
            disk_mb: 0,
        },
        model: None,
    }
}

/// The voices each cloud TTS offers without asking the service (well-known premade voices).
pub fn voices(id: &str) -> Vec<VoiceInfo> {
    let v = |id: &str, name: &str, language: &str| VoiceInfo {
        id: id.into(),
        name: name.into(),
        language: language.into(),
    };
    match id {
        OPENAI_TTS => [
            "alloy", "ash", "ballad", "coral", "echo", "fable", "nova", "onyx", "sage", "shimmer",
        ]
        .iter()
        .map(|n| {
            let mut name = n.to_string();
            name[..1].make_ascii_uppercase();
            v(n, &name, "*")
        })
        .collect(),
        ELEVENLABS => vec![
            v("21m00Tcm4TlvDq8ikWAM", "Rachel", "*"),
            v("EXAVITQu4vr4xnSDxMaL", "Bella", "*"),
            v("pNInz6obpgDQGcFmaJgB", "Adam", "*"),
            v("ErXwobaYiN019PkySvjV", "Antoni", "*"),
        ],
        AZURE => vec![
            v("en-US-AvaMultilingualNeural", "Ava (multilingual)", "*"),
            v(
                "en-US-AndrewMultilingualNeural",
                "Andrew (multilingual)",
                "*",
            ),
            v("en-GB-SoniaNeural", "Sonia", "en-GB"),
            v("hi-IN-SwaraNeural", "Swara", "hi-IN"),
        ],
        DEEPGRAM_AURA => vec![
            v("aura-2-thalia-en", "Thalia", "en"),
            v("aura-2-andromeda-en", "Andromeda", "en"),
            v("aura-2-apollo-en", "Apollo", "en"),
            v("aura-2-orion-en", "Orion", "en"),
        ],
        // Cartesia's voices are listed from the account when the engine loads.
        _ => Vec::new(),
    }
}

fn base(access: &CloudAccess, default: &str) -> String {
    access
        .base_url
        .as_deref()
        .unwrap_or(default)
        .trim_end_matches('/')
        .to_owned()
}

fn cloud_error(what: &str, e: impl std::fmt::Display) -> VoiceError {
    VoiceError::Engine(format!("{what}: {e}"))
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Refuses a response that isn't a success, with the service's own words where it gave some.
fn ok(
    what: &str,
    mut response: ureq::http::Response<ureq::Body>,
) -> VoiceResult<ureq::http::Response<ureq::Body>> {
    let status = response.status().as_u16();
    if (200..300).contains(&status) {
        return Ok(response);
    }
    let body = response.body_mut().read_to_string().unwrap_or_default();
    Err(match status {
        401 | 403 => VoiceError::Engine(format!("{what}: the key was refused")),
        429 => VoiceError::Engine(format!("{what}: too many requests right now")),
        _ => VoiceError::Engine(format!(
            "{what}: HTTP {status} {}",
            body.chars().take(200).collect::<String>()
        )),
    })
}

/// 16-bit little-endian PCM for `audio` in −1…1.
fn pcm16(audio: &[f32]) -> Vec<u8> {
    audio
        .iter()
        .flat_map(|s| {
            #[allow(clippy::cast_possible_truncation, reason = "clamped to i16")]
            let v = (s.clamp(-1.0, 1.0) * 32_767.0) as i16;
            v.to_le_bytes()
        })
        .collect()
}

/// A WAV file (16-bit mono, 16 kHz) of `audio`, for upload.
fn wav(audio: &[f32]) -> Vec<u8> {
    let data = pcm16(audio);
    let len = u32::try_from(data.len()).unwrap_or(u32::MAX);
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&data);
    out
}

// ───────── Speech to text ─────────

/// The streaming services' dialects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Dialect {
    Flux,
    AssemblyAi,
}

pub struct CloudStt {
    info: EngineInfo,
    access: CloudAccess,
    language: String,
}

impl CloudStt {
    pub fn new(id: &str, access: CloudAccess, language: &str) -> VoiceResult<Self> {
        let p = provider(id)
            .filter(|p| p.slot == EngineSlot::Stt)
            .ok_or_else(|| VoiceError::Unavailable(format!("the {id} speech recognizer")))?;
        if access.key.trim().is_empty() {
            return Err(VoiceError::Engine(format!("{}: no key yet", p.name)));
        }
        Ok(Self {
            info: info(p),
            access,
            language: language.to_owned(),
        })
    }

    fn connect(&self, dialect: Dialect) -> VoiceResult<WebSocket<MaybeTlsStream<TcpStream>>> {
        let url = match dialect {
            Dialect::Flux => format!(
                "{}/v2/listen?model=flux-general-en&encoding=linear16&sample_rate={SAMPLE_RATE}",
                base(&self.access, "wss://api.deepgram.com")
            ),
            Dialect::AssemblyAi => format!(
                "{}/v3/ws?sample_rate={SAMPLE_RATE}&encoding=pcm_s16le&format_turns=true",
                base(&self.access, "wss://streaming.assemblyai.com")
            ),
        };
        let mut request = url
            .into_client_request()
            .map_err(|e| cloud_error(&self.info.name, e))?;
        let auth = match dialect {
            Dialect::Flux => format!("Token {}", self.access.key),
            Dialect::AssemblyAi => self.access.key.clone(),
        };
        request.headers_mut().insert(
            "Authorization",
            auth.parse()
                .map_err(|_| cloud_error(&self.info.name, "bad key"))?,
        );
        let (socket, _) =
            tungstenite::connect(request).map_err(|e| cloud_error(&self.info.name, e))?;
        // Short reads, so results are collected between audio chunks.
        let tcp = match socket.get_ref() {
            MaybeTlsStream::Plain(s) => s,
            MaybeTlsStream::NativeTls(s) => s.get_ref(),
            _ => return Ok(socket),
        };
        tcp.set_read_timeout(Some(Duration::from_millis(5)))
            .map_err(|e| cloud_error(&self.info.name, e))?;
        Ok(socket)
    }

    /// The utterance uploaded as a WAV file (OpenAI).
    fn transcribe_file(&self, audio: &[f32]) -> VoiceResult<String> {
        let boundary = "kivo-audio-boundary";
        let language = self.language.split('-').next().unwrap_or("en");
        let mut body = Vec::new();
        let field = |body: &mut Vec<u8>, name: &str, value: &str| {
            body.extend_from_slice(
                format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n")
                    .as_bytes(),
            );
        };
        field(&mut body, "model", "gpt-4o-mini-transcribe");
        field(&mut body, "language", language);
        body.extend_from_slice(
            format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"speech.wav\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes(),
        );
        body.extend_from_slice(&wav(audio));
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        let response = agent()
            .post(format!(
                "{}/v1/audio/transcriptions",
                base(&self.access, "https://api.openai.com")
            ))
            .header("Authorization", &format!("Bearer {}", self.access.key))
            .header(
                "Content-Type",
                &format!("multipart/form-data; boundary={boundary}"),
            )
            .send(&body[..])
            .map_err(|e| cloud_error(&self.info.name, e))?;
        let mut response = ok(&self.info.name, response)?;
        let value: Value = serde_json::from_str(
            &response
                .body_mut()
                .read_to_string()
                .map_err(|e| cloud_error(&self.info.name, e))?,
        )
        .map_err(|e| cloud_error(&self.info.name, e))?;
        Ok(value["text"].as_str().unwrap_or_default().trim().to_owned())
    }
}

impl SttEngine for CloudStt {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn start(
        &mut self,
        _options: &SttOptions,
        cancel: CancellationToken,
    ) -> Box<dyn SttStream + '_> {
        let dialect = match self.info.id.as_str() {
            DEEPGRAM_FLUX => Some(Dialect::Flux),
            ASSEMBLYAI => Some(Dialect::AssemblyAi),
            _ => None,
        };
        match dialect {
            Some(d) => Box::new(Streaming {
                socket: self.connect(d),
                dialect: d,
                engine: self,
                cancel,
                finals: Vec::new(),
                partial: String::new(),
                ended: false,
            }),
            None => Box::new(Upload {
                engine: self,
                cancel,
                audio: Vec::new(),
            }),
        }
    }
}

/// OpenAI: audio gathered, then sent once when the utterance ends (no partials, and no paying
/// again for every half second).
struct Upload<'a> {
    engine: &'a CloudStt,
    cancel: CancellationToken,
    audio: Vec<f32>,
}

impl SttStream for Upload<'_> {
    fn accept(&mut self, audio: &[f32]) -> VoiceResult<Vec<SttEvent>> {
        self.audio.extend_from_slice(audio);
        Ok(Vec::new())
    }

    fn finish(&mut self) -> VoiceResult<String> {
        if self.cancel.is_cancelled() {
            return Err(VoiceError::Cancelled);
        }
        if self.audio.len() < SAMPLE_RATE as usize / 4 {
            return Ok(String::new());
        }
        self.engine.transcribe_file(&self.audio)
    }
}

/// Deepgram Flux or AssemblyAI: audio streamed as it comes, results read between chunks.
struct Streaming<'a> {
    socket: VoiceResult<WebSocket<MaybeTlsStream<TcpStream>>>,
    dialect: Dialect,
    engine: &'a CloudStt,
    cancel: CancellationToken,
    /// Finished turns, in order.
    finals: Vec<String>,
    partial: String,
    ended: bool,
}

impl Streaming<'_> {
    fn socket(&mut self) -> VoiceResult<&mut WebSocket<MaybeTlsStream<TcpStream>>> {
        match &mut self.socket {
            Ok(s) => Ok(s),
            Err(e) => Err(VoiceError::Engine(e.to_string())),
        }
    }

    /// Reads what has arrived; returns the events to show.
    fn drain(&mut self) -> VoiceResult<Vec<SttEvent>> {
        let mut events = Vec::new();
        loop {
            let message = match self.socket()?.read() {
                Ok(m) => m,
                Err(tungstenite::Error::Io(e))
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    break;
                }
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    self.ended = true;
                    break;
                }
                Err(e) => return Err(cloud_error(&self.engine.info.name, e)),
            };
            let text = match message {
                Message::Text(t) => t.to_string(),
                Message::Close(_) => {
                    self.ended = true;
                    break;
                }
                _ => continue,
            };
            let Ok(value) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            let transcript = value["transcript"]
                .as_str()
                .unwrap_or_default()
                .trim()
                .to_owned();
            let (is_final, is_turn, closing) = match self.dialect {
                Dialect::Flux => (
                    value["event"] == "EndOfTurn",
                    value["type"] == "TurnInfo",
                    false,
                ),
                Dialect::AssemblyAi => (
                    value["end_of_turn"] == true && value["turn_is_formatted"] != false,
                    value["type"] == "Turn",
                    value["type"] == "Termination",
                ),
            };
            if value["type"] == "Error" || value["error"].is_string() {
                return Err(cloud_error(
                    &self.engine.info.name,
                    value["error"]
                        .as_str()
                        .or(value["description"].as_str())
                        .unwrap_or("an error"),
                ));
            }
            if closing {
                self.ended = true;
            }
            if !is_turn || transcript.is_empty() {
                continue;
            }
            if is_final {
                self.finals.push(transcript);
                self.partial.clear();
                events.push(SttEvent::Stable(self.finals.join(" ")));
            } else if transcript != self.partial {
                self.partial.clone_from(&transcript);
                let mut shown = self.finals.clone();
                shown.push(transcript);
                events.push(SttEvent::Partial(shown.join(" ")));
            }
        }
        Ok(events)
    }
}

impl SttStream for Streaming<'_> {
    fn accept(&mut self, audio: &[f32]) -> VoiceResult<Vec<SttEvent>> {
        if self.cancel.is_cancelled() {
            return Err(VoiceError::Cancelled);
        }
        let bytes = pcm16(audio);
        self.socket()?
            .send(Message::Binary(bytes.into()))
            .map_err(|e| cloud_error(&self.engine.info.name, e))?;
        self.drain()
    }

    fn finish(&mut self) -> VoiceResult<String> {
        let close = match self.dialect {
            Dialect::Flux => json!({ "type": "CloseStream" }),
            Dialect::AssemblyAi => json!({ "type": "Terminate" }),
        };
        let name = self.engine.info.name.clone();
        self.socket()?
            .send(Message::Text(close.to_string().into()))
            .map_err(|e| cloud_error(&name, e))?;
        // The last results come after the close request; wait for them a little.
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.ended && Instant::now() < deadline {
            if self.cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            self.drain()?;
        }
        let _ = self.socket().map(|s| s.close(None));
        let mut text = self.finals.join(" ");
        if !self.partial.is_empty() {
            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(&self.partial);
        }
        Ok(text.trim().to_owned())
    }
}

// ───────── Text to speech ─────────

pub struct CloudTts {
    info: EngineInfo,
    access: CloudAccess,
    language: String,
    speed: f32,
    voices: Vec<VoiceInfo>,
}

impl CloudTts {
    pub fn new(id: &str, access: CloudAccess, language: &str) -> VoiceResult<Self> {
        let p = provider(id)
            .filter(|p| p.slot == EngineSlot::Tts)
            .ok_or_else(|| VoiceError::Unavailable(format!("the {id} voice")))?;
        if access.key.trim().is_empty() {
            return Err(VoiceError::Engine(format!("{}: no key yet", p.name)));
        }
        if id == AZURE
            && access.region.as_deref().is_none_or(str::is_empty)
            && access.base_url.is_none()
        {
            return Err(VoiceError::Engine(format!("{}: no region yet", p.name)));
        }
        let mut engine = Self {
            info: info(p),
            access,
            language: language.to_owned(),
            speed: 1.0,
            voices: voices(id),
        };
        if id == CARTESIA {
            engine.voices = engine.cartesia_voices()?;
        }
        Ok(engine)
    }

    /// The account's voices (Cartesia has no fixed premade list).
    fn cartesia_voices(&self) -> VoiceResult<Vec<VoiceInfo>> {
        let response = agent()
            .get(format!(
                "{}/voices?limit=20",
                base(&self.access, "https://api.cartesia.ai")
            ))
            .header("X-API-Key", &self.access.key)
            .header("Cartesia-Version", "2025-04-16")
            .call()
            .map_err(|e| cloud_error(&self.info.name, e))?;
        let mut response = ok(&self.info.name, response)?;
        let value: Value = serde_json::from_str(
            &response
                .body_mut()
                .read_to_string()
                .map_err(|e| cloud_error(&self.info.name, e))?,
        )
        .map_err(|e| cloud_error(&self.info.name, e))?;
        let list = value["data"]
            .as_array()
            .or(value.as_array())
            .cloned()
            .unwrap_or_default();
        Ok(list
            .iter()
            .filter_map(|v| {
                Some(VoiceInfo {
                    id: v["id"].as_str()?.to_owned(),
                    name: v["name"].as_str().unwrap_or("Voice").to_owned(),
                    language: v["language"].as_str().unwrap_or("*").to_owned(),
                })
            })
            .collect())
    }

    /// The request for `text` in `voice`: the address, headers and body, and the PCM sample rate
    /// that comes back.
    fn request(
        &self,
        text: &str,
        voice: &str,
    ) -> (String, Vec<(&'static str, String)>, Vec<u8>, u32) {
        let lang = self.language.split('-').next().unwrap_or("en").to_owned();
        match self.info.id.as_str() {
            OPENAI_TTS => (
                format!("{}/v1/audio/speech", base(&self.access, "https://api.openai.com")),
                vec![
                    ("Authorization", format!("Bearer {}", self.access.key)),
                    ("Content-Type", "application/json".into()),
                ],
                json!({ "model": "gpt-4o-mini-tts", "input": text, "voice": voice, "response_format": "pcm", "speed": self.speed })
                    .to_string()
                    .into_bytes(),
                24_000,
            ),
            ELEVENLABS => (
                format!(
                    "{}/v1/text-to-speech/{voice}/stream?output_format=pcm_16000",
                    base(&self.access, "https://api.elevenlabs.io")
                ),
                vec![
                    ("xi-api-key", self.access.key.clone()),
                    ("Content-Type", "application/json".into()),
                ],
                json!({ "text": text, "model_id": "eleven_flash_v2_5", "voice_settings": { "speed": self.speed.clamp(0.7, 1.2) } })
                    .to_string()
                    .into_bytes(),
                16_000,
            ),
            CARTESIA => (
                format!("{}/tts/bytes", base(&self.access, "https://api.cartesia.ai")),
                vec![
                    ("X-API-Key", self.access.key.clone()),
                    ("Cartesia-Version", "2025-04-16".into()),
                    ("Content-Type", "application/json".into()),
                ],
                json!({
                    "model_id": "sonic-3",
                    "transcript": text,
                    "voice": { "mode": "id", "id": voice },
                    "language": lang,
                    "output_format": { "container": "raw", "encoding": "pcm_s16le", "sample_rate": 24_000 },
                })
                .to_string()
                .into_bytes(),
                24_000,
            ),
            AZURE => {
                let region = self.access.region.clone().unwrap_or_default();
                let rate = format!("{:+.0}%", (self.speed - 1.0) * 100.0);
                let ssml = format!(
                    "<speak version='1.0' xml:lang='{}'><voice name='{voice}'><prosody rate='{rate}'>{}</prosody></voice></speak>",
                    escape_xml(&self.language),
                    escape_xml(text)
                );
                (
                    format!(
                        "{}/cognitiveservices/v1",
                        base(&self.access, &format!("https://{region}.tts.speech.microsoft.com"))
                    ),
                    vec![
                        ("Ocp-Apim-Subscription-Key", self.access.key.clone()),
                        ("Content-Type", "application/ssml+xml".into()),
                        ("X-Microsoft-OutputFormat", "raw-24khz-16bit-mono-pcm".into()),
                        ("User-Agent", "KIVO".into()),
                    ],
                    ssml.into_bytes(),
                    24_000,
                )
            }
            _ => (
                format!(
                    "{}/v1/speak?model={voice}&encoding=linear16&sample_rate=24000&container=none",
                    base(&self.access, "https://api.deepgram.com")
                ),
                vec![
                    ("Authorization", format!("Token {}", self.access.key)),
                    ("Content-Type", "application/json".into()),
                ],
                json!({ "text": text }).to_string().into_bytes(),
                24_000,
            ),
        }
    }
}

fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('\'', "&apos;")
        .replace('"', "&quot;")
}

impl TtsEngine for CloudTts {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn set_speed(&mut self, speed: f32) {
        self.speed = speed.clamp(0.5, 2.0);
    }

    fn voices(&self) -> Vec<VoiceInfo> {
        self.voices.clone()
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        cancel: &CancellationToken,
        sink: AudioSink<'_>,
    ) -> VoiceResult<()> {
        let voice = voice
            .filter(|v| !v.is_empty())
            .map(str::to_owned)
            .or_else(|| self.voices.first().map(|v| v.id.clone()))
            .ok_or_else(|| VoiceError::Engine(format!("{}: no voice", self.info.name)))?;
        let (url, headers, body, rate) = self.request(text, &voice);
        let mut request = agent().post(url);
        for (name, value) in &headers {
            request = request.header(*name, value);
        }
        let response = request
            .send(&body[..])
            .map_err(|e| cloud_error(&self.info.name, e))?;
        let mut response = ok(&self.info.name, response)?;
        let mut reader = response.body_mut().as_reader();
        // Stream the PCM through as it arrives, in 100 ms pieces.
        let chunk = usize::try_from(rate / 10).unwrap_or(2_400) * 2;
        let mut buffer = vec![0u8; chunk];
        let mut carry: Option<u8> = None;
        loop {
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            let n = reader
                .read(&mut buffer)
                .map_err(|e| cloud_error(&self.info.name, e))?;
            if n == 0 {
                break;
            }
            let mut bytes: Vec<u8> = carry.take().into_iter().collect();
            bytes.extend_from_slice(&buffer[..n]);
            if bytes.len() % 2 == 1 {
                carry = bytes.pop();
            }
            let samples: Vec<f32> = bytes
                .chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
                .collect();
            if !samples.is_empty() {
                sink(&samples, rate)?;
            }
        }
        Ok(())
    }
}

/// Checks a key with one small authenticated request, before it is saved (VOICE-10/11).
pub fn check(id: &str, access: &CloudAccess) -> VoiceResult<()> {
    let p = provider(id).ok_or_else(|| VoiceError::Unavailable(id.into()))?;
    let (url, header, value) = match p.vendor {
        "deepgram" => (
            format!("{}/v1/projects", base(access, "https://api.deepgram.com")),
            "Authorization",
            format!("Token {}", access.key),
        ),
        "assemblyai" => (
            format!(
                "{}/v2/transcript?limit=1",
                base(access, "https://api.assemblyai.com")
            ),
            "Authorization",
            access.key.clone(),
        ),
        "openai" => (
            format!("{}/v1/models", base(access, "https://api.openai.com")),
            "Authorization",
            format!("Bearer {}", access.key),
        ),
        "cartesia" => (
            format!("{}/voices?limit=1", base(access, "https://api.cartesia.ai")),
            "X-API-Key",
            access.key.clone(),
        ),
        "elevenlabs" => (
            format!("{}/v1/user", base(access, "https://api.elevenlabs.io")),
            "xi-api-key",
            access.key.clone(),
        ),
        _ => (
            format!(
                "{}/cognitiveservices/voices/list",
                base(
                    access,
                    &format!(
                        "https://{}.tts.speech.microsoft.com",
                        access.region.clone().unwrap_or_default()
                    )
                )
            ),
            "Ocp-Apim-Subscription-Key",
            access.key.clone(),
        ),
    };
    let response = agent()
        .get(url)
        .header(header, &value)
        .header("Cartesia-Version", "2025-04-16")
        .call()
        .map_err(|e| cloud_error(p.name, e))?;
    ok(p.name, response).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// A one-request HTTP stand-in: returns the request it got (head and body) and answers with
    /// `status` and `body`.
    fn http_once(
        status: u16,
        body: Vec<u8>,
    ) -> (String, std::thread::JoinHandle<(String, Vec<u8>)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap_or(0);
                }
                head.push_str(&line);
            }
            let mut got = vec![0; length];
            reader.read_exact(&mut got).unwrap();
            let mut stream = stream;
            write!(
                stream,
                "HTTP/1.1 {status} OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .unwrap();
            stream.write_all(&body).unwrap();
            (head, got)
        });
        (url, handle)
    }

    fn access(url: &str) -> CloudAccess {
        CloudAccess {
            key: "test-key".into(),
            base_url: Some(url.into()),
            region: Some("westeurope".into()),
        }
    }

    #[test]
    fn openai_transcribes_the_utterance_as_one_upload() {
        let (url, server) = http_once(200, br#"{"text":" Open Chrome. "}"#.to_vec());
        let mut engine = CloudStt::new(OPENAI_STT, access(&url), "en-US").unwrap();
        let mut stream = engine.start(
            &SttOptions {
                language: "en".into(),
                vocabulary: vec![],
            },
            CancellationToken::new(),
        );
        assert!(
            stream.accept(&vec![0.1; 16_000]).unwrap().is_empty(),
            "no partials"
        );
        assert_eq!(stream.finish().unwrap(), "Open Chrome.");
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("POST /v1/audio/transcriptions"), "{head}");
        assert!(
            head.contains("authorization: Bearer test-key")
                || head.contains("Authorization: Bearer test-key"),
            "{head}"
        );
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("gpt-4o-mini-transcribe") && body.contains("RIFF"));
    }

    /// A WebSocket stand-in that checks the auth header, reads audio and answers like `dialect`.
    fn ws_server(dialect: Dialect) -> (String, std::thread::JoinHandle<(String, usize)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut auth = String::new();
            // tungstenite's handshake callback fixes the closure's error type.
            #[allow(clippy::result_large_err)]
            let mut ws = tungstenite::accept_hdr(
                stream,
                |req: &tungstenite::handshake::server::Request, res| {
                    auth = format!(
                        "{} {}",
                        req.uri(),
                        req.headers()
                            .get("authorization")
                            .and_then(|v| v.to_str().ok())
                            .unwrap_or("")
                    );
                    Ok(res)
                },
            )
            .unwrap();
            let mut bytes = 0;
            let mut sent_partial = false;
            loop {
                match ws.read() {
                    Ok(Message::Binary(b)) => {
                        bytes += b.len();
                        if !sent_partial {
                            sent_partial = true;
                            let partial = match dialect {
                                Dialect::Flux => {
                                    json!({"type":"TurnInfo","event":"Update","transcript":"open"})
                                }
                                Dialect::AssemblyAi => {
                                    json!({"type":"Turn","transcript":"open","end_of_turn":false})
                                }
                            };
                            ws.send(Message::Text(partial.to_string().into())).unwrap();
                        }
                    }
                    Ok(Message::Text(t)) => {
                        let v: Value = serde_json::from_str(&t).unwrap();
                        assert!(v["type"] == "CloseStream" || v["type"] == "Terminate");
                        let last = match dialect {
                            Dialect::Flux => {
                                json!({"type":"TurnInfo","event":"EndOfTurn","transcript":"Open Chrome."})
                            }
                            Dialect::AssemblyAi => {
                                json!({"type":"Turn","transcript":"Open Chrome.","end_of_turn":true,"turn_is_formatted":true})
                            }
                        };
                        ws.send(Message::Text(last.to_string().into())).unwrap();
                        if dialect == Dialect::AssemblyAi {
                            ws.send(Message::Text(
                                json!({"type":"Termination"}).to_string().into(),
                            ))
                            .unwrap();
                        }
                        let _ = ws.close(None);
                        let _ = ws.flush();
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }
            (auth, bytes)
        });
        (url, handle)
    }

    fn stream_with(id: &str, dialect: Dialect, expected_auth: &str) {
        let (url, server) = ws_server(dialect);
        let mut engine = CloudStt::new(id, access(&url), "en-US").unwrap();
        let mut stream = engine.start(
            &SttOptions {
                language: "en".into(),
                vocabulary: vec![],
            },
            CancellationToken::new(),
        );
        let mut partials = Vec::new();
        for _ in 0..10 {
            partials.extend(stream.accept(&vec![0.05; 1_600]).unwrap());
            std::thread::sleep(Duration::from_millis(10));
        }
        assert!(
            partials.contains(&SttEvent::Partial("open".into())),
            "{partials:?}"
        );
        assert_eq!(stream.finish().unwrap(), "Open Chrome.");
        drop(stream);
        let (auth, bytes) = server.join().unwrap();
        assert!(auth.ends_with(expected_auth), "{auth}");
        assert_eq!(bytes, 10 * 1_600 * 2, "16-bit PCM");
    }

    #[test]
    fn deepgram_flux_streams_audio_and_reads_turns() {
        stream_with(DEEPGRAM_FLUX, Dialect::Flux, "Token test-key");
    }

    #[test]
    fn assemblyai_streams_audio_and_reads_turns() {
        stream_with(ASSEMBLYAI, Dialect::AssemblyAi, "test-key");
    }

    fn pcm_bytes(n: usize) -> Vec<u8> {
        (0..n)
            .flat_map(|i| ((i % 200) as i16 * 100).to_le_bytes())
            .collect()
    }

    #[test]
    fn every_cloud_voice_streams_pcm_back() {
        for (id, voice, rate, needle) in [
            (OPENAI_TTS, "alloy", 24_000, "gpt-4o-mini-tts"),
            (
                ELEVENLABS,
                "21m00Tcm4TlvDq8ikWAM",
                16_000,
                "eleven_flash_v2_5",
            ),
            (
                AZURE,
                "en-US-AvaMultilingualNeural",
                24_000,
                "<voice name='en-US-AvaMultilingualNeural'>",
            ),
            (DEEPGRAM_AURA, "aura-2-thalia-en", 24_000, "\"text\""),
        ] {
            let (url, server) = http_once(200, pcm_bytes(4_801));
            let mut engine = CloudTts::new(id, access(&url), "en-US").unwrap();
            let mut got = Vec::new();
            let mut rates = Vec::new();
            engine
                .speak(
                    "Hello & welcome",
                    Some(voice),
                    &CancellationToken::new(),
                    &mut |s, r| {
                        got.extend_from_slice(s);
                        rates.push(r);
                        Ok(())
                    },
                )
                .unwrap();
            assert_eq!(got.len(), 4_801, "{id}");
            assert!(rates.iter().all(|&r| r == rate), "{id}");
            let (head, body) = server.join().unwrap();
            let body = String::from_utf8_lossy(&body);
            assert!(body.contains(needle), "{id}: {body}");
            assert!(head.starts_with("POST "), "{id}");
            if id == AZURE {
                assert!(body.contains("Hello &amp; welcome"), "escaped");
            }
        }
    }

    #[test]
    fn cartesia_lists_the_accounts_voices_then_speaks() {
        let (url, server) = http_once(
            200,
            br#"{"data":[{"id":"v-1","name":"Katie","language":"en"}]}"#.to_vec(),
        );
        // Loading asks for the voices; speaking needs a second stand-in on the same address.
        let listener_url = url.clone();
        let loaded =
            std::thread::spawn(move || CloudTts::new(CARTESIA, access(&listener_url), "en"));
        let (head, _) = server.join().unwrap();
        assert!(head.starts_with("GET /voices"), "{head}");
        let engine = loaded.join().unwrap().unwrap();
        assert_eq!(engine.voices()[0].name, "Katie");
    }

    #[test]
    fn a_refused_key_says_so_and_no_key_is_caught_early() {
        let (url, server) = http_once(401, b"{}".to_vec());
        let err = check(DEEPGRAM_FLUX, &access(&url)).unwrap_err();
        assert!(
            err.detail().contains("the key was refused"),
            "{}",
            err.detail()
        );
        let (head, _) = server.join().unwrap();
        assert!(head.starts_with("GET /v1/projects"));
        assert!(CloudTts::new(OPENAI_TTS, CloudAccess::default(), "en").is_err());
        assert!(CloudStt::new("nope", access("http://x"), "en").is_err());
    }
}
