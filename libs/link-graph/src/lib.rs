//! link-property graph calculator -- the `docs/roadmap/Workbenches.md`
//! protocol/systems analysis tooling item: "bandwidth/latency/loss-
//! aware routing exploration over a declared transport-interface
//! struct." Three real, independently-established routing problems
//! over the same directed graph of links, each solved by a Dijkstra-
//! style greedy relaxation -- not one algorithm reused three times
//! blindly, but three different *semirings* Dijkstra's greedy
//! correctness argument still holds for:
//!
//! - **latency**: minimize the *sum* of edge latencies (the textbook
//!   shortest-path problem Dijkstra was written for).
//! - **bandwidth**: maximize the *minimum* edge bandwidth along the
//!   path (the "widest path"/maximum-capacity-path problem — greedy
//!   still works because `min` is associative/monotone the same way
//!   `+` is).
//! - **reliability**: maximize the *product* of each edge's survival
//!   probability `(1 - loss)` -- equivalent, via a log transform, to
//!   minimizing a sum of non-negative weights (`-log(survival)`),
//!   which is exactly shortest-path again. Implemented directly in
//!   the multiplicative domain not literally logging, but the
//!   correctness argument is the log-transform one -- Dijkstra's own
//!   greedy-choice proof, not a heuristic.
//!
//! no abstraction unifying the three into one generic "semiring
//! solver" -- three concrete, separately readable functions instead,
//! each auditable against its own textbook definition without an
//! extra layer of indirection to see through first.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub bandwidth_mbps: f64,
    pub latency_ms: f64,
    pub loss_percent: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Graph {
    pub edges: Vec<Edge>,
}

impl Graph {
    /// every node named by at least one edge endpoint, in first-seen
    /// order -- this graph has no separate node list, a node exists
    /// purely by being named in an edge.
    pub fn nodes(&self) -> Vec<String> {
        let mut seen = Vec::new();
        for edge in &self.edges {
            for node in [&edge.from, &edge.to] {
                if !seen.contains(node) {
                    seen.push(node.clone());
                }
            }
        }
        seen
    }

    fn edges_from<'a>(&'a self, node: &'a str) -> impl Iterator<Item = (usize, &'a Edge)> {
        self.edges.iter().enumerate().filter(move |(_, e)| e.from == node)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PathResult {
    pub nodes: Vec<String>,
    pub edge_indices: Vec<usize>,
    pub total_latency_ms: f64,
    pub bottleneck_bandwidth_mbps: f64,
    pub compound_loss_percent: f64,
}

fn path_metrics(graph: &Graph, edge_indices: &[usize]) -> (f64, f64, f64) {
    let mut total_latency_ms = 0.0;
    let mut bottleneck_bandwidth_mbps = f64::INFINITY;
    let mut survival = 1.0;
    for &i in edge_indices {
        let edge = &graph.edges[i];
        total_latency_ms += edge.latency_ms;
        bottleneck_bandwidth_mbps = bottleneck_bandwidth_mbps.min(edge.bandwidth_mbps);
        survival *= 1.0 - edge.loss_percent / 100.0;
    }
    (total_latency_ms, bottleneck_bandwidth_mbps, (1.0 - survival) * 100.0)
}

fn reconstruct(graph: &Graph, from: &str, to: &str, prev: &HashMap<String, (String, usize)>) -> Option<PathResult> {
    if from == to {
        return Some(PathResult {
            nodes: vec![from.to_string()],
            edge_indices: Vec::new(),
            total_latency_ms: 0.0,
            bottleneck_bandwidth_mbps: f64::INFINITY,
            compound_loss_percent: 0.0,
        });
    }
    let mut nodes = vec![to.to_string()];
    let mut edge_indices = Vec::new();
    let mut current = to.to_string();
    while current != from {
        let (prev_node, edge_idx) = prev.get(&current)?;
        edge_indices.push(*edge_idx);
        nodes.push(prev_node.clone());
        current = prev_node.clone();
    }
    nodes.reverse();
    edge_indices.reverse();
    let (total_latency_ms, bottleneck_bandwidth_mbps, compound_loss_percent) = path_metrics(graph, &edge_indices);
    Some(PathResult { nodes, edge_indices, total_latency_ms, bottleneck_bandwidth_mbps, compound_loss_percent })
}

/// `O(V^2)` node selection (no priority queue) -- auditable against
/// dijkstra's own textbook definition line by line, same "slower but
/// legible" stance `checksums`' bit-by-bit CRC implementations already
/// take; the graphs any example plugin here processes are nowhere near
/// where that would matter.
pub fn shortest_latency_path(graph: &Graph, from: &str, to: &str) -> Option<PathResult> {
    let nodes = graph.nodes();
    let mut dist: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), f64::INFINITY)).collect();
    let mut visited: HashMap<String, bool> = nodes.iter().map(|n| (n.clone(), false)).collect();
    let mut prev: HashMap<String, (String, usize)> = HashMap::new();
    dist.insert(from.to_string(), 0.0);

    loop {
        let current =
            nodes.iter().filter(|n| !visited[*n] && dist[*n].is_finite()).min_by(|a, b| dist[*a].total_cmp(&dist[*b])).cloned();
        let Some(current) = current else { break };
        if current == to {
            break;
        }
        visited.insert(current.clone(), true);
        for (idx, edge) in graph.edges_from(&current) {
            let candidate = dist[&current] + edge.latency_ms;
            if candidate < dist[&edge.to] {
                dist.insert(edge.to.clone(), candidate);
                prev.insert(edge.to.clone(), (current.clone(), idx));
            }
        }
    }
    reconstruct(graph, from, to, &prev)
}

