//! "Explain this position": a short search on its own engine process for the position, its parent
//! and what the mover threatens, then the facts of the last move. Never uses the live analysis
//! process and takes no caller-supplied lines, so every line is exact and searched here.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use shakmaty::{fen::Fen, uci::UciMove, CastlingMode, Chess, Move, Position};
use specta::Type;
use tokio::sync::watch;

use super::facts::{extract_move_facts, threats::null_move_fen, FactsContext, MoveFacts};
use crate::{
    chess::{search_once, BestMoves, EngineOptions, EngineProcess},
    engine::{EngineOption, GoMode},
    error::Error,
    jobs::JobRegistry,
    AppState,
};

#[derive(Deserialize, Debug, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExplainOptions {
    pub fen: String,
    /// UCI moves from `fen` to the position to explain.
    pub moves: Vec<String>,
    /// Lines per position, clamped to 1..=5.
    pub multipv: u16,
    pub with_threat: bool,
    /// Engine threads; 1 when unset.
    pub threads: Option<u16>,
}

#[derive(Serialize, Debug, Clone, Type)]
#[serde(rename_all = "camelCase")]
pub struct PositionExplanation {
    /// The parent position's lines (empty for the root).
    pub parent_lines: Vec<BestMoves>,
    /// This position's lines (empty when it is game over).
    pub lines: Vec<BestMoves>,
    /// What the side that just moved threatens, when searched.
    pub threat: Option<BestMoves>,
    /// The facts of the last move; `None` for the root or without an exact line.
    pub facts: Option<MoveFacts>,
}

/// Unregisters a job when the command ends, however it ends.
struct JobGuard<'a> {
    jobs: &'a JobRegistry,
    id: String,
    rx: watch::Receiver<bool>,
}

impl Drop for JobGuard<'_> {
    fn drop(&mut self) {
        self.jobs.remove(&self.id, &self.rx);
    }
}

fn options_with(uci_options: &[EngineOption], multipv: u16, threads: u16) -> Vec<EngineOption> {
    let mut options: Vec<EngineOption> = uci_options
        .iter()
        .filter(|o| o.name != "MultiPV" && o.name != "Threads")
        .cloned()
        .collect();
    options.push(EngineOption {
        name: "MultiPV".into(),
        value: multipv.to_string(),
    });
    options.push(EngineOption {
        name: "Threads".into(),
        value: threads.to_string(),
    });
    options
}

/// The position before the last move, that move, and the position after it.
fn replay(fen: &str, moves: &[String]) -> Result<(Option<(Chess, Move)>, Chess), Error> {
    let setup = Fen::from_ascii(fen.as_bytes())?.into_setup();
    let mode = CastlingMode::detect(&setup);
    let mut pos: Chess = setup.position(mode)?;
    let mut last = None;
    for uci in moves {
        let mv = UciMove::from_ascii(uci.as_bytes())?.to_move(&pos)?;
        let before = pos.clone();
        pos.play_unchecked(&mv);
        last = Some((before, mv));
    }
    Ok((last, pos))
}

/// One engine process and what every search on it shares.
struct Engine<'a> {
    proc: &'a mut EngineProcess,
    reader: &'a mut crate::engine::EngineReader,
    go_mode: &'a GoMode,
    cancel: &'a mut watch::Receiver<bool>,
}

impl Engine<'_> {
    async fn search(
        &mut self,
        fen: String,
        moves: Vec<String>,
        extra_options: Vec<EngineOption>,
    ) -> Result<Vec<BestMoves>, Error> {
        self.proc
            .set_options(EngineOptions {
                fen,
                moves,
                extra_options,
            })
            .await?;
        search_once(self.proc, self.reader, self.go_mode, self.cancel).await
    }
}

