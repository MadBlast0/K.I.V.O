//! Whisper through whisper.cpp (GGML), on the graphics card through Vulkan with the processor as
//! its fallback (DECISIONS "Local models on the GPU first"): the way desktop dictation apps run
//! Whisper. The models are single `.bin` files from the whisper.cpp project (MIT), in three sizes.
//! whisper.cpp windows long audio itself, and its abort callback stops a run as soon as the
//! request is cancelled.

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
#[cfg(feature = "whisper-cpp")]
pub use engine::WhisperCpp;

/// Whether KIVO's speech worker has whisper.cpp: Windows x64 builds do (built against the Vulkan
/// SDK, `pnpm build-tools`); elsewhere its engines aren't offered.
pub const AVAILABLE: bool = cfg!(all(windows, target_arch = "x86_64"));

/// One whisper.cpp model KIVO offers.
#[derive(Clone, Copy, Debug)]
pub struct Variant {
    pub id: &'static str,
    pub name: &'static str,
    pub file: &'static str,
    /// Only English (`.en` models), or every Whisper language.
    pub english_only: bool,
    pub disk_mb: u32,
    pub vram_mb: u32,
}

pub const VARIANTS: [Variant; 3] = [
    Variant {
        id: "whisper-cpp-small",
        name: "Whisper small (graphics card)",
        file: "ggml-small-q8_0.bin",
        english_only: false,
        disk_mb: 265,
        vram_mb: 600,
    },
    Variant {
        id: "whisper-cpp-turbo",
        name: "Whisper large-v3-turbo (graphics card)",
        file: "ggml-large-v3-turbo-q5_0.bin",
        english_only: false,
        disk_mb: 575,
        vram_mb: 1_200,
    },
    Variant {
        id: "whisper-cpp-base-en",
        name: "Whisper base, English (graphics card)",
        file: "ggml-base.en.bin",
        english_only: true,
        disk_mb: 148,
        vram_mb: 300,
    },
];

pub fn variant(id: &str) -> Option<&'static Variant> {
    VARIANTS.iter().find(|v| v.id == id)
}

pub fn info_for(v: &Variant) -> EngineInfo {
    EngineInfo {
        id: v.id.into(),
        name: v.name.into(),
        slot: EngineSlot::Stt,
        kind: EngineKind::Local,
        license: "MIT".into(),
        languages: if v.english_only {
            vec!["en".into()]
        } else {
            crate::whisper::LANGUAGES
                .iter()
                .map(|l| (*l).to_owned())
                .collect()
        },
        streaming: true,
        accel: vec![Accel::Vulkan, Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: v.vram_mb / 2,
            vram_mb: v.vram_mb,
            disk_mb: v.disk_mb,
        },
        model: Some(v.id.into()),
    }
}

/// The engine itself links whisper.cpp, which only the speech worker builds (feature
/// `whisper-cpp`, with the Vulkan SDK and CMake at build time).
#[cfg(feature = "whisper-cpp")]
mod engine {
    use super::{Variant, info_for};
    use crate::engine::{Accel, EngineInfo};
    use crate::error::{VoiceError, VoiceResult};
    use crate::traits::{SttEngine, SttOptions, SttStream};
    use crate::utterance::{Transcriber, Utterance};
    use std::path::{Path, PathBuf};
    use tokio_util::sync::CancellationToken;
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    pub struct WhisperCpp {
        info: EngineInfo,
        context: WhisperContext,
        threads: usize,
        language: String,
    }

    impl WhisperCpp {
        /// Loads the model in `dir`, on the graphics card when `gpu` (and whisper.cpp finds one it
        /// can use), else on the processor.
        pub fn load(dir: &Path, variant: &Variant, threads: usize, gpu: bool) -> VoiceResult<Self> {
            let file: PathBuf = dir.join(variant.file);
            if !file.is_file() {
                return Err(VoiceError::ModelMissing(variant.name.into()));
            }
            let mut params = WhisperContextParameters::default();
            params.use_gpu(gpu).flash_attn(gpu);
            let context = WhisperContext::new_with_params(&file, params)
                .map_err(|e| VoiceError::Engine(format!("whisper.cpp: {e}")))?;
            let mut info = info_for(variant);
            if !gpu {
                info.accel = vec![Accel::Cpu];
            }
            Ok(Self {
                info,
                context,
                threads: threads.max(1),
                language: "en".into(),
            })
        }

