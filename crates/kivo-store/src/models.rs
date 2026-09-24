//! The model manager (DISTRIBUTION §4, DIST-12): models are never bundled; each is downloaded
//! when the user chooses it. Each model has a manifest of files with sizes and sha256 hashes.
//! Downloads resume with HTTP ranges, every file is verified, archives are unpacked, and a model
//! appears in `%LOCALAPPDATA%\KIVO\models\<id>` only once all of it is there and verified (the
//! folder is renamed into place).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use tokio_util::sync::CancellationToken;

/// What a model is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ModelKind {
    Stt,
    Tts,
    Vad,
    Wake,
    Embedding,
    /// Recognizing the owner's voice (VOICE §5).
    Speaker,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    /// File name inside the model folder.
    pub name: String,
    pub url: String,
    pub size: u64,
    /// Lower-case hex sha256.
    pub sha256: String,
    /// For a `.tar.bz2` archive: the members to keep, and the file names they get. The archive
    /// itself is deleted once they are out.
    #[serde(default)]
    pub unpack: Vec<Unpack>,
}

/// One member taken out of a model archive.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Unpack {
    /// Its path inside the archive.
    pub from: String,
    /// Its name in the model folder.
    pub to: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub id: String,
    pub name: String,
    pub kind: ModelKind,
    /// SPDX id or named licence, shown before download (DIST-13).
    pub license: String,
    /// Required attribution (CC-BY) or use restrictions (OpenRAIL), shown before download.
    pub attribution: String,
    /// Where it comes from, for the licence screen.
    pub source: String,
    pub languages: Vec<String>,
    pub files: Vec<ModelFile>,
    /// Other models this one can't work without, installed with it (speech recognition needs the
    /// voice-activity model).
    #[serde(default)]
    pub requires: Vec<String>,
}

impl ModelManifest {
    pub fn download_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).sum()
    }
}

/// The models KIVO knows how to download.
pub fn catalog() -> Vec<ModelManifest> {
    const MOONSHINE: &str = "https://huggingface.co/csukuangfj2/sherpa-onnx-moonshine-base-en-quantized-2026-02-27/resolve/8f4d6c58c03d40bcea40043bb7120a878f2bbef6";
    let mut all = vec![moonshine(MOONSHINE)];
    all.extend(MOONSHINE_RELEASES.iter().map(moonshine_release));
    all.extend([
        kokoro(),
        silero(),
        smart_turn(),
        supertonic(),
        keyword_spotter(),
        campplus(),
        minilm(),
        parakeet(),
        whisper(),
        chatterbox(),
    ]);
    all
}

/// Parakeet TDT 0.6B v3's id: the Balanced recognizer for 25 European languages (VOICE-10).
pub const PARAKEET: &str = "parakeet-tdt-v3";