/// "widest path"/maximum-capacity-path problem -- maximizes the
/// *minimum* edge bandwidth along the path, not the sum. Real routing
/// relevance: the bottleneck link, not the total, is what actually
/// caps a path's throughput.
pub fn widest_bandwidth_path(graph: &Graph, from: &str, to: &str) -> Option<PathResult> {
    let nodes = graph.nodes();
    let mut width: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), 0.0)).collect();
    let mut visited: HashMap<String, bool> = nodes.iter().map(|n| (n.clone(), false)).collect();
    let mut prev: HashMap<String, (String, usize)> = HashMap::new();
    width.insert(from.to_string(), f64::INFINITY);

    loop {
        let current =
            nodes.iter().filter(|n| !visited[*n] && width[*n] > 0.0).max_by(|a, b| width[*a].total_cmp(&width[*b])).cloned();
        let Some(current) = current else { break };
        if current == to {
            break;
        }
        visited.insert(current.clone(), true);
        for (idx, edge) in graph.edges_from(&current) {
            let candidate = width[&current].min(edge.bandwidth_mbps);
            if candidate > width[&edge.to] {
                width.insert(edge.to.clone(), candidate);
                prev.insert(edge.to.clone(), (current.clone(), idx));
            }
        }
    }
    reconstruct(graph, from, to, &prev)
}

/// maximizes the *product* of each edge's survival probability
/// `(1 - loss/100)` -- equivalent to minimizing compound packet loss
/// across the path. Correct for the same reason `shortest_latency_
/// path` is: substitute `-log(survival)` for each edge weight and this
/// is ordinary non-negative-weight shortest path, so Dijkstra's greedy
/// choice still holds; this function just stays in the multiplicative
/// domain instead of literally taking logs.
pub fn most_reliable_path(graph: &Graph, from: &str, to: &str) -> Option<PathResult> {
    let nodes = graph.nodes();
    let mut survival: HashMap<String, f64> = nodes.iter().map(|n| (n.clone(), 0.0)).collect();
    let mut visited: HashMap<String, bool> = nodes.iter().map(|n| (n.clone(), false)).collect();
    let mut prev: HashMap<String, (String, usize)> = HashMap::new();
    survival.insert(from.to_string(), 1.0);

    loop {
        let current = nodes
            .iter()
            .filter(|n| !visited[*n] && survival[*n] > 0.0)
            .max_by(|a, b| survival[*a].total_cmp(&survival[*b]))
            .cloned();
        let Some(current) = current else { break };
        if current == to {
            break;
        }
        visited.insert(current.clone(), true);
        for (idx, edge) in graph.edges_from(&current) {
            let candidate = survival[&current] * (1.0 - edge.loss_percent / 100.0);
            if candidate > survival[&edge.to] {
                survival.insert(edge.to.clone(), candidate);
                prev.insert(edge.to.clone(), (current.clone(), idx));
            }
        }
    }
    reconstruct(graph, from, to, &prev)
}

