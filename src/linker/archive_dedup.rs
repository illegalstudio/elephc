//! Purpose:
//! Deduplicates Rust object members across multiple whole-archived bridges on macOS.
//! Keeps the platform-specific archive surgery separate from command rendering.
//!
//! Called from:
//! - `crate::linker` immediately before rendering a macOS linker command.
//!
//! Key details:
//! - Deduplication is best-effort and falls back to the original bridge archive.
//! - Only whole-archived bridge inputs participate; managed native archives are untouched.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use crate::link_plan::{LinkItem, LinkOrigin, LinkPlan};

/// A possibly rewritten plan and the temporary directory that owns rewritten archives.
pub(super) struct PreparedArchives {
    /// Plan whose later whole-archive bridges may point at deduplicated copies.
    pub(super) plan: LinkPlan,
    scratch: Option<PathBuf>,
}

impl PreparedArchives {
    /// Removes temporary archive copies after the linker has consumed the plan.
    pub(super) fn cleanup(self) {
        if let Some(scratch) = self.scratch {
            let _ = std::fs::remove_dir_all(scratch);
        }
    }
}

/// Prepares deduplicated copies when a plan force-loads two or more Rust bridges.
pub(super) fn prepare(plan: &LinkPlan) -> PreparedArchives {
    let whole_archives: Vec<PathBuf> = plan
        .items()
        .iter()
        .filter_map(|item| match item {
            LinkItem::StaticArchive {
                path,
                whole_archive: true,
                origin: LinkOrigin::Bridge { .. },
            } => Some(path.clone()),
            _ => None,
        })
        .collect();
    if whole_archives.len() < 2 {
        return PreparedArchives {
            plan: plan.clone(),
            scratch: None,
        };
    }

    let scratch = std::env::temp_dir().join(format!("elephc-link-dedup-{}", process::id()));
    let mut provider_names = HashSet::new();
    let mut provider_symbols = HashSet::new();
    let mut replacements = HashMap::new();

    for (index, archive) in whole_archives.iter().enumerate() {
        if index == 0 {
            if let Some(names) = ar_members(archive) {
                provider_names.extend(names);
            }
            for (_, symbols) in nm_member_globals(archive) {
                provider_symbols.extend(symbols);
            }
        } else if let Some(stripped) = dedup_macos_archive(
            archive,
            &mut provider_names,
            &mut provider_symbols,
            &scratch,
        ) {
            replacements.insert(archive.clone(), stripped);
        }
    }

    let items = plan
        .items()
        .iter()
        .cloned()
        .map(|item| replace_archive(item, &replacements))
        .collect();
    PreparedArchives {
        plan: LinkPlan::from_items(items),
        scratch: Some(scratch),
    }
}

/// Replaces one whole bridge archive path while preserving all typed metadata.
fn replace_archive(item: LinkItem, replacements: &HashMap<PathBuf, PathBuf>) -> LinkItem {
    match item {
        LinkItem::StaticArchive {
            path,
            whole_archive,
            origin,
        } => LinkItem::StaticArchive {
            path: replacements.get(&path).cloned().unwrap_or(path),
            whole_archive,
            origin,
        },
        other => other,
    }
}

/// Lists object member names in an archive through `ar t`.
fn ar_members(archive: &Path) -> Option<Vec<String>> {
    let output = Command::new("ar").arg("t").arg(archive).output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(|line| line.trim().to_string())
            .filter(|line| {
                !line.is_empty() && line != "__.SYMDEF" && line != "__.SYMDEF SORTED"
            })
            .collect(),
    )
}

/// Parses the readable member headers and global symbols emitted by macOS `nm -gU`.
fn nm_member_globals(archive: &Path) -> Vec<(String, Vec<String>)> {
    let Ok(output) = Command::new("nm").args(["-gU"]).arg(archive).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    let mut members: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        if line.ends_with(':') && !line.contains(char::is_whitespace) {
            let inner = &line[..line.len() - 1];
            let name = match inner.rfind('(') {
                Some(open) => inner[open + 1..]
                    .strip_suffix(')')
                    .unwrap_or(&inner[open + 1..]),
                None => inner,
            };
            members.push((name.to_string(), Vec::new()));
            continue;
        }
        if let Some(symbol) = line.split_whitespace().last() {
            if let Some(last) = members.last_mut() {
                last.1.push(symbol.to_string());
            }
        }
    }
    members
}

/// Copies an archive and removes members already provided by earlier whole archives.
fn dedup_macos_archive(
    archive: &Path,
    provider_names: &mut HashSet<String>,
    provider_symbols: &mut HashSet<String>,
    scratch: &Path,
) -> Option<PathBuf> {
    let names = ar_members(archive)?;
    let per_member = nm_member_globals(archive);
    let readable: HashMap<&str, &Vec<String>> = per_member
        .iter()
        .map(|(name, symbols)| (name.as_str(), symbols))
        .collect();
    let mut strip = HashSet::new();
    for name in &names {
        let duplicate_name = provider_names.contains(name);
        let duplicate_symbols = readable
            .get(name.as_str())
            .map(|symbols| {
                !symbols.is_empty()
                    && symbols
                        .iter()
                        .all(|symbol| provider_symbols.contains(symbol))
            })
            .unwrap_or(false);
        if duplicate_name || duplicate_symbols {
            strip.insert(name.clone());
        }
    }
    if strip.is_empty() {
        return None;
    }

    for name in &names {
        if !strip.contains(name) {
            provider_names.insert(name.clone());
            if let Some(symbols) = readable.get(name.as_str()) {
                for symbol in *symbols {
                    provider_symbols.insert(symbol.clone());
                }
            }
        }
    }

    let copy = scratch.join(archive.file_name()?);
    std::fs::create_dir_all(scratch).ok()?;
    std::fs::copy(archive, &copy).ok()?;
    let strip: Vec<&String> = strip.iter().collect();
    for chunk in strip.chunks(256) {
        let success = Command::new("ar")
            .arg("d")
            .arg(&copy)
            .args(chunk.iter().map(|member| member.as_str()))
            .status()
            .ok()?
            .success();
        if !success {
            return None;
        }
    }
    if !Command::new("ranlib")
        .arg(&copy)
        .status()
        .ok()?
        .success()
    {
        return None;
    }
    Some(copy)
}
