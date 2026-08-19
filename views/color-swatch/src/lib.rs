#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{
    HeatmapCells, HeatmapView, TableCell, TableView, ViewPrimitive,
};
use colorspace::{lab_in_srgb_gamut, lab_to_srgb, parse_swatches, Swatch};

struct Component;

/// cells wide per swatch row -- wide enough to read as a solid color
/// block at typical terminal widths, narrow enough that a realistic
/// palette (a dozen-plus swatches) still fits one screen tall. Purely
/// a display choice, not part of the `.color-swatches` format itself.
const SWATCH_WIDTH: u32 = 8;

/// new ADR-033 `rgb` arm end to end (ADR-033/ADR-034): one row per
/// swatch, `SWATCH_WIDTH` cells wide, each cell the swatch's own
/// lab→sRGB conversion -- not a scalar value mapped through a colormap,
/// since these values already *are* colors. Row-major, matching
/// `heatmap-cells.rgb`'s own documented layout: swatch 0's row first,
/// three bytes per cell.
fn build_heatmap(swatches: &[Swatch]) -> ViewPrimitive {
    let height = swatches.len() as u32;
    let mut bytes = Vec::with_capacity((SWATCH_WIDTH * height * 3) as usize);
    for swatch in swatches {
        let rgb = lab_to_srgb(swatch.l, swatch.a, swatch.b);
        for _ in 0..SWATCH_WIDTH {
            bytes.extend_from_slice(&[rgb.r, rgb.g, rgb.b]);
        }
    }
    ViewPrimitive::Heatmap(HeatmapView { width: SWATCH_WIDTH, height, cells: HeatmapCells::Rgb(bytes) })
}

fn build_table(swatches: &[Swatch]) -> ViewPrimitive {
    let rows = swatches
        .iter()
        .map(|s| {
            let rgb = lab_to_srgb(s.l, s.a, s.b);
            let in_gamut = lab_in_srgb_gamut(s.l, s.a, s.b);
            vec![
                TableCell { text: s.label.clone(), status: None },
                TableCell { text: format!("{:.2}", s.l), status: None },
                TableCell { text: format!("{:.2}", s.a), status: None },
                TableCell { text: format!("{:.2}", s.b), status: None },
                TableCell { text: format!("#{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b), status: None },
                TableCell {
                    text: if in_gamut { "yes".to_string() } else { "clamped".to_string() },
                    status: if in_gamut { None } else { Some(Severity::Warning) },
                },
            ]
        })
        .collect();
    ViewPrimitive::Table(TableView {
        headers: vec![
            "Swatch".to_string(),
            "L*".to_string(),
            "a*".to_string(),
            "b*".to_string(),
            "sRGB".to_string(),
            "in gamut".to_string(),
        ],
        rows,
    })
}

impl Guest for Component {
    fn view_id() -> String {
        "example-color-swatch".to_string()
    }

    /// deliberately not live-refreshed, same reasoning every other
    /// static-file capture/report view this project has already
    /// established: a declared palette doesn't change on its own
    /// between ticks.
    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("example-view-color-swatch: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(_state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let Some(path) = host::list_files("**/*.color-swatches").unwrap_or_default().into_iter().next() else {
            return Ok(vec![ViewPrimitive::Table(TableView {
                headers: vec!["".to_string()],
                rows: vec![vec![TableCell {
                    text: "no .color-swatches file found in this project".to_string(),
                    status: None,
                }]],
            })]);
        };
        let contents = host::read_file(&path)?;
        let swatches = parse_swatches(&contents)?;
        if swatches.is_empty() {
            return Ok(vec![ViewPrimitive::Table(TableView {
                headers: vec!["".to_string()],
                rows: vec![vec![TableCell { text: format!("{path}: no swatches declared"), status: None }]],
            })]);
        }
        Ok(vec![build_heatmap(&swatches), build_table(&swatches)])
    }

    fn handle_command(_state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        Err(format!("unknown command: {command:?} (this view is static, no commands)"))
    }
}

bindings::export!(Component with_types_in bindings);
