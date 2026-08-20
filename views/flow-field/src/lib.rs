#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{
    Colormap, HeatmapCells, HeatmapScalarCells, HeatmapView, TableCell, TableView, ViewPrimitive,
};
use potential_flow::{generate_speed_field, parse_field_spec, FieldSpec};

struct Component;

const SURFACE_SAMPLES: usize = 3600;
const STAGNATION_TOLERANCE: f64 = 0.05;

fn build_primitives(spec: &FieldSpec) -> Vec<ViewPrimitive> {
    let values = generate_speed_field(spec);
    let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
    let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);

    let heatmap = ViewPrimitive::Heatmap(HeatmapView {
        width: spec.x_steps as u32,
        height: spec.y_steps as u32,
        cells: HeatmapCells::Scalar(HeatmapScalarCells {
            values,
            value_min: min,
            value_max: max,
            colormap: Colormap::PerceptuallyUniform,
        }),
    });

    let max_surface_speed = spec.flow.max_surface_speed(SURFACE_SAMPLES);
    let stagnation = spec.flow.stagnation_points(SURFACE_SAMPLES, STAGNATION_TOLERANCE);
    let stagnation_text = if stagnation.len() == 2 {
        format!("{:.1}\u{b0}, {:.1}\u{b0}", stagnation[0].to_degrees(), stagnation[1].to_degrees())
    } else {
        format!("{} found (expected 2)", stagnation.len())
    };

    let table = ViewPrimitive::Table(TableView {
        headers: vec!["Metric".to_string(), "Value".to_string()],
        rows: vec![
            vec![
                TableCell { text: "Free-stream speed".to_string(), status: None },
                TableCell { text: format!("{:.2}", spec.flow.free_stream_speed), status: None },
            ],
            vec![
                TableCell { text: "Cylinder radius".to_string(), status: None },
                TableCell { text: format!("{:.2}", spec.flow.radius), status: None },
            ],
            vec![
                TableCell { text: "Domain".to_string(), status: None },
                TableCell {
                    text: format!(
                        "x [{:.1}, {:.1}] x y [{:.1}, {:.1}] ({}x{})",
                        spec.x_min, spec.x_max, spec.y_min, spec.y_max, spec.x_steps, spec.y_steps
                    ),
                    status: None,
                },
            ],
            vec![
                TableCell { text: "Speed range".to_string(), status: None },
                TableCell { text: format!("{min:.2} - {max:.2}"), status: None },
            ],
            vec![
                TableCell { text: "Max surface speed".to_string(), status: None },
                TableCell { text: format!("{max_surface_speed:.2} (2x free-stream)"), status: None },
            ],
            vec![
                TableCell { text: "Stagnation points".to_string(), status: None },
                TableCell { text: stagnation_text, status: None },
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
        "flow-field".to_string()
    }

    /// deliberately not live-refreshed -- a declared flow spec doesn't
    /// change on its own between ticks, same reasoning every other
    /// static-declaration view in this project already follows.
    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("flow-field: init for project at {}", project.root_path));
        Ok(Vec::new())
    }

    fn render(_state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let Some(path) = host::list_files("**/*.flow-field").unwrap_or_default().into_iter().next() else {
            return Ok(placeholder("no .flow-field file found in this project".to_string()));
        };
        let contents = host::read_file(&path)?;
        let spec = parse_field_spec(&contents)?;
        Ok(build_primitives(&spec))
    }

    fn handle_command(_state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        Err(format!("unknown command: {command:?} (this view is static, no commands)"))
    }
}

bindings::export!(Component with_types_in bindings);
