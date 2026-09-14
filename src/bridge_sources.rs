//! Purpose:
//! Checks bridge archives against their local transitive Cargo source inputs.
//!
//! Called from:
//! - BridgeStaticlib::sources_are_newer_than and the codegen test bridge builder.
//!
//! Key details:
//! - Separate staticlibs embed shared contracts, so dependency edits invalidate each owner.
//! - Normal/build and target-specific path dependencies participate; dev dependencies do not.
//! - Optional dependencies conservatively participate, while Cargo decides what to rebuild.
//! - Build output is ignored, and canonical directory identities bound dependency cycles.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Checks workspace configuration, the bridge itself, and all reachable local build dependencies.
pub fn local_inputs_newer_than(workspace: &Path, crate_name: &str, built_at: SystemTime) -> bool {
    for name in ["Cargo.toml", "Cargo.lock", ".cargo/config.toml", ".cargo/config"] {
        if file_newer_than(&workspace.join(name), built_at) { return true; }
    }
    let workspace_manifest = read_manifest(&workspace.join("Cargo.toml"));
    let mut pending = vec![workspace.join("crates").join(crate_name)];
    let mut visited = HashSet::new();
    while let Some(directory) = pending.pop() {
        let Ok(directory) = std::fs::canonicalize(directory) else { continue; };
        if !visited.insert(directory.clone()) { continue; }
        if any_file_newer_than(&directory, built_at) { return true; }
        let Some(manifest) = read_manifest(&directory.join("Cargo.toml")) else { continue; };
        add_dependencies(&manifest, &directory, workspace, workspace_manifest.as_ref(), &mut pending);
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            for target in targets.values() {
                add_dependencies(target, &directory, workspace, workspace_manifest.as_ref(), &mut pending);
            }
        }
    }
    false
}

/// Collects normal and build path dependencies, resolving workspace-inherited declarations.
fn add_dependencies(table: &toml::Value, directory: &Path, workspace: &Path, workspace_manifest: Option<&toml::Value>, pending: &mut Vec<PathBuf>) {
    for section in ["dependencies", "build-dependencies"] {
        let Some(dependencies) = table.get(section).and_then(toml::Value::as_table) else { continue; };
        for (name, dependency) in dependencies {
            let (dependency, base) = if dependency.get("workspace").and_then(toml::Value::as_bool) == Some(true) {
                let Some(inherited) = workspace_manifest.and_then(|manifest| manifest.get("workspace"))
                    .and_then(|value| value.get("dependencies")).and_then(|value| value.get(name)) else { continue; };
                (inherited, workspace)
            } else { (dependency, directory) };
            if let Some(path) = dependency.get("path").and_then(toml::Value::as_str) { pending.push(base.join(path)); }
        }
    }
}

/// Reads metadata without spawning Cargo for every generated-program link.
fn read_manifest(path: &Path) -> Option<toml::Value> {
    toml::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// Returns whether a readable file changed after the archive was produced.
fn file_newer_than(path: &Path, instant: SystemTime) -> bool {
    std::fs::metadata(path).and_then(|metadata| metadata.modified()).is_ok_and(|modified| modified > instant)
}

/// Walks source directories while excluding nested Cargo outputs and directory symlinks.
pub(crate) fn any_file_newer_than(directory: &Path, instant: SystemTime) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else { return false; };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else { continue; };
        if kind.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") { continue; }
            if any_file_newer_than(&path, instant) { return true; }
        } else if file_newer_than(&path, instant) { return true; }
    }
    false
}

#[cfg(test)]
mod tests;
