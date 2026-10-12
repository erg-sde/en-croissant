//! LLM narration: the typed request from the frontend goes out in chunks, each answer comes back as
//! a `NarrationChunk` event. The model writes sentence templates with slots and declares claims;
//! the frontend renders and validates them, so nothing here trusts the text.

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;
use tokio::sync::watch;

use super::{
    facts::{
        CommentaryKind, FactKind, FactVocabulary, MoveRef, NumberRef, Perspective, PieceRef,
        PositionRef, Side,
    },
    prompt::{narration_schema, user_message, ChunkContent, SYSTEM_PROMPT},
    provider::{NarrationFailure, NarratorConfig, Provider, ProviderRequest, Served},
    secrets::{FileStore, SecretStore},
};
use crate::{
    error::Error,
    jobs::is_cancelled,
    progress::{AppProgress, ProgressGuard, ProgressStage},
    AppState,
};

/// Moves per request.
pub const CHUNK_ITEMS: usize = 12;

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrationFact {
    pub id: String,
    pub kind: FactKind,
    pub vocabulary: FactVocabulary,
    /// The deterministic sentence that cited this fact.
    pub text_hint: String,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrationItem {
    pub index: u32,
    pub fen_before: String,
    pub move_number: u32,
    pub san: String,
    pub side: Side,
    /// The glyph or classification label, e.g. "??".
    pub classification: String,
    pub kind: CommentaryKind,
    pub budget_words: u16,
    pub facts: Vec<NarrationFact>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrationGame {
    pub facts: Vec<NarrationFact>,
    pub result: String,
    pub turning_points: Vec<NarrationItem>,
}

/// Everything sent to a provider: facts, their vocabulary and draft sentences. Never names,
/// headers, comments or paths.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarrationRequest {
    pub lang: String,
    pub perspective: Perspective,
    pub game: NarrationGame,
    pub items: Vec<NarrationItem>,
}

/// The values a template's placeholders may take. Fixed names keep the schema closed and flat.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Default, Type)]
pub struct Slots {
    pub m0: Option<MoveRef>,
    pub m1: Option<MoveRef>,
    pub p0: Option<PieceRef>,
    pub p1: Option<PieceRef>,
    pub s0: Option<String>,
    pub s1: Option<String>,
    pub n0: Option<NumberRef>,
    pub n1: Option<NumberRef>,
    pub side: Option<Side>,
    pub pieces: Vec<PieceRef>,
}

/// A relation a sentence asserts, checked against the cited facts.
#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Claim {
    Attacks {
        attacker: PieceRef,
        target: PieceRef,
    },
    Forks {
        attacker: PieceRef,
        targets: Vec<PieceRef>,
    },
    Pins {
        attacker: PieceRef,
        pinned: PieceRef,
    },
    Hanging {
        piece: PieceRef,
    },
    WinsMaterial {
        side: Side,
        pawns: i32,
        line: u8,
    },
    #[serde(rename_all = "camelCase")]
    Mates {
        side: Side,
        mate_in: u8,
        line: u8,
    },
    Better {
        side: Side,
        at: PositionRef,
    },
    Threatens {
        side: Side,
        line: u8,
    },
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarratedSentence {
    pub template: String,
    pub slots: Slots,
    pub claims: Vec<Claim>,
    pub fact_ids: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarratedMove {
    pub index: u32,
    /// Copied from the request item, not from the model.
    #[serde(default)]
    pub fen_before: String,
    #[serde(default)]
    pub san: String,
    pub sentences: Vec<NarratedSentence>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarratedTakeaway {
    pub indices: Vec<u32>,
    pub sentences: Vec<NarratedSentence>,
}

#[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Type)]
#[serde(rename_all = "camelCase")]
pub struct NarratedGame {
    pub sentences: Vec<NarratedSentence>,
    pub takeaways: Vec<NarratedTakeaway>,
}

/// One answered request.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct NarrationChunk {
    pub id: String,
    pub run_id: String,
    pub chunk: u32,
    pub chunks: u32,
    pub items: Vec<NarratedMove>,
    pub narrative: Option<NarratedGame>,
    pub served: Served,
}

#[derive(Serialize, Debug, Clone, PartialEq, Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum NarrationRun {
    Completed {
        chunks: u32,
    },
    #[serde(rename_all = "camelCase")]
    Cancelled {
        chunks_done: u32,
    },
    #[serde(rename_all = "camelCase")]
    Failed {
        chunks_done: u32,
        failure: NarrationFailure,
    },
}

/// What the model returns for one request.
#[derive(Deserialize)]
struct NarrationResponse {
    #[serde(default)]
    items: Vec<NarratedMove>,
    #[serde(default)]
    narrative: Option<NarratedGame>,
}

/// The requests a run sends: the moves in groups of `CHUNK_ITEMS`, then the game narrative.
pub fn plan_chunks(request: &NarrationRequest) -> Vec<ChunkContent<'_>> {
    let mut chunks: Vec<ChunkContent> = request
        .items
        .chunks(CHUNK_ITEMS)
        .map(ChunkContent::Moves)
        .collect();
    if !request.game.facts.is_empty() {
        chunks.push(ChunkContent::Narrative(&request.game));
    }
    chunks
}

