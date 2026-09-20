use std::collections::HashSet;

use crate::graph::Graph;

/// Renders a graph as Graphviz DOT. Every discovered crate is emitted as
/// a node even if it has no edges (so an isolated crate still shows up),
/// and any edge that's part of a detected cycle is styled red/bold so
/// the cycle is visually obvious when rendered with `dot -Tpng` or
/// similar - the report on stderr already has the concrete chain, this
/// is a visual echo of the same information, not a separate source of
/// truth.
pub fn render_dot(graph: &Graph, cycles: &[Vec<String>]) -> String {
    let mut cycle_edges: HashSet<(String, String)> = HashSet::new();
    for chain in cycles {
        for pair in chain.windows(2) {
            cycle_edges.insert((pair[0].clone(), pair[1].clone()));
        }
    }

    let mut out = String::from("digraph depgraph {\n");
    for node in &graph.nodes {
        out.push_str(&format!("  \"{node}\";\n"));
    }
    for (a, b) in &graph.edges {
        if cycle_edges.contains(&(a.clone(), b.clone())) {
            out.push_str(&format!("  \"{a}\" -> \"{b}\" [color=red, penwidth=2];\n"));
        } else {
            out.push_str(&format!("  \"{a}\" -> \"{b}\";\n"));
        }
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_nodes_and_plain_edges() {
        let graph = Graph {
            nodes: vec!["a".to_string(), "b".to_string()],
            edges: vec![("a".to_string(), "b".to_string())],
        };
        let dot = render_dot(&graph, &[]);
        assert!(dot.starts_with("digraph depgraph {\n"));
        assert!(dot.contains("\"a\";\n"));
        assert!(dot.contains("\"b\";\n"));
        assert!(dot.contains("\"a\" -> \"b\";\n"));
        assert!(!dot.contains("color=red"));
        assert!(dot.trim_end().ends_with('}'));
    }

    #[test]
    fn isolated_node_with_no_edges_still_appears() {
        let graph = Graph {
            nodes: vec!["lonely".to_string()],
            edges: vec![],
        };
        let dot = render_dot(&graph, &[]);
        assert!(dot.contains("\"lonely\";\n"));
    }

    #[test]
    fn cycle_edges_are_styled_red() {
        let graph = Graph {
            nodes: vec!["a".to_string(), "b".to_string(), "c".to_string()],
            edges: vec![
                ("a".to_string(), "b".to_string()),
                ("b".to_string(), "c".to_string()),
                ("c".to_string(), "a".to_string()),
            ],
        };
        let cycles = vec![vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "a".to_string(),
        ]];
        let dot = render_dot(&graph, &cycles);
        assert!(dot.contains("\"a\" -> \"b\" [color=red"));
        assert!(dot.contains("\"b\" -> \"c\" [color=red"));
        assert!(dot.contains("\"c\" -> \"a\" [color=red"));
    }
}
