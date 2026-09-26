//! Silero VAD v6 (MIT) on ONNX Runtime (VOICE §3). It runs in the runtime, on the detection
//! thread, on 32 ms frames of 16 kHz audio. The model is bundled with KIVO (DIST-02).

use crate::engine::{Accel, EngineInfo, EngineKind, EngineSlot, ResourceEstimate};
use crate::error::VoiceResult;
use crate::traits::VadEngine;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;

/// Samples per frame at 16 kHz.
const FRAME: usize = 512;
/// The model also sees the end of the previous frame (Silero v5+).
const CONTEXT: usize = 64;
const STATE: usize = 2 * 128;

pub struct SileroVad {
    info: EngineInfo,
    session: Session,
    state: Vec<f32>,
    context: [f32; CONTEXT],
    input: Vec<f32>,
}

pub fn info() -> EngineInfo {
    EngineInfo {
        id: "silero-vad-v6".into(),
        name: "Silero VAD".into(),
        slot: EngineSlot::Vad,
        kind: EngineKind::Local,
        license: "MIT".into(),
        languages: vec!["*".into()],
        streaming: true,
        accel: vec![Accel::Cpu],
        resources: ResourceEstimate {
            ram_mb: 10,
            vram_mb: 0,
            disk_mb: 0,
        },
        model: None,
    }
}

impl SileroVad {
    pub fn load(model: &Path) -> VoiceResult<Self> {
        // One thread: VAD must stay cheap while KIVO listens (VOICE §10: ≤ 2% CPU).
        let session = crate::onnx::session(model, 1)?;
        Ok(Self {
            info: info(),
            session,
            state: vec![0.0; STATE],
            context: [0.0; CONTEXT],
            input: Vec::with_capacity(CONTEXT + FRAME),
        })
    }
}

impl VadEngine for SileroVad {
    fn info(&self) -> &EngineInfo {
        &self.info
    }

    fn frame_len(&self) -> usize {
        FRAME
    }

    fn process(&mut self, frame: &[f32]) -> VoiceResult<f32> {
        debug_assert_eq!(frame.len(), FRAME);
        self.input.clear();
        self.input.extend_from_slice(&self.context);
        self.input.extend_from_slice(frame);
        let input = Tensor::from_array(([1usize, CONTEXT + FRAME], self.input.clone()))?;
        let state = Tensor::from_array(([2usize, 1, 128], self.state.clone()))?;
        let sr = Tensor::from_array(((), vec![16_000_i64]))?;
        let outputs = self
            .session
            .run(ort::inputs!["input" => input, "state" => state, "sr" => sr])?;
        let (_, prob) = outputs["output"].try_extract_tensor::<f32>()?;
        let probability = prob.first().copied().unwrap_or(0.0);
        let (_, next) = outputs["stateN"].try_extract_tensor::<f32>()?;
        self.state.copy_from_slice(&next[..STATE]);
        self.context.copy_from_slice(&frame[FRAME - CONTEXT..]);
        Ok(probability)
    }

    fn reset(&mut self) {
        self.state.fill(0.0);
        self.context = [0.0; CONTEXT];
    }
}
