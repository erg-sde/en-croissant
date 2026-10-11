//! Clock facts from the `[%clk]` times stored in the PGN.

use super::types::TimeFacts;

/// Below this many seconds a move is played under time pressure.
const PRESSURE_SECONDS: f32 = 20.0;
/// Or below this share of the initial time.
const PRESSURE_SHARE: f32 = 0.1;

/// The clocks of a game, aligned to its moves: `clocks[k]` is the mover's remaining time in seconds
/// after move `k` (0-based), when the PGN recorded it.
pub struct Clocks<'a> {
    pub clocks: &'a [Option<f32>],
    pub increment_s: Option<f32>,
    pub initial_s: Option<f32>,
}

/// Time facts for move `k` (0-based). `None` when the PGN has no clock for it.
pub fn time_facts(clocks: &Clocks, k: usize) -> Option<TimeFacts> {
    let remaining_s = (*clocks.clocks.get(k)?)?;
    // The same player's previous clock is two moves back; for their first move it is the
    // initial time.
    let previous = match k.checked_sub(2) {
        Some(prev) => clocks.clocks.get(prev).copied().flatten(),
        None => clocks.initial_s,
    };
    let spent_s = previous.map(|prev| prev - remaining_s + clocks.increment_s.unwrap_or(0.0));
    let under_pressure = remaining_s < PRESSURE_SECONDS
        || clocks
            .initial_s
            .is_some_and(|initial| remaining_s < PRESSURE_SHARE * initial);
    Some(TimeFacts {
        remaining_s,
        spent_s,
        under_pressure,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLOCKS: [Option<f32>; 5] = [Some(178.0), Some(179.0), Some(150.0), None, Some(12.0)];

    fn clocks() -> Clocks<'static> {
        Clocks {
            clocks: &CLOCKS,
            increment_s: Some(2.0),
            initial_s: Some(180.0),
        }
    }

    #[test]
    fn spent_time_counts_the_increment() {
        let first = time_facts(&clocks(), 0).unwrap();
        assert_eq!(first.remaining_s, 178.0);
        assert_eq!(first.spent_s, Some(4.0));
        assert!(!first.under_pressure);

        let third = time_facts(&clocks(), 2).unwrap();
        assert_eq!(third.spent_s, Some(30.0));
    }

    #[test]
    fn a_missing_clock_gives_no_facts_and_no_spent_time_after_it() {
        assert_eq!(time_facts(&clocks(), 3), None);
        // Move 4's previous clock (move 2) is known; move 5 would have none.
        assert_eq!(
            time_facts(&clocks(), 4).unwrap().spent_s,
            Some(150.0 - 12.0 + 2.0)
        );
        assert_eq!(time_facts(&clocks(), 9), None);
    }

    #[test]
    fn pressure_by_seconds_or_share_of_the_initial_time() {
        assert!(time_facts(&clocks(), 4).unwrap().under_pressure);
        let long = Clocks {
            clocks: &[Some(400.0)],
            increment_s: None,
            initial_s: Some(5400.0),
        };
        assert!(time_facts(&long, 0).unwrap().under_pressure);
    }
}