/// Parses one answer and keeps only moves that were asked for, with their position from the request.
fn read_answer(
    json: &str,
    content: &ChunkContent<'_>,
) -> Result<(Vec<NarratedMove>, Option<NarratedGame>), NarrationFailure> {
    let response: NarrationResponse =
        serde_json::from_str(json).map_err(|e| NarrationFailure::MalformedResponse {
            message: super::provider::Redacted::new(&e.to_string(), None),
        })?;
    match content {
        ChunkContent::Moves(items) => {
            let moves = response
                .items
                .into_iter()
                .filter_map(|mut narrated| {
                    let item = items.iter().find(|i| i.index == narrated.index)?;
                    narrated.fen_before = item.fen_before.clone();
                    narrated.san = item.san.clone();
                    Some(narrated)
                })
                .collect();
            Ok((moves, None))
        }
        ChunkContent::Narrative(_) => Ok((vec![], response.narrative)),
    }
}

/// Sends every chunk in order and hands each answer to `emit`. Stops at the first failure or on
/// cancel; chunks already emitted stay applied.
#[allow(clippy::too_many_arguments)]
pub async fn run_narration(
    id: &str,
    run_id: &str,
    request: &NarrationRequest,
    config: &NarratorConfig,
    provider: &Provider,
    cancel: &mut watch::Receiver<bool>,
    mut emit: impl FnMut(NarrationChunk) -> Result<(), Error>,
    mut progress: impl FnMut(f32),
) -> Result<NarrationRun, Error> {
    let plan = plan_chunks(request);
    let chunks = plan.len() as u32;
    let schema = narration_schema();
    for (done, content) in plan.iter().enumerate() {
        let done = done as u32;
        if is_cancelled(cancel) {
            return Ok(NarrationRun::Cancelled { chunks_done: done });
        }
        progress(done as f32 / chunks as f32 * 100.0);
        let user = user_message(request, config, content);
        let call = ProviderRequest {
            system: SYSTEM_PROMPT,
            user: &user,
            schema: &schema,
        };
        let answer = provider
            .complete(&call, cancel)
            .await
            .and_then(|response| Ok((read_answer(&response.json, content)?, response.served)));
        match answer {
            Ok(((items, narrative), served)) => emit(NarrationChunk {
                id: id.to_string(),
                run_id: run_id.to_string(),
                chunk: done,
                chunks,
                items,
                narrative,
                served,
            })?,
            Err(NarrationFailure::Cancelled) => {
                return Ok(NarrationRun::Cancelled { chunks_done: done })
            }
            Err(failure) => {
                return Ok(NarrationRun::Failed {
                    chunks_done: done,
                    failure,
                })
            }
        }
    }
    Ok(NarrationRun::Completed { chunks })
}

fn provider_for(
    config: &NarratorConfig,
    app: &tauri::AppHandle,
) -> Result<Result<Provider, NarrationFailure>, Error> {
    let secret = FileStore::for_app(app)?
        .get(config.provider)?
        .filter(|s| !s.is_empty());
    Ok(Provider::new(config, secret))
}

