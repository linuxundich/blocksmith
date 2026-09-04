//! Blocking REST clients for the chat pane's four supported providers -
//! Gemini, ChatGPT (OpenAI), Claude (Anthropic), and Ollama (self-hosted,
//! no API key). Blocking for the same reason as `wpclient` - see its
//! module docs: this app already committed to `oo7`'s async-std reactor
//! for keyring access, so a blocking client run on a spawned thread is
//! simpler than reconciling two async runtimes for one occasional call.

use std::time::Duration;

use base64::Engine;
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

    /// Sends a single image plus an instruction prompt and returns the
    /// model's text reply - a one-shot "describe this image" call (used for
    /// AI-generated alt text, see `aialt.rs`), unlike `send`'s multi-turn
    /// conversation history. `image_bytes` goes over the wire as base64,
    /// inlined directly in the request body - all four providers support
    /// this for a single image without needing a separate upload step.
    pub fn describe_image(&self, prompt: &str, image_bytes: &[u8], mime_type: &str) -> Result<String> {
        let data = base64::engine::general_purpose::STANDARD.encode(image_bytes);
        match self.provider {
            Provider::Gemini => self.describe_image_gemini(prompt, mime_type, &data),
            Provider::OpenAi => self.describe_image_openai(prompt, mime_type, &data),
            Provider::Claude => self.describe_image_claude(prompt, mime_type, &data),
            Provider::Ollama => self.describe_image_ollama(prompt, &data),
        }
    }

    fn describe_image_gemini(&self, prompt: &str, mime_type: &str, data: &str) -> Result<String> {
        let body = gemini_image_body(prompt, mime_type, data);
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

    fn describe_image_openai(&self, prompt: &str, mime_type: &str, data: &str) -> Result<String> {
        let body = openai_image_body(&self.model, prompt, mime_type, data);
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

    fn describe_image_claude(&self, prompt: &str, mime_type: &str, data: &str) -> Result<String> {
        let body = claude_image_body(&self.model, prompt, mime_type, data);
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

    fn describe_image_ollama(&self, prompt: &str, data: &str) -> Result<String> {
        let body = ollama_image_body(&self.model, prompt, data);
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

    fn get(&self, url: &str, headers: &[(&str, &str)]) -> Result<(u16, String)> {
        let mut request = self.agent.get(url);
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let mut response = request.call().map_err(|err| ApiError { message: err.to_string() })?;
        let status = response.status().as_u16();
        let body_text = response.body_mut().read_to_string().unwrap_or_default();
        Ok((status, body_text))
    }

    /// Lists the models available to this account/instance. Doubles as an
    /// API-key check: a successful call (200, non-empty list) means the key
    /// is valid, which is why the settings UI uses this same call both to
    /// populate the model picker and to report "key OK" to the user.
    pub fn list_models(&self) -> Result<Vec<String>> {
        match self.provider {
            Provider::Gemini => self.list_models_gemini(),
            Provider::OpenAi => self.list_models_openai(),
            Provider::Claude => self.list_models_claude(),
            Provider::Ollama => self.list_models_ollama(),
        }
    }

    fn list_models_gemini(&self) -> Result<Vec<String>> {
        let url = "https://generativelanguage.googleapis.com/v1beta/models";
        let (status, body_text) = self.get(url, &[("x-goog-api-key", &self.api_key)])?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error", "message"]));
        }
        Ok(extract_gemini_models(&parse_json(&body_text)?))
    }

    fn list_models_openai(&self) -> Result<Vec<String>> {
        let auth = format!("Bearer {}", self.api_key);
        let (status, body_text) = self.get("https://api.openai.com/v1/models", &[("Authorization", &auth)])?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error", "message"]));
        }
        Ok(extract_openai_models(&parse_json(&body_text)?))
    }

    fn list_models_claude(&self) -> Result<Vec<String>> {
        let (status, body_text) = self.get(
            "https://api.anthropic.com/v1/models",
            &[("x-api-key", &self.api_key), ("anthropic-version", "2023-06-01")],
        )?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error", "message"]));
        }
        Ok(extract_claude_models(&parse_json(&body_text)?))
    }

    fn list_models_ollama(&self) -> Result<Vec<String>> {
        let url = format!("{}/api/tags", self.base_url);
        let (status, body_text) = self.get(&url, &[])?;
        if !(200..300).contains(&status) {
            return Err(error_from_body(status, &body_text, &["error"]));
        }
        Ok(extract_ollama_models(&parse_json(&body_text)?))
    }
}

/// Request-body builders for `describe_image_*` - kept as pure functions
/// (not inlined) so their JSON shape can be unit-tested the same way
/// `extract_*_models` is, without needing a live network call.
fn gemini_image_body(prompt: &str, mime_type: &str, data: &str) -> Value {
    serde_json::json!({
        "contents": [{
            "role": "user",
            "parts": [
                {"text": prompt},
                {"inline_data": {"mime_type": mime_type, "data": data}}
            ]
        }]
    })
}

fn openai_image_body(model: &str, prompt: &str, mime_type: &str, data: &str) -> Value {
    serde_json::json!({
        "model": model,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": prompt},
                {"type": "image_url", "image_url": {"url": format!("data:{mime_type};base64,{data}")}}
            ]
        }]
    })
}

fn claude_image_body(model: &str, prompt: &str, mime_type: &str, data: &str) -> Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 1024,
        "messages": [{
            "role": "user",
            "content": [
                {"type": "image", "source": {"type": "base64", "media_type": mime_type, "data": data}},
                {"type": "text", "text": prompt}
            ]
        }]
    })
}

