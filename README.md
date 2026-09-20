# depgraph

Builds a dependency graph between Rust crates - either the members of a
real Cargo workspace, or (the more unusual case this tool was actually
built for) a directory of standalone `Cargo.toml` files that *aren't* a
workspace at all, like this very monorepo's own `tools/` directory,
where every subdirectory is its own independently-versioned crate.
Renders the graph as Graphviz DOT and reports any circular dependency
chains it finds. `cargo tree` shows one crate's own dependency tree
(including every transitive crates.io dependency); this instead answers
"which of *my own* crates depend on which other of my own crates," and
nothing outside that set becomes a graph node.

## Usage

```bash
depgraph --dir tools/                       # this monorepo's own layout: <dir>/*/Cargo.toml, one per crate
depgraph --workspace path/to/Cargo.toml     # a real Cargo workspace root, reads [workspace].members
depgraph --dir tools/ --cycles-only         # skip the DOT graph, just report cycles
```

DOT goes to stdout, the human-readable cycle report (or "none found")
goes to stderr, and the exit code is `1` if any circular dependency was
found, `0` otherwise - scriptable in CI the same way `structdiff` and
`commitguard` already are in this monorepo.

```
$ depgraph --dir fixture/
digraph depgraph {
  "cyc-a";
  "cyc-b";
  "cyc-c";
  "cyc-a" -> "cyc-b" [color=red, penwidth=2];
  "cyc-b" -> "cyc-c" [color=red, penwidth=2];
  "cyc-c" -> "cyc-a" [color=red, penwidth=2];
}
circular dependencies found:
  cyc-a -> cyc-b -> cyc-c -> cyc-a
```

Edges that are part of a detected cycle render in red/bold so `dot
-Tpng` output makes the loop obvious at a glance, not just in the text
report.

Only `[dependencies]` is read from each manifest, not
`[dev-dependencies]`/`[build-dependencies]` - the graph answers "what
does this crate need to actually ship," matching `cargo tree`'s default
dependency kind. An edge only exists between two crates that were both
actually discovered; a dependency on `serde` or `clap` never becomes a
graph node because it's external to the set being graphed.

## How the two modes differ

- `--dir <path>` scans one level deep for `<path>/*/Cargo.toml` and
  treats every hit as an independent crate. This is the mode for a
  directory like `tools/` that deliberately *isn't* a Cargo workspace
  (each tool has its own `Cargo.toml`, no shared root, no shared
  lockfile - see this monorepo's own top-level `tools/README.md` for
  why).
- `--workspace <Cargo.toml>` reads a real workspace root's
  `[workspace].members`, resolving each entry either as a plain
  relative path (`"crates/foo"`) or a single trailing `/*` glob one
  level deep (`"crates/*"`). More exotic glob shapes - `**`, a glob in
  the middle of a path, brace expansion - are not supported; see scope
  limits below.

## Status: built and verified against both a real 84-crate directory and a real synthetic cycle on disk

- **20 unit tests** (`cargo test --lib`): manifest parsing (plain
  version strings, inline-table dependencies with `features`, `path`
  dependencies, a missing `[package].name` failing cleanly instead of
  panicking, a manifest with no `[dependencies]` table at all yielding
  an empty list); `--dir` scanning correctly skipping non-crate entries
  (a `README.md`, an empty directory with no `Cargo.toml`) instead of
  erroring; `--workspace` resolving both an explicit member path and a
  `crates/*` glob in the same run; graph construction correctly turning
  only *internal* dependencies into edges while dropping external ones
  like `serde`, and treating a crate that (invalidly) lists itself as a
  dependency as no-op rather than a spurious self-loop; cycle detection
  on a plain DAG (no false positive), a direct 2-crate cycle, a 3-crate
  cycle, a diamond-shaped dependency (`a` depends on both `b` and `c`,
  both depend on `d` - correctly *not* a cycle), a cycle coexisting with
  an unrelated standalone crate (only the cyclic ones get reported), and
  two independent cycles in the same graph both being found; DOT
  rendering (isolated nodes with no edges still appear, cycle edges get
  the red/bold style, non-cycle edges don't).
- **Live-dogfooded against this actual monorepo's real `tools/`
  directory** (not a toy fixture) via `--dir`: **84 real crates**
  discovered, cross-referencing every one of their real
  `[dependencies]` tables. Result: **0 internal edges, 0 cycles** - the
  honest correct answer, since (independently verified by grepping
  every `tools/*/Cargo.toml` for a `path = "../"` dependency across the
  whole directory) none of these crates actually depend on each other
  today; each is genuinely standalone, exactly as this monorepo's own
  top-level README describes. This is locked down as a real
  `#[test]` (`real_monorepo_tools_directory_has_no_cycles`, sanity-
  asserting at least 50 crates were found so the test can't silently
  pass against an empty/misconfigured directory) rather than only
  eyeballed once from the CLI.
- **A real synthetic 3-crate cycle, written to real files on disk in a
  temp directory** (not an in-memory graph fixture) - `cyc-a` depends on
  `cyc-b`, `cyc-b` depends on `cyc-c`, `cyc-c` depends back on `cyc-a` -
  read back through the exact same `--dir` scanning path used against
  the real monorepo above, and the tool correctly found the one real
  cycle: `cyc-a -> cyc-b -> cyc-c -> cyc-a`. Reproduced the identical
  result live through the actual built CLI binary (shown above), not
  just the library-level test.

**Not done / deliberately deferred**: cycle detection returns one
representative concrete chain per strongly-connected component, not
every distinct elementary cycle a component might contain - if three
crates form two overlapping loops that share an edge, only one of those
loops is printed (the component's membership, i.e. which crates are
involved in *some* cycle, is still exactly correct - it's derived from
strongly-connected components directly, not from the one printed
chain). Workspace member globs support only a single trailing `/*`, not
`**` or mid-path wildcards. `[dev-dependencies]` and
`[build-dependencies]` are never considered, even though a dev-only
circular dependency is possible in real Cargo. Version constraints are
ignored entirely - if two discovered crates happen to share a name (not
possible within one real `--dir` scan or workspace, but nothing stops
two *different* `--dir`/`--workspace` invocations from being compared
by hand) there is no version-aware disambiguation.
