//! Persistence for the Gemini chat's configurable bits: the system prompt
//! (editable and resettable in the "KI-Chat" settings page) and the model
//! id. The API key itself is a secret and goes through `secrets.rs`
//! instead - this only ever holds plain-text, non-sensitive config.

use std::path::PathBuf;

use gtk4::glib;

use crate::default_prompt::DEFAULT_SYSTEM_PROMPT;

pub const DEFAULT_MODEL: &str = "gemini-2.5-flash";

fn config_dir() -> PathBuf {
    let mut dir = glib::user_config_dir();
    dir.push("blocksmith");
    dir
}

fn system_prompt_path() -> PathBuf {
    let mut path = config_dir();
    path.push("chat_system_prompt.txt");
    path
}

fn model_path() -> PathBuf {
    let mut path = config_dir();
    path.push("chat_model.txt");
    path
}

/// The active system prompt: the user's saved custom one if they've set
/// one, otherwise [`DEFAULT_SYSTEM_PROMPT`].
pub fn load_system_prompt() -> String {
    std::fs::read_to_string(system_prompt_path()).unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string())
}

pub fn save_system_prompt(text: &str) -> std::io::Result<()> {
    let path = system_prompt_path();
    std::fs::create_dir_all(config_dir())?;
    std::fs::write(path, text)
}

/// Reverts to [`DEFAULT_SYSTEM_PROMPT`] by removing the saved custom prompt
/// file, if any.
pub fn reset_system_prompt() -> std::io::Result<()> {
    match std::fs::remove_file(system_prompt_path()) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err),
    }
}

pub fn is_system_prompt_customized() -> bool {
    system_prompt_path().exists()
}

pub fn load_model() -> String {
    std::fs::read_to_string(model_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_MODEL.to_string())
}

pub fn save_model(model: &str) -> std::io::Result<()> {
    let path = model_path();
    std::fs::create_dir_all(config_dir())?;
    std::fs::write(path, model.trim())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_prompt_is_non_empty_and_mentions_key_topics() {
        assert!(DEFAULT_SYSTEM_PROMPT.contains("Open Source"));
        assert!(DEFAULT_SYSTEM_PROMPT.contains("Linux"));
    }

    /// Touches the real config dir (there's no pure logic to isolate here -
    /// it's just read-or-default / write-raw-text), so this captures and
    /// restores whatever was already saved, to avoid leaving the test
    /// machine's actual Blocksmith config changed.
    #[test]
    fn system_prompt_save_load_reset_round_trips() {
        let was_customized = is_system_prompt_customized();
        let original = load_system_prompt();

        save_system_prompt("Custom test prompt").expect("save failed");
        assert_eq!(load_system_prompt(), "Custom test prompt");
        assert!(is_system_prompt_customized());

        reset_system_prompt().expect("reset failed");
        assert_eq!(load_system_prompt(), DEFAULT_SYSTEM_PROMPT);
        assert!(!is_system_prompt_customized());

        if was_customized {
            save_system_prompt(&original).expect("restore failed");
        }
    }

    #[test]
    fn model_save_load_round_trips() {
        let original = load_model();

        save_model("gemini-test-model").expect("save failed");
        assert_eq!(load_model(), "gemini-test-model");

        save_model(&original).expect("restore failed");
    }
}
