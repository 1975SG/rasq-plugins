//! generic totalistic 2D cellular automaton -- the "local-rule /
//! cellular-automaton-style exploration" item from
//! `docs/roadmap/Workbenches.md`'s emergence/discrete-simulation
//! category. Engineering-sandbox framing per that doc's own words, not
//! a physics claim. `Rule::LIFE` (Conway's Game of Life, B3/S23) is
//! the one concrete instantiation this crate ships and tests against --
//! real, independently-documented pattern behavior (block, blinker,
//! glider) to verify `Board::step` against, not just self-consistency
//! -- but the engine itself takes any birth/survive neighbor-count
//! rule, not hardcoded to Life specifically, per ADR-011.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rule {
    /// `birth[n]` -- a dead cell with exactly `n` live neighbors
    /// becomes alive next generation.
    pub birth: [bool; 9],
    /// `survive[n]` -- a live cell with exactly `n` live neighbors
    /// stays alive next generation.
    pub survive: [bool; 9],
}

impl Rule {
    /// conway's Game of Life -- born with exactly 3 live neighbors,
    /// survives with 2 or 3.
    pub const LIFE: Rule = Rule {
        birth: [false, false, false, true, false, false, false, false, false],
        survive: [false, false, true, true, false, false, false, false, false],
    };

    fn from_counts(counts: &[u8]) -> [bool; 9] {
        let mut mask = [false; 9];
        for &c in counts {
            if let Some(slot) = mask.get_mut(c as usize) {
                *slot = true;
            }
        }
        mask
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Board {
    pub width: usize,
    pub height: usize,
    cells: Vec<bool>,
}

fn wrap(coord: usize, delta: isize, size: usize) -> usize {
    (coord as isize + delta).rem_euclid(size as isize) as usize
}

impl Board {
    pub fn new(width: usize, height: usize) -> Self {
        Board { width, height, cells: vec![false; width * height] }
    }

    pub fn from_live_cells(width: usize, height: usize, live: &[(usize, usize)]) -> Self {
        let mut board = Self::new(width, height);
        for &(x, y) in live {
            board.set(x, y, true);
        }
        board
    }

    pub fn get(&self, x: usize, y: usize) -> bool {
        self.cells[y * self.width + x]
    }

    pub fn set(&mut self, x: usize, y: usize, alive: bool) {
        self.cells[y * self.width + x] = alive;
    }

    pub fn live_count(&self) -> usize {
        self.cells.iter().filter(|&&c| c).count()
    }

    pub fn live_cells(&self) -> Vec<(usize, usize)> {
        let mut cells = Vec::new();
        for y in 0..self.height {
            for x in 0..self.width {
                if self.get(x, y) {
                    cells.push((x, y));
                }
            }
        }
        cells
    }

    /// toroidal wrap (each edge connects to its own opposite edge) --
    /// keeps a moving pattern like a glider inside a finite board
    /// indefinitely instead of just dying at an edge, the standard
    /// convention for a finite Life-style demo board.
    fn live_neighbor_count(&self, x: usize, y: usize) -> u8 {
        let mut count = 0u8;
        for dy in [-1isize, 0, 1] {
            for dx in [-1isize, 0, 1] {
                if dx == 0 && dy == 0 {
                    continue;
                }
                if self.get(wrap(x, dx, self.width), wrap(y, dy, self.height)) {
                    count += 1;
                }
            }
        }
        count
    }

    /// one generation forward under `rule` -- a fresh `Board`, the
    /// previous generation is untouched (every real Life-style
    /// implementation computes the next generation from a frozen
    /// snapshot of the current one, never updates cells in place mid-
    /// step, or a cell's own already-updated neighbor would corrupt
    /// the count for cells visited later in the same pass).
    pub fn step(&self, rule: &Rule) -> Board {
        let mut next = Board::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let n = self.live_neighbor_count(x, y) as usize;
                let next_alive = if self.get(x, y) { rule.survive[n] } else { rule.birth[n] };
                next.set(x, y, next_alive);
            }
        }
        next
    }
}

/// what a bounded forward simulation found -- mirrors
/// `example-doctor-invariant-checker`'s own "not fully explored within
/// budget" honesty: `StillEvolving` isn't "nothing interesting is
/// happening", it's "no cycle shorter than the search window was
/// found within the generation budget", an honest limit, not a claim
/// the pattern never repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// every cell died -- `generation` is when the board first went
    /// empty.
    DiedOut { generation: usize },
    /// board exactly repeated one from `period` generations
    /// earlier -- `period` 1 means a still life, greater than 1 means
    /// an oscillator.
    Stabilized { generation: usize, period: usize },
    /// neither of the above happened within the generation budget or
    /// the cycle-detection window.
    StillEvolving,
}

