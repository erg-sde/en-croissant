use std::{
    fmt::Display,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use derivative::Derivative;
use governor::{Quota, RateLimiter};
use log::{info, warn};
use nonzero_ext::*;
use serde::{Deserialize, Serialize};
use shakmaty::{
    fen::Fen, san::SanPlus, uci::UciMove, ByColor, CastlingMode, Chess, Color, EnPassantMode,
    Position, Role,
};
use specta::Type;
use tauri_specta::Event;
use tokio::sync::{watch, Mutex};
use vampirc_uci::{
    parse_one,
    uci::{Score, ScoreValue},
    UciInfoAttribute, UciMessage, UciOptionConfig,
};

use crate::{
    db::{is_position_in_db, GameQuery, PositionQueryJs},
    engine::{
        parse_fen_and_apply_moves, BaseEngine, EngineLog, EngineOption, EngineReader, GoMode,
    },
    error::Error,
    jobs::is_cancelled,
    progress::{AppProgress, ProgressGuard, ProgressStage},
    AppState,
};

pub struct EngineProcess {
    base: BaseEngine,
    last_depth: u32,
    best_moves: Vec<BestMoves>,
    last_best_moves: Vec<BestMoves>,
    last_progress: f32,
    options: EngineOptions,
    go_mode: GoMode,
    running: bool,
    real_multipv: u16,
    start: Instant,
}

impl EngineProcess {
    async fn new(path: PathBuf) -> Result<(Self, EngineReader), Error> {
        let mut base = BaseEngine::spawn(path).await?;
        base.init_uci().await?;
        let reader = base.take_reader().ok_or(Error::EngineDisconnected)?;

        Ok((
            Self {
                base,
                last_depth: 0,
                best_moves: Vec::new(),
                last_best_moves: Vec::new(),
                last_progress: 0.0,
                options: EngineOptions::default(),
                real_multipv: 0,
                go_mode: GoMode::Infinite,
                running: false,
                start: Instant::now(),
            },
            reader,
        ))
    }

    async fn set_option<T>(&mut self, name: &str, value: T) -> Result<(), Error>
    where
        T: Display,
    {
        self.base.set_option(name, value).await
    }

    async fn set_options(&mut self, options: EngineOptions) -> Result<(), Error> {
        let fen_changed = options.fen != self.options.fen;
        let fen: Fen = options.fen.parse()?;
        let setup = fen.as_setup();
        let castling_mode = CastlingMode::detect(setup);
        let pos = parse_fen_and_apply_moves(&options.fen, &options.moves)?;

        if fen_changed {
            if castling_mode.is_chess960() {
                self.set_option("UCI_Chess960", "true").await?;
            } else {
                self.set_option("UCI_Chess960", "false").await?;
            }
        }

        let multipv = options
            .extra_options
            .iter()
            .find(|x| x.name == "MultiPV")
            .map(|x| x.value.parse().unwrap_or(1))
            .unwrap_or(1);

        self.real_multipv = multipv.min(pos.legal_moves().len() as u16);

        for option in &options.extra_options {
            if !self.options.extra_options.contains(option) && option.name != "UCI_Chess960" {
                self.set_option(&option.name, &option.value).await?;
            }
        }

        if fen_changed || options.moves != self.options.moves {
            self.set_position(&options.fen, &options.moves).await?;
        }
        self.last_depth = 0;
        self.options = options.clone();
        self.best_moves.clear();
        self.last_best_moves.clear();
        Ok(())
    }

    async fn set_position(&mut self, fen: &str, moves: &[String]) -> Result<(), Error> {
        self.base.set_position(fen, moves).await?;
        self.options.fen = fen.to_string();
        self.options.moves = moves.to_owned();
        Ok(())
    }

    async fn go(&mut self, mode: &GoMode) -> Result<(), Error> {
        self.go_mode = mode.clone();
        self.base.go(mode).await?;
        self.running = true;
        self.start = Instant::now();
        Ok(())
    }

    async fn stop(&mut self) -> Result<(), Error> {
        self.base.stop().await?;
        self.running = false;
        Ok(())
    }

    async fn kill(&mut self) -> Result<(), Error> {
        self.base.quit().await?;
        self.running = false;
        Ok(())
    }

    pub fn kill_sync(&mut self) {
        self.base.kill_sync();
    }
}

#[derive(Clone, Serialize, Debug, Derivative, Type)]
#[derivative(Default)]
pub struct BestMoves {
    nodes: u32,
    depth: u32,
    score: Score,
    #[serde(rename = "uciMoves")]
    uci_moves: Vec<String>,
    #[serde(rename = "sanMoves")]
    san_moves: Vec<String>,
    #[derivative(Default(value = "1"))]
    multipv: u16,
    nps: u32,
}

#[derive(Serialize, Debug, Clone, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct BestMovesPayload {
    pub best_lines: Vec<BestMoves>,
    pub engine: String,
    pub tab: String,
    pub fen: String,
    pub moves: Vec<String>,
    pub progress: f64,
}

