//! Accented English for the `stt` suite (BENCH-02): EdAcc, the University of Edinburgh's
//! International Accents of English Corpus (CC BY-SA 4.0) — conversations between speakers of
//! many first and second-language accents. `pnpm bench:data` fetches one test shard at a pinned
//! revision; the suite takes the same utterances from it on every machine, spread across the
//! accents in it.

use super::data::{self, Utterance};
use parquet::file::reader::{FileReader, SerializedFileReader};
use parquet::record::Field;
use std::collections::BTreeMap;

const SHARD: &str = "test-00004-of-00010-806407c9bc68112a.parquet";
/// Turns this long are what a spoken request to KIVO looks like; longer ones are conversation.
const SHORTEST: f64 = 2.0;
const LONGEST: f64 = 20.0;

struct Row {
    accent: String,
    text: String,
    wav: Vec<u8>,
}

/// The reference text without EdAcc's annotations (`<laugh>`, `<overlap>`, …).
fn clean(text: &str) -> String {
    text.split_whitespace()
        .filter(|w| !(w.starts_with('<') || w.ends_with('>')))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 16-bit PCM WAV (mono, any rate) to 16 kHz samples.
fn decode(wav: &[u8]) -> Result<Vec<f32>, String> {
    let field = |at: usize, len: usize| wav.get(at..at + len).ok_or("a short WAV");
    if field(0, 4)? != b"RIFF" || field(8, 4)? != b"WAVE" {
        return Err("not a WAV".into());
    }
    let (mut at, mut rate, mut pcm) = (12, 0, None);
    while at + 8 <= wav.len() {
        let id = field(at, 4)?;
        let size = u32::from_le_bytes(field(at + 4, 4)?.try_into().map_err(|_| "size")?) as usize;
        let body = field(at + 8, size.min(wav.len() - at - 8))?;
        match id {
            b"fmt " => {
                let channels = u16::from_le_bytes([body[2], body[3]]);
                let bits = u16::from_le_bytes([body[14], body[15]]);
                if channels != 1 || bits != 16 {
                    return Err(format!("expected 16-bit mono, got {bits}-bit × {channels}"));
                }
                rate = u32::from_le_bytes(body[4..8].try_into().map_err(|_| "rate")?);
            }
            b"data" => pcm = Some(body),
            _ => {}
        }
        at += 8 + size + size % 2;
    }
    let pcm = pcm.ok_or("no audio in the WAV")?;
    if rate == 0 {
        return Err("no format in the WAV".into());
    }
    let samples: Vec<f32> = pcm
        .chunks_exact(2)
        .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
        .collect();
    Ok(kivo_audio::resample::RateConverter::convert_all(
        rate, 16_000, &samples,
    ))
}

fn read_rows() -> Result<Vec<Row>, String> {
    let path = data::root()?.join("data").join("edacc").join(SHARD);
    let file = std::fs::File::open(&path).map_err(|e| {
        format!(
            "EdAcc is missing ({e}): run `pnpm bench:data` ({})",
            path.display()
        )
    })?;
    let reader = SerializedFileReader::new(file).map_err(|e| format!("EdAcc: {e}"))?;
    let mut rows = Vec::new();
    for row in reader
        .get_row_iter(None)
        .map_err(|e| format!("EdAcc: {e}"))?
    {
        let row = row.map_err(|e| format!("EdAcc: {e}"))?;
        let (mut accent, mut text, mut wav) = (String::new(), String::new(), Vec::new());
        for (name, field) in row.get_column_iter() {
            match (name.as_str(), field) {
                ("accent", Field::Str(s)) => accent.clone_from(s),
                ("text", Field::Str(s)) => text = clean(s),
                ("audio", Field::Group(audio)) => {
                    for (part, value) in audio.get_column_iter() {
                        if let ("bytes", Field::Bytes(b)) = (part.as_str(), value) {
                            wav = b.data().to_vec();
                        }
                    }
                }
                _ => {}
            }
        }
        if !text.is_empty() && !wav.is_empty() {
            rows.push(Row { accent, text, wav });
        }
    }
    Ok(rows)
}

/// Up to `count` utterances of 2–20 s, taken in turn from each accent (alphabetical), in the
/// shard's order within an accent. Also returns how many accents they cover.
pub fn utterances(count: usize) -> Result<(Vec<Utterance>, usize), String> {
    let mut by_accent: BTreeMap<String, Vec<Row>> = BTreeMap::new();
    for row in read_rows()? {
        by_accent.entry(row.accent.clone()).or_default().push(row);
    }
    let mut queues: Vec<std::vec::IntoIter<Row>> =
        by_accent.into_values().map(Vec::into_iter).collect();
    let mut picked = Vec::new();
    let mut accents = vec![false; queues.len()];
    while picked.len() < count && !queues.iter().all(|q| q.len() == 0) {
        for (i, queue) in queues.iter_mut().enumerate() {
            if picked.len() >= count {
                break;
            }
            for row in queue.by_ref() {
                let samples = decode(&row.wav)?;
                let utterance = Utterance {
                    samples,
                    text: row.text,
                };
                if (SHORTEST..=LONGEST).contains(&utterance.seconds()) {
                    picked.push(utterance);
                    accents[i] = true;
                    break;
                }
            }
        }
    }
    if picked.is_empty() {
        return Err("EdAcc: no utterances of 2–20 s in the shard".into());
    }
    Ok((picked, accents.iter().filter(|a| **a).count()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotations_leave_the_reference() {
        assert_eq!(clean("so <overlap> I went <laugh> home"), "so I went home");
        assert_eq!(clean("<no-speech>"), "");
    }

    #[test]
    fn a_32_khz_wav_becomes_16_khz() {
        let mut wav = Vec::new();
        let pcm: Vec<u8> = (0..32_000_i16)
            .flat_map(|i| (i % 100).to_le_bytes())
            .collect();
        wav.extend(b"RIFF");
        wav.extend(u32::try_from(36 + pcm.len()).unwrap().to_le_bytes());
        wav.extend(b"WAVEfmt ");
        wav.extend(16_u32.to_le_bytes());
        wav.extend(1_u16.to_le_bytes()); // PCM
        wav.extend(1_u16.to_le_bytes()); // mono
        wav.extend(32_000_u32.to_le_bytes());
        wav.extend(64_000_u32.to_le_bytes());
        wav.extend(2_u16.to_le_bytes());
        wav.extend(16_u16.to_le_bytes());
        wav.extend(b"data");
        wav.extend(u32::try_from(pcm.len()).unwrap().to_le_bytes());
        wav.extend(pcm);
        let samples = decode(&wav).unwrap();
        assert!(
            (15_500..=16_500).contains(&samples.len()),
            "{}",
            samples.len()
        );
    }
}