/// runs `board` forward under `rule` up to `max_generations` steps,
/// checking each new generation against the last `history_window`
/// generations for an exact repeat -- a bounded, honest search, not an
/// exhaustive one: a real oscillator with period longer than
/// `history_window` reads as `StillEvolving`, not falsely as "no
/// cycle exists".
pub fn simulate_until_stable(board: &Board, rule: &Rule, max_generations: usize, history_window: usize) -> Outcome {
    let mut history: std::collections::VecDeque<(usize, Board)> = std::collections::VecDeque::new();
    history.push_back((0, board.clone()));
    let mut current = board.clone();

    for generation in 1..=max_generations {
        current = current.step(rule);
        if current.live_count() == 0 {
            return Outcome::DiedOut { generation };
        }
        if let Some(entry) = history.iter().find(|entry| entry.1 == current) {
            return Outcome::Stabilized { generation, period: generation - entry.0 };
        }
        history.push_back((generation, current.clone()));
        if history.len() > history_window {
            history.pop_front();
        }
    }
    Outcome::StillEvolving
}

fn parse_usize(fields: &[&str], line_no: usize, name: &str) -> Result<usize, String> {
    let Some(&field) = fields.first() else {
        return Err(format!("line {}: {name} needs a value", line_no + 1));
    };
    field.parse::<usize>().map_err(|_| format!("line {}: invalid {name} {field:?}", line_no + 1))
}

fn parse_u8_list(fields: &[&str], line_no: usize, name: &str) -> Result<Vec<u8>, String> {
    if fields.is_empty() {
        return Err(format!("line {}: {name} needs at least one value", line_no + 1));
    }
    fields
        .iter()
        .map(|f| f.parse::<u8>().map_err(|_| format!("line {}: invalid {name} value {f:?}", line_no + 1)))
        .collect()
}

