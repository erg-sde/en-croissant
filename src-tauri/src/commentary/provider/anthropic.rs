//! The Anthropic Messages API, called over HTTP. The key is sent only in the `x-api-key` header and
//! is never formatted into text; response bodies are parsed, not kept.

use serde::Deserialize;
use serde_json::json;

use super::{
    endpoint, send_with_retry, NarrationFailure, NarratorProvider, ProviderRequest,
    ProviderResponse, Redacted, Served,
};

pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const API_VERSION: &str = "2023-06-01";
const FALLBACK_BETA: &str = "server-side-fallback-2026-07-01";
/// Thinking tokens count against this; non-streaming requests stay well inside the timeout.
const MAX_TOKENS: u32 = 16000;

pub struct AnthropicProvider {
    pub client: reqwest::Client,
    pub base: reqwest::Url,
    pub key: String,
    pub model: String,
}

/// Models the API can serve with a fallback when they are unavailable.
fn has_server_fallback(model: &str) -> bool {
    ["claude-opus-5", "claude-fable-5", "claude-sonnet-5-5"]
        .iter()
        .any(|prefix| model.starts_with(prefix))
}

impl AnthropicProvider {
    fn body(&self, request: &ProviderRequest<'_>) -> serde_json::Value {
        let mut body = json!({
            "model": self.model,
            "max_tokens": MAX_TOKENS,
            "system": [{
                "type": "text",
                "text": request.system,
                "cache_control": { "type": "ephemeral" },
            }],
            "messages": [{ "role": "user", "content": request.user }],
            "output_config": {
                "effort": "low",
                "format": { "type": "json_schema", "schema": request.schema },
            },
        });
        if has_server_fallback(&self.model) {
            body["fallbacks"] = json!("default");
        }
        body
    }
}

#[derive(Deserialize)]
struct MessageResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    content: Vec<ContentBlock>,
    stop_reason: Option<String>,
    stop_details: Option<StopDetails>,
    #[serde(default)]
    usage: Usage,
}

#[derive(Deserialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Deserialize)]
struct StopDetails {
    category: Option<String>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    input_tokens: u32,
    #[serde(default)]
    output_tokens: u32,
    #[serde(default)]
    cache_read_input_tokens: u32,
    #[serde(default)]
    cache_creation_input_tokens: u32,
}

/// Reads a successful response: the stop reason first, then the text block.
fn parse_response(body: &str) -> Result<ProviderResponse, NarrationFailure> {
    let malformed = |message: &str| NarrationFailure::MalformedResponse {
        message: Redacted::new(message, None),
    };
    let response: MessageResponse =
        serde_json::from_str(body).map_err(|e| malformed(&e.to_string()))?;
    match response.stop_reason.as_deref() {
        Some("refusal") => {
            return Err(NarrationFailure::Refused {
                category: response.stop_details.and_then(|d| d.category),
            })
        }
        Some("max_tokens") => return Err(malformed("the response was cut at max_tokens")),
        _ => {}
    }
    let json = response
        .content
        .into_iter()
        .filter(|block| block.kind == "text")
        .find_map(|block| block.text)
        .ok_or_else(|| malformed("the response has no text block"))?;
    let usage = response.usage;
    Ok(ProviderResponse {
        json,
        served: Served {
            model: response.model,
            input_tokens: usage.input_tokens
                + usage.cache_read_input_tokens
                + usage.cache_creation_input_tokens,
            output_tokens: usage.output_tokens,
        },
    })
}

impl NarratorProvider for AnthropicProvider {
    async fn complete(
        &self,
        request: &ProviderRequest<'_>,
    ) -> Result<ProviderResponse, NarrationFailure> {
        let url = endpoint(&self.base, "/v1/messages");
        let body = self.body(request);
        let mut beta = vec![];
        if has_server_fallback(&self.model) {
            beta.push(FALLBACK_BETA);
        }
        let (_, text) = send_with_retry(
            || {
                let mut builder = self
                    .client
                    .post(url.clone())
                    .header("x-api-key", &self.key)
                    .header("anthropic-version", API_VERSION)
                    .json(&body);
                if !beta.is_empty() {
                    builder = builder.header("anthropic-beta", beta.join(","));
                }
                builder
            },
            Some(&self.key),
        )
        .await?;
        parse_response(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_is_read_before_content() {
        let body = r#"{"model":"m","content":[{"type":"text","text":"{}"}],"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber"},"usage":{"input_tokens":1,"output_tokens":1}}"#;
        assert_eq!(
            parse_response(body).err(),
            Some(NarrationFailure::Refused {
                category: Some("cyber".into())
            })
        );
    }

    #[test]
    fn max_tokens_is_malformed() {
        let body = r#"{"model":"m","content":[{"type":"text","text":"{\"items\""}],"stop_reason":"max_tokens","usage":{}}"#;
        assert!(matches!(
            parse_response(body),
            Err(NarrationFailure::MalformedResponse { .. })
        ));
    }

    #[test]
    fn thinking_blocks_are_skipped_and_usage_recorded() {
        let body = r#"{"model":"claude-opus-5-5","content":[{"type":"thinking","thinking":"..."},{"type":"text","text":"{\"items\":[]}"}],"stop_reason":"end_turn","usage":{"input_tokens":10,"cache_read_input_tokens":90,"output_tokens":5}}"#;
        let response = parse_response(body).ok().unwrap();
        assert_eq!(response.json, r#"{"items":[]}"#);
        assert_eq!(
            response.served,
            Served {
                model: "claude-opus-5-5".into(),
                input_tokens: 100,
                output_tokens: 5,
            }
        );
    }

    #[test]
    fn fallback_only_for_models_that_have_it() {
        assert!(has_server_fallback("claude-opus-5-5"));
        assert!(has_server_fallback("claude-sonnet-5-5"));
        assert!(!has_server_fallback("claude-haiku-5-5"));
    }
}
