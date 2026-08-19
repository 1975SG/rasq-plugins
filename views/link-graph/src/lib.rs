#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::host;
use bindings::iderm::plugin::types::ProjectInfo;
use bindings::iderm::plugin::view::{
    DiagramEdge, DiagramNode, NodeShape, StateGraphView, TableCell, TableView, ViewPrimitive,
};
use link_graph::{most_reliable_path, parse_transport_graph, shortest_latency_path, widest_bandwidth_path, Graph, PathResult};

struct Component;

/// reserved live-refresh token -- see the `emc` view's own copy of
/// this constant for the full explanation. a declared topology is
/// static in the common case this plugin was originally built
/// against, but a live-changing mesh topology (nodes/links appearing
/// and dropping in real time, e.g. a live sensor-mesh network) is a
/// real future producer for exactly this file format -- closing this
/// the same way the `analog-capture`/`vcd-capture` views were, not
/// leaving it as the one remaining "maybe" after auditing every
/// view-provider plugin for the same gap.
const LIVE_TICK_COMMAND: &str = "\0tick";

#[derive(Clone, Copy, PartialEq)]
enum Criterion {
    Latency,
    Bandwidth,
    Reliability,
}

impl Criterion {
    fn from_byte(b: u8) -> Self {
        match b {
            1 => Criterion::Bandwidth,
            2 => Criterion::Reliability,
            _ => Criterion::Latency,
        }
    }

    fn to_byte(self) -> u8 {
        match self {
            Criterion::Latency => 0,
            Criterion::Bandwidth => 1,
            Criterion::Reliability => 2,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Criterion::Latency => "fastest-latency",
            Criterion::Bandwidth => "widest-bandwidth",
            Criterion::Reliability => "most-reliable",
        }
    }

    fn path(self, graph: &Graph, from: &str, to: &str) -> Option<PathResult> {
        match self {
            Criterion::Latency => shortest_latency_path(graph, from, to),
            Criterion::Bandwidth => widest_bandwidth_path(graph, from, to),
            Criterion::Reliability => most_reliable_path(graph, from, to),
        }
    }

    /// per-edge property this criterion cares about, shown as each
    /// edge's own label so the diagram reads correctly regardless of
    /// which objective is currently selected.
    fn edge_label(self, edge: &link_graph::Edge) -> String {
        match self {
            Criterion::Latency => format!("{:.0}ms", edge.latency_ms),
            Criterion::Bandwidth => format!("{:.0} Mbps", edge.bandwidth_mbps),
            Criterion::Reliability => format!("{:.2}% loss", edge.loss_percent),
        }
    }
}

fn build_primitives(graph: &Graph, query: &(String, String), criterion: Criterion) -> Vec<ViewPrimitive> {
    let path = criterion.path(graph, &query.0, &query.1);
    let path_edges: Vec<usize> = path.as_ref().map(|p| p.edge_indices.clone()).unwrap_or_default();

    let nodes = graph
        .nodes()
        .into_iter()
        .map(|id| DiagramNode { label: id.clone(), id, shape: NodeShape::Rectangle, status: None, anchor: None })
        .collect();

    let edges = graph
        .edges
        .iter()
        .enumerate()
        .map(|(i, edge)| DiagramEdge {
            from_id: edge.from.clone(),
            to_id: edge.to.clone(),
            label: Some(criterion.edge_label(edge)),
            highlighted: path_edges.contains(&i),
        })
        .collect();

    let graph_view = ViewPrimitive::StateGraph(StateGraphView { nodes, edges, initial_node_id: query.0.clone() });

    let summary_row = match &path {
        Some(r) => vec![
            TableCell { text: criterion.label().to_string(), status: None },
            TableCell { text: r.nodes.join(" -> "), status: None },
            TableCell { text: format!("{:.1}", r.total_latency_ms), status: None },
            TableCell { text: format!("{:.1}", r.bottleneck_bandwidth_mbps), status: None },
            TableCell { text: format!("{:.3}", r.compound_loss_percent), status: None },
        ],
        None => vec![
            TableCell { text: criterion.label().to_string(), status: None },
            TableCell { text: format!("no path from {:?} to {:?}", query.0, query.1), status: None },
            TableCell { text: "-".to_string(), status: None },
            TableCell { text: "-".to_string(), status: None },
            TableCell { text: "-".to_string(), status: None },
        ],
    };
    let table = ViewPrimitive::Table(TableView {
        headers: vec![
            "Objective".to_string(),
            "Path".to_string(),
            "Latency (ms)".to_string(),
            "Bottleneck (Mbps)".to_string(),
            "Loss (%)".to_string(),
        ],
        rows: vec![summary_row],
    });

    vec![graph_view, table]
}

fn placeholder(text: String) -> Vec<ViewPrimitive> {
    vec![ViewPrimitive::Table(TableView {
        headers: vec!["".to_string()],
        rows: vec![vec![TableCell { text, status: None }]],
    })]
}

fn render_at(criterion: Criterion) -> Result<Vec<ViewPrimitive>, String> {
    let Some(path) = host::list_files("**/*.transport-graph").unwrap_or_default().into_iter().next() else {
        return Ok(placeholder("no .transport-graph file found in this project".to_string()));
    };
    let contents = host::read_file(&path)?;
    let (graph, queries) = parse_transport_graph(&contents)?;
    let Some(query) = queries.first() else {
        return Ok(placeholder(format!("{path}: no queries declared")));
    };
    Ok(build_primitives(&graph, query, criterion))
}

impl Guest for Component {
    fn view_id() -> String {
        "example-link-graph".to_string()
    }

    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        host::log(&format!("example-view-link-graph: init for project at {}", project.root_path));
        Ok(vec![Criterion::Latency.to_byte()])
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        render_at(Criterion::from_byte(*state.first().unwrap_or(&0)))
    }

    /// `latency`/`bandwidth`/`reliability` switch which routing
    /// objective's path is highlighted (`diagram-edge.highlighted`) --
    /// same graph, same edges, a different subset marked. The tick
    /// branch (live-refresh, ADR-057) re-reads the `.transport-graph`
    /// file fresh under the currently-selected objective -- real signal
    /// for a topology a running task could still be rewriting.
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        let current = Criterion::from_byte(*state.first().unwrap_or(&0));
        if command == LIVE_TICK_COMMAND {
            return Ok((state, render_at(current)?));
        }
        let criterion = match command.as_str() {
            "latency" => Criterion::Latency,
            "bandwidth" => Criterion::Bandwidth,
            "reliability" => Criterion::Reliability,
            other => {
                return Err(format!("unknown command: {other:?} (try \"latency\", \"bandwidth\", or \"reliability\")"))
            }
        };
        Ok((vec![criterion.to_byte()], render_at(criterion)?))
    }
}

bindings::export!(Component with_types_in bindings);
