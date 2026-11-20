//! A minimal streaming client for the Claude Messages API, over raw HTTPS
//! (there is no official Rust SDK).

use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::time::Duration;

const BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2026-10-21";
/// Lets the server retry on a fallback model if the request is declined.
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
const OAUTH_BETA: &str = "oauth-2025-04-20";
pub const DEFAULT_MODEL: &str = "claude-opus-5-5";

enum Credential {
    ApiKey(String),
    Token(String),
}

/// A configured connection to Claude.
pub struct Llm {
    credential: Credential,
    model: String,
    url: String,
    agent: ureq::Agent,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub text: String,
}

impl Llm {
    /// Uses `ANTHROPIC_API_KEY`, or else `ANTHROPIC_AUTH_TOKEN`, and
    /// `ANTHROPIC_BASE_URL` if set. The model can be changed with
    /// `POKER_COACH_MODEL`. `None` when no credential is set.
    pub fn from_env() -> Option<Llm> {
        let var = |name: &str| std::env::var(name).ok().filter(|v| !v.trim().is_empty());
        let credential = var("ANTHROPIC_API_KEY")
            .map(Credential::ApiKey)
            .or_else(|| var("ANTHROPIC_AUTH_TOKEN").map(Credential::Token))?;
        let model = var("POKER_COACH_MODEL").unwrap_or_else(|| DEFAULT_MODEL.to_string());
        let base = var("ANTHROPIC_BASE_URL").unwrap_or_else(|| BASE_URL.to_string());
        let url = format!("{}/v1/messages", base.trim_end_matches('/'));
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(90)))
            .http_status_as_error(false)
            .build()
            .into();
        Some(Llm {
            credential,
            model,
            url,
            agent,
        })
    }

    pub fn model(&self) -> &str {
        &self.model
    }

    /// Sends the conversation and streams the reply, calling `on_text` with
    /// each piece as it arrives. Returns the whole reply.
    pub fn stream(
        &self,
        system: &str,
        messages: &[Message],
        mut on_text: impl FnMut(&str),
    ) -> Result<String, String> {
        let body = request_body(&self.model, system, messages);
        let mut betas = vec![FALLBACK_BETA];
        let mut request = self
            .agent
            .post(&self.url)
            .header("content-type", "application/json")
            .header("anthropic-version", API_VERSION);
        request = match &self.credential {
            Credential::ApiKey(key) => request.header("x-api-key", key),
            Credential::Token(token) => {
                betas.push(OAUTH_BETA);
                request.header("authorization", &format!("Bearer {token}"))
            }
        };
        let mut response = request
            .header("anthropic-beta", &betas.join(","))
            .send(body.to_string())
            .map_err(|e| format!("could not reach Claude: {e}"))?;
        let status = response.status().as_u16();
        if status != 200 {
            let text = response.body_mut().read_to_string().unwrap_or_default();
            return Err(api_error(status, &text));
        }
        let reader = BufReader::new(response.body_mut().as_reader());
        let mut parser = SseParser::default();
        for line in reader.lines() {
            let line = line.map_err(|e| format!("stream interrupted: {e}"))?;
            match parser.line(&line)? {
                Some(Event::Text(text)) => on_text(&text),
                Some(Event::Done) => break,
                None => {}
            }
        }
        parser.finish()
    }
}

fn request_body(model: &str, system: &str, messages: &[Message]) -> Value {
    let messages: Vec<Value> = messages
        .iter()
        .map(|m| {
            json!({
                "role": match m.role { Role::User => "user", Role::Assistant => "assistant" },
                "content": m.text,
            })
        })
        .collect();
    json!({
        "model": model,
        "max_tokens": 4000,
        "stream": true,
        // The system prompt never changes, so it is cached across requests.
        "system": [{ "type": "text", "text": system, "cache_control": { "type": "ephemeral" } }],
        "messages": messages,
        // Coaching replies are short; low effort keeps them quick at the table.
        "output_config": { "effort": "low" },
        "fallbacks": "default",
    })
}

fn api_error(status: u16, body: &str) -> String {
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_else(|| body.chars().take(200).collect());
    match status {
        401 | 403 => format!("Claude rejected the credentials ({status}): {message}"),
        429 => format!("Claude is rate limiting requests: {message}"),
        _ => format!("Claude returned {status}: {message}"),
    }
}

#[derive(Debug)]
enum Event {
    Text(String),
    Done,
}

/// Reads the server-sent events of a streamed Messages response.
#[derive(Default)]
struct SseParser {
    text: String,
    stop_reason: Option<String>,
}

impl SseParser {
    fn line(&mut self, line: &str) -> Result<Option<Event>, String> {
        let Some(data) = line.strip_prefix("data:") else {
            return Ok(None);
        };
        let Ok(event) = serde_json::from_str::<Value>(data.trim()) else {
            return Ok(None);
        };
        match event["type"].as_str() {
            Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
                let text = event["delta"]["text"].as_str().unwrap_or_default();
                self.text.push_str(text);
                Ok(Some(Event::Text(text.to_string())))
            }
            Some("message_delta") => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    self.stop_reason = Some(reason.to_string());
                }
                Ok(None)
            }
            Some("message_stop") => Ok(Some(Event::Done)),
            Some("error") => Err(format!(
                "Claude reported an error: {}",
                event["error"]["message"].as_str().unwrap_or("unknown")
            )),
            _ => Ok(None),
        }
    }

    fn finish(self) -> Result<String, String> {
        if self.stop_reason.as_deref() == Some("refusal") && self.text.trim().is_empty() {
            return Err("Claude declined to answer this one.".into());
        }
        Ok(self.text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_streamed_reply() {
        let stream = r#"event: message_start
data: {"type":"message_start","message":{"id":"msg_1"}}

event: content_block_start
data: {"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}

event: content_block_start
data: {"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"Nice "}}

event: content_block_delta
data: {"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"fold."}}

event: message_delta
data: {"type":"message_delta","delta":{"stop_reason":"end_turn"}}

event: message_stop
data: {"type":"message_stop"}
"#;
        let mut parser = SseParser::default();
        let mut pieces = Vec::new();
        for line in stream.lines() {
            if let Some(Event::Text(t)) = parser.line(line).unwrap() {
                pieces.push(t);
            }
        }
        assert_eq!(pieces, ["Nice ", "fold."]);
        assert_eq!(parser.finish().unwrap(), "Nice fold.");
    }

    #[test]
    fn reports_errors_and_refusals() {
        let mut parser = SseParser::default();
        let error =
            r#"data: {"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
        assert!(parser.line(error).unwrap_err().contains("Overloaded"));

        let mut parser = SseParser::default();
        parser
            .line(r#"data: {"type":"message_delta","delta":{"stop_reason":"refusal"}}"#)
            .unwrap();
        assert!(parser.finish().is_err());
        assert!(api_error(401, r#"{"error":{"message":"bad key"}}"#).contains("bad key"));
    }

    #[test]
    fn request_body_has_the_conversation() {
        let body = request_body(
            "claude-opus-5-5",
            "be a coach",
            &[Message {
                role: Role::User,
                text: "hi".into(),
            }],
        );
        assert_eq!(body["model"], "claude-opus-5-5");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["system"][0]["text"], "be a coach");
        assert_eq!(body["stream"], true);
    }
}
