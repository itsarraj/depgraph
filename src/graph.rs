use std::collections::{BTreeSet, HashMap, HashSet};

use crate::manifest::CrateInfo;

/// The inter-crate dependency graph: every discovered crate is a node,
/// and an edge `a -> b` means crate `a` depends on crate `b`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Graph {
    pub nodes: Vec<String>,
    pub edges: Vec<(String, String)>,
}

/// Builds the inter-crate dependency graph from a set of discovered
/// crates. An edge only exists between two crates that were *both*
/// actually discovered - a dependency on an external crates.io package
/// like `serde` never becomes a node, since it was never one of the
/// crates handed in. A crate listing its own name as a dependency (which
/// real Cargo rejects outright) is ignored rather than turned into a
/// self-loop.
pub fn build_graph(crates: &[CrateInfo]) -> Graph {
    let names: BTreeSet<&str> = crates.iter().map(|c| c.name.as_str()).collect();

    let mut nodes: Vec<String> = names.iter().map(|s| s.to_string()).collect();
    nodes.sort();

    let mut edges = Vec::new();
    for c in crates {
        for dep in &c.dependencies {
            if dep == &c.name {
                continue;
            }
            if names.contains(dep.as_str()) {
                edges.push((c.name.clone(), dep.clone()));
            }
        }
    }
    edges.sort();
    edges.dedup();

    Graph { nodes, edges }
}

/// Finds circular dependency chains in the graph.
///
/// Returns one concrete chain per strongly-connected component of size
/// greater than one - e.g. `["a", "b", "c", "a"]` for a real cycle
/// `a -> b -> c -> a`. Membership (which crates participate in *some*
/// cycle) is always exact, since it's derived from strongly-connected
/// components; if a component contains more than one distinct
/// elementary cycle, only one representative chain through it is
/// returned, not every possible loop (see the README's scope-limits
/// section).
pub fn detect_cycles(graph: &Graph) -> Vec<Vec<String>> {
    let adj = adjacency(graph);
    let sccs = strongly_connected_components(&graph.nodes, &adj);

    let mut cycles = Vec::new();
    for scc in sccs {
        if scc.len() < 2 {
            continue;
        }
        let scc_set: HashSet<&str> = scc.iter().map(|s| s.as_str()).collect();
        if let Some(chain) = find_cycle_in_component(&scc, &adj, &scc_set) {
            cycles.push(chain);
        }
    }
    cycles.sort();
    cycles
}

fn adjacency(graph: &Graph) -> HashMap<String, Vec<String>> {
    let mut adj: HashMap<String, Vec<String>> = graph
        .nodes
        .iter()
        .map(|n| (n.clone(), Vec::new()))
        .collect();
    for (a, b) in &graph.edges {
        adj.entry(a.clone()).or_default().push(b.clone());
    }
    adj
}

fn reverse_adjacency(
    nodes: &[String],
    adj: &HashMap<String, Vec<String>>,
) -> HashMap<String, Vec<String>> {
    let mut rev: HashMap<String, Vec<String>> =
        nodes.iter().map(|n| (n.clone(), Vec::new())).collect();
    for (from, tos) in adj {
        for to in tos {
            rev.entry(to.clone()).or_default().push(from.clone());
        }
    }
    rev
}

/// Kosaraju's algorithm: a DFS pass recording finish order, then a
/// second DFS pass over the reversed graph in reverse finish order.
/// Each tree produced by the second pass is one strongly-connected
/// component.
fn strongly_connected_components(
    nodes: &[String],
    adj: &HashMap<String, Vec<String>>,
) -> Vec<Vec<String>> {
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    for n in nodes {
        if !visited.contains(n) {
            dfs_postorder(n, adj, &mut visited, &mut order);
        }
    }

    let reverse = reverse_adjacency(nodes, adj);

    let mut assigned = HashSet::new();
    let mut sccs = Vec::new();
    for n in order.into_iter().rev() {
        if assigned.contains(&n) {
            continue;
        }
        let mut component = Vec::new();
        collect_component(&n, &reverse, &mut assigned, &mut component);
        component.sort();
        sccs.push(component);
    }
    sccs
}

/// Iterative post-order DFS (an explicit stack, not recursion, so a
/// large graph can't blow the call stack).
fn dfs_postorder(
    start: &str,
    adj: &HashMap<String, Vec<String>>,
    visited: &mut HashSet<String>,
    order: &mut Vec<String>,
) {
    let mut stack = vec![(start.to_string(), 0usize)];
    visited.insert(start.to_string());
    while let Some((node, idx)) = stack.pop() {
        let neighbors = adj.get(&node).map(|v| v.as_slice()).unwrap_or(&[]);
        if idx < neighbors.len() {
            let next = neighbors[idx].clone();
            stack.push((node, idx + 1));
            if !visited.contains(&next) {
                visited.insert(next.clone());
                stack.push((next, 0));
            }
        } else {
            order.push(node);
        }
    }
}

