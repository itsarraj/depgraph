use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};

/// One crate discovered on disk: its declared package name, where its
/// manifest lives, and the names it lists under `[dependencies]`.
///
/// Only `[dependencies]` is read - `[dev-dependencies]` and
/// `[build-dependencies]` are deliberately excluded, since the question
/// this tool answers is "what does this crate need to actually build and
/// ship," matching what `cargo tree` calls the normal dependency kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrateInfo {
    pub name: String,
    pub manifest_path: PathBuf,
    pub dependencies: Vec<String>,
}

/// Parses one `Cargo.toml` file into a [`CrateInfo`].
pub fn parse_manifest(path: &Path) -> Result<CrateInfo> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let doc: toml::Value = text
        .parse()
        .with_context(|| format!("parsing {} as TOML", path.display()))?;

    let name = doc
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(|n| n.as_str())
        .ok_or_else(|| anyhow!("{}: missing [package].name", path.display()))?
        .to_string();

    let mut dependencies = Vec::new();
    if let Some(deps) = doc.get("dependencies").and_then(|d| d.as_table()) {
        for key in deps.keys() {
            dependencies.push(key.clone());
        }
    }
    dependencies.sort();

    Ok(CrateInfo {
        name,
        manifest_path: path.to_path_buf(),
        dependencies,
    })
}

/// Scans `dir` one level deep for `<dir>/*/Cargo.toml`, treating each as
/// an independent crate rather than a Cargo workspace - this is the
/// layout this very monorepo's own `tools/` directory uses: every
/// subdirectory is its own standalone crate, not a workspace member.
/// Subdirectories with no `Cargo.toml` (a `README.md`, a stray script, a
/// `target/` left over from a previous build one level too deep to
/// matter here) are silently skipped, not treated as errors.
pub fn scan_dir(dir: &Path) -> Result<Vec<CrateInfo>> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading directory {}", dir.display()))?
        .collect::<std::result::Result<Vec<_>, _>>()
        .with_context(|| format!("reading directory {}", dir.display()))?
        .into_iter()
        .map(|e| e.path())
        .collect();
    entries.sort();

    let mut crates = Vec::new();
    for path in entries {
        if !path.is_dir() {
            continue;
        }
        let manifest = path.join("Cargo.toml");
        if manifest.is_file() {
            crates.push(parse_manifest(&manifest)?);
        }
    }
    Ok(crates)
}

/// Parses a Cargo workspace root manifest's `[workspace].members`,
/// resolves each member to a directory, and reads each member's own
/// `Cargo.toml`.
///
/// Member entries are either a plain relative path (`"crates/foo"`) or a
/// single trailing `/*` glob one level deep (`"crates/*"`), which is
/// resolved by listing that directory's own immediate subdirectories
/// that contain a `Cargo.toml`. More exotic glob patterns (`**`, a glob
/// in the middle of the path, brace expansion) are not supported - see
/// the README's scope-limits section.
pub fn scan_workspace(root_manifest: &Path) -> Result<Vec<CrateInfo>> {
    let text = fs::read_to_string(root_manifest)
        .with_context(|| format!("reading {}", root_manifest.display()))?;
    let doc: toml::Value = text
        .parse()
        .with_context(|| format!("parsing {} as TOML", root_manifest.display()))?;

    let members = doc
        .get("workspace")
        .and_then(|w| w.get("members"))
        .and_then(|m| m.as_array())
        .ok_or_else(|| anyhow!("{}: missing [workspace].members", root_manifest.display()))?;

    let base = root_manifest
        .parent()
        .ok_or_else(|| anyhow!("{}: has no parent directory", root_manifest.display()))?;

    let mut member_dirs: Vec<PathBuf> = Vec::new();
    for m in members {
        let pattern = m.as_str().ok_or_else(|| {
            anyhow!(
                "{}: [workspace].members entry is not a string",
                root_manifest.display()
            )
        })?;

        if let Some(prefix) = pattern.strip_suffix("/*") {
            let glob_base = base.join(prefix);
            let mut children: Vec<PathBuf> = fs::read_dir(&glob_base)
                .with_context(|| format!("reading {}", glob_base.display()))?
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .map(|e| e.path())
                .collect();
            children.sort();
            for child in children {
                if child.is_dir() && child.join("Cargo.toml").is_file() {
                    member_dirs.push(child);
                }
            }
        } else {
            member_dirs.push(base.join(pattern));
        }
    }

    let mut crates = Vec::new();
    for dir in member_dirs {
        let manifest = dir.join("Cargo.toml");
        crates.push(parse_manifest(&manifest)?);
    }
    Ok(crates)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    fn write_manifest(dir: &Path, name: &str, deps: &[&str]) {
        let deps_block = deps
            .iter()
            .map(|d| format!("{d} = \"1\""))
            .collect::<Vec<_>>()
            .join("\n");
        let contents = format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\n{deps_block}\n"
        );
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("Cargo.toml"), contents).unwrap();
    }

    #[test]
    fn parses_package_name_and_dependency_names() {
        let dir = tempdir().unwrap();
        write_manifest(dir.path(), "foo", &["serde", "anyhow"]);
        let info = parse_manifest(&dir.path().join("Cargo.toml")).unwrap();
        assert_eq!(info.name, "foo");
        assert_eq!(
            info.dependencies,
            vec!["anyhow".to_string(), "serde".to_string()]
        );
    }

    #[test]
    fn parses_manifest_with_inline_table_and_path_dependencies() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        let contents = r#"