/// `edge,from,to,bandwidth_mbps,latency_ms,loss_percent` and
/// `query,from,to` lines -- `#` comments/blank lines skipped, same
/// plain-text convention every prior project-authored format in this
/// codebase already uses.
pub fn parse_transport_graph(text: &str) -> Result<(Graph, Vec<(String, String)>), String> {
    let mut edges = Vec::new();
    let mut queries = Vec::new();

    for (line_no, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split(',').map(str::trim).collect();
        match fields.as_slice() {
            ["edge", from, to, bandwidth, latency, loss] => {
                let parse_f64 = |field: &str, name: &str| -> Result<f64, String> {
                    field.parse::<f64>().map_err(|_| format!("line {}: invalid {name} {field:?}", line_no + 1))
                };
                edges.push(Edge {
                    from: from.to_string(),
                    to: to.to_string(),
                    bandwidth_mbps: parse_f64(bandwidth, "bandwidth_mbps")?,
                    latency_ms: parse_f64(latency, "latency_ms")?,
                    loss_percent: parse_f64(loss, "loss_percent")?,
                });
            }
            ["query", from, to] => queries.push((from.to_string(), to.to_string())),
            _ => {
                return Err(format!(
                    "line {}: expected \"edge,from,to,bandwidth,latency,loss\" or \"query,from,to\", got {line:?}",
                    line_no + 1
                ))
            }
        }
    }

    if edges.is_empty() {
        return Err("no \"edge,...\" lines declared".to_string());
    }
    Ok((Graph { edges }, queries))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// hand-constructed graph where a direct hop wins on latency but
    /// loses on both bandwidth and reliability to a 2-hop alternative --
    /// proves the three functions actually optimize *different*
    /// objectives, not just the same shortest-path three times over.
    ///
    /// direct A->C: latency 10ms, bottleneck 10 Mbps, loss 5%
    /// via B (A->B->C): latency 60ms, bottleneck 100 Mbps,
    ///   compound loss = 1 - (0.999 * 0.999) = 0.1999% (hand-computed)
    fn sample_graph() -> Graph {
        Graph {
            edges: vec![
                Edge { from: "A".into(), to: "C".into(), bandwidth_mbps: 10.0, latency_ms: 10.0, loss_percent: 5.0 },
                Edge { from: "A".into(), to: "B".into(), bandwidth_mbps: 100.0, latency_ms: 30.0, loss_percent: 0.1 },
                Edge { from: "B".into(), to: "C".into(), bandwidth_mbps: 100.0, latency_ms: 30.0, loss_percent: 0.1 },
            ],
        }
    }

    fn assert_close(actual: f64, expected: f64, tolerance: f64) {
        assert!((actual - expected).abs() <= tolerance, "expected ~{expected}, got {actual}");
    }

    #[test]
    fn shortest_latency_picks_the_direct_hop() {
        let result = shortest_latency_path(&sample_graph(), "A", "C").unwrap();
        assert_eq!(result.nodes, vec!["A", "C"]);
        assert_close(result.total_latency_ms, 10.0, 1e-9);
        assert_close(result.bottleneck_bandwidth_mbps, 10.0, 1e-9);
        assert_close(result.compound_loss_percent, 5.0, 1e-9);
    }

    #[test]
    fn widest_bandwidth_picks_the_two_hop_route() {
        let result = widest_bandwidth_path(&sample_graph(), "A", "C").unwrap();
        assert_eq!(result.nodes, vec!["A", "B", "C"]);
        assert_close(result.bottleneck_bandwidth_mbps, 100.0, 1e-9);
        assert_close(result.total_latency_ms, 60.0, 1e-9);
    }

    #[test]
    fn most_reliable_picks_the_two_hop_route_with_the_hand_computed_loss() {
        let result = most_reliable_path(&sample_graph(), "A", "C").unwrap();
        assert_eq!(result.nodes, vec!["A", "B", "C"]);
        // 1 - (0.999 * 0.999) = 0.199900...%
        assert_close(result.compound_loss_percent, 0.1999, 1e-3);
    }

    #[test]
    fn same_start_and_end_is_a_trivial_zero_cost_path() {
        let result = shortest_latency_path(&sample_graph(), "A", "A").unwrap();
        assert_eq!(result.nodes, vec!["A"]);
        assert_eq!(result.edge_indices.len(), 0);
        assert_close(result.total_latency_ms, 0.0, 1e-9);
    }

    #[test]
    fn an_unreachable_node_has_no_path() {
        let graph = Graph { edges: vec![Edge { from: "A".into(), to: "B".into(), bandwidth_mbps: 10.0, latency_ms: 1.0, loss_percent: 0.0 }] };
        assert_eq!(shortest_latency_path(&graph, "A", "Z"), None);
        assert_eq!(widest_bandwidth_path(&graph, "A", "Z"), None);
        assert_eq!(most_reliable_path(&graph, "A", "Z"), None);
    }

    #[test]
    fn parse_transport_graph_reads_a_real_declaration() {
        let text = "\
# sample topology
edge,A,C,10,10,5
edge,A,B,100,30,0.1
edge,B,C,100,30,0.1
query,A,C
";
        let (graph, queries) = parse_transport_graph(text).unwrap();
        assert_eq!(graph.edges.len(), 3);
        assert_eq!(queries, vec![("A".to_string(), "C".to_string())]);
        assert_eq!(graph.nodes(), vec!["A", "C", "B"]);
    }

    #[test]
    fn parse_transport_graph_rejects_a_malformed_line() {
        let err = parse_transport_graph("edge,A,B,10\n").unwrap_err();
        assert!(err.contains("line 1"), "error should name the bad line: {err}");
    }

    #[test]
    fn parse_transport_graph_rejects_no_edges() {
        let err = parse_transport_graph("query,A,B\n").unwrap_err();
        assert!(err.contains("edge"), "error should say no edges were declared: {err}");
    }
}