fn collect_component(
    start: &str,
    reverse: &HashMap<String, Vec<String>>,
    assigned: &mut HashSet<String>,
    component: &mut Vec<String>,
) {
    let mut stack = vec![start.to_string()];
    while let Some(node) = stack.pop() {
        component.push(node.clone());
        if let Some(neighbors) = reverse.get(&node) {
            for n in neighbors {
                if !assigned.contains(n) {
                    assigned.insert(n.clone());
                    stack.push(n.clone());
                }
            }
        }
    }
}

/// Within one strongly-connected component (every node here is, by
/// definition, reachable from every other), greedily walks a DFS
/// restricted to the component's own nodes until it revisits a node
/// already on the current path, then returns the closed loop from that
/// point. This always terminates and always finds a cycle for a genuine
/// SCC of size > 1: every node in such a component has at least one
/// within-component outgoing edge, so the walk can only grow the path
/// (bounded by the component's size) until it's forced to close a loop.
fn find_cycle_in_component(
    scc: &[String],
    adj: &HashMap<String, Vec<String>>,
    scc_set: &HashSet<&str>,
) -> Option<Vec<String>> {
    let start = scc.first()?.clone();
    let mut path = vec![start.clone()];
    let mut on_path: HashMap<String, usize> = HashMap::new();
    on_path.insert(start.clone(), 0);

    let mut current = start;
    loop {
        let neighbors = adj.get(&current).map(|v| v.as_slice()).unwrap_or(&[]);
        let mut advanced = false;
        for next in neighbors {
            if !scc_set.contains(next.as_str()) {
                continue;
            }
            if let Some(&idx) = on_path.get(next) {
                let mut chain = path[idx..].to_vec();
                chain.push(next.clone());
                return Some(chain);
            }
            path.push(next.clone());
            on_path.insert(next.clone(), path.len() - 1);
            current = next.clone();
            advanced = true;
            break;
        }
        if !advanced {
            return None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, deps: &[&str]) -> CrateInfo {
        CrateInfo {
            name: name.to_string(),
            manifest_path: format!("{name}/Cargo.toml").into(),
            dependencies: deps.iter().map(|d| d.to_string()).collect(),
        }
    }

    #[test]
    fn external_dependencies_are_not_nodes_or_edges() {
        let crates = vec![info("a", &["serde", "anyhow"])];
        let graph = build_graph(&crates);
        assert_eq!(graph.nodes, vec!["a".to_string()]);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn internal_dependency_becomes_an_edge() {
        let crates = vec![info("a", &["b"]), info("b", &[])];
        let graph = build_graph(&crates);
        assert_eq!(graph.nodes, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(graph.edges, vec![("a".to_string(), "b".to_string())]);
    }

    #[test]
    fn self_dependency_is_ignored_not_a_self_loop() {
        let crates = vec![info("a", &["a"])];
        let graph = build_graph(&crates);
        assert!(graph.edges.is_empty());
    }

    #[test]
    fn no_cycle_in_a_simple_dag() {
        let crates = vec![info("a", &["b"]), info("b", &["c"]), info("c", &[])];
        let graph = build_graph(&crates);
        assert!(detect_cycles(&graph).is_empty());
    }

    #[test]
    fn detects_a_direct_two_crate_cycle() {
        let crates = vec![info("a", &["b"]), info("b", &["a"])];
        let graph = build_graph(&crates);
        let cycles = detect_cycles(&graph);
        assert_eq!(cycles.len(), 1);
        assert_eq!(
            cycles[0],
            vec!["a".to_string(), "b".to_string(), "a".to_string()]
        );
    }

    #[test]
    fn detects_a_three_crate_cycle() {
        let crates = vec![info("a", &["b"]), info("b", &["c"]), info("c", &["a"])];
        let graph = build_graph(&crates);
        let cycles = detect_cycles(&graph);
        assert_eq!(cycles.len(), 1);
        assert_eq!(
            cycles[0],
            vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "a".to_string()
            ]
        );
    }

    #[test]
    fn a_diamond_shape_is_not_a_cycle() {
        // a depends on b and c; both b and c depend on d. No loop.
        let crates = vec![
            info("a", &["b", "c"]),
            info("b", &["d"]),
            info("c", &["d"]),
            info("d", &[]),
        ];
        let graph = build_graph(&crates);
        assert!(detect_cycles(&graph).is_empty());
    }

    #[test]
    fn cycle_plus_unrelated_crate_only_reports_the_cyclic_ones() {
        let crates = vec![
            info("a", &["b"]),
            info("b", &["a"]),
            info("standalone", &[]),
        ];
        let graph = build_graph(&crates);
        let cycles = detect_cycles(&graph);
        assert_eq!(cycles.len(), 1);
        assert!(!cycles[0].contains(&"standalone".to_string()));
    }

    #[test]
    fn two_separate_cycles_are_both_reported() {
        let crates = vec![
            info("a", &["b"]),
            info("b", &["a"]),
            info("x", &["y"]),
            info("y", &["x"]),
        ];
        let graph = build_graph(&crates);
        let cycles = detect_cycles(&graph);
        assert_eq!(cycles.len(), 2);
    }
}