/// Parakeet TDT 0.6B v3 (NVIDIA, CC-BY-4.0), the int8 ONNX export of its transducer branch by
/// sherpa-onnx, pinned.
fn parakeet() -> ModelManifest {
    const BASE: &str = "https://huggingface.co/csukuangfj/sherpa-onnx-nemo-parakeet-tdt-0.6b-v3-int8/resolve/2bda32ec70b097a55adaa07d9a7173915b43cc78";
    let file = |name: &str, size: u64, sha256: &str| ModelFile {
        name: name.into(),
        url: format!("{BASE}/{name}"),
        size,
        sha256: sha256.into(),
        unpack: Vec::new(),
    };
    ModelManifest {
        id: PARAKEET.into(),
        name: "Parakeet TDT v3 (25 languages)".into(),
        kind: ModelKind::Stt,
        license: "CC-BY-4.0".into(),
        attribution:
            "Parakeet TDT 0.6B v3 by NVIDIA, licensed CC-BY-4.0; ONNX export by sherpa-onnx.".into(),
        source: "https://huggingface.co/nvidia/parakeet-tdt-0.6b-v3".into(),
        languages: [
            "bg", "hr", "cs", "da", "nl", "en", "et", "fi", "fr", "de", "el", "hu", "it", "lv",
            "lt", "mt", "pl", "pt", "ro", "sk", "sl", "es", "sv", "ru", "uk",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        requires: vec![SILERO_VAD.into(), SMART_TURN.into()],
        files: vec![
            file(
                "encoder.int8.onnx",
                652_184_281,
                "acfc2b4456377e15d04f0243af540b7fe7c992f8d898d751cf134c3a55fd2247",
            ),
            file(
                "decoder.int8.onnx",
                11_845_275,
                "179e50c43d1a9de79c8a24149a2f9bac6eb5981823f2a2ed88d655b24248db4e",
            ),
            file(
                "joiner.int8.onnx",
                6_355_277,
                "3164c13fc2821009440d20fcb5fdc78bff28b4db2f8d0f0b329101719c0948b3",
            ),
            file(
                "tokens.txt",
                93_939,
                "d58544679ea4bc6ac563d1f545eb7d474bd6cfa467f0a6e2c1dc1c7d37e3c35d",
            ),
        ],
    }
}

/// Whisper large-v3-turbo's id: the Accurate recognizer (VOICE-10).
pub const WHISPER: &str = "whisper-large-v3-turbo";

/// Whisper large-v3-turbo (OpenAI, MIT), the int8 ONNX export by sherpa-onnx, pinned.
fn whisper() -> ModelManifest {
    const BASE: &str = "https://huggingface.co/csukuangfj/sherpa-onnx-whisper-turbo/resolve/2ca6ff69fc878651b770880507669577ac41c2ff";
    let file = |name: &str, size: u64, sha256: &str| ModelFile {
        name: name.into(),
        url: format!("{BASE}/{name}"),
        size,
        sha256: sha256.into(),
        unpack: Vec::new(),
    };
    ModelManifest {
        id: WHISPER.into(),
        name: "Whisper large-v3-turbo".into(),
        kind: ModelKind::Stt,
        license: "MIT".into(),
        attribution: "Whisper large-v3-turbo by OpenAI, MIT License; ONNX export by sherpa-onnx."
            .into(),
        source: "https://huggingface.co/openai/whisper-large-v3-turbo".into(),
        languages: [
            "en", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr", "hu",
            "id", "it", "ja", "ko", "lt", "lv", "nl", "pa", "pl", "pt", "ro", "ru", "sk", "sl",
            "sv", "tr", "uk", "vi", "zh",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        requires: vec![SILERO_VAD.into(), SMART_TURN.into()],
        files: vec![
            file(
                "turbo-encoder.int8.onnx",
                674_716_297,
                "b02dcdf54f348741e93fe732b67d933c8dcb6735655f710640143081db38878b",
            ),
            file(
                "turbo-decoder.int8.onnx",
                361_080_764,
                "20accd02388482eb3a46bd615631adfdc85e1eb2c7db9ea3f02a40ffe6b81547",
            ),
            file(
                "turbo-tokens.txt",
                816_730,
                "b34b360dbb493e781e479794586d661700670d65564001f23024971d1f2fa126",
            ),
        ],
    }
}

/// Chatterbox Turbo's id: the Expressive voice (VOICE-11).
pub const CHATTERBOX: &str = "chatterbox-turbo";

/// Chatterbox Turbo (Resemble AI, MIT): the publisher's own ONNX export, q4 weights, pinned.
fn chatterbox() -> ModelManifest {
    const BASE: &str = "https://huggingface.co/ResembleAI/chatterbox-turbo-ONNX/resolve/d21799bd0354adb85e348b8a0442a8405110a2cf";
    let file = |name: &str, size: u64, sha256: &str| ModelFile {
        name: name.into(),
        url: format!("{BASE}/{name}"),
        size,
        sha256: sha256.into(),
        unpack: Vec::new(),
    };
    ModelManifest {
        id: CHATTERBOX.into(),
        name: "Chatterbox Turbo".into(),
        kind: ModelKind::Tts,
        license: "MIT".into(),
        attribution: "Chatterbox Turbo by Resemble AI, MIT License. KIVO gives it one of your Windows voices to copy; no one's recording is used.".into(),
        source: "https://huggingface.co/ResembleAI/chatterbox-turbo-ONNX".into(),
        languages: vec!["en".into()],
        requires: Vec::new(),
        files: vec![
            file("tokenizer.json", 3_562_272, "3f04e34bea22f9144d1a19151154095bc9ce0430bf421304f5797e716288a906"),
            file("onnx/speech_encoder_q4.onnx", 1_200_346, "37956c20b67bed85a0da4bc83509d67b5969a1b257d1c546516a5236a17ad71e"),
            file("onnx/speech_encoder_q4.onnx_data", 229_560_112, "58956db217c6443e49c91bdd54d7cf76b4a243f225c748b7bf746459fc27bc7d"),
            file("onnx/embed_tokens_q4.onnx", 2_844, "fd6ba1d22902e8f539d3dd6d7c1c44b98ebb4c84ebbb5e47fcb826ddcf667561"),
            file("onnx/embed_tokens_q4.onnx_data", 37_286_384, "f54a51e234b509b64c3a03bb79e1149fba7e2eba6c2d9c222f18883379e1f5d8"),
            file("onnx/language_model_q4.onnx", 274_572, "b39d03d3f8b943b9e60c6fce3fb41191dbc1df4589f913291db1e214eef669b1"),
            file("onnx/language_model_q4.onnx_data", 204_456_572, "2c029dc0acf48752473d8c74c72b5ceaaad76b9886fe106eaf2022142d5b5d5e"),
            file("onnx/conditional_decoder_q4.onnx", 2_179_022, "dccb7a6cea3472dc7f7d070eeb70ade18e6327fb4ec61a3d62cf211bfed90ea2"),
            file("onnx/conditional_decoder_q4.onnx_data", 246_397_384, "b5c5317e0b79a1a19dd3d5e2b2091ea06b15716716ab801a54eaeb906c6971ec"),
        ],
    }
}

/// The sentence-embedding model's id: the intent router's semantic stage (BRAIN-03).
pub const EMBEDDING_MODEL: &str = "minilm-l6-v2";

/// all-MiniLM-L6-v2 (sentence-transformers, Apache-2.0): the quantized ONNX export and its
/// WordPiece vocabulary, pinned. Lets KIVO understand paraphrased commands ("kill the sound")
/// without a brain.
fn minilm() -> ModelManifest {
    const REPO: &str = "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/1110a243fdf4706b3f48f1d95db1a4f5529b4d41";
    ModelManifest {
        id: EMBEDDING_MODEL.into(),
        name: "Command understanding (paraphrases)".into(),
        kind: ModelKind::Embedding,
        license: "Apache-2.0".into(),
        attribution: "all-MiniLM-L6-v2 by sentence-transformers (UKP Lab), Apache License 2.0."
            .into(),
        source: "https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2".into(),
        languages: vec!["en".into()],
        files: vec![
            ModelFile {
                name: "model.onnx".into(),
                url: format!("{REPO}/onnx/model_quint8_avx2.onnx"),
                size: 23_046_789,
                sha256: "b941bf19f1f1283680f449fa6a7336bb5600bdcd5f84d10ddc5cd72218a0fd21".into(),
                unpack: Vec::new(),
            },
            ModelFile {
                name: "vocab.txt".into(),
                url: format!("{REPO}/vocab.txt"),
                size: 231_508,
                sha256: "07eced375cec144d27c900241f3e339478dec958f92fddbc551f295c992038a3".into(),
                unpack: Vec::new(),
            },
        ],
        requires: Vec::new(),
    }
}

/// A Moonshine model from the sherpa-onnx author's releases, pinned.
struct MoonshineRelease {
    id: &'static str,
    name: &'static str,
    language: &'static str,
    repo: &'static str,
    revision: &'static str,
    encoder: (u64, &'static str),
    decoder: (u64, &'static str),
    tokens: (u64, &'static str),
}

/// The other Moonshine models (VOICE-43/46): Tiny for English (MIT), and models for other
/// languages under the non-commercial Moonshine Community License, which is shown before the
/// download.
const MOONSHINE_RELEASES: [MoonshineRelease; 9] = [
    MoonshineRelease {
        id: "moonshine-tiny-en",
        name: "Moonshine Tiny (English)",
        language: "en",
        repo: "csukuangfj2/sherpa-onnx-moonshine-tiny-en-quantized-2026-02-27",
        revision: "d1e6c30921780b8508d04b492dfb3ce8a51605d4",
        encoder: (
            13_281_600,
            "94e90a4654fc45cdfedb77c4c08e1739f48862998e58fada384b25118134f221",
        ),
        decoder: (
            30_412_256,
            "cf524c4862d36e9e5ab032eddc73637efd822d70e868ac575cf1a46e1e4708a0",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
    MoonshineRelease {
        id: "moonshine-base-es",
        name: "Moonshine Base (Spanish)",
        language: "es",
        repo: "csukuangfj2/sherpa-onnx-moonshine-base-es-quantized-2026-02-27",
        revision: "59e235a505b7372eb7f896471a74c776f6f6e061",
        encoder: (
            20_964_320,
            "331aafa2fc7f7e55ba28376eef08eaf919ae105bb719cd64ba6875505dca72b3",
        ),
        decoder: (
            43_612_200,
            "8e6513ad66a3a71ca86824746a09051eeb60468940eaf8201ce758e8919e2b5d",
        ),
        tokens: (
            532_090,
            "c6e35883ba038f70ceea8b4a21cf79742adbda7e6730cbff27460857e0667a02",
        ),
    },
    MoonshineRelease {
        id: "moonshine-base-ja",
        name: "Moonshine Base (Japanese)",
        language: "ja",
        repo: "csukuangfj2/sherpa-onnx-moonshine-base-ja-quantized-2026-02-27",
        revision: "cf95fc9c3a55a240892c80c501e80b35603d20bc",
        encoder: (
            31_326_816,
            "3230cafb84d5f08800e60de6f932aad7c69c649a37fa6997fe04fe25f808b56b",
        ),
        decoder: (
            109_424_424,
            "35a522052d2d8695d0dd2870666f088d633111e08cfbed0418f61c4122b4ba25",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
    MoonshineRelease {
        id: "moonshine-base-zh",
        name: "Moonshine Base (Chinese)",
        language: "zh",
        repo: "csukuangfj2/sherpa-onnx-moonshine-base-zh-quantized-2026-02-27",
        revision: "19042e30b8680b101d0db3b0b8cabc84ae954b7f",
        encoder: (
            31_326_816,
            "c725a24b58595905921ea2a47e2bcf0f18c78f4d171d96136f2dcbc8c77a58a6",
        ),
        decoder: (
            109_424_520,
            "bf79fce626e123739ec37eceb2b2a010a93d720da266dd5d8ef9a47ef9a7dc36",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
    MoonshineRelease {
        id: "moonshine-base-ar",
        name: "Moonshine Base (Arabic)",
        language: "ar",
        repo: "csukuangfj2/sherpa-onnx-moonshine-base-ar-quantized-2026-02-27",
        revision: "00c1062f1b70e4da96658486f4fd79552d8bc22b",
        encoder: (
            31_326_824,
            "68e50ebe0317ce909f098044a5dda2a76e6b86dc882829a317771fbafc5826ae",
        ),
        decoder: (
            109_424_552,
            "8f272cb50818e28ad86bbffc21e1450a4d57155e95f099ca6a236f38f4d9eafb",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
    MoonshineRelease {
        id: "moonshine-base-uk",
        name: "Moonshine Base (Ukrainian)",
        language: "uk",
        repo: "csukuangfj2/sherpa-onnx-moonshine-base-uk-quantized-2026-02-27",
        revision: "336649c4c1cd81d066c586163381fe8bb65d9145",
        encoder: (
            31_326_816,
            "ea37ff7a2c308b566def1fd6e6860e23db4968590f192d5cc0fbc494666e30f9",
        ),
        decoder: (
            109_424_424,
            "2c0c1ebc20ba75a21ff5315fd039e53d2fc9ba77a9ca9ad3bc4c7c1c1de93cbc",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
    MoonshineRelease {
        id: "moonshine-base-vi",
        name: "Moonshine Base (Vietnamese)",
        language: "vi",
        repo: "csukuangfj2/sherpa-onnx-moonshine-base-vi-quantized-2026-02-27",
        revision: "2946c5272fcb2d92f5cd99edb6e828c66be94f2e",
        encoder: (
            31_326_816,
            "5a63cf0e248ef463776fbad50784813b4185eb73136f1fb062c5722e667130dc",
        ),
        decoder: (
            109_424_520,
            "0a4f007e9d585348d94124d8de47f7aefbc3e2d3fa44152646af7b94c639770d",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
    MoonshineRelease {
        id: "moonshine-tiny-ko",
        name: "Moonshine Tiny (Korean)",
        language: "ko",
        repo: "csukuangfj2/sherpa-onnx-moonshine-tiny-ko-quantized-2026-02-27",
        revision: "3f2e79d6fe04ae0009fcb150a035586c12ac742f",
        encoder: (
            13_238_176,
            "947260d46252f48eada86a34986b3f70c01d68a343959949a77375b94debd055",
        ),
        decoder: (
            58_327_336,
            "95aa9f2e764b80625d2889d6ec9f05c965808e540ac50c16abd10c7ea33fe44b",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
    MoonshineRelease {
        id: "moonshine-tiny-ja",
        name: "Moonshine Tiny (Japanese)",
        language: "ja",
        repo: "csukuangfj2/sherpa-onnx-moonshine-tiny-ja-quantized-2026-02-27",
        revision: "550e2eb0a8b33092f3b394f64649ee1b3d9eb506",
        encoder: (
            13_238_184,
            "86ece73812604b9b5f1274b4d1e6eec0d783b96088ff46d49e30a53f881cad73",
        ),
        decoder: (
            58_327_272,
            "9fcd9b71323a496b307e20dd305c4e9a1b533c7bedd6e4f660e974967dd60bb6",
        ),
        tokens: (
            549_350,
            "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049",
        ),
    },
];

fn moonshine_release(r: &MoonshineRelease) -> ModelManifest {
    let base = format!("https://huggingface.co/{}/resolve/{}", r.repo, r.revision);
    let file = |name: &str, (size, sha256): (u64, &str)| ModelFile {
        name: name.into(),
        url: format!("{base}/{name}"),
        size,
        sha256: sha256.into(),
        unpack: Vec::new(),
    };
    let english = r.language == "en";
    ModelManifest {
        id: r.id.into(),
        name: r.name.into(),
        kind: ModelKind::Stt,
        license: if english {
            "MIT".into()
        } else {
            "Moonshine Community License (non-commercial)".into()
        },
        attribution: if english {
            "Moonshine by Moonshine AI, MIT License; ONNX export by sherpa-onnx.".into()
        } else {
            "Moonshine by Moonshine AI under the Moonshine Community License, a non-commercial licence: personal use only. ONNX export by sherpa-onnx.".into()
        },
        source: format!("https://huggingface.co/{}", r.repo),
        languages: vec![r.language.into()],
        requires: vec![SILERO_VAD.into(), SMART_TURN.into()],
        files: vec![
            file("encoder_model.ort", r.encoder),
            file("decoder_model_merged.ort", r.decoder),
            file("tokens.txt", r.tokens),
        ],
    }
}

/// The keyword-spotting model's id: "Hey Kivo", custom wake words and "Kivo stop" (VOICE §4).
pub const KEYWORD_SPOTTER: &str = "kws-zipformer-gigaspeech-en";
/// The speaker model's id (VOICE §5).
pub const SPEAKER_MODEL: &str = "campplus-voxceleb-en";

/// sherpa-onnx's English keyword spotter (Apache-2.0, icefall Zipformer trained on GigaSpeech,
/// 3.3M parameters): the int8 model, its tokens and its BPE vocabulary, out of the official
/// release archive. KIVO runs it with its own code (DECISIONS "No training").
fn keyword_spotter() -> ModelManifest {
    const DIR: &str = "sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01";
    const STEM: &str = "epoch-12-avg-2-chunk-16-left-64.int8.onnx";
    let member = |from: String, to: &str| Unpack {
        from: format!("{DIR}/{from}"),
        to: to.into(),
    };
    ModelManifest {
        id: KEYWORD_SPOTTER.into(),
        name: "Wake-word listener (English)".into(),
        kind: ModelKind::Wake,
        license: "Apache-2.0".into(),
        attribution: "Keyword-spotting Zipformer by the k2-fsa project (sherpa-onnx, icefall), Apache License 2.0.".into(),
        source: "https://github.com/k2-fsa/sherpa-onnx".into(),
        languages: vec!["en".into()],
        files: vec![ModelFile {
            name: format!("{DIR}.tar.bz2"),
            url: format!("https://github.com/k2-fsa/sherpa-onnx/releases/download/kws-models/{DIR}.tar.bz2"),
            size: 17_626_723,
            sha256: "f170013b4716e41b62b9bfd809687c207cef798ef9bc6534d524e17af9b6561a".into(),
            unpack: vec![
                member(format!("encoder-{STEM}"), "encoder.onnx"),
                member(format!("decoder-{STEM}"), "decoder.onnx"),
                member(format!("joiner-{STEM}"), "joiner.onnx"),
                member("tokens.txt".into(), "tokens.txt"),
                member("bpe.model".into(), "bpe.model"),
            ],
        }],
        requires: Vec::new(),
    }
}

/// CAM++ from 3D-Speaker (Apache-2.0), trained on VoxCeleb: 512-number voice embeddings for
/// recognizing the owner (VOICE §5). Downloaded only if the user enrolls their voice.
fn campplus() -> ModelManifest {
    ModelManifest {
        id: SPEAKER_MODEL.into(),
        name: "Voice recognition (CAM++)".into(),
        kind: ModelKind::Speaker,
        license: "Apache-2.0".into(),
        attribution: "CAM++ speaker model by Alibaba's 3D-Speaker project, Apache License 2.0; ONNX export by sherpa-onnx.".into(),
        source: "https://github.com/modelscope/3D-Speaker".into(),
        languages: vec!["*".into()],
        files: vec![ModelFile {
            name: "campplus.onnx".into(),
            url: "https://github.com/k2-fsa/sherpa-onnx/releases/download/speaker-recongition-models/3dspeaker_speech_campplus_sv_en_voxceleb_16k.onnx".into(),
            size: 29_596_978,
            sha256: "357a834f702b80161e5b981182c038e18553c1f2ca752ed6cec2052365d4129b".into(),
            unpack: Vec::new(),
        }],
        requires: Vec::new(),
    }
}

/// Silero VAD v6 (MIT): tells KIVO when someone is speaking. Installed with any speech-recognition
/// model; not bundled (owner: no models in the installer).
fn silero() -> ModelManifest {
    ModelManifest {
        id: SILERO_VAD.into(),
        name: "Silero voice activity detection".into(),
        kind: ModelKind::Vad,
        license: "MIT".into(),
        attribution: "Silero VAD by the Silero Team, MIT License.".into(),
        source: "https://github.com/snakers4/silero-vad".into(),
        languages: Vec::new(),
        files: vec![ModelFile {
            name: "silero_vad.onnx".into(),
            url: "https://raw.githubusercontent.com/snakers4/silero-vad/4c00cd14be0ff5b8bd6846a6eec72741aac837f2/src/silero_vad/data/silero_vad.onnx".into(),
            size: 2_327_524,
            sha256: "597d30b3ec076608d059477bb14cfeffdf951bf5cae370d38f65d33bbfe82004".into(),
            unpack: Vec::new(),
        }],
        requires: Vec::new(),
    }
}

/// Supertonic 3 (Supertone; weights OpenRAIL-M, code MIT): natural voices in 31 languages from
/// characters, no phonemizer (VOICE-46). Its licence carries use restrictions, shown before the
/// download (DIST-13).
fn supertonic() -> ModelManifest {
    const BASE: &str = "https://huggingface.co/Supertone/supertonic-3/resolve/3cadd1ee6394adea1bd021217a0e650ede09a323";
    let file = |name: &str, size: u64, sha256: &str| ModelFile {
        name: name.into(),
        url: format!("{BASE}/{name}"),
        size,
        sha256: sha256.into(),
        unpack: Vec::new(),
    };
    ModelManifest {
        id: SUPERTONIC.into(),
        name: "Supertonic 3 (31 languages)".into(),
        kind: ModelKind::Tts,
        license: "OpenRAIL-M".into(),
        attribution: "Supertonic 3 by Supertone Inc. Model weights under the BigScience Open RAIL-M License, which forbids certain uses (see the licence); code under the MIT License.".into(),
        source: "https://huggingface.co/Supertone/supertonic-3".into(),
        languages: vec![
            "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi",
            "hr", "hu", "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv",
            "tr", "uk", "vi",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect(),
        files: vec![
            file("onnx/tts.json", 8_253, "42078d3aef1cd43ab43021f3c54f47d2d75ceb4e75f627f118890128b06a0d09"),
            file("onnx/unicode_indexer.json", 277_676, "9bf7346e43883a81f8645c81224f786d43c5b57f3641f6e7671a7d6c493cb24f"),
            file("onnx/duration_predictor.onnx", 3_700_147, "c3eb91414d5ff8a7a239b7fe9e34e7e2bf8a8140d8375ffb14718b1c639325db"),
            file("onnx/text_encoder.onnx", 36_416_150, "c7befd5ea8c3119769e8a6c1486c4edc6a3bc8365c67621c881bbb774b9902ff"),
            file("onnx/vector_estimator.onnx", 256_534_781, "883ac868ea0275ef0e991524dc64f16b3c0376efd7c320af6b53f5b780d7c61c"),
            file("onnx/vocoder.onnx", 101_424_195, "085de76dd8e8d5836d6ca66826601f615939218f90e519f70ee8a36ed2a4c4ba"),
            file("voice_styles/F1.json", 292_046, "bbdec6ee00231c2c742ad05483df5334cab3b52fda3ba38e6a07059c4563dbc2"),
            file("voice_styles/F2.json", 292_423, "7c722c6a72707b1a77f035d67f0d1351ba187738e06f7683e8c72b1df3477fc6"),
            file("voice_styles/F3.json", 290_794, "12f6ef2573baa2defa1128069cb59f203e3ab67c92af77b42df8a0e3a2f7c6ab"),
            file("voice_styles/F4.json", 291_808, "c2fa764c1225a76dfc3e2c73e8aa4f70d9ee48793860eb34c295fff01c2e032b"),
            file("voice_styles/F5.json", 291_479, "45966e73316415626cf41a7d1c6f3b4c70dbc1ba2bee5c1978ef0ce33244fc8d"),
            file("voice_styles/M1.json", 291_748, "e35604687f5d23694b8e91593a93eec0e4eca6c0b02bb8ed69139ab2ea6b0a5b"),
            file("voice_styles/M2.json", 292_055, "b76cbf62bac707c710cf0ae5aba5e31eea1a6339a9734bfae33ab98499534a50"),
            file("voice_styles/M3.json", 290_198, "ea1ac35ccb91b0d7ecad533a2fbd0eec10c91513d8951e3b25fbba99954e159b"),
            file("voice_styles/M4.json", 291_522, "ca8eefad4fcd989c9379032ff3e50738adc547eeb5e221b82593a6d7b3bac303"),
            file("voice_styles/M5.json", 291_469, "dd22b92740314321f8ae11c5e87f8dd60d060f15dd3a632b5adf77f471f77af2"),
        ],
        requires: Vec::new(),
    }
}

/// Supertonic's id (VOICE-46).
pub const SUPERTONIC: &str = "supertonic-3";

/// The voice-activity model's id.
pub const SILERO_VAD: &str = "silero-vad-v6";
/// The end-of-turn model's id (VOICE-33).
pub const SMART_TURN: &str = "smart-turn-v3";

/// Smart Turn v3.2 (Daily/pipecat, BSD-2-Clause): tells a finished sentence from a pause
/// mid-thought (VOICE-33). Installed with any speech-recognition model.
fn smart_turn() -> ModelManifest {
    ModelManifest {
        id: SMART_TURN.into(),
        name: "Smart Turn (end of sentence)".into(),
        kind: ModelKind::Vad,
        license: "BSD-2-Clause".into(),
        attribution: "Smart Turn v3 by Daily (pipecat-ai), BSD 2-Clause License.".into(),
        source: "https://github.com/pipecat-ai/smart-turn".into(),
        languages: vec!["*".into()],
        files: vec![ModelFile {
            name: "smart-turn.onnx".into(),
            url: "https://huggingface.co/pipecat-ai/smart-turn-v3/resolve/f766f81d3cfdf7737ac64aad813d91bbfd56bf93/smart-turn-v3.2-cpu.onnx".into(),
            size: 8_679_182,
            sha256: "2bb026316b14a660486a75b1733cd3fbab8c2fd0314dc9af7be49f8cca967e4f".into(),
            unpack: Vec::new(),
        }],
        requires: Vec::new(),
    }
}

/// Kokoro-82M (Apache-2.0): the quantized ONNX model, five voices, and misaki's US dictionaries
/// (Apache-2.0) for KIVO's phonemizer (VOICE-09).
fn kokoro() -> ModelManifest {
    const MODEL: &str = "https://huggingface.co/onnx-community/Kokoro-82M-v1.0-ONNX/resolve/1939ad2a8e416c0acfeecc08a694d14ef25f2231";
    const MISAKI: &str = "https://raw.githubusercontent.com/hexgrad/misaki/fba1236595f2d2bf21d414ba6e57d25256afada3/misaki/data";
    let voice = |id: &str, sha256: &str| ModelFile {
        name: format!("voices/{id}.bin"),
        url: format!("{MODEL}/voices/{id}.bin"),
        size: 522_240,
        sha256: sha256.into(),
        unpack: Vec::new(),
    };
    ModelManifest {
        id: "kokoro-82m".into(),
        name: "Kokoro (English voices)".into(),
        kind: ModelKind::Tts,
        license: "Apache-2.0".into(),
        attribution: "Kokoro-82M by hexgrad, Apache License 2.0. Pronunciation dictionaries from misaki by hexgrad, Apache License 2.0.".into(),
        source: "https://huggingface.co/hexgrad/Kokoro-82M".into(),
        languages: vec!["en".into()],
        requires: Vec::new(),
        files: vec![
            ModelFile {
                name: "model_fp16.onnx".into(),
                url: format!("{MODEL}/onnx/model_fp16.onnx"),
                size: 163_234_740,
                sha256: "ba4527a874b42b21e35f468c10d326fdff3c7fc8cac1f85e9eb6c0dfc35c334a".into(),
                unpack: Vec::new(),
            },
            voice("af_heart", "d583ccff3cdca2f7fae535cb998ac07e9fcb90f09737b9a41fa2734ec44a8f0b"),
            voice("af_bella", "f69d836209b78eb8c66e75e3cda491e26ea838a3674257e9d4e5703cbaf55c8b"),
            voice("am_michael", "1d1f21dd8da39c30705cd4c75d039d265e9bc4a2a93ed09bc9e1b1225eb95ba1"),
            voice("bf_emma", "669fe0647f9dd04fcab92f1439a40eeb4c8b4ab1f82e4996fe3d918ce4a63b73"),
            voice("bm_george", "c4b235a4c1f2cd3b939fed08b899ce9385638b763f7b73a59616c4fc9bd6c9bc"),
            ModelFile {
                name: "us_gold.json".into(),
                url: format!("{MISAKI}/us_gold.json"),
                size: 3_000_469,
                sha256: "dc414872a49a28ae6c141463d502fd945f3b2fde040484fdc47d00cc4612686f".into(),
                unpack: Vec::new(),
            },
            ModelFile {
                name: "us_silver.json".into(),
                url: format!("{MISAKI}/us_silver.json"),
                size: 3_099_517,
                sha256: "de8f67be911bb6c659187b4a65fd966b6a30e56350e0f790d763210b053ac475".into(),
                unpack: Vec::new(),
            },
        ],
    }
}

fn moonshine(base: &str) -> ModelManifest {
    ModelManifest {
        id: "moonshine-base-en".into(),
        name: "Moonshine Base (English)".into(),
        kind: ModelKind::Stt,
        license: "MIT".into(),
        attribution: "Moonshine by Useful Sensors (Moonshine AI), MIT License.".into(),
        source: "https://github.com/moonshine-ai/moonshine".into(),
        languages: vec!["en".into()],
        requires: vec![SILERO_VAD.into(), SMART_TURN.into()],
        files: vec![
            ModelFile {
                name: "encoder_model.ort".into(),
                url: format!("{base}/encoder_model.ort"),
                size: 31_326_816,
                sha256: "7c66495948d0d08ec1af454cd4b5514862ae6511e94712a60e6d83eaec8dc8cf".into(),
                unpack: Vec::new(),
            },
            ModelFile {
                name: "decoder_model_merged.ort".into(),
                url: format!("{base}/decoder_model_merged.ort"),
                size: 109_424_400,
                sha256: "d9d7b333af34bc552580576ddcf248a1c6c839e0d3b43b09afb9376ed009899d".into(),
                unpack: Vec::new(),
            },
            ModelFile {
                name: "tokens.txt".into(),
                url: format!("{base}/tokens.txt"),
                size: 549_350,
                sha256: "2870d843e14c1e187bf1913a521562a63b53933814bd7f2145120468f494a049".into(),
                unpack: Vec::new(),
            },
        ],
    }
}

/// A model on disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledModel {
    pub manifest: ModelManifest,
    pub dir: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("KIVO doesn't know a model called {0}")]
    Unknown(String),
    #[error("the download failed; check the internet connection and try again")]
    Network(String),
    #[error("a downloaded file was damaged; try again")]
    Corrupt { file: String },
    #[error("there isn't enough space or the models folder can't be written")]
    Disk(#[from] io::Error),
    #[error("the download was cancelled")]
    Cancelled,
}

/// Where downloads come from. The real one speaks HTTP; tests use an in-memory one.
pub trait Fetcher: Send + Sync {
    /// Opens `url` starting at byte `from` (an HTTP range request when `from > 0`). Returns the
    /// reader and whether the server honoured the range (if not, it starts from byte 0).
    fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read + Send>, bool), ModelError>;
}

/// Progress of one model download.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

pub struct ModelStore {
    root: PathBuf,
}

impl ModelStore {
    /// Models under `root` (`Paths::models()`).
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn dir(&self, id: &str) -> PathBuf {
        self.root.join(id)
    }

    /// The installed model, if it is complete.
    pub fn installed(&self, id: &str) -> Option<InstalledModel> {
        let dir = self.dir(id);
        let manifest: ModelManifest =
            serde_json::from_slice(&fs::read(dir.join("manifest.json")).ok()?).ok()?;
        // An archive is gone once unpacked: its members are what must be there.
        let bytes = manifest
            .files
            .iter()
            .flat_map(|f| {
                if f.unpack.is_empty() {
                    vec![f.name.as_str()]
                } else {
                    f.unpack.iter().map(|u| u.to.as_str()).collect()
                }
            })
            .map(|name| fs::metadata(dir.join(name)).map(|m| m.len()).ok())
            .sum::<Option<u64>>()?;
        Some(InstalledModel {
            manifest,
            dir,
            bytes,
        })
    }

    /// Every complete model on this PC (Voice → Models on this PC, DIST-13).
    pub fn list(&self) -> Vec<InstalledModel> {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut models: Vec<_> = entries
            .filter_map(Result::ok)
            .filter_map(|e| self.installed(e.file_name().to_str()?))
            .collect();
        models.sort_by(|a, b| a.manifest.id.cmp(&b.manifest.id));
        models
    }

    /// Deletes a model (and any unfinished download of it).
    pub fn remove(&self, id: &str) -> Result<(), ModelError> {
        for dir in [self.dir(id), self.partial_dir(id)] {
            if dir.exists() {
                fs::remove_dir_all(&dir)?;
            }
        }
        Ok(())
    }

    /// The folder models live in.
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn partial_dir(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.partial"))
    }

    /// A download was started and stopped before it finished (Pause): it resumes from here.
    pub fn has_partial(&self, id: &str) -> bool {
        self.partial_dir(id).is_dir()
    }

    /// Drops an unfinished download (Cancel).
    pub fn discard_partial(&self, id: &str) -> Result<(), ModelError> {
        let dir = self.partial_dir(id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// The catalog has a different version of an installed model (other files or checksums).
    pub fn update_available(&self, installed: &InstalledModel, latest: &ModelManifest) -> bool {
        let files = |m: &ModelManifest| {
            m.files
                .iter()
                .map(|f| (f.name.clone(), f.sha256.clone()))
                .collect::<Vec<_>>()
        };
        files(&installed.manifest) != files(latest)
    }

    /// Installs the catalog's version of a model that is already installed: the new files are
    /// downloaded beside it and swapped in when complete.
    pub fn update(
        &self,
        manifest: &ModelManifest,
        fetcher: &dyn Fetcher,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<InstalledModel, ModelError> {
        self.install_files(manifest, fetcher, cancel, progress)
    }

    /// Downloads and installs `manifest`, resuming an earlier partial download. `progress` is
    /// called as bytes arrive.
    pub fn install(
        &self,
        manifest: &ModelManifest,
        fetcher: &dyn Fetcher,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<InstalledModel, ModelError> {
        if let Some(done) = self.installed(&manifest.id) {
            return Ok(done);
        }
        self.install_files(manifest, fetcher, cancel, progress)
    }

    fn install_files(
        &self,
        manifest: &ModelManifest,
        fetcher: &dyn Fetcher,
        cancel: &CancellationToken,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<InstalledModel, ModelError> {
        let partial = self.partial_dir(&manifest.id);
        fs::create_dir_all(&partial)?;
        let total = manifest.download_size();
        let mut done_before = 0;
        for file in &manifest.files {
            let target = partial.join(&file.name);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let unpacked = !file.unpack.is_empty()
                && file.unpack.iter().all(|u| partial.join(&u.to).is_file());
            if unpacked || (target.is_file() && fs::metadata(&target)?.len() == file.size) {
                done_before += file.size;
                continue; // finished and verified in an earlier attempt
            }
            let part = partial.join(format!("{}.part", file.name));
            let mut attempts = 0;
            loop {
                attempts += 1;
                match download(file, &part, fetcher, cancel, &mut |n| {
                    progress(Progress {
                        done: done_before + n,
                        total,
                    });
                }) {
                    Ok(()) => break,
                    // A damaged file starts over once; a second failure is reported.
                    Err(ModelError::Corrupt { .. }) if attempts == 1 => {
                        fs::remove_file(&part)?;
                    }
                    Err(e) => return Err(e),
                }
            }
            fs::rename(&part, &target)?;
            if !file.unpack.is_empty() {
                unpack(&target, &file.unpack, &partial)?;
                fs::remove_file(&target)?;
            }
            done_before += file.size;
        }
        fs::write(
            partial.join("manifest.json"),
            serde_json::to_vec_pretty(manifest).map_err(io::Error::other)?,
        )?;
        let dir = self.dir(&manifest.id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        fs::rename(&partial, &dir)?;
        tracing::info!(model = manifest.id, "model installed");
        self.installed(&manifest.id)
            .ok_or_else(|| ModelError::Corrupt {
                file: "manifest.json".into(),
            })
    }
}

/// Takes the listed members out of a verified `.tar.bz2` archive into `dir`.
fn unpack(archive: &Path, members: &[Unpack], dir: &Path) -> Result<(), ModelError> {
    let reader = bzip2::read::BzDecoder::new(io::BufReader::new(File::open(archive)?));
    let mut tar = tar::Archive::new(reader);
    let mut found = 0;
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_string_lossy().replace('\\', "/");
        if let Some(member) = members.iter().find(|m| m.from == path) {
            let target = dir.join(&member.to);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            io::copy(&mut entry, &mut File::create(&target)?)?;
            found += 1;
        }
    }
    if found != members.len() {
        return Err(ModelError::Corrupt {
            file: archive
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        });
    }
    Ok(())
}

/// Downloads one file into `part`, resuming from its current length, then checks its hash.
fn download(
    file: &ModelFile,
    part: &Path,
    fetcher: &dyn Fetcher,
    cancel: &CancellationToken,
    progress: &mut dyn FnMut(u64),
) -> Result<(), ModelError> {
    let have = fs::metadata(part).map_or(0, |m| m.len()).min(file.size);
    let (mut body, resumed) = fetcher.open(&file.url, have)?;
    let mut out = if resumed && have > 0 {
        OpenOptions::new().append(true).open(part)?
    } else {
        File::create(part)?
    };
    let mut written = if resumed { have } else { 0 };
    let mut buf = vec![0; 256 * 1024];
    loop {
        if cancel.is_cancelled() {
            return Err(ModelError::Cancelled);
        }
        let n = body
            .read(&mut buf)
            .map_err(|e| ModelError::Network(e.to_string()))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        written += n as u64;
        progress(written);
        if written > file.size {
            break; // more than the manifest promised: the hash check will reject it
        }
    }
    out.sync_all()?;
    drop(out);
    if written != file.size || sha256_file(part)? != file.sha256 {
        return Err(ModelError::Corrupt {
            file: file.name.clone(),
        });
    }
    Ok(())
}

pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut f = File::open(path)?;
    let mut buf = vec![0; 1024 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads over HTTPS (Windows' own TLS), following redirects (Hugging Face → its CDN).
pub struct HttpFetcher {
    agent: ureq::Agent,
}

impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .tls_config(
                ureq::tls::TlsConfig::builder()
                    .provider(ureq::tls::TlsProvider::NativeTls)
                    .build(),
            )
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .user_agent(concat!("KIVO/", env!("CARGO_PKG_VERSION")))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Fetcher for HttpFetcher {
    fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read + Send>, bool), ModelError> {
        let mut request = self.agent.get(url);
        if from > 0 {
            request = request.header("Range", format!("bytes={from}-"));
        }
        let response = request
            .call()
            .map_err(|e| ModelError::Network(e.to_string()))?;
        let resumed = response.status() == 206;
        Ok((Box::new(response.into_body().into_reader()), resumed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Serves files from memory; can cut a response short to simulate a dropped connection.
    struct FakeServer {
        files: HashMap<String, Vec<u8>>,
        cut_first_after: Mutex<Option<usize>>,
        range_requests: AtomicUsize,
    }

    impl Fetcher for FakeServer {
        fn open(&self, url: &str, from: u64) -> Result<(Box<dyn Read + Send>, bool), ModelError> {
            let data = self
                .files
                .get(url)
                .ok_or_else(|| ModelError::Network("404".into()))?;
            if from > 0 {
                self.range_requests.fetch_add(1, Ordering::Relaxed);
            }
            let from = usize::try_from(from).unwrap();
            let mut body = data[from..].to_vec();
            if let Some(cut) = self.cut_first_after.lock().unwrap().take() {
                body.truncate(cut);
            }
            Ok((Box::new(io::Cursor::new(body)), true))
        }
    }

    fn sha(data: &[u8]) -> String {
        hex(&Sha256::digest(data))
    }

    fn manifest(files: &[(&str, &[u8])]) -> ModelManifest {
        ModelManifest {
            id: "test-model".into(),
            name: "Test".into(),
            kind: ModelKind::Stt,
            license: "MIT".into(),
            attribution: String::new(),
            source: String::new(),
            languages: vec!["en".into()],
            requires: Vec::new(),
            files: files
                .iter()
                .map(|(name, data)| ModelFile {
                    name: (*name).into(),
                    url: format!("https://example/{name}"),
                    size: data.len() as u64,
                    sha256: sha(data),
                    unpack: Vec::new(),
                })
                .collect(),
        }
    }

    fn server(files: &[(&str, &[u8])], cut: Option<usize>) -> FakeServer {
        FakeServer {
            files: files
                .iter()
                .map(|(n, d)| (format!("https://example/{n}"), d.to_vec()))
                .collect(),
            cut_first_after: Mutex::new(cut),
            range_requests: AtomicUsize::new(0),
        }
    }

    /// UX-61: a paused download resumes where it stopped; Cancel drops it; a changed catalog
    /// entry reads as an update, which replaces the installed files.
    #[test]
    fn pause_resume_cancel_and_update() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let files: [(&str, &[u8]); 2] = [("a.onnx", &[7; 4000]), ("b.bin", &[9; 3000])];
        let m = manifest(&files);
        // Pause: the first file is cut off and the download stopped.
        let cancel = CancellationToken::new();
        let fake = server(&files, Some(1000));
        let first = store.install(&m, &fake, &cancel, &mut |p| {
            if p.done >= 900 {
                cancel.cancel();
            }
        });
        assert!(first.is_err());
        assert!(store.has_partial("test-model"), "kept for a resume");
        assert!(store.installed("test-model").is_none());
        // Resume: continues with a range request.
        let fake = server(&files, None);
        let installed = store
            .install(&m, &fake, &CancellationToken::new(), &mut |_| {})
            .unwrap();
        assert!(fake.range_requests.load(Ordering::Relaxed) >= 1);
        assert!(!store.update_available(&installed, &m));
        // A new version in the catalog is an update, and updating swaps the files in.
        let newer_files: [(&str, &[u8]); 2] = [("a.onnx", &[8; 4000]), ("b.bin", &[9; 3000])];
        let newer = manifest(&newer_files);
        assert!(store.update_available(&installed, &newer));
        let updated = store
            .update(
                &newer,
                &server(&newer_files, None),
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(
            std::fs::read(updated.dir.join("a.onnx")).unwrap(),
            vec![8; 4000]
        );
        assert!(!store.update_available(&updated, &newer));
        // Cancel: an unfinished download is dropped.
        let other = ModelManifest {
            id: "other".into(),
            ..manifest(&files)
        };
        let cancel = CancellationToken::new();
        let _ = store.install(&other, &server(&files, Some(500)), &cancel, &mut |_| {
            cancel.cancel()
        });
        assert!(store.has_partial("other"));
        store.discard_partial("other").unwrap();
        assert!(!store.has_partial("other"));
    }

    #[test]
    fn a_model_installs_verified_and_atomically() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let files: [(&str, &[u8]); 3] = [
            ("a.onnx", &[1; 5000]),
            ("tokens.txt", b"hello 0"),
            ("voices/one.bin", b"style"),
        ];
        let m = manifest(&files);
        let mut last = Progress { done: 0, total: 0 };
        let installed = store
            .install(
                &m,
                &server(&files, None),
                &CancellationToken::new(),
                &mut |p| last = p,
            )
            .unwrap();
        assert_eq!(installed.bytes, 5012);
        assert_eq!(
            last,
            Progress {
                done: 5012,
                total: 5012
            }
        );
        assert!(
            installed.dir.join("voices/one.bin").is_file(),
            "files in subfolders"
        );
        assert!(!tmp.path().join("test-model.partial").exists());
        assert_eq!(store.list().len(), 1);
        store.remove("test-model").unwrap();
        assert!(store.installed("test-model").is_none());
    }

    /// A `.tar.bz2` with the given members.
    fn tar_bz2(members: &[(&str, &[u8])]) -> Vec<u8> {
        let mut tar = tar::Builder::new(Vec::new());
        for (path, data) in members {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, path, *data).unwrap();
        }
        let raw = tar.into_inner().unwrap();
        let mut bz = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        bz.write_all(&raw).unwrap();
        bz.finish().unwrap()
    }

    #[test]
    fn archives_are_verified_and_only_the_listed_members_kept() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let archive = tar_bz2(&[
            ("pkg/encoder-long-name.onnx", &[7; 3000]),
            ("pkg/tokens.txt", b"<blk> 0"),
            ("pkg/test_wavs/0.wav", &[0; 900]),
        ]);
        let files: [(&str, &[u8]); 1] = [("pkg.tar.bz2", &archive)];
        let mut m = manifest(&files);
        m.files[0].unpack = vec![
            Unpack {
                from: "pkg/encoder-long-name.onnx".into(),
                to: "encoder.onnx".into(),
            },
            Unpack {
                from: "pkg/tokens.txt".into(),
                to: "tokens.txt".into(),
            },
        ];
        let installed = store
            .install(
                &m,
                &server(&files, None),
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(
            fs::read(installed.dir.join("encoder.onnx")).unwrap(),
            [7; 3000]
        );
        assert!(installed.dir.join("tokens.txt").is_file());
        assert!(
            !installed.dir.join("pkg.tar.bz2").exists(),
            "the archive is deleted"
        );
        assert!(
            !installed.dir.join("test_wavs").exists(),
            "unlisted members are left out"
        );

        // A member the archive doesn't have is a damaged download, not a half-installed model.
        m.id = "other".into();
        m.files[0].unpack.push(Unpack {
            from: "pkg/missing.onnx".into(),
            to: "missing.onnx".into(),
        });
        let err = store
            .install(
                &m,
                &server(&files, None),
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(err, ModelError::Corrupt { .. }));
        assert!(store.installed("other").is_none());
    }

    #[test]
    fn an_interrupted_download_resumes_with_a_range_request() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let data: Vec<u8> = (0..10_000u32).map(|i| (i % 251) as u8).collect();
        let files: [(&str, &[u8]); 1] = [("big.onnx", &data)];
        let m = manifest(&files);
        // The connection drops after 4000 bytes: the file is short, so its hash fails once and it
        // restarts; a cut file left on disk (e.g. after a crash) resumes with a range instead.
        let part = tmp.path().join("test-model.partial").join("big.onnx.part");
        fs::create_dir_all(part.parent().unwrap()).unwrap();
        fs::write(&part, &data[..4000]).unwrap();
        let srv = server(&files, None);
        store
            .install(&m, &srv, &CancellationToken::new(), &mut |_| {})
            .unwrap();
        assert_eq!(srv.range_requests.load(Ordering::Relaxed), 1);
        assert_eq!(
            fs::read(store.dir("test-model").join("big.onnx")).unwrap(),
            data
        );
    }

    #[test]
    fn a_damaged_file_is_retried_then_reported() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let files: [(&str, &[u8]); 1] = [("a.onnx", &[7; 100])];
        let mut m = manifest(&files);
        // One cut response: retried and installed.
        store
            .install(
                &m,
                &server(&files, Some(10)),
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap();
        // A wrong hash in the manifest can never verify.
        store.remove("test-model").unwrap();
        m.files[0].sha256 = "00".repeat(32);
        let err = store
            .install(
                &m,
                &server(&files, None),
                &CancellationToken::new(),
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(err, ModelError::Corrupt { .. }), "{err:?}");
        assert!(
            store.installed("test-model").is_none(),
            "nothing half-installed"
        );
    }

    #[test]
    fn cancelling_stops_the_download_and_keeps_the_part() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ModelStore::new(tmp.path().to_path_buf());
        let files: [(&str, &[u8]); 1] = [("a.onnx", &[7; 100])];
        let cancel = CancellationToken::new();
        cancel.cancel();
        let err = store
            .install(
                &manifest(&files),
                &server(&files, None),
                &cancel,
                &mut |_| {},
            )
            .unwrap_err();
        assert!(matches!(err, ModelError::Cancelled));
    }

    #[test]
    fn the_catalog_has_complete_manifests() {
        for m in catalog() {
            assert!(!m.files.is_empty() && !m.license.is_empty() && !m.attribution.is_empty());
            for f in &m.files {
                assert_eq!(f.sha256.len(), 64, "{}", f.name);
                assert!(f.url.starts_with("https://") && f.size > 0);
            }
        }
    }

    #[test]
    fn speech_recognition_brings_the_voice_activity_model() {
        let all = catalog();
        for m in &all {
            for dep in &m.requires {
                assert!(all.iter().any(|d| &d.id == dep), "{} needs {dep}", m.id);
            }
            if m.kind == ModelKind::Stt {
                assert!(m.requires.iter().any(|d| d == SILERO_VAD), "{}", m.id);
            }
        }
    }
}