/// Ollama's `/api/chat` takes images as a plain array of base64 strings
/// alongside the message - no data-URL prefix and no per-image mime type,
/// unlike the other three providers.
fn ollama_image_body(model: &str, prompt: &str, data: &str) -> Value {
    serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt, "images": [data]}],
        "stream": false
    })
}

fn extract_gemini_models(value: &Value) -> Vec<String> {
    value
        .get("models")
        .and_then(Value::as_array)
        .map(|models| {
            models
                .iter()
                .filter(|m| {
                    m.get("supportedGenerationMethods")
                        .and_then(Value::as_array)
                        .is_some_and(|methods| methods.iter().any(|m| m.as_str() == Some("generateContent")))
                })
                .filter_map(|m| m.get("name").and_then(Value::as_str))
                .map(|name| name.strip_prefix("models/").unwrap_or(name).to_string())
                .collect()
        })
        .unwrap_or_default()
}

/// Excludes obviously non-chat model families (embeddings, audio, image
/// generation, moderation) to keep the picker focused - OpenAI's `/models`
/// endpoint lists everything the account can use, chat or not.
fn extract_openai_models(value: &Value) -> Vec<String> {
    const EXCLUDED_SUBSTRINGS: &[&str] = &["embedding", "whisper", "tts", "dall-e", "moderation"];
    let mut models: Vec<String> = value
        .get("data")
        .and_then(Value::as_array)
        .map(|data| {
            data.iter()
                .filter_map(|m| m.get("id").and_then(Value::as_str))
                .filter(|id| !EXCLUDED_SUBSTRINGS.iter().any(|excluded| id.contains(excluded)))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    models.sort();
    models
}

fn extract_claude_models(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .map(|data| data.iter().filter_map(|m| m.get("id").and_then(Value::as_str)).map(str::to_string).collect())
        .unwrap_or_default()
}

fn extract_ollama_models(value: &Value) -> Vec<String> {
    value
        .get("models")
        .and_then(Value::as_array)
        .map(|models| models.iter().filter_map(|m| m.get("name").and_then(Value::as_str)).map(str::to_string).collect())
        .unwrap_or_default()
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

    #[test]
    fn gemini_models_are_filtered_to_generate_content_and_stripped_of_prefix() {
        let body = r#"{"models":[
            {"name":"models/gemini-2.5-flash","supportedGenerationMethods":["generateContent"]},
            {"name":"models/embedding-001","supportedGenerationMethods":["embedContent"]}
        ]}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(extract_gemini_models(&value), vec!["gemini-2.5-flash".to_string()]);
    }

    #[test]
    fn openai_models_exclude_non_chat_families_and_are_sorted() {
        let body = r#"{"data":[
            {"id":"gpt-4o-mini"},
            {"id":"text-embedding-3-small"},
            {"id":"whisper-1"},
            {"id":"gpt-4o"}
        ]}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(extract_openai_models(&value), vec!["gpt-4o".to_string(), "gpt-4o-mini".to_string()]);
    }

    #[test]
    fn claude_models_are_extracted_in_api_order() {
        let body = r#"{"data":[{"id":"claude-sonnet-5"},{"id":"claude-haiku-4-5"}]}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(extract_claude_models(&value), vec!["claude-sonnet-5".to_string(), "claude-haiku-4-5".to_string()]);
    }

    #[test]
    fn ollama_models_are_extracted_from_tags() {
        let body = r#"{"models":[{"name":"llama3.2:latest"},{"name":"mistral:latest"}]}"#;
        let value: Value = parse_json(body).unwrap();
        assert_eq!(extract_ollama_models(&value), vec!["llama3.2:latest".to_string(), "mistral:latest".to_string()]);
    }

    #[test]
    fn gemini_image_body_inlines_the_image_alongside_the_prompt() {
        let body = gemini_image_body("Beschreibe dieses Bild.", "image/png", "QUJD");
        assert_eq!(body.pointer("/contents/0/parts/0/text").and_then(Value::as_str), Some("Beschreibe dieses Bild."));
        assert_eq!(body.pointer("/contents/0/parts/1/inline_data/mime_type").and_then(Value::as_str), Some("image/png"));
        assert_eq!(body.pointer("/contents/0/parts/1/inline_data/data").and_then(Value::as_str), Some("QUJD"));
    }

    #[test]
    fn openai_image_body_uses_a_data_url() {
        let body = openai_image_body("gpt-4o-mini", "Beschreibe dieses Bild.", "image/png", "QUJD");
        assert_eq!(body.pointer("/messages/0/content/0/text").and_then(Value::as_str), Some("Beschreibe dieses Bild."));
        assert_eq!(
            body.pointer("/messages/0/content/1/image_url/url").and_then(Value::as_str),
            Some("data:image/png;base64,QUJD")
        );
    }

    #[test]
    fn claude_image_body_uses_base64_source() {
        let body = claude_image_body("claude-sonnet-5", "Beschreibe dieses Bild.", "image/png", "QUJD");
        assert_eq!(body.pointer("/messages/0/content/0/source/media_type").and_then(Value::as_str), Some("image/png"));
        assert_eq!(body.pointer("/messages/0/content/0/source/data").and_then(Value::as_str), Some("QUJD"));
        assert_eq!(body.pointer("/messages/0/content/1/text").and_then(Value::as_str), Some("Beschreibe dieses Bild."));
    }

    #[test]
    fn ollama_image_body_carries_images_as_plain_base64_array() {
        let body = ollama_image_body("llava", "Beschreibe dieses Bild.", "QUJD");
        assert_eq!(body.pointer("/messages/0/content").and_then(Value::as_str), Some("Beschreibe dieses Bild."));
        assert_eq!(body.pointer("/messages/0/images/0").and_then(Value::as_str), Some("QUJD"));
    }
}
