//! The sentences the player reads: keys of `crates/f4x-lang/content/text/en/setup.ron` (docs/text.md).

use std::path::Path;

use f4x_lang::text::{load_texts, Args, Texts};

/// The language of the setup. The client's language setting will pick it when there is more than one.
const LANGUAGE: &str = "en";

/// The setup's messages.
pub struct Say {
    texts: Texts,
}

impl Say {
    /// Load the messages from the game's own files in `game` (the folder that has `crates/`).
    pub fn load(game: &Path) -> Result<Say, String> {
        let content = game.join(f4x_lang::LANG_CONTENT);
        let texts = load_texts(&[content.as_path()], LANGUAGE)
            .map_err(|e| format!("cannot read the messages in {}: {e}", content.display()))?;
        Ok(Say { texts })
    }

    /// A message with its values.
    pub fn get(&self, key: &str, args: &Args) -> String {
        self.texts.get(key, args)
    }

    /// A message without values.
    pub fn plain(&self, key: &str) -> String {
        self.texts.get(key, &Args::new())
    }

    /// Names as "a, b and c", cut after a few with the count of the rest.
    pub fn names(&self, names: &[String]) -> String {
        const SHOWN: usize = 8;
        let shown: Vec<String> = names.iter().take(SHOWN).cloned().collect();
        let joined = self.texts.join(&shown);
        if names.len() > SHOWN {
            self.get(
                "setup.names_more",
                &Args::new()
                    .text("names", joined)
                    .count("n", names.len() - SHOWN),
            )
        } else {
            joined
        }
    }
}

#[cfg(test)]
pub(crate) fn test_say() -> Say {
    let game = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    Say::load(&game).expect("the shipped messages load")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_joined_and_cut_with_a_count() {
        let say = test_say();
        let few: Vec<String> = ["A", "B", "C"].map(String::from).to_vec();
        assert_eq!(say.names(&few), "A, B and C");
        let many: Vec<String> = (0..11).map(|i| format!("F{i}")).collect();
        let text = say.names(&many);
        assert!(text.ends_with("and 3 more"), "{text}");
        assert!(text.contains("F7") && !text.contains("F8"), "{text}");
    }
}
