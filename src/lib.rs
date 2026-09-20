pub mod dot;
pub mod graph;
pub mod manifest;

pub use dot::render_dot;
pub use graph::{build_graph, detect_cycles, Graph};
pub use manifest::{parse_manifest, scan_dir, scan_workspace, CrateInfo};

#[cfg(test)]
mod live_tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    use tempfile::tempdir;

    /// Dogfoods against this actual monorepo's own `tools/` directory -
    /// 79+ real, independently-versioned crates, none of which declare a
    /// path dependency on any other (verified separately by grepping
    /// every `tools/*/Cargo.toml` for a `path = "../"` dependency: zero
    /// matches). So the honest, correct answer for this real directory is
    /// zero internal edges and zero cycles - this test locks that in
    /// rather than asserting something more dramatic than what's
    /// actually true of this repo today.
    #[test]
    fn real_monorepo_tools_directory_has_no_cycles() {
        let depgraph_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let tools_dir = depgraph_dir
            .parent()
            .expect("depgraph lives at tools/depgraph");

        let crates = scan_dir(tools_dir).expect("scanning the real tools/ directory");
        // Sanity: this is genuinely a big real scan, not an empty directory.
        assert!(
            crates.len() >= 50,
            "expected dozens of real crates under tools/, found {}",
            crates.len()
        );

        let graph = build_graph(&crates);
        let cycles = detect_cycles(&graph);
        assert!(
            cycles.is_empty(),
            "expected no circular dependencies among this repo's real tools, found: {cycles:?}"
        );
    }

    /// Writes a real 3-crate fixture to a real temp directory on disk -
    /// cyc-a depends on cyc-b, cyc-b depends on cyc-c, cyc-c depends back
    /// on cyc-a - and confirms depgraph actually finds that cycle by
    /// reading the real files, not a synthetic in-memory graph.
    #[test]
    fn synthetic_three_crate_cycle_is_detected_from_real_files_on_disk() {
        let root = tempdir().unwrap();

        let write = |name: &str, dep: &str| {
            let dir = root.path().join(name);
            fs::create_dir_all(&dir).unwrap();
            let contents = format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{dep} = \"0.1.0\"\n"
            );
            fs::write(dir.join("Cargo.toml"), contents).unwrap();
        };
        write("cyc-a", "cyc-b");
        write("cyc-b", "cyc-c");
        write("cyc-c", "cyc-a");

        let crates = scan_dir(root.path()).expect("scanning the synthetic fixture directory");
        assert_eq!(crates.len(), 3);

        let graph = build_graph(&crates);
        let cycles = detect_cycles(&graph);

        assert_eq!(
            cycles.len(),
            1,
            "expected exactly one 3-crate cycle, found: {cycles:?}"
        );
        assert_eq!(
            cycles[0],
            vec![
                "cyc-a".to_string(),
                "cyc-b".to_string(),
                "cyc-c".to_string(),
                "cyc-a".to_string(),
            ]
        );
    }
}
