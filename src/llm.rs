//! Blocking REST clients for the chat pane's four supported providers -
//! Gemini, ChatGPT (OpenAI), Claude (Anthropic), and Ollama (self-hosted,
//! no API key). Blocking for the same reason as `wpclient` - see its
//! module docs: this app already committed to `oo7`'s async-std reactor
//! for keyring access, so a blocking client run on a spawned thread is
//! simpler than reconciling two async runtimes for one occasional call.

use std::time::Duration;

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Gemini,
    OpenAi,
    Claude,
    Ollama,
}

impl Provider {
    pub const ALL: [Provider; 4] = [Provider::Gemini, Provider::OpenAi, Provider::Claude, Provider::Ollama];

    /// Stable identifier used in config files and keyring attributes - not
    /// shown to the user (see `label` for that).
    pub fn id(&self) -> &'static str {
        match self {
            Provider::Gemini => "gemini",
            Provider::OpenAi => "openai",
            Provider::Claude => "claude",
            Provider::Ollama => "ollama",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s.trim() {
            "openai" => Provider::OpenAi,
            "claude" => Provider::Claude,
            "ollama" => Provider::Ollama,
            _ => Provider::Gemini,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Provider::Gemini => "Gemini",
            Provider::OpenAi => "ChatGPT",
            Provider::Claude => "Claude",
            Provider::Ollama => "Ollama",
        }
    }

    pub fn default_model(&self) -> &'static str {
        match self {
            Provider::Gemini => "gemini-2.5-flash",
            Provider::OpenAi => "gpt-4o-mini",
            Provider::Claude => "claude-sonnet-5",
            Provider::Ollama => "llama3.2",
        }
    }

    /// Ollama runs locally with no account, so it has no API key to enter.
    pub fn needs_api_key(&self) -> bool {
        !matches!(self, Provider::Ollama)
    }

    /// Only Ollama's endpoint is user-configurable (it's self-hosted, often
    /// not on the default port/host); the others have a fixed cloud API.
    pub fn needs_base_url(&self) -> bool {
        matches!(self, Provider::Ollama)
    }
}

pub const DEFAULT_OLLAMA_BASE_URL: &str = "http://localhost:11434";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    User,
    Model,
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

pub struct Client {
    provider: Provider,
    agent: ureq::Agent,
    api_key: String,
    model: String,
    base_url: String,
}

impl Client {
    pub fn new(provider: Provider, api_key: &str, model: &str, base_url: &str) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(Duration::from_secs(60)))
            .build();
        Self {
            provider,
            agent: ureq::Agent::new_with_config(config),
            api_key: api_key.to_string(),
            model: model.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
        }
    }

    /// Sends the full conversation history (the last element being the new
    /// user message) plus a system prompt, returning the model's reply text.
    pub fn send(&self, system_prompt: &str, history: &[ChatMessage]) -> Result<String> {
        match self.provider {
            Provider::Gemini => self.send_gemini(system_prompt, history),
            Provider::OpenAi => self.send_openai(system_prompt, history),
            Provider::Claude => self.send_claude(system_prompt, history),
            Provider::Ollama => self.send_ollama(system_prompt, history),
        }
    }

    fn send_gemini(&self, system_prompt: &str, history: &[ChatMessage]) -> Result<String> {
        let contents: Vec<Value> = history
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": match m.role { Role::User => "user", Role::Model => "model" },
                    "parts": [{"text": m.text}],
                })
            })
            .collect();
        let mut body = serde_json::json!({ "contents": contents });
        if !system_prompt.trim().is_empty() {
            body["system_instruction"] = serde_json::json!({ "parts": [{"text": system_prompt}] });
        }

        let url = format!("https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent", self.model);
        let (status, body_text) = self.post_json(&url, &[("x-goog-api-key", &self.api_key)], &body)?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error", "message"]));
        }
        let value: Value = parse_json(&body_text)?;
        value
            .pointer("/candidates/0/content/parts/0/text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError {
                message: "Keine Antwort erhalten (möglicherweise durch einen Sicherheitsfilter blockiert).".to_string(),
            })
    }

    fn send_openai(&self, system_prompt: &str, history: &[ChatMessage]) -> Result<String> {
        let mut messages = Vec::new();
        if !system_prompt.trim().is_empty() {
            messages.push(serde_json::json!({ "role": "system", "content": system_prompt }));
        }
        for m in history {
            messages.push(serde_json::json!({
                "role": match m.role { Role::User => "user", Role::Model => "assistant" },
                "content": m.text,
            }));
        }
        let body = serde_json::json!({ "model": self.model, "messages": messages });

        let auth = format!("Bearer {}", self.api_key);
        let (status, body_text) = self.post_json("https://api.openai.com/v1/chat/completions", &[("Authorization", &auth)], &body)?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error", "message"]));
        }
        let value: Value = parse_json(&body_text)?;
        value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError { message: "Keine Antwort erhalten.".to_string() })
    }

    fn send_claude(&self, system_prompt: &str, history: &[ChatMessage]) -> Result<String> {
        let messages: Vec<Value> = history
            .iter()
            .map(|m| {
                serde_json::json!({
                    "role": match m.role { Role::User => "user", Role::Model => "assistant" },
                    "content": m.text,
                })
            })
            .collect();
        let mut body = serde_json::json!({ "model": self.model, "max_tokens": 4096, "messages": messages });
        if !system_prompt.trim().is_empty() {
            body["system"] = serde_json::Value::String(system_prompt.to_string());
        }

        let (status, body_text) = self.post_json(
            "https://api.anthropic.com/v1/messages",
            &[("x-api-key", &self.api_key), ("anthropic-version", "2023-06-01")],
            &body,
        )?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error", "message"]));
        }
        let value: Value = parse_json(&body_text)?;
        value
            .pointer("/content/0/text")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| ApiError { message: "Keine Antwort erhalten.".to_string() })
    }

    fn send_ollama(&self, system_prompt: &str, history: &[ChatMessage]) -> Result<String> {
        let mut messages = Vec::new();
        if !system_prompt.trim().is_empty() {
            messages.push(serde_json::json!({ "role": "system", "content": system_prompt }));
        }
        for m in history {
            messages.push(serde_json::json!({
                "role": match m.role { Role::User => "user", Role::Model => "assistant" },
                "content": m.text,
            }));
        }
        let body = serde_json::json!({ "model": self.model, "messages": messages, "stream": false });

        let url = format!("{}/api/chat", self.base_url);
        let (status, body_text) = self.post_json(&url, &[], &body)?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error"]));
        }
        let value: Value = parse_json(&body_text)?;
        if let Some(text) = value.pointer("/message/content").and_then(Value::as_str) {
            return Ok(text.to_string());
        }
        Err(error_from_body(status, &body_text, &["error"]))
    }

    fn post_json(&self, url: &str, headers: &[(&str, &str)], body: &Value) -> Result<(u16, String)> {
        let mut request = self.agent.post(url).header("Content-Type", "application/json");
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let mut response = request.send_json(body).map_err(|err| ApiError { message: err.to_string() })?;
        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        Ok((status, body_text))
    }
}