#[tauri::command]
#[specta::specta]
pub async fn narrate_game(
    id: String,
    run_id: String,
    request: NarrationRequest,
    config: NarratorConfig,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<NarrationRun, Error> {
    let guard = ProgressGuard::start(
        AppProgress {
            store: &state.progress_state,
            app: &app,
        },
        &state.jobs,
        id.clone(),
        ProgressStage::Commentary,
    );
    guard.progress(0.0)?;
    let provider = match provider_for(&config, &app)? {
        Ok(provider) => provider,
        Err(failure) => {
            guard.finished()?;
            return Ok(NarrationRun::Failed {
                chunks_done: 0,
                failure,
            });
        }
    };
    let mut cancel = guard.cancel_token();
    let run = run_narration(
        &id,
        &run_id,
        &request,
        &config,
        &provider,
        &mut cancel,
        |chunk| Ok(chunk.emit(&app)?),
        |p| {
            let _ = guard.progress(p);
        },
    )
    .await?;
    match run {
        NarrationRun::Cancelled { .. } => guard.cancelled()?,
        _ => guard.finished()?,
    }
    Ok(run)
}

/// Sends a one-move request and reports why it failed, or `None` when the provider answered with
/// valid JSON.
#[tauri::command]
#[specta::specta]
pub async fn test_narrator(
    config: NarratorConfig,
    app: tauri::AppHandle,
) -> Result<Option<NarrationFailure>, Error> {
    let provider = match provider_for(&config, &app)? {
        Ok(provider) => provider,
        Err(failure) => return Ok(Some(failure)),
    };
    let request = super::prompt::connection_test_request(&config);
    let (_keep, mut cancel) = watch::channel(false);
    let run = run_narration(
        "test",
        "test",
        &request,
        &config,
        &provider,
        &mut cancel,
        |_| Ok(()),
        |_| {},
    )
    .await?;
    Ok(match run {
        NarrationRun::Failed { failure, .. } => Some(failure),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
    };

    use super::*;
    use crate::commentary::{
        prompt::connection_test_request,
        provider::{tests::config, ProviderKind},
    };

    /// A one-request-at-a-time HTTP server. Each connection gets the next canned response (or
    /// none, when `None`, holding the connection open). Returns the base URL and the raw requests.
    async fn mock_server(
        responses: Vec<Option<(u16, &'static str, String)>>,
    ) -> (String, tokio::sync::mpsc::UnboundedReceiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let (seen, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(async move {
            for response in responses {
                let (mut socket, _) = listener.accept().await.unwrap();
                let request = read_request(&mut socket).await;
                let _ = seen.send(request);
                match response {
                    Some((status, headers, body)) => {
                        let reply = format!(
                            "HTTP/1.1 {status} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n{headers}\r\n{body}",
                            body.len()
                        );
                        socket.write_all(reply.as_bytes()).await.unwrap();
                    }
                    None => {
                        // Never answer; wait until the client hangs up.
                        let mut buf = [0u8; 64];
                        while socket.read(&mut buf).await.map(|n| n > 0).unwrap_or(false) {}
                        let _ = seen.send("closed".into());
                    }
                }
            }
        });
        (base, rx)
    }

    async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
        let mut data = vec![];
        let mut buf = [0u8; 4096];
        loop {
            let n = socket.read(&mut buf).await.unwrap();
            data.extend_from_slice(&buf[..n]);
            let text = String::from_utf8_lossy(&data).to_string();
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|l| {
                        let (name, value) = l.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().ok())?
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + length {
                    return text;
                }
            }
            if n == 0 {
                return String::from_utf8_lossy(&data).to_string();
            }
        }
    }

    fn anthropic_ok(json: &str) -> String {
        serde_json::json!({
            "model": "claude-opus-5-5",
            "content": [{ "type": "text", "text": json }],
            "stop_reason": "end_turn",
            "usage": { "input_tokens": 3, "output_tokens": 2 },
        })
        .to_string()
    }

    fn openai_ok(json: &str) -> String {
        serde_json::json!({
            "model": "local",
            "choices": [{ "message": { "content": json }, "finish_reason": "stop" }],
        })
        .to_string()
    }

    async fn run(
        request: &NarrationRequest,
        config: &NarratorConfig,
        secret: Option<&str>,
        cancel: &mut watch::Receiver<bool>,
    ) -> (NarrationRun, Vec<NarrationChunk>) {
        let provider = Provider::new(config, secret.map(String::from)).unwrap();
        let mut chunks = vec![];
        let run = run_narration(
            "report_a",
            "run",
            request,
            config,
            &provider,
            cancel,
            |c| {
                chunks.push(c);
                Ok(())
            },
            |_| {},
        )
        .await
        .unwrap();
        (run, chunks)
    }

    const MOVE_ANSWER: &str = r#"{"items":[{"index":1,"sentences":[{"template":"{{m0}} opens the game.","slots":{"m0":{"san":"e4","side":"white","at":{"t":"before"}},"m1":null,"p0":null,"p1":null,"s0":null,"s1":null,"n0":null,"n1":null,"side":null,"pieces":[]},"claims":[],"factIds":["line:0"]}]},{"index":99,"sentences":[]}],"narrative":null}"#;

    #[tokio::test]
    async fn anthropic_chunks_are_emitted_with_positions_from_the_request() {
        let (base, mut seen) = mock_server(vec![
            Some((200, "", anthropic_ok(MOVE_ANSWER))),
            Some((
                200,
                "",
                anthropic_ok(r#"{"items":[],"narrative":{"sentences":[],"takeaways":[]}}"#),
            )),
        ])
        .await;
        let config = config(ProviderKind::Anthropic, Some(&base));
        let request = connection_test_request(&config);
        let (_keep, mut cancel) = watch::channel(false);
        let (outcome, chunks) = run(&request, &config, Some("sk-ant-secret"), &mut cancel).await;
        assert_eq!(outcome, NarrationRun::Completed { chunks: 2 });
        assert_eq!(chunks.len(), 2);
        // The unrequested index 99 is dropped; the position comes from the request.
        assert_eq!(chunks[0].items.len(), 1);
        assert_eq!(chunks[0].items[0].fen_before, request.items[0].fen_before);
        assert_eq!(chunks[0].served.model, "claude-opus-5-5");
        assert!(chunks[1].narrative.is_some());

        let first = seen.recv().await.unwrap();
        assert!(first.starts_with("POST /v1/messages"));
        assert!(first.contains("x-api-key: sk-ant-secret"));
        assert!(first.contains("anthropic-version: 2023-06-01"));
        assert!(first.contains("\"effort\":\"low\""));
    }

    #[tokio::test]
    async fn local_server_without_a_key_gets_no_authorization() {
        let (base, mut seen) = mock_server(vec![
            Some((200, "", openai_ok(MOVE_ANSWER))),
            Some((200, "", openai_ok(r#"{"items":[],"narrative":null}"#))),
        ])
        .await;
        let config = config(ProviderKind::OpenaiCompatible, Some(&base));
        let request = connection_test_request(&config);
        let (_keep, mut cancel) = watch::channel(false);
        let (outcome, _) = run(&request, &config, None, &mut cancel).await;
        assert_eq!(outcome, NarrationRun::Completed { chunks: 2 });
        let first = seen.recv().await.unwrap();
        assert!(first.starts_with("POST /v1/chat/completions"));
        assert!(!first.to_ascii_lowercase().contains("authorization"));
    }

    #[tokio::test]
    async fn openai_compat_degrades_to_json_object_on_400() {
        let (base, mut seen) = mock_server(vec![
            Some((
                400,
                "",
                r#"{"error":{"message":"response_format json_schema unsupported"}}"#.into(),
            )),
            Some((200, "", openai_ok(MOVE_ANSWER))),
            Some((200, "", openai_ok(r#"{"narrative":null}"#))),
        ])
        .await;
        let config = config(ProviderKind::OpenaiCompatible, Some(&base));
        let request = connection_test_request(&config);
        let (_keep, mut cancel) = watch::channel(false);
        let (outcome, chunks) = run(&request, &config, Some("local-key"), &mut cancel).await;
        assert_eq!(outcome, NarrationRun::Completed { chunks: 2 });
        assert_eq!(chunks[0].items.len(), 1);
        assert!(seen.recv().await.unwrap().contains("json_schema"));
        let second = seen.recv().await.unwrap();
        assert!(second.contains("json_object"));
        assert!(second.contains("authorization: Bearer local-key"));
    }

    #[tokio::test]
    async fn failures_stop_the_run_and_never_carry_the_key() {
        let (base, _seen) = mock_server(vec![Some((
            500,
            "",
            r#"{"error":{"message":"bad key sk-ant-secret-123 rejected"}}"#.into(),
        ))])
        .await;
        let config = config(ProviderKind::Anthropic, Some(&base));
        let request = connection_test_request(&config);
        let (_keep, mut cancel) = watch::channel(false);
        let (outcome, chunks) =
            run(&request, &config, Some("sk-ant-secret-123"), &mut cancel).await;
        assert!(chunks.is_empty());
        let NarrationRun::Failed {
            chunks_done: 0,
            failure,
        } = outcome
        else {
            panic!("expected a failure, got {outcome:?}");
        };
        let shown = format!("{failure:?}{}", serde_json::to_string(&failure).unwrap());
        assert!(matches!(
            failure,
            NarrationFailure::Provider { status: 500, .. }
        ));
        assert!(!shown.contains("secret-123"));
    }

    #[tokio::test]
    async fn auth_and_rate_limit_map_to_their_failures() {
        let (base, _seen) = mock_server(vec![
            Some((
                401,
                "",
                r#"{"error":{"message":"invalid x-api-key"}}"#.into(),
            )),
            Some((429, "retry-after: 120\r\n", "{}".into())),
        ])
        .await;
        let config = config(ProviderKind::Anthropic, Some(&base));
        let request = connection_test_request(&config);
        let (_keep, mut cancel) = watch::channel(false);
        let (auth, _) = run(&request, &config, Some("k"), &mut cancel).await;
        assert!(matches!(
            auth,
            NarrationRun::Failed {
                failure: NarrationFailure::Auth { status: 401 },
                ..
            }
        ));
        // A long retry-after is not waited for.
        let (limited, _) = run(&request, &config, Some("k"), &mut cancel).await;
        assert!(matches!(
            limited,
            NarrationRun::Failed {
                failure: NarrationFailure::RateLimit {
                    retry_after_ms: Some(120_000)
                },
                ..
            }
        ));
    }

    #[tokio::test]
    async fn a_short_retry_after_is_retried_once() {
        let (base, _seen) = mock_server(vec![
            Some((429, "retry-after: 0\r\n", "{}".into())),
            Some((200, "", anthropic_ok(MOVE_ANSWER))),
            Some((200, "", anthropic_ok(r#"{"narrative":null}"#))),
        ])
        .await;
        let config = config(ProviderKind::Anthropic, Some(&base));
        let request = connection_test_request(&config);
        let (_keep, mut cancel) = watch::channel(false);
        let (outcome, _) = run(&request, &config, Some("k"), &mut cancel).await;
        assert_eq!(outcome, NarrationRun::Completed { chunks: 2 });
    }

    #[tokio::test]
    async fn cancel_returns_quickly_and_closes_the_socket() {
        let (base, mut seen) = mock_server(vec![None]).await;
        let config = config(ProviderKind::Anthropic, Some(&base));
        let request = connection_test_request(&config);
        let (cancel_tx, mut cancel) = watch::channel(false);
        let task =
            tokio::spawn(async move { run(&request, &config, Some("k"), &mut cancel).await });
        // Wait until the request has reached the server.
        seen.recv().await.unwrap();
        let start = Instant::now();
        cancel_tx.send_replace(true);
        let (outcome, _) = task.await.unwrap();
        assert!(start.elapsed() < Duration::from_millis(100));
        assert_eq!(outcome, NarrationRun::Cancelled { chunks_done: 0 });
        let closed = tokio::time::timeout(Duration::from_secs(2), seen.recv()).await;
        assert_eq!(closed.unwrap().as_deref(), Some("closed"));
    }

    #[test]
    fn chunks_hold_twelve_moves_and_the_narrative_comes_last() {
        let config = config(ProviderKind::Anthropic, None);
        let mut request = connection_test_request(&config);
        let item = request.items[0].clone();
        request.items = (1..=25)
            .map(|i| NarrationItem {
                index: i,
                ..item.clone()
            })
            .collect();
        let plan = plan_chunks(&request);
        let sizes: Vec<_> = plan
            .iter()
            .map(|c| match c {
                ChunkContent::Moves(items) => items.len(),
                ChunkContent::Narrative(_) => 0,
            })
            .collect();
        assert_eq!(sizes, vec![12, 12, 1, 0]);
    }

    #[test]
    fn claims_use_camel_case_fields() {
        let claim: Claim =
            serde_json::from_str(r#"{"kind":"mates","side":"white","mateIn":2,"line":0}"#).unwrap();
        assert_eq!(
            claim,
            Claim::Mates {
                side: Side::White,
                mate_in: 2,
                line: 0
            }
        );
        let run = NarrationRun::Failed {
            chunks_done: 1,
            failure: NarrationFailure::NoKey,
        };
        assert_eq!(
            serde_json::to_value(&run).unwrap(),
            serde_json::json!({ "status": "failed", "chunksDone": 1, "failure": { "kind": "noKey" } })
        );
    }
}