[package]
name = "bar"
version = "0.1.0"
edition = "2021"

[dependencies]
clap = { version = "4", features = ["derive"] }
localcrate = { path = "../localcrate" }
"#;
        fs::write(dir.path().join("Cargo.toml"), contents).unwrap();
        let info = parse_manifest(&dir.path().join("Cargo.toml")).unwrap();
        assert_eq!(info.name, "bar");
        assert_eq!(
            info.dependencies,
            vec!["clap".to_string(), "localcrate".to_string()]
        );
    }

    #[test]
    fn missing_package_name_is_an_error_not_a_panic() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(dir.path().join("Cargo.toml"), "[dependencies]\n").unwrap();
        let result = parse_manifest(&dir.path().join("Cargo.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn manifest_with_no_dependencies_table_yields_empty_deps() {
        let dir = tempdir().unwrap();
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"nodep\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        let info = parse_manifest(&dir.path().join("Cargo.toml")).unwrap();
        assert!(info.dependencies.is_empty());
    }

    #[test]
    fn scan_dir_finds_each_immediate_subdirectory_crate() {
        let root = tempdir().unwrap();
        write_manifest(&root.path().join("crate-a"), "crate-a", &["crate-b"]);
        write_manifest(&root.path().join("crate-b"), "crate-b", &[]);
        fs::write(root.path().join("README.md"), "not a crate").unwrap();
        fs::create_dir_all(root.path().join("empty-dir")).unwrap();

        let crates = scan_dir(root.path()).unwrap();
        let mut names: Vec<&str> = crates.iter().map(|c| c.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["crate-a", "crate-b"]);
    }

    #[test]
    fn scan_workspace_resolves_explicit_members_and_star_glob() {
        let root = tempdir().unwrap();
        write_manifest(&root.path().join("crates/foo"), "foo", &[]);
        write_manifest(&root.path().join("crates/bar"), "bar", &["foo"]);
        write_manifest(&root.path().join("standalone"), "standalone", &[]);

        let root_manifest = root.path().join("Cargo.toml");
        fs::write(
            &root_manifest,
            "[workspace]\nmembers = [\"crates/*\", \"standalone\"]\n",
        )
        .unwrap();

        let crates = scan_workspace(&root_manifest).unwrap();
        let mut names: Vec<&str> = crates.iter().map(|c| c.name.as_str()).collect();
        names.sort();
        assert_eq!(names, vec!["bar", "foo", "standalone"]);
    }
}
