#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{
    Colormap, HeatmapCells, HeatmapScalarCells, HeatmapView, TableCell, TableView, ViewPrimitive,
};
use black_scholes::{generate_surface, parse_surface_spec, SurfaceSpec};

struct Component;

fn build_primitives(spec: &SurfaceSpec) -> Vec<ViewPrimitive> {
    let values = generate_surface(spec);
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let heatmap = ViewPrimitive::Heatmap(HeatmapView {
        width: spec.strike_steps as u32,
        height: spec.expiry_steps as u32,
        cells: HeatmapCells::Scalar(HeatmapScalarCells {
            values,
            value_min: min,
            value_max: max,
            colormap: Colormap::PerceptuallyUniform,
        }),
    });

    let atm_price = spec.price_at(spec.spot, spec.expiry_min);
    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Metric".to_string(), "Value".to_string()],
        rows: vec![
            vec![TableCell { text: "Spot".to_string(), status: None }, TableCell { text: format!("{:.2}", spec.spot), status: None }],
            vec![
                TableCell { text: "Rate / Volatility".to_string(), status: None },
                TableCell { text: format!("{:.2}% / {:.1}%", spec.rate * 100.0, spec.volatility * 100.0), status: None },
            ],
            vec![
                TableCell { text: "Strike range".to_string(), status: None },
                TableCell { text: format!("{:.2} - {:.2} ({} steps)", spec.strike_min, spec.strike_max, spec.strike_steps), status: None },
            ],
            vec![
                TableCell { text: "Expiry range".to_string(), status: None },
                TableCell { text: format!("{:.2} - {:.2} yr ({} steps)", spec.expiry_min, spec.expiry_max, spec.expiry_steps), status: None },
            ],
            vec![
                TableCell { text: "Price range".to_string(), status: None },
                TableCell { text: format!("{min:.2} - {max:.2}"), status: None },
            ],
            vec![
                TableCell { text: format!("At-the-money @ T={:.2}", spec.expiry_min), status: None },
                TableCell { text: format!("{atm_price:.2}"), status: None },
            ],
        ],
    });

    vec![heatmap, table]
}

fn placeholder(text: String) -> Vec<ViewPrimitive> {
    vec![ViewPrimitive::Table(TableView {
        headers: vec!["".to_string()],
        rows: vec![vec![TableCell { text, status: None }]],
    })]
}

impl Guest for Component {
    fn view_id() -> String {
        "option-surface".to_string()
    }

    /// deliberately not live-refreshed -- a declared surface spec
    /// doesn't change on its own between ticks, same reasoning every
    /// other static-declaration view in this project already follows.
    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("option-surface: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(_state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let Some(path) = host::list_files("**/*.option-surface").unwrap_or_default().into_iter().next() else {
            return Ok(placeholder("no .option-surface file found in this project".to_string()));
        };
        let contents = host::read_file(&path)?;
        let spec = parse_surface_spec(&contents)?;
        Ok(build_primitives(&spec))
    }

    fn handle_command(_state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        Err(format!("unknown command: {command:?} (this view is static, no commands)"))
    }
}

bindings::export!(Component with_types_in bindings);
