//! Facts: what was counted about a move, from positions and engine lines. Each extractor is a
//! pure function of positions; nothing here touches an engine, a database or app state.

pub mod geometry;
pub mod motifs;
pub mod threats;
pub mod types;

#[cfg(test)]
mod fixtures;

pub use types::*;
