#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{HeatmapCells, HeatmapView, TableCell, TableView, ViewPrimitive};
use cellular_automaton::{parse_pattern, Board, Rule};

struct Component;

/// reserved live-refresh token -- see `app.rs`'s `LIVE_TICK_COMMAND`.
/// opted into here specifically: this is the one view in the project
/// where live-refresh isn't cosmetic (a drifting phase, a re-read
/// file) but the actual point -- each tick genuinely steps the
/// simulation forward, the same "observe what macroscopic behavior
/// emerges" `docs/roadmap/Workbenches.md` names as this category's own
/// purpose.
const LIVE_TICK_COMMAND: &str = "\0tick";

const ALIVE_RGB: (u8, u8, u8) = (0, 180, 90);
const DEAD_RGB: (u8, u8, u8) = (255, 255, 255);

fn mask_to_u16(mask: &[bool; 9]) -> u16 {
    mask.iter().enumerate().fold(0u16, |acc, (i, &b)| if b { acc | (1 << i) } else { acc })
}

fn u16_to_mask(bits: u16) -> [bool; 9] {
    let mut mask = [false; 9];
    for (i, slot) in mask.iter_mut().enumerate() {
        *slot = (bits >> i) & 1 == 1;
    }
    mask
}

/// opaque per-tick state: `width`/`height`/`rule` (so a tick never has
/// to re-read the declaration file) plus `generation` and the current
/// board, one byte per cell. `width == 0` is the "no `.ca-pattern`
/// file found" sentinel -- reuses this same encoding not a
/// separate empty-state shape.
fn encode_state(width: u32, height: u32, rule: &Rule, generation: u32, board: &Board) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(16 + (width * height) as usize);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&mask_to_u16(&rule.birth).to_le_bytes());
    bytes.extend_from_slice(&mask_to_u16(&rule.survive).to_le_bytes());
    bytes.extend_from_slice(&generation.to_le_bytes());
    for y in 0..height as usize {
        for x in 0..width as usize {
            bytes.push(u8::from(board.get(x, y)));
        }
    }
    bytes
}

fn decode_state(bytes: &[u8]) -> Result<(u32, u32, Rule, u32, Board), String> {
    if bytes.len() < 16 {
        return Err("view state too short".to_string());
    }
    let width = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
    let height = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let birth = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    let survive = u16::from_le_bytes(bytes[10..12].try_into().unwrap());
    let generation = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    let cell_bytes = &bytes[16..];
    if cell_bytes.len() != (width * height) as usize {
        return Err("view state cell data length mismatch".to_string());
    }
    let mut board = Board::new(width as usize, height as usize);
    for y in 0..height as usize {
        for x in 0..width as usize {
            if cell_bytes[y * width as usize + x] == 1 {
                board.set(x, y, true);
            }
        }
    }
    Ok((width, height, Rule { birth: u16_to_mask(birth), survive: u16_to_mask(survive) }, generation, board))
}

fn build_primitives(board: &Board, generation: u32) -> Vec<ViewPrimitive> {
    let mut bytes = Vec::with_capacity(board.width * board.height * 3);
    for y in 0..board.height {
        for x in 0..board.width {
            let (r, g, b) = if board.get(x, y) { ALIVE_RGB } else { DEAD_RGB };
            bytes.extend_from_slice(&[r, g, b]);
        }
    }
    let heatmap = ViewPrimitive::Heatmap(HeatmapView {
        width: board.width as u32,
        height: board.height as u32,
        cells: HeatmapCells::Rgb(bytes),
    });
    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Generation".to_string(), "Live cells".to_string(), "Board".to_string()],
        rows: vec![vec![
            TableCell { text: generation.to_string(), status: None },
            TableCell { text: board.live_count().to_string(), status: None },
            TableCell { text: format!("{}x{}", board.width, board.height), status: None },
        ]],
    });
    vec![heatmap, table]
}

fn placeholder() -> Vec<ViewPrimitive> {
    vec![ViewPrimitive::Table(TableView {
        headers: vec!["".to_string()],
        rows: vec![vec![TableCell { text: "no .ca-pattern file found in this project".to_string(), status: None }]],
    })]
}

impl Guest for Component {
    fn view_id() -> String {
        "emergence".to_string()
    }

    /// parses the declared board/rule once and encodes generation `0`
    /// into the opaque state -- unlike every other static-file view
    /// this project has built, this one *is* live-refreshed, so the
    /// state has to carry the running simulation forward across ticks
    /// not being recomputed from the file each time.
    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("emergence: init for project at {}", project.root_path));
        let Some(path) = host::list_files("**/*.ca-pattern").unwrap_or_default().into_iter().next() else {
            return Ok(encode_state(0, 0, &Rule::LIFE, 0, &Board::new(0, 0)));
        };
        let contents = host::read_file(&path)?;
        let (board, rule) = parse_pattern(&contents)?;
        Ok(encode_state(board.width as u32, board.height as u32, &rule, 0, &board))
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let (width, _height, _rule, generation, board) = decode_state(&state)?;
        if width == 0 {
            return Ok(placeholder());
        }
        Ok(build_primitives(&board, generation))
    }

    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        let (width, height, rule, generation, board) = decode_state(&state)?;
        if width == 0 {
            return Err(format!("unknown command: {command:?} (no .ca-pattern file found)"));
        }
        if command == LIVE_TICK_COMMAND {
            let next_board = board.step(&rule);
            let next_generation = generation + 1;
            let new_state = encode_state(width, height, &rule, next_generation, &next_board);
            let primitives = build_primitives(&next_board, next_generation);
            return Ok((new_state, primitives));
        }
        Err(format!("unknown command: {command:?} (this view only supports live-refresh ticks — press F5)"))
    }
}

bindings::export!(Component with_types_in bindings);
