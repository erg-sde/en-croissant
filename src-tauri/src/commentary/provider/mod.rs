//! Model providers for narration. A provider turns one prompt into one JSON document constrained by
//! a schema; it never sees chess, only text. The API key stays in Rust: `NarratorConfig` has no key
//! field, provider failures carry only `Redacted` text, and nothing here logs request or response
//! bodies.

pub mod anthropic;
pub mod openai_compat;

use std::{fmt, time::Duration};

use reqwest::Url;
use serde::{Deserialize, Serialize, Serializer};
use specta::Type;
use tokio::sync::watch;

use super::facts::Perspective;

/// Requests that take longer than this fail as `Network`.
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
/// The longest `retry-after` honoured before a rate-limited request is retried once.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);
/// Redacted text is cut to this many characters.
const REDACTED_MAX: usize = 200;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    Anthropic,
    OpenaiCompatible,
}

impl ProviderKind {
    pub fn key(self) -> &'static str {
        match self {
            ProviderKind::Anthropic => "anthropic",
            ProviderKind::OpenaiCompatible => "openaiCompatible",
        }
    }
}

/// Word limits for one move, a key moment and the game narrative.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "camelCase")]
pub struct WordBudget {
    pub move_words: u16,
    pub key_moment_words: u16,
    pub narrative_words: u16,
}

/// How to reach a model. Crosses IPC, so it has no key field: the key is read from the secret store
/// in Rust when a request is built.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarratorConfig {
    pub provider: ProviderKind,
    pub model: String,
    /// `None` for the provider's default; `https` unless the host is local.
    pub base_url: Option<String>,
    /// BCP-47 language of the output, e.g. "pt-PT".
    pub language: String,
    /// Resolved in the frontend; never "auto" here.
    pub perspective: Perspective,
    pub budget: WordBudget,
    /// Rounds in which the validator's findings are sent back. Reserved; not used yet.
    pub repair_rounds: u8,
}

/// Text that cannot leak a credential: API keys (`sk-...`), bearer tokens and the stored secret are
/// masked, and the text is cut to 200 characters. Its `Debug` and `Serialize` show only the
/// redacted form, so even an accidental `log::error!("{:?}", failure)` stays safe.
#[derive(Clone, PartialEq, Eq, Type)]
pub struct Redacted(#[specta(type = String)] String);

impl Redacted {
    pub fn new(raw: &str, secret: Option<&str>) -> Self {
        let mut text = raw.to_string();
        if let Some(secret) = secret.filter(|s| !s.is_empty()) {
            text = text.replace(secret, "[redacted]");
        }
        text = mask_after(&text, "sk-", |c| {
            c.is_ascii_alphanumeric() || c == '-' || c == '_'
        });
        text = mask_after(&text, "Bearer ", |c| !c.is_whitespace() && c != '"');
        if text.chars().count() > REDACTED_MAX {
            text = text.chars().take(REDACTED_MAX).collect::<String>() + "…";
        }
        Redacted(text)
    }

    #[cfg(test)]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Replaces the token following each occurrence of `prefix` with `[redacted]`.
fn mask_after(text: &str, prefix: &str, in_token: impl Fn(char) -> bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(prefix) {
        out.push_str(&rest[..at + prefix.len()]);
        let after = &rest[at + prefix.len()..];
        let len: usize = after
            .chars()
            .take_while(|&c| in_token(c))
            .map(char::len_utf8)
            .sum();
        if len > 0 {
            out.push_str("[redacted]");
        }
        rest = &after[len..];
    }
    out.push_str(rest);
    out
}

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Redacted({:?})", self.0)
    }
}

impl Serialize for Redacted {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

/// Why a narration request failed. Returned as data, never as a command error, so the frontend can
/// show it and keep the deterministic commentary.
#[derive(Serialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub enum NarrationFailure {
    /// The provider needs a key and none is stored.
    NoKey,
    /// The base URL is not `https` and not on this computer.
    InsecureBaseUrl,
    /// An OpenAI-compatible server needs a base URL.
    MissingBaseUrl,
    #[serde(rename_all = "camelCase")]
    Auth {
        status: u16,
    },
    #[serde(rename_all = "camelCase")]
    RateLimit {
        retry_after_ms: Option<u32>,
    },
    Network {
        message: Redacted,
    },
    Provider {
        status: u16,
        message: Redacted,
    },
    MalformedResponse {
        message: Redacted,
    },
    Refused {
        category: Option<String>,
    },
    Cancelled,
}

