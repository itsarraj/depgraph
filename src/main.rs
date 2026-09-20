use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;

use depgraph::{build_graph, detect_cycles, render_dot, scan_dir, scan_workspace};

#[derive(Parser)]
#[command(
    name = "depgraph",
    about = "Builds an inter-crate dependency graph (Graphviz DOT) and reports circular dependency chains"
)]
struct Cli {
    /// A directory containing one crate per immediate subdirectory (this
    /// monorepo's own tools/ layout) - each <dir>/Cargo.toml is read
    /// independently, not as a Cargo workspace.
    #[arg(long, conflicts_with = "workspace")]
    dir: Option<PathBuf>,

    /// Path to a Cargo workspace root Cargo.toml; its [workspace].members
    /// are resolved and each member's own Cargo.toml is read.
    #[arg(long, conflicts_with = "dir")]
    workspace: Option<PathBuf>,

    /// Print only the circular-dependency report; skip the DOT graph.
    #[arg(long)]
    cycles_only: bool,
}

fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse();

    let crates = match (&cli.dir, &cli.workspace) {
        (Some(dir), None) => scan_dir(dir)?,
        (None, Some(ws)) => scan_workspace(ws)?,
        (None, None) => anyhow::bail!("pass one of --dir <path> or --workspace <Cargo.toml>"),
        (Some(_), Some(_)) => unreachable!("clap enforces conflicts_with"),
    };

    let graph = build_graph(&crates);
    let cycles = detect_cycles(&graph);

    if !cli.cycles_only {
        print!("{}", render_dot(&graph, &cycles));
    }

    if cycles.is_empty() {
        eprintln!(
            "no circular dependencies found ({} crates, {} internal edges)",
            graph.nodes.len(),
            graph.edges.len()
        );
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!("circular dependencies found:");
        for chain in &cycles {
            eprintln!("  {}", chain.join(" -> "));
        }
        Ok(ExitCode::FAILURE)
    }
}
