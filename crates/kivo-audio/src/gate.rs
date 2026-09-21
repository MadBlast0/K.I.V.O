//! The energy gate in front of the VAD (VOICE §1): a frame reaches Silero only when it is louder
//! than the room's noise floor, so a quiet room costs almost nothing. The floor adapts slowly to
//! the room and the gate stays open briefly after speech (hangover) so word endings aren't lost.

/// dB above the noise floor that opens the gate.
const MARGIN_DB: f32 = 9.0;
/// Never gate out anything louder than this (a loud room still gets VAD).
const ALWAYS_OPEN_DB: f32 = -35.0;
/// Never open for anything quieter than this.
const SILENCE_DB: f32 = -65.0;
/// Frames the gate stays open after the last loud frame (at 32 ms: ~300 ms).
const HANGOVER: u32 = 10;

pub struct EnergyGate {
    floor_db: f32,
    hangover: u32,
}

impl Default for EnergyGate {
    fn default() -> Self {
        Self::new()
    }
}

impl EnergyGate {
    pub fn new() -> Self {
        Self {
            floor_db: -60.0,
            hangover: 0,
        }
    }

    /// True when `frame` may contain speech and should go to the VAD.
    pub fn open(&mut self, frame: &[f32]) -> bool {
        let db = level_db(frame);
        // The floor follows quiet frames quickly and loud ones very slowly.
        let rate = if db < self.floor_db { 0.2 } else { 0.005 };
        self.floor_db += (db - self.floor_db) * rate;
        let loud = db > SILENCE_DB && (db > self.floor_db + MARGIN_DB || db > ALWAYS_OPEN_DB);
        if loud {
            self.hangover = HANGOVER;
            true
        } else if self.hangover > 0 {
            self.hangover -= 1;
            true
        } else {
            false
        }
    }

    pub fn floor_db(&self) -> f32 {
        self.floor_db
    }
}

/// RMS level in dBFS (−100 for silence).
pub fn level_db(frame: &[f32]) -> f32 {
    if frame.is_empty() {
        return -100.0;
    }
    #[allow(clippy::cast_precision_loss, reason = "a frame length")]
    let mean = frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32;
    if mean <= 1e-10 {
        -100.0
    } else {
        10.0 * mean.log10()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(amplitude: f32) -> Vec<f32> {
        // Deterministic pseudo-noise.
        let mut x = 0x1234_5678_u32;
        (0..512)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                #[allow(clippy::cast_precision_loss)]
                let unit = (x as f32 / u32::MAX as f32) * 2.0 - 1.0;
                unit * amplitude
            })
            .collect()
    }

    #[test]
    fn silence_stays_closed_and_speech_opens_it() {
        let mut gate = EnergyGate::new();
        for _ in 0..50 {
            assert!(!gate.open(&noise(0.0005)));
        }
        assert!(gate.open(&noise(0.05)), "speech well above the floor");
    }

    #[test]
    fn the_gate_holds_open_after_speech_then_closes() {
        let mut gate = EnergyGate::new();
        for _ in 0..50 {
            gate.open(&noise(0.0005));
        }
        assert!(gate.open(&noise(0.05)));
        let held = (0..HANGOVER).filter(|_| gate.open(&noise(0.0005))).count();
        assert_eq!(held, HANGOVER as usize);
        assert!(!gate.open(&noise(0.0005)));
    }

    #[test]
    fn a_noisy_room_raises_the_floor() {
        let mut gate = EnergyGate::new();
        // Steady fan noise around −45 dBFS: the floor climbs towards it over a few seconds.
        for _ in 0..300 {
            gate.open(&noise(0.01));
        }
        assert!(gate.floor_db() > -50.0, "{}", gate.floor_db());
        assert!(
            !gate.open(&noise(0.01)),
            "steady noise no longer opens the gate"
        );
        assert_eq!(level_db(&[]), -100.0);
    }
}
