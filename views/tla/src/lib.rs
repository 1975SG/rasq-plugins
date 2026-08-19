#[allow(warnings)]
mod bindings;

use bindings::exports::iderm::plugin::view_provider::Guest;
use bindings::iderm::plugin::types::{ProjectInfo, Severity};
use bindings::iderm::plugin::view::{
    DiagramEdge, DiagramNode, NodeShape, StateGraphView, TraceStep, TraceView, ViewPrimitive,
};

struct Component;

/// tiny hand-authored mutual-exclusion state machine and one real
/// counterexample through it -- self-contained, no `.tla` file parsing,
/// same "no external data dependency" simplicity `example-doctor-rule`/
/// `example-view-emc` already follow. The violation: a second process
/// enters `Critical` while the first is still there (`Waiting -> Critical`
/// a second time without an intervening `Exit`) -- a textbook
/// mutual-exclusion counterexample, the kind `trace-view` is named for
/// (docs/modules/Idermviz.md: "same as the TLA+ Toolbox's own
/// error-trace view").
fn counterexample() -> Vec<TraceStep> {
    let step = |state_label: &str, action_label: Option<&str>| TraceStep {
        state_label: state_label.to_string(),
        action_label: action_label.map(str::to_string),
    };
    vec![
        step("Init", None),
        step("Idle", Some("Start")),
        step("Waiting", Some("Request")),
        step("Critical", Some("Enter")),
        step("Waiting", Some("Request")),
        step("Critical", Some("Enter")),
    ]
}

/// full reachable-state space the `counterexample()` trace above is
/// one path through -- same underlying tiny model, now returning every
/// node/edge instead of just the violating path, per
/// docs/modules/Idermviz.md's "natural reuse" framing for this example.
/// includes a `Diamond` decision node (`Enter?`) and two back edges
/// (`Enter? -> Waiting` on "No", `Critical -> Idle` on "Exit") so a
/// single example exercises the primitive's full shape/edge-kind
/// vocabulary, not just the straight-chain case.
fn state_graph() -> StateGraphView {
    // `anchor: None` on every node -- this plugin's positions come
    // entirely from Core's `graph_layout` pass, unlike a `composed-view`
    // overlay's explicit spatial placement (see
    // docs/modules/Idermviz.md's "composed" section).
    let node = |id: &str, label: &str, shape: NodeShape, status: Option<Severity>| DiagramNode {
        id: id.to_string(),
        label: label.to_string(),
        shape,
        status,
        anchor: None,
    };
    let edge = |from: &str, to: &str, label: Option<&str>, highlighted: bool| DiagramEdge {
        from_id: from.to_string(),
        to_id: to.to_string(),
        label: label.map(str::to_string),
        highlighted,
    };
    StateGraphView {
        nodes: vec![
            node("init", "Init", NodeShape::Rectangle, None),
            node("idle", "Idle", NodeShape::Rectangle, None),
            node("waiting", "Waiting", NodeShape::Rectangle, None),
            node("enter_decision", "Enter?", NodeShape::Diamond, None),
            node("critical", "Critical", NodeShape::Rectangle, None),
            node("violation", "Violation", NodeShape::RoundedRectangle, Some(Severity::Error)),
        ],
        edges: vec![
            edge("init", "idle", Some("Start"), false),
            edge("idle", "waiting", Some("Request"), false),
            edge("waiting", "enter_decision", None, false),
            edge("enter_decision", "critical", Some("Yes"), false),
            edge("enter_decision", "waiting", Some("No"), false),
            edge("critical", "idle", Some("Exit"), false),
            edge("critical", "violation", Some("Enter"), true),
        ],
        initial_node_id: "init".to_string(),
    }
}

impl Guest for Component {
    fn view_id() -> String {
        "example-tla-trace".to_string()
    }

    /// deliberately calls back into `host::log`, same round-trip proof
    /// `example-view-emc::init` already establishes for `view-provider`
    /// plugins.
    fn init(project: ProjectInfo) -> Result<Vec<u8>, String> {
        bindings::iderm::plugin::host::log(&format!(
            "example-view-tla: init for project at {}",
            project.root_path
        ));
        // single mode-flag byte: 0 = trace, 1 = full state graph. No
        // other view-state needed -- both are fixed, pure functions of
        // nothing, unlike example-view-emc's cursor position/live-
        // refresh phase.
        Ok(vec![0])
    }

    fn render(state: Vec<u8>) -> Result<Vec<ViewPrimitive>, String> {
        let graph_mode = state.first().copied().unwrap_or(0) != 0;
        if graph_mode {
            Ok(vec![ViewPrimitive::StateGraph(state_graph())])
        } else {
            Ok(vec![ViewPrimitive::Trace(TraceView { steps: counterexample() })])
        }
    }

    /// `graph` toggles between the counterexample trace and the full
    /// reachable-state graph it was drawn from. Anything else, including
    /// the reserved live-refresh token, is an unrecognized command --
    /// this example has no live/moving data, matching how a plugin
    /// that hasn't opted into live mode is meant to behave per
    /// docs/modules/Idermviz.md's "Interaction / command layer".
    fn handle_command(state: Vec<u8>, command: String) -> Result<(Vec<u8>, Vec<ViewPrimitive>), String> {
        match command.as_str() {
            "graph" => {
                let graph_mode = state.first().copied().unwrap_or(0) != 0;
                let toggled = !graph_mode;
                let new_state = vec![toggled as u8];
                let primitives = if toggled {
                    vec![ViewPrimitive::StateGraph(state_graph())]
                } else {
                    vec![ViewPrimitive::Trace(TraceView { steps: counterexample() })]
                };
                Ok((new_state, primitives))
            }
            other => Err(format!("unknown command: {other:?} (try \"graph\")")),
        }
    }
}

bindings::export!(Component with_types_in bindings);
