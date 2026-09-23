//! `e2e`: the plan §102 journeys end to end (BENCH-08): "Mute." (simple), "Open Chrome and search
//! for cute cats." (medium) and a coding request (complex), spoken by Windows' voice into a
//! scripted microphone, through the real turn engine, speech worker and models. Apps and system
//! controls are fakes, so nothing on this PC changes.
//!
//! The journeys run in `kivo-e2e` (built with the runtime; it links `ort`, which can't share a
//! process with this harness's sherpa-onnx). Each run asks it for one pass and reports the T
//! spans (plan §97) relative to the end of speech, the measure users feel.

use crate::harness::{Sample, Suite};
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

/// T spans reported for each journey, as "end of speech → …".
const SPANS: [(&str, &str); 5] = [
    ("t5FinalTranscript", "final transcript"),
    ("t6Intent", "intent"),
    ("t8ToolDone", "action done"),
    ("t9FirstAudio", "first audio"),
    ("t10Complete", "turn complete"),
];

pub struct E2e {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    outcomes: Vec<String>,
}

impl E2e {
    pub fn start() -> Result<Self, String> {
        let program = std::env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(|d| d.join("kivo-e2e.exe")))
            .filter(|p| p.is_file())
            .ok_or(
                "kivo-e2e.exe isn't beside kivo-bench (cargo build -p kivo-runtime -p kivo-infer)",
            )?;
        let mut child = Command::new(program)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("couldn't start kivo-e2e: {e}"))?;
        let input = child.stdin.take().ok_or("no stdin")?;
        let output = BufReader::new(child.stdout.take().ok_or("no stdout")?);
        Ok(Self {
            child,
            input,
            output,
            outcomes: Vec::new(),
        })
    }
}

/// Samples from one pass's JSON: per journey, each span after the end of speech.
pub fn samples(pass: &Value) -> Result<(Vec<Sample>, Vec<String>), String> {
    if let Some(error) = pass["error"].as_str() {
        return Err(error.to_owned());
    }
    let journeys = pass["journeys"]
        .as_array()
        .ok_or("no journeys in the output")?;
    let mut out = Vec::new();
    let mut outcomes = Vec::new();
    for j in journeys {
        let name = j["name"].as_str().unwrap_or("?");
        let outcome = j["outcome"].as_str().unwrap_or("?");
        outcomes.push(format!("{name}: {outcome}"));
        let spans = &j["spans"];
        let Some(end) = spans["t4EndOfSpeech"].as_f64() else {
            continue;
        };
        for (key, label) in SPANS {
            if let Some(at) = spans[key].as_f64() {
                out.push(Sample::cost(
                    format!("{name}: end of speech → {label}"),
                    "ms",
                    (at - end).max(0.0),
                ));
            }
        }
    }
    Ok((out, outcomes))
}

impl Suite for E2e {
    fn name(&self) -> &'static str {
        "e2e"
    }

    fn run(&mut self) -> Result<Vec<Sample>, String> {
        writeln!(self.input, "run").map_err(|e| e.to_string())?;
        self.input.flush().map_err(|e| e.to_string())?;
        let mut line = String::new();
        self.output
            .read_line(&mut line)
            .map_err(|e| e.to_string())?;
        let pass: Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("kivo-e2e said {line:?}: {e}"))?;
        let (samples, outcomes) = samples(&pass)?;
        self.outcomes = outcomes;
        Ok(samples)
    }

    fn notes(&self) -> Vec<String> {
        let mut notes = vec![
            "journeys: plan §102 simple / medium / complex, spoken by the Windows voice into a scripted microphone".into(),
            "fakes for apps, windows and system controls; real speech worker, Moonshine Base, Silero VAD, grammar, permission engine".into(),
            "spans measured from the end of speech (t4); replies are cues only (no speech)".into(),
        ];
        notes.push(format!("outcomes (last run): {}", self.outcomes.join(", ")));
        notes
    }
}

impl Drop for E2e {
    fn drop(&mut self) {
        let _ = writeln!(self.input, "quit");
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn spans_become_samples_from_the_end_of_speech() {
        let pass = json!({ "journeys": [
            { "name": "simple", "outcome": "done", "spans": {
                "t4EndOfSpeech": 1300, "t5FinalTranscript": 1350, "t6Intent": 1351,
                "t8ToolDone": 1352, "t10Complete": 1360 } },
            { "name": "medium", "outcome": "unhandled", "spans": {
                "t4EndOfSpeech": 3000, "t5FinalTranscript": 3100, "t6Intent": 3100 } },
            { "name": "complex", "outcome": "timeout" }
        ]});
        let (measured, outcomes) = samples(&pass).unwrap();
        let get = |m: &str| measured.iter().find(|s| s.metric == m).map(|s| s.value);
        assert_eq!(get("simple: end of speech → action done"), Some(52.0));
        assert_eq!(get("medium: end of speech → intent"), Some(100.0));
        assert_eq!(get("medium: end of speech → action done"), None);
        assert_eq!(
            outcomes,
            ["simple: done", "medium: unhandled", "complex: timeout"]
        );
        assert!(samples(&json!({ "error": "no model" })).is_err());
    }
}