/// The model that answered and what it used, from the response.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default, Type)]
#[serde(rename_all = "camelCase")]
pub struct Served {
    pub model: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

/// One prompt for a provider: instructions, the user message and the response schema.
pub struct ProviderRequest<'a> {
    pub system: &'a str,
    pub user: &'a str,
    pub schema: &'a serde_json::Value,
}

/// A provider call's answer: the JSON text the schema constrains, and who answered.
pub struct ProviderResponse {
    pub json: String,
    pub served: Served,
}

/// A narration provider. Implementations send one request, map HTTP and API outcomes onto
/// `NarrationFailure`, and return the response text unparsed.
pub(crate) trait NarratorProvider {
    async fn complete(
        &self,
        request: &ProviderRequest<'_>,
    ) -> Result<ProviderResponse, NarrationFailure>;
}

/// A provider for `config`, with the key read from the secret store (`None` when none is stored).
pub enum Provider {
    Anthropic(anthropic::AnthropicProvider),
    OpenaiCompatible(openai_compat::OpenAiCompatProvider),
}

impl Provider {
    pub fn new(config: &NarratorConfig, secret: Option<String>) -> Result<Self, NarrationFailure> {
        let base = base_url(config)?;
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| NarrationFailure::Network {
                message: Redacted::new(&e.to_string(), None),
            })?;
        Ok(match config.provider {
            ProviderKind::Anthropic => Provider::Anthropic(anthropic::AnthropicProvider {
                client,
                base,
                key: secret.ok_or(NarrationFailure::NoKey)?,
                model: config.model.clone(),
            }),
            ProviderKind::OpenaiCompatible => {
                Provider::OpenaiCompatible(openai_compat::OpenAiCompatProvider {
                    client,
                    base,
                    key: secret,
                    model: config.model.clone(),
                })
            }
        })
    }

    /// Sends the request, giving up as soon as `cancel` changes: dropping the request future closes
    /// the connection.
    pub async fn complete(
        &self,
        request: &ProviderRequest<'_>,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<ProviderResponse, NarrationFailure> {
        if crate::jobs::is_cancelled(cancel) {
            return Err(NarrationFailure::Cancelled);
        }
        let call = async {
            match self {
                Provider::Anthropic(p) => p.complete(request).await,
                Provider::OpenaiCompatible(p) => p.complete(request).await,
            }
        };
        tokio::select! {
            result = call => result,
            _ = cancel.changed() => Err(NarrationFailure::Cancelled),
        }
    }
}

/// The base URL to call: the configured one, checked, or the provider's default.
fn base_url(config: &NarratorConfig) -> Result<Url, NarrationFailure> {
    let raw = match (&config.base_url, config.provider) {
        (Some(url), _) => url.trim_end_matches('/').to_string(),
        (None, ProviderKind::Anthropic) => anthropic::DEFAULT_BASE_URL.to_string(),
        (None, ProviderKind::OpenaiCompatible) => return Err(NarrationFailure::MissingBaseUrl),
    };
    let url = Url::parse(&raw).map_err(|_| NarrationFailure::InsecureBaseUrl)?;
    let local = matches!(
        url.host_str(),
        Some("localhost") | Some("127.0.0.1") | Some("[::1]") | Some("::1")
    );
    match url.scheme() {
        "https" => Ok(url),
        "http" if local => Ok(url),
        _ => Err(NarrationFailure::InsecureBaseUrl),
    }
}

/// `url` with `path` appended to its path.
fn endpoint(base: &Url, path: &str) -> Url {
    let mut url = base.clone();
    let joined = format!("{}{}", url.path().trim_end_matches('/'), path);
    url.set_path(&joined);
    url
}

/// Maps a non-success HTTP status onto a failure. `body` is redacted before it is kept.
fn status_failure(
    status: u16,
    retry_after: Option<Duration>,
    body: &str,
    secret: Option<&str>,
) -> NarrationFailure {
    match status {
        401 | 403 => NarrationFailure::Auth { status },
        429 => NarrationFailure::RateLimit {
            retry_after_ms: retry_after.map(|d| d.as_millis().min(u32::MAX as u128) as u32),
        },
        _ => NarrationFailure::Provider {
            status,
            message: Redacted::new(&error_message(body), secret),
        },
    }
}

/// The `error.message` of an API error body, or the body itself.
fn error_message(body: &str) -> String {
    serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .and_then(|m| m.as_str())
                .map(String::from)
        })
        .unwrap_or_else(|| body.to_string())
}

