//! The router's semantic stage, backed by the local embedding model (BRAIN-03). It runs only
//! when the user has downloaded the model (nothing downloads unasked); loading embeds the
//! command exemplars once, off the async threads.

use kivo_voice::embed::MiniLm;
use std::path::Path;
use std::sync::Arc;

/// The embedding model as the router sees it.
pub struct LocalEmbedder(pub Arc<MiniLm>);

impl kivo_intent::Embedder for LocalEmbedder {
    fn embed(&self, text: &str) -> Option<Vec<f32>> {
        self.0.embed(text).ok()
    }
}

/// Loads the model from `dir` and embeds the exemplars for `language`.
pub fn load(dir: &Path, language: &str) -> Option<kivo_intent::Semantic> {
    let model = match MiniLm::load(dir) {
        Ok(m) => Arc::new(m),
        Err(e) => {
            tracing::warn!(
                detail = e.detail(),
                "the command-understanding model didn't load"
            );
            return None;
        }
    };
    kivo_intent::Semantic::new(language, Box::new(LocalEmbedder(model)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kivo_intent::{Context, Grammar, Index, IndexEntry};

    /// The real model on real paraphrases (where a test machine has it, `KIVO_MINILM_DIR`):
    /// paraphrases resolve, other requests and two-action requests don't.
    #[test]
    fn paraphrases_are_understood_and_other_requests_go_to_a_brain() {
        let Some(dir) = std::env::var_os("KIVO_MINILM_DIR") else {
            eprintln!("KIVO_MINILM_DIR isn't set; skipping");
            return;
        };
        let semantic = load(Path::new(&dir), "en").expect("the model loads");
        let grammar = Grammar::bundled("en").unwrap();
        let apps = Index::new(vec![
            IndexEntry {
                id: "Chrome".into(),
                name: "Google Chrome".into(),
                aliases: vec!["Chrome".into()],
            },
            IndexEntry {
                id: "Spotify".into(),
                name: "Spotify".into(),
                aliases: Vec::new(),
            },
        ]);
        let windows = Index::default();
        let cx = Context {
            apps: &apps,
            windows: &windows,
        };
        let tool = |text: &str| {
            eprintln!("{text}: {:?}", semantic.nearest(text, &cx, 2));
            semantic.match_text(text, &grammar, &cx).map(|m| m.tool)
        };
        // Paraphrases that aren't in the exemplars.
        for (said, want) in [
            ("cut the sound please", "audio.mute"),
            ("i need spotify open", "apps.launch"),
            ("quit chrome i'm finished with it", "apps.close"),
            ("skip past this one", "media.next"),
            ("what's this track called", "media.now_playing"),
            ("grab a screenshot for me", "screen.screenshot"),
            ("stop the video for a second", "media.play_pause"),
            ("i want to use spotify now", "apps.launch"),
            ("make the sound louder", "audio.volume_up"),
            ("lock my computer i am leaving", "system.lock"),
            ("can you snap my screen", "screen.screenshot"),
        ] {
            assert_eq!(tool(said).as_deref(), Some(want), "{said}");
        }
        for said in [
            "what's the weather in paris",
            "write me a poem about the sea",
            "open chrome and search for cats",
            "how do i fix a borrow checker error",
            "shut everything down now",
            "tell me about the history of music",
            "remind me to call mom at five",
            "make it quieter it's way too loud",
        ] {
            assert_eq!(tool(said), None, "{said}");
        }
    }
}