fn parse_json(body_text: &str) -> Result<Value> {
    serde_json::from_str(body_text).map_err(|err| ApiError {
        message: format!("Antwort nicht lesbar: {err}"),
    })
}

/// Digs an error message out of a provider's error response body, walking
/// `path` (e.g. `["error", "message"]`) into the parsed JSON; falls back to
/// the raw body text if that shape doesn't match (Ollama in particular
/// sometimes returns a bare string message instead of nested JSON).
fn error_from_body(status: u16, body_text: &str, path: &[&str]) -> ApiError {
    let message = serde_json::from_str::<Value>(body_text)
        .ok()
        .and_then(|v| {
            let mut current = &v;
            for key in path {
                current = current.get(key)?;
            }
            current.as_str().map(str::to_string)
        })
        .unwrap_or_else(|| body_text.to_string());
    ApiError {
        message: format!("HTTP {status}: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_id_round_trips() {
        for provider in Provider::ALL {
            assert_eq!(Provider::from_id(provider.id()), provider);
        }
    }

    #[test]
    fn only_ollama_skips_the_api_key() {
        assert!(!Provider::Ollama.needs_api_key());
        assert!(Provider::Gemini.needs_api_key());
        assert!(Provider::OpenAi.needs_api_key());
        assert!(Provider::Claude.needs_api_key());
    }

    #[test]
    fn only_ollama_has_a_configurable_base_url() {
        assert!(Provider::Ollama.needs_base_url());
        assert!(!Provider::Gemini.needs_base_url());
    }

    #[test]
    fn gemini_success_body_is_parsed() {
        let body = r#"{"candidates":[{"content":{"parts":[{"text":"Hallo!"}]}}]}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(value.pointer("/candidates/0/content/parts/0/text").and_then(Value::as_str), Some("Hallo!"));
    }

    #[test]
    fn openai_success_body_is_parsed() {
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"Hi!"}}]}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(value.pointer("/choices/0/message/content").and_then(Value::as_str), Some("Hi!"));
    }

    #[test]
    fn claude_success_body_is_parsed() {
        let body = r#"{"content":[{"type":"text","text":"Servus!"}],"role":"assistant"}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(value.pointer("/content/0/text").and_then(Value::as_str), Some("Servus!"));
    }

    #[test]
    fn ollama_success_body_is_parsed() {
        let body = r#"{"message":{"role":"assistant","content":"Moin!"},"done":true}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(value.pointer("/message/content").and_then(Value::as_str), Some("Moin!"));
    }

    #[test]
    fn error_from_body_extracts_nested_message() {
        let err = error_from_body(401, r#"{"error":{"message":"invalid key"}}"#, &["error", "message"]);
        assert_eq!(err.message, "HTTP 401: invalid key");
    }

    #[test]
    fn error_from_body_falls_back_to_raw_text_on_mismatch() {
        let err = error_from_body(500, "plain text error", &["error", "message"]);
        assert_eq!(err.message, "HTTP 500: plain text error");
    }
}
