//! Whisper through whisper.cpp (GGML), on the graphics card with the processor as its fallback
//! (DECISIONS "Local models on the GPU first", VOICE-50): the way desktop dictation apps run
//! Whisper. The GPU backend is the worker's build: Vulkan in `kivo-infer` on Windows, CUDA in
//! `kivo-infer-cuda`, Metal on a Mac. The models are single `.bin` files from the whisper.cpp
//! project (MIT), in three sizes. whisper.cpp windows long audio itself, and its abort callback
//! stops a run as soon as the request is cancelled.

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
#[cfg(feature = "whisper-cpp")]
pub use engine::WhisperCpp;

/// Whether KIVO's speech worker has whisper.cpp: Windows x64 builds do (Vulkan, and CUDA in the
/// separate worker) and macOS builds do (Metal); elsewhere its engines aren't offered.
pub const AVAILABLE: bool = cfg!(any(
    all(windows, target_arch = "x86_64"),
    target_os = "macos"
));

/// The GPU backends KIVO's workers offer on this platform, fastest first.
pub fn gpu_backends() -> Vec<Accel> {
    if cfg!(all(windows, target_arch = "x86_64")) {
        vec![Accel::Cuda, Accel::Vulkan]
    } else if cfg!(target_os = "macos") {
        vec![Accel::Metal]
    } else {
        Vec::new()
    }
}

/// The GPU backend this build of whisper.cpp has, if any (the worker reports it at start).
pub const BUILT_GPU: Option<Accel> = if cfg!(feature = "cuda") {
    Some(Accel::Cuda)
} else if cfg!(feature = "metal") {
    Some(Accel::Metal)
} else if cfg!(feature = "vulkan") {
    Some(Accel::Vulkan)
} else {
    None
};

/// Whether a GPU device named `device` by the backend is the card the system calls `wanted`
/// (DXGI's name on Windows; Vulkan and CUDA report the same marketing name).
pub fn same_card(device: &str, wanted: &str) -> bool {
    let norm = |s: &str| {
        s.split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase()
    };
    let (device, wanted) = (norm(device), norm(wanted));
    !wanted.is_empty() && (device.contains(&wanted) || wanted.contains(&device))
}

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
        accel: gpu_backends().into_iter().chain([Accel::Cpu]).collect(),
        resources: ResourceEstimate {
            ram_mb: v.vram_mb / 2,
            vram_mb: v.vram_mb,
            disk_mb: v.disk_mb,
        },
        model: Some(v.id.into()),
    }
}

/// The engine itself links whisper.cpp, which only the speech workers build (feature
/// `whisper-cpp`, plus a GPU backend's feature; CMake and that backend's SDK at build time).
#[cfg(feature = "whisper-cpp")]
mod engine {
    use super::{BUILT_GPU, Variant, info_for, same_card};
    use crate::engine::{Accel, EngineInfo};
    use crate::error::{VoiceError, VoiceResult};
    use crate::traits::{SttEngine, SttOptions, SttStream};
    use crate::utterance::{Transcriber, Utterance};
    use std::ffi::CStr;
    use std::path::{Path, PathBuf};
    use tokio_util::sync::CancellationToken;
    use whisper_rs::whisper_rs_sys as sys;
    use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

    pub struct WhisperCpp {
        info: EngineInfo,
        context: WhisperContext,
        threads: usize,
        language: String,
    }