fn network_failure(error: reqwest::Error, secret: Option<&str>) -> NarrationFailure {
    // reqwest errors can include the URL; neither the key nor a body is part of it, but redact anyway.
    NarrationFailure::Network {
        message: Redacted::new(&error.without_url().to_string(), secret),
    }
}

fn retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<f64>()
        .ok()
        .map(|s| Duration::from_secs_f64(s.max(0.0)))
}

/// Sends `build()` and retries once after a 429 whose `retry-after` is short enough.
async fn send_with_retry(
    build: impl Fn() -> reqwest::RequestBuilder,
    secret: Option<&str>,
) -> Result<(u16, String), NarrationFailure> {
    for attempt in 0..2 {
        let response = build()
            .send()
            .await
            .map_err(|e| network_failure(e, secret))?;
        let status = response.status().as_u16();
        let wait = retry_after(response.headers());
        let body = response
            .text()
            .await
            .map_err(|e| network_failure(e, secret))?;
        if status == 429 && attempt == 0 {
            if let Some(wait) = wait.filter(|w| *w <= MAX_RETRY_AFTER) {
                tokio::time::sleep(wait).await;
                continue;
            }
        }
        if !(200..300).contains(&status) {
            return Err(status_failure(status, wait, &body, secret));
        }
        return Ok((status, body));
    }
    unreachable!("the loop returns on its second attempt")
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub fn config(provider: ProviderKind, base_url: Option<&str>) -> NarratorConfig {
        NarratorConfig {
            provider,
            model: "test-model".into(),
            base_url: base_url.map(String::from),
            language: "en-US".into(),
            perspective: Perspective::Both,
            budget: WordBudget {
                move_words: 60,
                key_moment_words: 140,
                narrative_words: 220,
            },
            repair_rounds: 0,
        }
    }

    #[test]
    fn redacted_never_shows_a_key() {
        let body = r#"{"error":{"message":"invalid x-api-key sk-ant-api03-AbC_d-123 for Bearer abc.def"}}"#;
        let failure = NarrationFailure::Provider {
            status: 401,
            message: Redacted::new(body, Some("my-stored-secret")),
        };
        let shown = format!("{failure:?} {}", serde_json::to_string(&failure).unwrap());
        assert!(!shown.contains("sk-ant-api03"));
        assert!(!shown.contains("abc.def"));
        assert!(shown.contains("sk-[redacted]"));
        let secret = Redacted::new("key is my-stored-secret!", Some("my-stored-secret"));
        assert_eq!(secret.as_str(), "key is [redacted]!");
        assert!(
            Redacted::new(&"x".repeat(500), None)
                .as_str()
                .chars()
                .count()
                <= 201
        );
    }

    #[test]
    fn base_urls_must_be_https_unless_local() {
        let check = |p, url| base_url(&config(p, url));
        assert!(check(ProviderKind::Anthropic, None).is_ok());
        assert!(check(ProviderKind::Anthropic, Some("https://proxy.example/v1")).is_ok());
        assert_eq!(
            check(ProviderKind::Anthropic, Some("http://evil.example")),
            Err(NarrationFailure::InsecureBaseUrl)
        );
        assert!(check(
            ProviderKind::OpenaiCompatible,
            Some("http://localhost:11434")
        )
        .is_ok());
        assert!(check(
            ProviderKind::OpenaiCompatible,
            Some("http://127.0.0.1:8080/")
        )
        .is_ok());
        assert_eq!(
            check(ProviderKind::OpenaiCompatible, None),
            Err(NarrationFailure::MissingBaseUrl)
        );
    }

    #[test]
    fn statuses_map_to_failures() {
        assert_eq!(
            status_failure(401, None, "", None),
            NarrationFailure::Auth { status: 401 }
        );
        assert_eq!(
            status_failure(429, Some(Duration::from_secs(2)), "", None),
            NarrationFailure::RateLimit {
                retry_after_ms: Some(2000)
            }
        );
        assert!(matches!(
            status_failure(529, None, r#"{"error":{"message":"Overloaded"}}"#, None),
            NarrationFailure::Provider { status: 529, ref message } if message.as_str() == "Overloaded"
        ));
    }

    #[test]
    fn endpoints_keep_the_base_path() {
        let base = Url::parse("https://proxy.example/anthropic").unwrap();
        assert_eq!(
            endpoint(&base, "/v1/messages").as_str(),
            "https://proxy.example/anthropic/v1/messages"
        );
    }
}
