//! OpenAI-compatible chat completions (Ollama, LM Studio, llama-server, hosted gateways). Sends an
//! `Authorization` header only when a secret is stored, so a local server works with no key.

use serde::Deserialize;
use serde_json::json;

use super::{
    endpoint, send_with_retry, NarrationFailure, NarratorProvider, ProviderRequest,
    ProviderResponse, Redacted, Served,
};

pub struct OpenAiCompatProvider {
    pub client: reqwest::Client,
    pub base: reqwest::Url,
    pub key: Option<String>,
    pub model: String,
}

/// How the response is constrained: by the schema, or (for servers without structured outputs)
/// as any JSON object with the schema written into the prompt.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Format {
    Schema,
    JsonObject,
}

impl OpenAiCompatProvider {
    fn body(&self, request: &ProviderRequest<'_>, format: Format) -> serde_json::Value {
        let (system, response_format) = match format {
            Format::Schema => (
                request.system.to_string(),
                json!({
                    "type": "json_schema",
                    "json_schema": { "name": "narration", "schema": request.schema, "strict": true },
                }),
            ),
            Format::JsonObject => (
                format!(
                    "{}\n\nAnswer with one JSON object that matches this JSON Schema:\n{}",
                    request.system, request.schema
                ),
                json!({ "type": "json_object" }),
            ),
        };
        json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": system },
                { "role": "user", "content": request.user },
            ],
            "response_format": response_format,
        })
    }

    async fn send(
        &self,
        request: &ProviderRequest<'_>,
        format: Format,
    ) -> Result<String, NarrationFailure> {
        let url = endpoint(&self.base, "/v1/chat/completions");
        let body = self.body(request, format);
        let (_, text) = send_with_retry(
            || {
                let builder = self.client.post(url.clone()).json(&body);
                match &self.key {
                    Some(key) => builder.bearer_auth(key),
                    None => builder,
                }
            },
            self.key.as_deref(),
        )
        .await?;
        Ok(text)
    }
}

#[derive(Deserialize)]
struct ChatResponse {
    #[serde(default)]
    model: String,
    #[serde(default)]
    choices: Vec<Choice>,
    #[serde(default)]
    usage: Usage,
}

#[derive(Deserialize)]
struct Choice {
    message: Message,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct Message {
    content: Option<String>,
    refusal: Option<String>,
}

#[derive(Deserialize, Default)]
struct Usage {
    #[serde(default)]
    prompt_tokens: u32,
    #[serde(default)]
    completion_tokens: u32,
}

fn parse_response(body: &str, model: &str) -> Result<ProviderResponse, NarrationFailure> {
    let malformed = |message: &str| NarrationFailure::MalformedResponse {
        message: Redacted::new(message, None),
    };
    let response: ChatResponse =
        serde_json::from_str(body).map_err(|e| malformed(&e.to_string()))?;
    let choice = response
        .choices
        .into_iter()
        .next()
        .ok_or_else(|| malformed("the response has no choices"))?;
    if choice.message.refusal.is_some() {
        return Err(NarrationFailure::Refused { category: None });
    }
    if choice.finish_reason.as_deref() == Some("length") {
        return Err(malformed("the response was cut at the token limit"));
    }
    let json = choice
        .message
        .content
        .ok_or_else(|| malformed("the response has no content"))?;
    Ok(ProviderResponse {
        json,
        served: Served {
            model: if response.model.is_empty() {
                model.to_string()
            } else {
                response.model
            },
            input_tokens: response.usage.prompt_tokens,
            output_tokens: response.usage.completion_tokens,
        },
    })
}

impl NarratorProvider for OpenAiCompatProvider {
    async fn complete(
        &self,
        request: &ProviderRequest<'_>,
    ) -> Result<ProviderResponse, NarrationFailure> {
        let text = match self.send(request, Format::Schema).await {
            // Servers without structured outputs reject `json_schema` with a 400.
            Err(NarrationFailure::Provider { status: 400, .. }) => {
                self.send(request, Format::JsonObject).await?
            }
            other => other?,
        };
        parse_response(&text, &self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_content_and_usage() {
        let body = r#"{"model":"llama3","choices":[{"message":{"role":"assistant","content":"{\"items\":[]}"},"finish_reason":"stop"}],"usage":{"prompt_tokens":7,"completion_tokens":3}}"#;
        let response = parse_response(body, "configured").ok().unwrap();
        assert_eq!(response.json, r#"{"items":[]}"#);
        assert_eq!(response.served.model, "llama3");
        assert_eq!(response.served.input_tokens, 7);
    }

    #[test]
    fn length_and_refusal_fail() {
        let cut = r#"{"choices":[{"message":{"content":"{"},"finish_reason":"length"}]}"#;
        assert!(matches!(
            parse_response(cut, "m"),
            Err(NarrationFailure::MalformedResponse { .. })
        ));
        let refused =
            r#"{"choices":[{"message":{"content":null,"refusal":"no"},"finish_reason":"stop"}]}"#;
        assert_eq!(
            parse_response(refused, "m").err(),
            Some(NarrationFailure::Refused { category: None })
        );
    }
}