fn invert_score(score: Score) -> Score {
    let new_value = match score.value {
        ScoreValue::Cp(x) => ScoreValue::Cp(-x),
        ScoreValue::Mate(x) => ScoreValue::Mate(-x),
    };
    let new_wdl = score.wdl.map(|(w, d, l)| (l, d, w));
    Score {
        value: new_value,
        wdl: new_wdl,
        ..score
    }
}

/// Whether a UCI score is exact or a fail-high/fail-low bound from an interrupted search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bound {
    Exact,
    Lower,
    Upper,
}

/// One parsed `info` line. Bound lines carry an unreliable score and a truncated PV.
#[derive(Debug)]
struct ParsedLine {
    line: BestMoves,
    bound: Bound,
}

fn parse_uci_attrs(
    attrs: Vec<UciInfoAttribute>,
    fen: &Fen,
    moves: &[String],
) -> Result<ParsedLine, Error> {
    let mut best_moves = BestMoves::default();

    let mut pos = parse_fen_and_apply_moves(&fen.to_string(), moves)?;
    let turn = pos.turn();

    for a in attrs {
        match a {
            UciInfoAttribute::Pv(m) => {
                for mv in m {
                    let uci: UciMove = mv.to_string().parse()?;
                    let m = uci.to_move(&pos)?;
                    let san = SanPlus::from_move_and_play_unchecked(&mut pos, &m);
                    best_moves.san_moves.push(san.to_string());
                    best_moves.uci_moves.push(uci.to_string());
                }
            }
            UciInfoAttribute::Nps(nps) => {
                best_moves.nps = nps as u32;
            }
            UciInfoAttribute::Nodes(nodes) => {
                best_moves.nodes = nodes as u32;
            }
            UciInfoAttribute::Depth(depth) => {
                best_moves.depth = depth;
            }
            UciInfoAttribute::MultiPv(multipv) => {
                best_moves.multipv = multipv;
            }
            UciInfoAttribute::Score(score) => {
                best_moves.score = score;
            }
            _ => (),
        }
    }

    if best_moves.san_moves.is_empty() {
        return Err(Error::NoMovesFound);
    }

    let bound = if best_moves.score.lower_bound == Some(true) {
        Bound::Lower
    } else if best_moves.score.upper_bound == Some(true) {
        Bound::Upper
    } else {
        Bound::Exact
    };

    if turn == Color::Black {
        best_moves.score = invert_score(best_moves.score);
    }

    Ok(ParsedLine {
        line: best_moves,
        bound,
    })
}

#[derive(Deserialize, Debug, Clone, Type, Derivative, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
#[derivative(Default)]
pub struct EngineOptions {
    pub fen: String,
    pub moves: Vec<String>,
    pub extra_options: Vec<EngineOption>,
}

