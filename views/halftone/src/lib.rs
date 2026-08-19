#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{HeatmapCells, HeatmapView, TableCell, TableView, ViewPrimitive};
use halftone::{average_coverage, dot_is_on, Screen};

struct Component;

/// grid width shared by every screen's band -- wide enough to show
/// several full dot-pattern periods at a typical terminal width.
const GRID_WIDTH: u32 = 48;
/// rows per screen's own band.
const BAND_HEIGHT: u32 = 10;
/// more than this and each band gets too short to read as a real dot
/// pattern not noise -- the rest still get their own row in the
/// `Table` below, same "chart what's readable, list the rest"
/// convention `example-view-analog-capture` already established.
const MAX_RENDERED_SCREENS: usize = 4;
/// sample grid resolution for the `Table`'s measured-coverage
/// cross-check column -- coarser than `halftone`'s own test suite uses
/// since this runs on every render, not just in a test.
const COVERAGE_SAMPLES: usize = 24;

/// stacks each screen's own halftone dot pattern into its own
/// `BAND_HEIGHT`-row band within a single grid, separated by a blank
/// (white) row — `idermviz`'s primary-slot dispatch only ever draws
/// one `Heatmap` primitive at a time (see `ui.rs`'s fixed precedence),
/// so multiple screens have to live inside one grid, not one
/// `ViewPrimitive` each.
fn build_heatmap(screens: &[Screen]) -> ViewPrimitive {
    let shown = &screens[..screens.len().min(MAX_RENDERED_SCREENS)];
    let band_count = shown.len() as u32;
    let height = if band_count == 0 { 0 } else { band_count * BAND_HEIGHT + (band_count - 1) };
    let mut bytes = Vec::with_capacity((GRID_WIDTH * height * 3) as usize);

    for band_row in 0..height {
        let band_index = band_row / (BAND_HEIGHT + 1);
        let row_in_band = band_row % (BAND_HEIGHT + 1);
        let is_gap_row = row_in_band == BAND_HEIGHT;

        for col in 0..GRID_WIDTH {
            let rgb = if is_gap_row {
                (255, 255, 255)
            } else {
                let screen = &shown[band_index as usize];
                let x = col as f64 / GRID_WIDTH as f64;
                let y = row_in_band as f64 / BAND_HEIGHT as f64;
                if dot_is_on(screen.gray, x, y, screen.angle_deg, screen.frequency) {
                    (0, 0, 0)
                } else {
                    (255, 255, 255)
                }
            };
            bytes.extend_from_slice(&[rgb.0, rgb.1, rgb.2]);
        }
    }

    ViewPrimitive::Heatmap(HeatmapView { width: GRID_WIDTH, height, cells: HeatmapCells::Rgb(bytes) })
}

fn build_table(screens: &[Screen]) -> ViewPrimitive {
    let rows = screens
        .iter()
        .map(|s| {
            let measured = average_coverage(s.gray, s.angle_deg, s.frequency, COVERAGE_SAMPLES);
            vec![
                TableCell { text: s.label.clone(), status: None },
                TableCell { text: format!("{:.1}\u{b0}", s.angle_deg), status: None },
                TableCell { text: format!("{:.2}", s.frequency), status: None },
                TableCell { text: format!("{:.2}", s.gray), status: None },
                TableCell { text: format!("{:.2}", measured), status: None },
            ]
        })
        .collect();
    ViewPrimitive::Table(TableView {
        headers: vec![
            "Screen".to_string(),
            "Angle".to_string(),
            "Freq".to_string(),
            "Gray".to_string(),
            "Measured coverage".to_string(),
        ],
        rows,
    })
}

impl Guest for Component {
    fn view_id() -> String {
        "example-halftone".to_string()
    }

    /// deliberately not live-refreshed, same reasoning every other
    /// static-file-declaration view in this project already follows: a
    /// declared screen set doesn't change on its own between ticks.
    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("example-view-halftone: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(_state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let Some(path) = host::list_files("**/*.halftone-screens").unwrap_or_default().into_iter().next() else {
            return Ok(vec![ViewPrimitive::Table(TableView {
                headers: vec!["".to_string()],
                rows: vec![vec![TableCell {
                    text: "no .halftone-screens file found in this project".to_string(),
                    status: None,
                }]],
            })]);
        };
        let contents = host::read_file(&path)?;
        let screens = halftone::parse_screens(&contents)?;
        if screens.is_empty() {
            return Ok(vec![ViewPrimitive::Table(TableView {
                headers: vec!["".to_string()],
                rows: vec![vec![TableCell { text: format!("{path}: no screens declared"), status: None }]],
            })]);
        }
        Ok(vec![build_heatmap(&screens), build_table(&screens)])
    }

    fn handle_command(_state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        Err(format!("unknown command: {command:?} (this view is static, no commands)"))
    }
}

bindings::export!(Component with_types_in bindings);
