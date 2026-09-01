//! Blocking Gemini (Google Generative Language API) client for the chat
//! pane. Blocking for the same reason as `wpclient` - see its module docs:
//! this app already committed to `oo7`'s async-std reactor for keyring
//! access, and mixing in a second async runtime just for this one call
//! wasn't worth it, so callers run it on a spawned thread instead.

use std::time::Duration;

use serde_json::Value;

pub struct Client {
    agent: ureq::Agent,
    api_key: String,
    model: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Model,
}

impl Role {
    fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Model => "model",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: Role,
    pub text: String,
}

#[derive(Debug)]
pub struct ApiError {
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for ApiError {}

pub type Result<T> = std::result::Result<T, ApiError>;

impl Client {
    pub fn new(api_key: &str, model: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(60)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
            api_key: api_key.to_string(),
            model: model.to_string(),
        }
    }

    /// Sends the full conversation history (the last element being the new
    /// user message) plus a system prompt, returning the model's reply text.
    pub fn send(&self, system_prompt: &str, history: &[ChatMessage]) -> Result<String> {
        let contents: Vec<Value> = history
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": m.role.as_str(),
                    "parts": [{"text": m.text}],
                })
            })
            .collect();

        let mut body = serde_json::json!({ "contents": contents });
        if !system_prompt.trim().is_empty() {
            body["system_instruction"] = serde_json::json!({ "parts": [{"text": system_prompt}] });
        }

        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", self.model);
        let mut response = self
            .agent
            .post(&url)
            .header("x-goog-api-key", self.api_key.as_str())
            .header("Content-Type", "application/json")
            .send_json(&body)
            .map_err(|err| ApiError { message: err.to_string() })?;

        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        if !(200..300).contains(&status) {
            let message = serde_json::from_str::<Value>(&body_text)
                .ok()
                .and_then(|v| v.get("error").and_then(|e| e.get("message")).and_then(Value::as_str).map(str::to_string))
                .unwrap_or(body_text);
            return Err(ApiError { message: format!("HTTP {status}: {message}") });
        }

        extract_reply_text(&body_text)
    }
}

fn extract_reply_text(body_text: &str) -> Result<String> {
    let value: Value = serde_json::from_str(body_text).map_err(|err| ApiError { message: format!("Antwort nicht lesbar: {err}") })?;
    value
        .get("candidates")
        .and_then(|c| c.get(0))
        .and_then(|c| c.get("content"))
        .and_then(|c| c.get("parts"))
        .and_then(|p| p.get(0))
        .and_then(|p| p.get("text"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| ApiError {
            message: "Keine Antwort erhalten (möglicherweise durch einen Sicherheitsfilter blockiert).".to_string(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_reply_text_reads_the_first_candidates_text() {
        let body = r#"{
            "candidates": [
                {"content": {"parts": [{"text": "Hallo!"}], "role": "model"}, "finishReason": "STOP"}
            ]
        }"#;
        assert_eq!(extract_reply_text(body).unwrap(), "Hallo!");
    }

    #[test]
    fn extract_reply_text_errors_on_missing_candidates() {
        let body = r#"{"candidates": []}"#;
        assert!(extract_reply_text(body).is_err());
    }

    #[test]
    fn extract_reply_text_errors_on_unparseable_body() {
        assert!(extract_reply_text("not json").is_err());
    }

    #[test]
    fn role_as_str_matches_gemini_api_values() {
        assert_eq!(Role::User.as_str(), "user");
        assert_eq!(Role::Model.as_str(), "model");
    }
}