#[tauri::command]
#[specta::specta]
pub async fn kill_engines(tab: String, state: tauri::State<'_, AppState>) -> Result<(), Error> {
    let keys: Vec<_> = state
        .engine_processes
        .iter()
        .map(|x| x.key().clone())
        .collect();
    for key in keys.clone() {
        if key.0.starts_with(&tab) {
            {
                let process = state.engine_processes.get_mut(&key).unwrap();
                let mut process = process.lock().await;
                process.kill().await?;
            }
            state.engine_processes.remove(&key);
        }
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn kill_engine(
    engine: String,
    tab: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    let key = (tab, engine);
    if let Some(process) = state.engine_processes.get(&key) {
        let mut process = process.lock().await;
        process.kill().await?;
    }
    Ok(())
}
#[tauri::command]
#[specta::specta]
pub async fn stop_engine(
    engine: String,
    tab: String,
    state: tauri::State<'_, AppState>,
) -> Result<(), Error> {
    let key = (tab, engine);
    if let Some(process) = state.engine_processes.get(&key) {
        let mut process = process.lock().await;
        process.stop().await?;
    }
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn get_engine_logs(
    engine: String,
    tab: String,
    state: tauri::State<'_, AppState>,
) -> Result<Vec<EngineLog>, Error> {
    let key = (tab, engine);
    if let Some(process) = state.engine_processes.get(&key) {
        let process = process.lock().await;
        Ok(process.base.get_logs())
    } else {
        Ok(Vec::new())
    }
}

#[tauri::command]
#[specta::specta]
pub async fn get_best_moves(
    id: String,
    engine: String,
    tab: String,
    go_mode: GoMode,
    options: EngineOptions,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<(f32, Vec<BestMoves>)>, Error> {
    let path = PathBuf::from(&engine);

    let key = (tab.clone(), id.clone());

    if state.engine_processes.contains_key(&key) {
        {
            let process = state.engine_processes.get_mut(&key).unwrap();
            let mut process = process.lock().await;
            if options == process.options && go_mode == process.go_mode && process.running {
                return Ok(Some((
                    process.last_progress,
                    process.last_best_moves.clone(),
                )));
            }
            process.stop().await?;
        }
        // give time for engine to stop and process previous lines
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        {
            let process = state.engine_processes.get_mut(&key).unwrap();
            let mut process = process.lock().await;
            process.set_options(options.clone()).await?;
            process.go(&go_mode).await?;
        }
        return Ok(None);
    }

    let (mut process, mut reader) = EngineProcess::new(path).await?;
    process.set_options(options.clone()).await?;
    process.go(&go_mode).await?;

    let process = Arc::new(Mutex::new(process));

    state.engine_processes.insert(key.clone(), process.clone());

    let lim = RateLimiter::direct(Quota::per_second(nonzero!(5u32)));

    while let Some(line) = reader.next_line().await? {
        let mut proc = process.lock().await;
        match parse_one(&line) {
            UciMessage::Info(attrs) => {
                match parse_uci_attrs(attrs, &proc.options.fen.parse()?, &proc.options.moves) {
                    Ok(ParsedLine {
                        line: best_moves,
                        bound,
                    }) => {
                        if bound != Bound::Exact {
                            continue;
                        }
                        let multipv = best_moves.multipv;
                        let cur_depth = best_moves.depth;
                        let cur_nodes = best_moves.nodes;
                        if multipv as usize == proc.best_moves.len() + 1 {
                            proc.best_moves.push(best_moves);
                            if multipv == proc.real_multipv {
                                if proc.best_moves.iter().all(|x| x.depth == cur_depth)
                                    && cur_depth >= proc.last_depth
                                    && lim.check().is_ok()
                                {
                                    let progress = match proc.go_mode {
                                        GoMode::Depth(depth) => {
                                            (cur_depth as f64 / depth as f64) * 100.0
                                        }
                                        GoMode::Time(time) => {
                                            (proc.start.elapsed().as_millis() as f64 / time as f64)
                                                * 100.0
                                        }
                                        GoMode::Nodes(nodes) => {
                                            (cur_nodes as f64 / nodes as f64) * 100.0
                                        }
                                        GoMode::PlayersTime(_) => 99.99,
                                        GoMode::Infinite => 99.99,
                                    };
                                    BestMovesPayload {
                                        best_lines: proc.best_moves.clone(),
                                        engine: id.clone(),
                                        tab: tab.clone(),
                                        fen: proc.options.fen.clone(),
                                        moves: proc.options.moves.clone(),
                                        progress,
                                    }
                                    .emit(&app)?;
                                    proc.last_depth = cur_depth;
                                    proc.last_best_moves = proc.best_moves.clone();
                                    proc.last_progress = progress as f32;
                                }
                                proc.best_moves.clear();
                            }
                        }
                    }
                    Err(e) => match e {
                        Error::NoMovesFound => {}
                        _ => {
                            warn!("Failed to parse info line: {}, error: {:?}", line, e);
                        }
                    },
                }
            }
            UciMessage::BestMove { .. } => {
                BestMovesPayload {
                    best_lines: proc.last_best_moves.clone(),
                    engine: id.clone(),
                    tab: tab.clone(),
                    fen: proc.options.fen.clone(),
                    moves: proc.options.moves.clone(),
                    progress: 100.0,
                }
                .emit(&app)?;
                proc.last_progress = 100.0;
            }
            _ => {}
        }
        proc.base.log_engine(&line);
    }
    info!("Engine process finished: tab: {}, engine: {}", tab, engine);
    state.engine_processes.remove(&key);
    Ok(None)
}

#[derive(Serialize, Debug, Default, Type)]
pub struct MoveAnalysis {
    best: Vec<BestMoves>,
    novelty: bool,
    is_sacrifice: bool,
    /// The position that was analysed: the root for index 0, otherwise the position after move i.
    fen: String,
    /// The game is over in this position; it was not searched and `best` is empty.
    terminal: bool,
}

#[derive(Deserialize, Debug, Default, Type)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisOptions {
    pub fen: String,
    pub moves: Vec<String>,
    pub annotate_novelties: bool,
    pub reference_db: Option<PathBuf>,
    pub reversed: bool,
    /// Lines searched per position, clamped to 2..=4. Defaults to 2.
    #[serde(default)]
    pub multipv: Option<u16>,
}

const DEFAULT_REPORT_MULTIPV: u16 = 2;

fn report_multipv(requested: Option<u16>) -> u16 {
    requested.unwrap_or(DEFAULT_REPORT_MULTIPV).clamp(2, 4)
}

/// One position of a report: the position reached by `moves` from the root.
#[derive(Debug)]
struct ReportPosition {
    fen: Fen,
    moves: Vec<String>,
    is_sacrifice: bool,
    terminal: bool,
}

/// The root and the position after each move, in game order. FENs are normalized
/// (`EnPassantMode::Legal`, counters as replayed), and game-over positions are included and marked
/// so the report keeps one entry per position.
fn report_positions(fen: &str, moves: &[String]) -> Result<Vec<ReportPosition>, Error> {
    let setup = Fen::from_ascii(fen.as_bytes())?.into_setup();
    let castling_mode = CastlingMode::detect(&setup);
    let mut chess: Chess = setup.position(castling_mode)?;

    let mut positions = vec![ReportPosition {
        fen: Fen::from_position(chess.clone(), EnPassantMode::Legal),
        moves: vec![],
        is_sacrifice: false,
        terminal: chess.is_game_over(),
    }];
    for (i, m) in moves.iter().enumerate() {
        let m = UciMove::from_ascii(m.as_bytes())?.to_move(&chess)?;
        let previous_pos = chess.clone();
        chess.play_unchecked(&m);
        let terminal = chess.is_game_over();
        let is_sacrifice = !terminal && naive_eval(&previous_pos) > -naive_eval(&chess) + 100;
        positions.push(ReportPosition {
            fen: Fen::from_position(chess.clone(), EnPassantMode::Legal),
            moves: moves[..=i].to_vec(),
            is_sacrifice,
            terminal,
        });
    }
    Ok(positions)
}

/// Assembles complete MultiPV sets from a stream of `info` lines, keeping exact lines only.
///
/// A set is complete when lines `1..=real_multipv` arrive in order at the same depth. The result
/// is the last complete set at a depth no lower than any earlier complete set, so a search that
/// ends on a bound or a partial set still reports the last exact one.
struct MultiPvCollector {
    real_multipv: usize,
    pending: Vec<BestMoves>,
    last_depth: u32,
    best: Vec<BestMoves>,
}

impl MultiPvCollector {
    fn new(real_multipv: u16) -> Self {
        Self {
            real_multipv: real_multipv as usize,
            pending: Vec::new(),
            last_depth: 0,
            best: Vec::new(),
        }
    }

    fn push(&mut self, parsed: ParsedLine) {
        if parsed.bound != Bound::Exact {
            return;
        }
        let line = parsed.line;
        if line.multipv == 1 {
            self.pending.clear();
        }
        if line.multipv as usize != self.pending.len() + 1 {
            return;
        }
        let depth = line.depth;
        self.pending.push(line);
        if self.pending.len() == self.real_multipv {
            if self.pending.iter().all(|x| x.depth == depth) && depth >= self.last_depth {
                self.best = std::mem::take(&mut self.pending);
                self.last_depth = depth;
            } else {
                self.pending.clear();
            }
        }
    }

    fn into_best(self) -> Vec<BestMoves> {
        self.best
    }
}

/// How long a stopped engine gets to print its final `bestmove` before the search is abandoned.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// Runs one search on the position already set on `proc` and returns the last complete exact
/// MultiPV set (empty if the engine never produced one).
///
/// Cancellation is observed inside the read loop: the engine is told to `stop`, its output is
/// drained up to `bestmove`, and the search returns `Error::AnalysisCancelled`.
async fn search_once(
    proc: &mut EngineProcess,
    reader: &mut EngineReader,
    go_mode: &GoMode,
    cancel: &mut watch::Receiver<bool>,
) -> Result<Vec<BestMoves>, Error> {
    if matches!(go_mode, GoMode::Infinite) {
        return Err(Error::InvalidGoMode);
    }
    if is_cancelled(cancel) {
        return Err(Error::AnalysisCancelled);
    }

    let fen: Fen = proc.options.fen.parse()?;
    let moves = proc.options.moves.clone();
    let mut collector = MultiPvCollector::new(proc.real_multipv);

    proc.go(go_mode).await?;
    loop {
        tokio::select! {
            line = reader.next_line() => {
                let Some(line) = line? else {
                    return Err(Error::EngineDisconnected);
                };
                match parse_one(&line) {
                    UciMessage::Info(attrs) => {
                        if let Ok(parsed) = parse_uci_attrs(attrs, &fen, &moves) {
                            collector.push(parsed);
                        }
                    }
                    UciMessage::BestMove { .. } => return Ok(collector.into_best()),
                    _ => {}
                }
            }
            _ = cancel.changed() => {
                proc.stop().await?;
                let drained = tokio::time::timeout(STOP_GRACE, async {
                    while let Some(line) = reader.next_line().await? {
                        if matches!(parse_one(&line), UciMessage::BestMove { .. }) {
                            break;
                        }
                    }
                    Ok::<(), Error>(())
                })
                .await;
                if let Ok(Err(e)) = drained {
                    warn!("Engine output failed while stopping a search: {:?}", e);
                }
                return Err(Error::AnalysisCancelled);
            }
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn cancel_analysis(id: String, state: tauri::State<'_, AppState>) -> Result<(), Error> {
    state.jobs.cancel(&id);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn analyze_game(
    id: String,
    engine: String,
    go_mode: GoMode,
    options: AnalysisOptions,
    uci_options: Vec<EngineOption>,
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Vec<MoveAnalysis>, Error> {
    let guard = ProgressGuard::start(
        AppProgress {
            store: &state.progress_state,
            app: &app,
        },
        &state.jobs,
        id,
        ProgressStage::Engine,
    );
    let mut cancel = guard.cancel_token();

    let mut positions = report_positions(&options.fen, &options.moves)?;
    if options.reversed {
        positions.reverse();
    }

    let path = PathBuf::from(&engine);
    let mut analysis: Vec<MoveAnalysis> = Vec::new();

    let (mut proc, mut reader) = EngineProcess::new(path).await?;

    let mut extra_options: Vec<EngineOption> = uci_options
        .into_iter()
        .filter(|x| x.name != "MultiPV")
        .collect();
    extra_options.push(EngineOption {
        name: "MultiPV".to_string(),
        value: report_multipv(options.multipv).to_string(),
    });

    let mut novelty_found = false;

    for (i, position) in positions.iter().enumerate() {
        guard.progress((i as f32 / positions.len() as f32) * 100.0)?;

        if position.terminal {
            analysis.push(MoveAnalysis {
                terminal: true,
                ..Default::default()
            });
            continue;
        }

        proc.set_options(EngineOptions {
            fen: options.fen.clone(),
            moves: position.moves.clone(),
            extra_options: extra_options.clone(),
        })
        .await?;

        match search_once(&mut proc, &mut reader, &go_mode, &mut cancel).await {
            Ok(best) => analysis.push(MoveAnalysis {
                best,
                ..Default::default()
            }),
            Err(Error::AnalysisCancelled) => {
                if let Err(e) = proc.kill().await {
                    warn!("Failed to stop the analysis engine: {:?}", e);
                }
                guard.cancelled()?;
                return Err(Error::AnalysisCancelled);
            }
            Err(e) => return Err(e),
        }
    }

    if options.reversed {
        analysis.reverse();
        positions.reverse();
    }

    for (analysis, position) in analysis.iter_mut().zip(&positions) {
        analysis.fen = position.fen.to_string();
        analysis.is_sacrifice = position.is_sacrifice;
        if position.terminal {
            continue;
        }

        // let query = PositionQuery::exact_from_fen(&fen.to_string())?;
        let query = PositionQueryJs {
            fen: analysis.fen.clone(),
            type_: "exact".to_string(),
        };

        if options.annotate_novelties && !novelty_found {
            if let Some(reference) = options.reference_db.clone() {
                analysis.novelty = !is_position_in_db(
                    reference,
                    GameQuery::new().position(query.clone()).clone(),
                    state.clone(),
                )
                .await?;
                if analysis.novelty {
                    novelty_found = true;
                }
            } else {
                return Err(Error::MissingReferenceDatabase);
            }
        }
    }
    guard.finished()?;
    Ok(analysis)
}

fn count_material(position: &Chess) -> i32 {
    if position.is_checkmate() {
        return -10000;
    }
    let material: ByColor<i32> = position.board().material().map(|p| {
        p.pawn as i32 * piece_value(Role::Pawn)
            + p.knight as i32 * piece_value(Role::Knight)
            + p.bishop as i32 * piece_value(Role::Bishop)
            + p.rook as i32 * piece_value(Role::Rook)
            + p.queen as i32 * piece_value(Role::Queen)
    });
    if position.turn() == Color::White {
        material.white - material.black
    } else {
        material.black - material.white
    }
}

fn piece_value(role: Role) -> i32 {
    match role {
        Role::Pawn => 90,
        Role::Knight => 300,
        Role::Bishop => 300,
        Role::Rook => 500,
        Role::Queen => 1000,
        _ => 0,
    }
}

fn qsearch(position: &Chess, mut alpha: i32, beta: i32) -> i32 {
    let stand_pat = count_material(position);

    if stand_pat >= beta {
        return beta;
    }
    if alpha < stand_pat {
        alpha = stand_pat;
    }
    let legal_moves = position.legal_moves();
    let mut captures: Vec<_> = legal_moves.iter().filter(|m| m.is_capture()).collect();

    captures.sort_by(|a, b| {
        let a_value = piece_value(a.capture().unwrap());
        let b_value = piece_value(b.capture().unwrap());
        b_value.cmp(&a_value)
    });

    for capture in captures {
        let mut new_position = position.clone();
        new_position.play_unchecked(capture);
        let score = -qsearch(&new_position, -beta, -alpha);
        if score >= beta {
            return beta;
        }
        if score > alpha {
            alpha = score;
        }
    }

    alpha
}

fn naive_eval(pos: &Chess) -> i32 {
    pos.legal_moves()
        .iter()
        .map(|mv| {
            let mut new_position = pos.clone();
            new_position.play_unchecked(mv);
            -qsearch(&new_position, i32::MIN, i32::MAX)
        })
        .max()
        .unwrap_or(i32::MIN)
}

#[cfg(test)]
mod tests {
    use shakmaty::FromSetup;

    use super::*;

    fn pos(fen: &str) -> Chess {
        let fen: Fen = fen.parse().unwrap();
        Chess::from_setup(fen.into_setup(), CastlingMode::Chess960).unwrap()
    }

    #[test]
    fn eval_start_pos() {
        assert_eq!(naive_eval(&Chess::default()), 0);
    }

    #[test]
    fn eval_scandi() {
        let position = pos("rnbqkbnr/ppp1pppp/8/3p4/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2");
        assert_eq!(naive_eval(&position), 0);
    }

    #[test]
    fn eval_hanging_pawn() {
        let position = pos("r1bqkbnr/ppp1pppp/2n5/1B1p4/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 3 3");
        assert_eq!(naive_eval(&position), 100);
    }

    #[test]
    fn eval_complex_center() {
        let position = pos("r1bqkbnr/ppp2ppp/2n5/1B1pp3/4P3/5N2/PPPP1PPP/RNBQK2R w KQkq - 0 4");
        assert_eq!(naive_eval(&position), 100);
    }

    #[test]
    fn eval_in_check() {
        let position = pos("r1bqkbnr/ppp2ppp/2B5/3pp3/4P3/5N2/PPPP1PPP/RNBQK2R b KQkq - 0 4");
        assert_eq!(naive_eval(&position), -100);
    }

    #[test]
    fn eval_rook_stack() {
        let position = pos("rnrq4/8/8/1R6/1R6/1R5K/1Q6/7k w - - 0 1");
        assert_eq!(naive_eval(&position), 500);
    }

    #[test]
    fn eval_rook_stack2() {
        let position = pos("rnrq4/8/8/1R6/1Q6/1R5K/1R6/7k w - - 0 1");
        assert_eq!(naive_eval(&position), 200);
    }

    #[test]
    fn eval_opera_game1() {
        let position = pos("4kb1r/p2rqppp/5n2/1B2p1B1/4P3/1Q6/PPP2PPP/2K4R w k - 0 14");
        assert_eq!(naive_eval(&position), -100);
    }

    #[test]
    fn eval_opera_game2() {
        let position = pos("4kb1r/p2rqppp/5n2/1B2p1B1/4P3/1Q6/PPP2PPP/2KR4 b k - 1 14");
        assert_eq!(naive_eval(&position), 0);
    }

    const START: &str = "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1";

    fn parse(line: &str) -> ParsedLine {
        let UciMessage::Info(attrs) = parse_one(line) else {
            panic!("not an info line: {line}");
        };
        parse_uci_attrs(attrs, &START.parse().unwrap(), &[]).unwrap()
    }

    fn collect(multipv: u16, lines: &[&str]) -> Vec<BestMoves> {
        let mut collector = MultiPvCollector::new(multipv);
        for line in lines {
            collector.push(parse(line));
        }
        collector.into_best()
    }

    fn depths(best: &[BestMoves]) -> Vec<(u16, u32)> {
        best.iter().map(|b| (b.multipv, b.depth)).collect()
    }

    #[test]
    fn parse_reads_bound_flags() {
        let line = "info depth 12 multipv 1 score cp 31 pv e2e4 e7e5";
        assert_eq!(parse(line).bound, Bound::Exact);
        let line = "info depth 12 multipv 1 score cp 31 lowerbound pv e2e4";
        assert_eq!(parse(line).bound, Bound::Lower);
        let line = "info depth 12 multipv 1 score cp 31 upperbound pv e2e4";
        assert_eq!(parse(line).bound, Bound::Upper);
    }

    #[test]
    fn collector_keeps_last_complete_set() {
        let best = collect(
            2,
            &[
                "info depth 10 multipv 1 score cp 30 pv e2e4 e7e5",
                "info depth 10 multipv 2 score cp 25 pv d2d4 d7d5",
                "info depth 11 multipv 1 score cp 28 pv e2e4 c7c5",
                "info depth 11 multipv 2 score cp 20 pv g1f3 d7d5",
            ],
        );
        assert_eq!(depths(&best), vec![(1, 11), (2, 11)]);
        assert_eq!(best[0].san_moves, vec!["e4", "c5"]);
    }

    #[test]
    fn collector_drops_bound_only_sets() {
        let best = collect(
            2,
            &[
                "info depth 10 multipv 1 score cp 30 lowerbound pv e2e4",
                "info depth 10 multipv 2 score cp 25 upperbound pv d2d4",
            ],
        );
        assert!(best.is_empty());
    }

    #[test]
    fn collector_keeps_exact_set_when_search_stops_on_a_bound() {
        let best = collect(
            2,
            &[
                "info depth 10 multipv 1 score cp 30 pv e2e4 e7e5",
                "info depth 10 multipv 2 score cp 25 pv d2d4 d7d5",
                "info depth 11 multipv 1 score cp 90 lowerbound pv e2e4",
                "info depth 11 multipv 2 score cp 20 pv g1f3 d7d5",
            ],
        );
        assert_eq!(depths(&best), vec![(1, 10), (2, 10)]);
    }

    #[test]
    fn collector_survives_irregular_multipv_sequence() {
        let best = collect(
            2,
            &[
                "info depth 10 multipv 1 score cp 30 pv e2e4 e7e5",
                "info depth 10 multipv 2 score cp 25 pv d2d4 d7d5",
                "info depth 11 multipv 2 score cp 20 pv g1f3 d7d5",
                "info depth 11 multipv 1 score cp 28 pv e2e4 c7c5",
                "info depth 12 multipv 1 score cp 27 pv e2e4 e7e5",
            ],
        );
        assert_eq!(depths(&best), vec![(1, 10), (2, 10)]);
    }

    #[test]
    fn collector_rejects_mixed_depth_sets() {
        let best = collect(
            2,
            &[
                "info depth 10 multipv 1 score cp 30 pv e2e4 e7e5",
                "info depth 9 multipv 2 score cp 25 pv d2d4 d7d5",
            ],
        );
        assert!(best.is_empty());
    }

    fn uci(moves: &str) -> Vec<String> {
        moves.split_whitespace().map(String::from).collect()
    }

    #[test]
    fn report_positions_cover_root_and_every_move() {
        let positions = report_positions(START, &uci("e2e4 e7e5 g1f3")).unwrap();
        let fens: Vec<String> = positions.iter().map(|p| p.fen.to_string()).collect();
        assert_eq!(
            fens,
            [
                START,
                "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1",
                "rnbqkbnr/pppp1ppp/8/4p3/4P3/8/PPPP1PPP/RNBQKBNR w KQkq - 0 2",
                "rnbqkbnr/pppp1ppp/8/4p3/4P3/5N2/PPPP1PPP/RNBQKB1R b KQkq - 1 2",
            ]
        );
        assert_eq!(positions[2].moves, uci("e2e4 e7e5"));
        assert!(positions.iter().all(|p| !p.terminal));
    }

    #[test]
    fn report_positions_normalize_the_root_fen() {
        // No counters and an en passant square without a legal capture.
        let root = "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq e3";
        let positions = report_positions(root, &[]).unwrap();
        assert_eq!(
            positions[0].fen.to_string(),
            "rnbqkbnr/pppppppp/8/8/4P3/8/PPPP1PPP/RNBQKBNR b KQkq - 0 1"
        );
    }

    #[test]
    fn report_positions_keep_the_final_game_over_position() {
        let positions = report_positions(START, &uci("f2f3 e7e5 g2g4 d8h4")).unwrap();
        assert_eq!(positions.len(), 5);
        assert!(positions[4].terminal);
        assert!(!positions[4].is_sacrifice);
        assert!(positions[..4].iter().all(|p| !p.terminal));
    }

    #[test]
    fn report_positions_reject_an_illegal_move() {
        assert!(report_positions(START, &uci("e2e5")).is_err());
    }

    #[test]
    fn report_multipv_defaults_to_two_and_is_clamped() {
        assert_eq!(report_multipv(None), 2);
        assert_eq!(report_multipv(Some(1)), 2);
        assert_eq!(report_multipv(Some(3)), 3);
        assert_eq!(report_multipv(Some(9)), 4);
    }

    /// A UCI engine for the tests below, from `EC_TEST_ENGINE`. Run them with
    /// `EC_TEST_ENGINE=/path/to/stockfish cargo test -- --ignored`.
    async fn test_engine(multipv: u16) -> (EngineProcess, EngineReader) {
        let path = std::env::var("EC_TEST_ENGINE").expect("EC_TEST_ENGINE is not set");
        let (mut proc, reader) = EngineProcess::new(PathBuf::from(path)).await.unwrap();
        proc.set_options(EngineOptions {
            fen: START.to_string(),
            moves: vec!["e2e4".to_string()],
            extra_options: vec![EngineOption {
                name: "MultiPV".to_string(),
                value: multipv.to_string(),
            }],
        })
        .await
        .unwrap();
        (proc, reader)
    }

    #[tokio::test]
    #[ignore = "needs a UCI engine in EC_TEST_ENGINE"]
    async fn search_once_returns_one_complete_exact_set() {
        let (mut proc, mut reader) = test_engine(3).await;
        let (_cancel, mut rx) = watch::channel(false);
        let best = search_once(&mut proc, &mut reader, &GoMode::Depth(12), &mut rx)
            .await
            .unwrap();
        assert_eq!(
            best.iter().map(|b| b.multipv).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert!(best
            .iter()
            .all(|b| b.depth == best[0].depth && b.depth >= 1));
        proc.kill().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs a UCI engine in EC_TEST_ENGINE"]
    async fn search_once_stops_inside_a_long_search() {
        let (mut proc, mut reader) = test_engine(2).await;
        let (cancel, mut rx) = watch::channel(false);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(300)).await;
            cancel.send_replace(true);
            // Keep the sender alive until the search has observed the value.
            tokio::time::sleep(Duration::from_secs(5)).await;
        });
        let started = Instant::now();
        let result = search_once(&mut proc, &mut reader, &GoMode::Depth(99), &mut rx).await;
        assert!(matches!(result, Err(Error::AnalysisCancelled)));
        assert!(started.elapsed() < Duration::from_secs(3));
        proc.kill().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "needs a UCI engine in EC_TEST_ENGINE"]
    async fn search_once_rejects_an_infinite_search() {
        let (mut proc, mut reader) = test_engine(2).await;
        let (_cancel, mut rx) = watch::channel(false);
        let result = search_once(&mut proc, &mut reader, &GoMode::Infinite, &mut rx).await;
        assert!(matches!(result, Err(Error::InvalidGoMode)));
        assert!(!proc.running);
        proc.kill().await.unwrap();
    }
}

#[derive(Type, Default, Serialize, Debug)]
pub struct EngineConfig {
    pub name: String,
    pub options: Vec<UciOptionConfig>,
}

#[tauri::command]
#[specta::specta]
pub async fn get_engine_config(path: PathBuf) -> Result<EngineConfig, Error> {
    let mut base = BaseEngine::spawn(path).await?;

    base.send("uci").await?;

    let mut config = EngineConfig::default();

    let reader = base.reader_mut().ok_or(Error::EngineDisconnected)?;
    while let Some(line) = reader.next_line().await? {
        if let UciMessage::Id {
            name: Some(name),
            author: _,
        } = parse_one(&line)
        {
            config.name = name;
        }
        if let UciMessage::Option(opt) = parse_one(&line) {
            config.options.push(opt);
        }
        if let UciMessage::UciOk = parse_one(&line) {
            break;
        }
    }
    println!("{:?}", config);
    base.quit().await?;
    Ok(config)
}