        /// Transcribes 16 kHz mono audio in the utterance's language.
        pub fn transcribe(
            &mut self,
            audio: &[f32],
            cancel: &CancellationToken,
        ) -> VoiceResult<String> {
            if audio.len() < crate::traits::SAMPLE_RATE as usize * 3 / 10 {
                return Ok(String::new());
            }
            let mut state = self
                .context
                .create_state()
                .map_err(|e| VoiceError::Engine(format!("whisper.cpp: {e}")))?;
            let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
            let language = self.language.split('-').next().unwrap_or("en").to_owned();
            params.set_language(Some(&language));
            params.set_n_threads(i32::try_from(self.threads).unwrap_or(4));
            params.set_no_context(true);
            params.set_no_timestamps(true);
            params.set_suppress_blank(true);
            params.set_print_special(false);
            params.set_print_progress(false);
            params.set_print_realtime(false);
            params.set_print_timestamps(false);
            let stop = cancel.clone();
            params.set_abort_callback_safe(move || stop.is_cancelled());
            let ran = state.full(params, audio);
            if cancel.is_cancelled() {
                return Err(VoiceError::Cancelled);
            }
            ran.map_err(|e| VoiceError::Engine(format!("whisper.cpp: {e}")))?;
            let mut text = String::new();
            for segment in state.as_iter() {
                let piece = segment
                    .to_str_lossy()
                    .map_err(|e| VoiceError::Engine(format!("whisper.cpp: {e}")))?;
                text.push_str(&piece);
            }
            Ok(text.trim().to_owned())
        }
    }

    impl Transcriber for WhisperCpp {
        fn transcribe(&mut self, audio: &[f32], cancel: &CancellationToken) -> VoiceResult<String> {
            WhisperCpp::transcribe(self, audio, cancel)
        }
    }

    impl SttEngine for WhisperCpp {
        fn info(&self) -> &EngineInfo {
            &self.info
        }

        fn start(
            &mut self,
            options: &SttOptions,
            cancel: CancellationToken,
        ) -> Box<dyn SttStream + '_> {
            self.language.clone_from(&options.language);
            Box::new(Utterance::new(self, cancel))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "whisper-cpp")]
    use std::path::PathBuf;
    #[cfg(feature = "whisper-cpp")]
    use tokio_util::sync::CancellationToken;

    #[cfg(feature = "whisper-cpp")]
    /// Runs the real model when it is here (`KIVO_WHISPER_CPP_DIR`, a folder with
    /// `ggml-base.en.bin`), on the GPU and on the processor.
    #[test]
    fn transcribes_the_sample_on_the_gpu_and_the_processor_when_the_model_is_here() {
        let Some(dir) = std::env::var_os("KIVO_WHISPER_CPP_DIR").map(PathBuf::from) else {
            eprintln!("KIVO_WHISPER_CPP_DIR not set; skipping");
            return;
        };
        let Some(wav) = std::env::var_os("KIVO_WHISPER_CPP_WAV")
            .map(PathBuf::from)
            .and_then(|p| std::fs::read(p).ok())
        else {
            eprintln!("KIVO_WHISPER_CPP_WAV not set; skipping");
            return;
        };
        let audio = crate::utterance::wav_samples(&wav);
        let variant = variant("whisper-cpp-base-en").unwrap();
        for gpu in [true, false] {
            let mut engine = WhisperCpp::load(&dir, variant, 4, gpu).unwrap();
            let cancel = CancellationToken::new();
            let _ = engine.transcribe(&audio, &cancel).unwrap();
            let started = std::time::Instant::now();
            let text = engine.transcribe(&audio, &cancel).unwrap();
            println!("whisper.cpp gpu={gpu}: {:?} {text}", started.elapsed());
            assert!(text.to_lowercase().contains("country"), "{text}");
        }
    }

    #[test]
    fn every_variant_names_its_file() {
        for v in VARIANTS {
            assert!(v.file.starts_with("ggml-") && v.file.ends_with(".bin"));
            assert_eq!(info_for(&v).model.as_deref(), Some(v.id));
        }
    }
}