/// The searches and facts, on an engine process already started.
async fn explain(
    proc: &mut EngineProcess,
    reader: &mut crate::engine::EngineReader,
    go_mode: &GoMode,
    options: &ExplainOptions,
    uci_options: &[EngineOption],
    cancel: &mut watch::Receiver<bool>,
) -> Result<PositionExplanation, Error> {
    let (last, position) = replay(&options.fen, &options.moves)?;
    let threads = options.threads.unwrap_or(1).max(1);
    let lines_options = options_with(uci_options, options.multipv.clamp(1, 5), threads);
    let mut engine = Engine {
        proc,
        reader,
        go_mode,
        cancel,
    };

    let parent_lines = match options.moves.split_last() {
        Some((_, parent)) => {
            engine
                .search(options.fen.clone(), parent.to_vec(), lines_options.clone())
                .await?
        }
        None => vec![],
    };
    let terminal = position.is_game_over();
    let lines = if terminal {
        vec![]
    } else {
        engine
            .search(options.fen.clone(), options.moves.clone(), lines_options)
            .await?
    };
    let threat = match null_move_fen(&position) {
        Some(fen) if options.with_threat && !terminal && last.is_some() => {
            let one = options_with(uci_options, 1, threads);
            engine.search(fen, vec![], one).await?.into_iter().next()
        }
        _ => None,
    };
    let facts = last.as_ref().and_then(|(before, mv)| {
        extract_move_facts(&FactsContext {
            index: options.moves.len() as u32,
            before,
            mv,
            parent_lines: &parent_lines,
            own_lines: &lines,
            opening: None,
            prev: None,
            time: None,
            terminal,
            novelty: false,
            threat_line: threat.as_ref(),
        })
    });
    Ok(PositionExplanation {
        parent_lines,
        lines,
        threat,
        facts,
    })
}

#[tauri::command]
#[specta::specta]
pub async fn explain_position(
    id: String,
    engine: String,
    go_mode: GoMode,
    options: ExplainOptions,
    uci_options: Vec<EngineOption>,
    state: tauri::State<'_, AppState>,
) -> Result<PositionExplanation, Error> {
    if matches!(go_mode, GoMode::Infinite) {
        return Err(Error::InvalidGoMode);
    }
    let mut cancel = state.jobs.insert(&id);
    let _job = JobGuard {
        jobs: &state.jobs,
        id,
        rx: cancel.clone(),
    };
    let (mut proc, mut reader) = EngineProcess::new(PathBuf::from(engine)).await?;
    let result = explain(
        &mut proc,
        &mut reader,
        &go_mode,
        &options,
        &uci_options,
        &mut cancel,
    )
    .await;
    // Dropping the process kills it too; quitting first lets the engine exit cleanly.
    if let Err(e) = proc.kill().await {
        log::warn!("Failed to stop the explain engine: {:?}", e);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    const BEFORE_NF6: &str = "r1bqkbnr/pppp1ppp/2n5/4p2Q/2B1P3/8/PPPP1PPP/RNB1K1NR b KQkq - 3 3";

    #[test]
    fn replay_returns_the_last_move_and_its_position() {
        let (last, after) = replay(BEFORE_NF6, &["g8f6".into()]).unwrap();
        let (before, mv) = last.unwrap();
        assert_eq!(before.turn(), shakmaty::Color::Black);
        assert_eq!(mv.to(), shakmaty::Square::F6);
        assert_eq!(after.turn(), shakmaty::Color::White);
        assert!(replay(BEFORE_NF6, &[]).unwrap().0.is_none());
        assert!(replay(BEFORE_NF6, &["e2e4".into()]).is_err());
    }

    #[test]
    fn engine_options_replace_multipv_and_threads() {
        let options = options_with(
            &[
                EngineOption {
                    name: "Hash".into(),
                    value: "64".into(),
                },
                EngineOption {
                    name: "Threads".into(),
                    value: "8".into(),
                },
            ],
            3,
            1,
        );
        let pairs: Vec<_> = options
            .iter()
            .map(|o| (o.name.as_str(), o.value.as_str()))
            .collect();
        assert_eq!(pairs, [("Hash", "64"), ("MultiPV", "3"), ("Threads", "1")]);
    }

    #[tokio::test]
    #[ignore = "needs a UCI engine in EC_TEST_ENGINE"]
    async fn explains_a_blunder_with_its_refutation_and_a_threat_after_a_good_move() {
        let path = std::env::var("EC_TEST_ENGINE").expect("EC_TEST_ENGINE is not set");
        let (mut proc, mut reader) = EngineProcess::new(PathBuf::from(path)).await.unwrap();
        let (_keep, mut cancel) = watch::channel(false);
        let options = ExplainOptions {
            fen: BEFORE_NF6.into(),
            moves: vec!["g8f6".into()],
            multipv: 2,
            with_threat: true,
            threads: Some(1),
        };
        let explained = explain(
            &mut proc,
            &mut reader,
            &GoMode::Depth(10),
            &options,
            &[],
            &mut cancel,
        )
        .await
        .unwrap();
        let facts = explained.facts.unwrap();
        assert_eq!(explained.parent_lines.len(), 2);
        assert!(facts.refutation.is_some());
        // White to move threatens nothing new after a blunder that allows mate, but a threat line
        // was still searched for the position after Nf6.
        assert!(explained.threat.is_some());
        assert!(facts.threat.is_some());
        proc.kill().await.unwrap();
    }
}