/// small tagged-line format for declaring a starting board and rule
/// -- `width`/`height`/`birth`/`survive` each once, any number of
/// `cell,x,y` lines for the initial live cells. Not a real interchange
/// standard (unlike VCD) since none exists for this; this project's
/// own minimal shape for it, same "label/tag + numbers, one crate owns
/// the format's math and its parsing" convention `colorspace`/
/// `halftone` already established.
pub fn parse_pattern(text: &str) -> Result<(Board, Rule), String> {
    let mut width = None;
    let mut height = None;
    let mut birth_counts = None;
    let mut survive_counts = None;
    let mut live_cells = Vec::new();

    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        let (tag, rest) = (fields[0], &fields[1..]);
        match tag {
            "width" => width = Some(parse_usize(rest, line_no, "width")?),
            "height" => height = Some(parse_usize(rest, line_no, "height")?),
            "birth" => birth_counts = Some(parse_u8_list(rest, line_no, "birth")?),
            "survive" => survive_counts = Some(parse_u8_list(rest, line_no, "survive")?),
            "cell" => {
                if rest.len() != 2 {
                    return Err(format!("line {}: expected \"cell,x,y\", got {line:?}", line_no + 1));
                }
                let x = parse_usize(&rest[0..1], line_no, "cell x")?;
                let y = parse_usize(&rest[1..2], line_no, "cell y")?;
                live_cells.push((x, y));
            }
            other => return Err(format!("line {}: unknown tag {other:?}", line_no + 1)),
        }
    }

    let width = width.ok_or_else(|| "missing \"width\" line".to_string())?;
    let height = height.ok_or_else(|| "missing \"height\" line".to_string())?;
    let birth_counts = birth_counts.ok_or_else(|| "missing \"birth\" line".to_string())?;
    let survive_counts = survive_counts.ok_or_else(|| "missing \"survive\" line".to_string())?;

    for &(x, y) in &live_cells {
        if x >= width || y >= height {
            return Err(format!("cell ({x}, {y}) is outside the {width}x{height} board"));
        }
    }

    let rule = Rule { birth: Rule::from_counts(&birth_counts), survive: Rule::from_counts(&survive_counts) };
    let board = Board::from_live_cells(width, height, &live_cells);
    Ok((board, rule))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sorted(mut cells: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
        cells.sort_unstable();
        cells
    }

    /// 2x2 block is Life's simplest still life -- every real
    /// reference (including Conway's own original description) lists
    /// it as period-1.
    #[test]
    fn block_is_a_still_life() {
        let board = Board::from_live_cells(10, 10, &[(4, 4), (5, 4), (4, 5), (5, 5)]);
        let next = board.step(&Rule::LIFE);
        assert_eq!(next, board);
    }

    /// blinker (3 in a row) is Life's simplest oscillator -- real,
    /// independently documented period-2 behavior: it alternates
    /// between the horizontal and vertical orientation every
    /// generation.
    #[test]
    fn blinker_oscillates_with_period_two() {
        let horizontal = Board::from_live_cells(10, 10, &[(3, 4), (4, 4), (5, 4)]);
        let vertical = horizontal.step(&Rule::LIFE);
        assert_eq!(sorted(vertical.live_cells()), sorted(vec![(4, 3), (4, 4), (4, 5)]));
        let back_to_horizontal = vertical.step(&Rule::LIFE);
        assert_eq!(back_to_horizontal, horizontal);
    }

    /// glider is Life's smallest moving pattern -- real, independently
    /// documented behavior: it returns to its original shape translated
    /// by (+1, +1) every 4 generations. Board large enough that the
    /// toroidal wrap can't interfere within 4 steps.
    #[test]
    fn glider_translates_by_one_one_every_four_generations() {
        let start = [(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)];
        let board = Board::from_live_cells(20, 20, &start);
        let mut current = board.clone();
        for _ in 0..4 {
            current = current.step(&Rule::LIFE);
        }
        let expected: Vec<(usize, usize)> = start.iter().map(|&(x, y)| (x + 1, y + 1)).collect();
        assert_eq!(sorted(current.live_cells()), sorted(expected));
    }

    #[test]
    fn a_lone_cell_dies_out_after_one_generation() {
        let board = Board::from_live_cells(10, 10, &[(5, 5)]);
        let outcome = simulate_until_stable(&board, &Rule::LIFE, 20, 8);
        assert_eq!(outcome, Outcome::DiedOut { generation: 1 });
    }

    #[test]
    fn a_block_is_reported_stabilized_at_generation_one_with_period_one() {
        let board = Board::from_live_cells(10, 10, &[(4, 4), (5, 4), (4, 5), (5, 5)]);
        let outcome = simulate_until_stable(&board, &Rule::LIFE, 20, 8);
        assert_eq!(outcome, Outcome::Stabilized { generation: 1, period: 1 });
    }

    #[test]
    fn a_blinker_is_reported_stabilized_at_generation_two_with_period_two() {
        let board = Board::from_live_cells(10, 10, &[(3, 4), (4, 4), (5, 4)]);
        let outcome = simulate_until_stable(&board, &Rule::LIFE, 20, 8);
        assert_eq!(outcome, Outcome::Stabilized { generation: 2, period: 2 });
    }

    /// glider keeps moving; on a board large enough that it hasn't
    /// wrapped back around to a past position within the budget, a
    /// bounded, honest search correctly reports `StillEvolving` -- not
    /// a false claim that no cycle exists, just that none was found
    /// within this search's own stated limits.
    #[test]
    fn a_glider_on_a_large_board_reads_as_still_evolving_within_a_short_budget() {
        let board = Board::from_live_cells(30, 30, &[(1, 0), (2, 1), (0, 2), (1, 2), (2, 2)]);
        let outcome = simulate_until_stable(&board, &Rule::LIFE, 20, 8);
        assert_eq!(outcome, Outcome::StillEvolving);
    }

    #[test]
    fn parse_pattern_reads_a_real_declaration() {
        let text = "\
# a blinker
width,10
height,10
birth,3
survive,2,3
cell,3,4
cell,4,4
cell,5,4
";
        let (board, rule) = parse_pattern(text).unwrap();
        assert_eq!(board.width, 10);
        assert_eq!(board.height, 10);
        assert_eq!(rule, Rule::LIFE);
        assert_eq!(sorted(board.live_cells()), sorted(vec![(3, 4), (4, 4), (5, 4)]));
    }

    #[test]
    fn parse_pattern_rejects_a_cell_outside_the_board() {
        let text = "width,5\nheight,5\nbirth,3\nsurvive,2,3\ncell,9,9\n";
        let err = parse_pattern(text).unwrap_err();
        assert!(err.contains("outside"), "error should explain the out-of-bounds cell: {err}");
    }

    #[test]
    fn parse_pattern_rejects_a_missing_required_line() {
        let err = parse_pattern("width,5\nheight,5\n").unwrap_err();
        assert!(err.contains("birth"), "error should name the missing line: {err}");
    }
}