    /// The GPU devices ggml has in this build, in whisper.cpp's `gpu_device` order (discrete and
    /// integrated cards, in registry order), by their description.
    pub fn gpu_devices() -> Vec<String> {
        let mut out = Vec::new();
        // SAFETY: ggml's device registry is initialized on first use and lives for the process;
        // the descriptions are NUL-terminated strings it owns.
        unsafe {
            for i in 0..sys::ggml_backend_dev_count() {
                let dev = sys::ggml_backend_dev_get(i);
                let kind = sys::ggml_backend_dev_type(dev);
                if kind != sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_GPU
                    && kind != sys::ggml_backend_dev_type_GGML_BACKEND_DEVICE_TYPE_IGPU
                {
                    continue;
                }
                let description = sys::ggml_backend_dev_description(dev);
                out.push(if description.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(description).to_string_lossy().into_owned()
                });
            }
        }
        out
    }

    /// whisper.cpp asks this between graph steps whether to stop; `data` is the request's
    /// `CancellationToken`. (whisper-rs 0.16's `set_abort_callback_safe` boxes its closure as a
    /// `Box<dyn FnMut>` but reads it back as the closure's own type: garbage that happened to work
    /// on Vulkan and hung or crashed on CUDA.)
    unsafe extern "C" fn stop_requested(data: *mut std::ffi::c_void) -> bool {
        // SAFETY: `transcribe` passes a `&CancellationToken` that outlives the run.
        unsafe { data.cast::<CancellationToken>().as_ref() }
            .is_some_and(CancellationToken::is_cancelled)
    }

    impl WhisperCpp {
        /// Loads the model in `dir`, on the graphics card the system calls `gpu` (the one the
        /// runtime's GPU policy chose) when this build has a GPU backend that finds it, else on
        /// the processor.
        pub fn load(
            dir: &Path,
            variant: &Variant,
            threads: usize,
            gpu: Option<&str>,
        ) -> VoiceResult<Self> {
            let file: PathBuf = dir.join(variant.file);
            if !file.is_file() {
                return Err(VoiceError::ModelMissing(variant.name.into()));
            }
            // The chosen card's place among ggml's GPU devices: whisper.cpp would otherwise take
            // the first one, which on a laptop can be the integrated GPU.
            let device = gpu.filter(|_| BUILT_GPU.is_some()).and_then(|wanted| {
                let devices = gpu_devices();
                let found = devices.iter().position(|d| same_card(d, wanted));
                if found.is_none() {
                    eprintln!(
                        "whisper.cpp has no GPU device named {wanted:?} (it has {devices:?})"
                    );
                }
                found
            });
            let mut params = WhisperContextParameters::default();
            let on_gpu = device.is_some();
            params
                .use_gpu(on_gpu)
                .flash_attn(on_gpu)
                .gpu_device(device.and_then(|d| i32::try_from(d).ok()).unwrap_or(0));
            let context = WhisperContext::new_with_params(&file, params)
                .map_err(|e| VoiceError::Engine(format!("whisper.cpp: {e}")))?;
            let mut info = info_for(variant);
            info.accel = match (on_gpu, BUILT_GPU) {
                (true, Some(backend)) => vec![backend, Accel::Cpu],
                _ => vec![Accel::Cpu],
            };
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
            // SAFETY: `stop_requested` reads its data as a `CancellationToken`, and `cancel` outlives
            // the `full` call below, the only time whisper.cpp calls it.
            unsafe {
                params.set_abort_callback(Some(stop_requested));
                params.set_abort_callback_user_data(std::ptr::from_ref(cancel).cast_mut().cast());
            }
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
    /// `ggml-base.en.bin`): on every GPU device this build's backend finds, chosen by name, and
    /// on the processor.
    #[test]
    fn transcribes_the_sample_on_each_gpu_and_the_processor_when_the_model_is_here() {
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
        let devices = engine::gpu_devices();
        println!("{BUILT_GPU:?} devices: {devices:?}");
        let targets = devices.iter().map(|d| Some(d.as_str())).chain([None]);
        for gpu in targets {
            let mut engine = WhisperCpp::load(&dir, variant, 4, gpu).unwrap();
            let ran_on = crate::traits::SttEngine::info(&engine).accel[0];
            if gpu.is_some() {
                assert_eq!(Some(ran_on), BUILT_GPU, "{gpu:?} should run on the GPU");
            } else {
                assert_eq!(ran_on, Accel::Cpu);
            }
            let cancel = CancellationToken::new();
            let _ = engine.transcribe(&audio, &cancel).unwrap();
            let started = std::time::Instant::now();
            let text = engine.transcribe(&audio, &cancel).unwrap();
            println!(
                "whisper.cpp on {gpu:?} ({ran_on:?}): {:?} {text}",
                started.elapsed()
            );
            assert!(text.to_lowercase().contains("country"), "{text}");
            // A cancel stops a run in progress (the abort callback reads the token).
            let long: Vec<f32> = audio
                .iter()
                .copied()
                .cycle()
                .take(audio.len() * 2)
                .collect();
            let cancel = CancellationToken::new();
            let stop = cancel.clone();
            let canceller = std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(40));
                stop.cancel();
                std::time::Instant::now()
            });
            let result = engine.transcribe(&long, &cancel);
            let returned = std::time::Instant::now();
            let cancelled_at = canceller.join().unwrap();
            println!("cancel → return {:?}", returned - cancelled_at);
            assert!(
                matches!(result, Err(crate::error::VoiceError::Cancelled)),
                "{result:?}"
            );
        }
    }

    #[test]
    fn cards_are_matched_by_the_name_the_system_gives_them() {
        let rtx = "NVIDIA GeForce RTX 3060 Laptop GPU";
        assert!(same_card(rtx, rtx));
        assert!(same_card(
            "NVIDIA  GeForce RTX 3060 Laptop GPU",
            "nvidia geforce rtx 3060 laptop gpu"
        ));
        assert!(!same_card("AMD Radeon(TM) Graphics", rtx));
        assert!(!same_card(rtx, ""));
    }

    #[test]
    fn whisper_engines_list_the_gpu_backends_first() {
        let info = info_for(&VARIANTS[0]);
        assert_eq!(info.accel.last(), Some(&Accel::Cpu));
        assert_eq!(info.accel.len(), gpu_backends().len() + 1);
    }

    #[test]
    fn every_variant_names_its_file() {
        for v in VARIANTS {
            assert!(v.file.starts_with("ggml-") && v.file.ends_with(".bin"));
            assert_eq!(info_for(&v).model.as_deref(), Some(v.id));
        }
    }
}
