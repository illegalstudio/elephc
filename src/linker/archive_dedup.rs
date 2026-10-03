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

#[cfg(target_os = "macos")]
use std::collections::{HashMap, HashSet};
#[cfg(target_os = "macos")]
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::process::Command;

#[cfg(target_os = "macos")]
use crate::link_plan::{LinkItem, LinkOrigin};
use crate::link_plan::LinkPlan;

/// A possibly rewritten plan and the temporary directory that owns rewritten archives.
#[cfg(target_os = "macos")]
pub(super) struct PreparedArchives {
    /// Plan whose later whole-archive bridges may point at deduplicated copies.
    pub(super) plan: LinkPlan,
    scratch: Option<PathBuf>,
}

#[cfg(target_os = "macos")]
impl PreparedArchives {
    /// Removes temporary archive copies after the linker has consumed the plan.
    pub(super) fn cleanup(self) {
        if let Some(scratch) = self.scratch {
            let _ = std::fs::remove_dir_all(scratch);
        }
    }
}

/// Prepares deduplicated copies when a plan force-loads two or more Rust bridges.
#[cfg(target_os = "macos")]
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

    // FAIL CLOSED: without a private scratch directory there is nowhere safe to write the
    // deduplicated copies, so the plan is returned unchanged and the link proceeds with the
    // original archives. Deduplication is an optimization; a predictable scratch path is not
    // an acceptable price for it.
    let Some(scratch) = create_private_scratch() else {
        return PreparedArchives {
            plan: plan.clone(),
            scratch: None,
        };
    };
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
#[cfg(target_os = "macos")]
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

/// Creates an unpredictable, owner-only scratch directory for the deduplicated copies.
///
/// `mkdtemp(3)` in one step: the name carries six characters of kernel-chosen randomness, the
/// directory is created EXCLUSIVELY (so an attacker-planted directory cannot be adopted), and
/// the mode is `0700` from the moment it exists, with no window between creation and
/// permission fixup.
///
/// The path used to be `<tmp>/elephc-link-dedup-<pid>`, created with `create_dir_all` — which
/// SUCCEEDS on a directory that already exists. On a multi-user machine another local user
/// could predict or race the compiler's pid, pre-create that directory with permissive
/// access, and plant a symlink named after a bridge archive; the copy would then truncate and
/// overwrite the symlink's target with the compiler user's permissions (issue #889).
#[cfg(target_os = "macos")]
fn create_private_scratch() -> Option<PathBuf> {
    use std::ffi::{CString, OsStr};
    use std::os::unix::ffi::OsStrExt;

    let mut template = std::env::temp_dir();
    template.push("elephc-link-dedup-XXXXXX");
    let template = CString::new(template.as_os_str().as_bytes()).ok()?;
    let mut buffer = template.into_bytes_with_nul();
    // SAFETY: `buffer` is a NUL-terminated, writable C string ending in the six `X`
    // characters `mkdtemp` requires; it stays alive and unaliased for the call, and
    // `mkdtemp` only rewrites those six bytes in place.
    let created = unsafe { libc::mkdtemp(buffer.as_mut_ptr() as *mut libc::c_char) };
    if created.is_null() {
        return None;
    }
    let path_bytes = &buffer[..buffer.len() - 1];
    Some(PathBuf::from(OsStr::from_bytes(path_bytes)))
}

/// Lists object member names in an archive through `ar t`.
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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
#[cfg(target_os = "macos")]
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
    // `create_new` is `O_CREAT | O_EXCL`, which refuses to follow a symlink and fails
    // outright if anything already sits at the destination — unlike `fs::copy`, which
    // happily truncates whatever a symlink points at. The private scratch directory already
    // makes a pre-placed symlink unreachable; this is the second lock on the same door.
    //
    // `io::copy` rather than `fs::read` + `write_all`: a whole-archived bridge can be tens
    // of megabytes and several are deduplicated in one link, so reading each one entirely
    // into memory first would add a spike proportional to the archive set for no reason.
    // The streaming copy is what `fs::copy` would do, minus the symlink-following open.
    let mut source = std::fs::File::open(archive).ok()?;
    let mut destination = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&copy)
        .ok()?;
    std::io::copy(&mut source, &mut destination).ok()?;
    drop(destination);
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

/// No archive member surgery is needed outside Mach-O links.
///
/// Windows GNU uses COFF archives and Linux links the bridge archives without the
/// macOS duplicate-object failure this workaround addresses.  Returning the typed
/// plan unchanged is therefore deliberate rather than a best-effort attempt to
/// invoke Apple `ar`/`nm` conventions on another platform.
#[cfg(not(target_os = "macos"))]
pub(super) struct PreparedArchives {
    /// The original plan, preserved verbatim for the target linker.
    pub(super) plan: LinkPlan,
}

#[cfg(not(target_os = "macos"))]
impl PreparedArchives {
    /// There is no temporary archive copy outside the macOS deduplication path.
    pub(super) fn cleanup(self) {}
}

#[cfg(not(target_os = "macos"))]
pub(super) fn prepare(plan: &LinkPlan) -> PreparedArchives {
    PreparedArchives { plan: plan.clone() }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;

    /// Issue #889: the scratch directory is unpredictable, owner-only, and freshly created.
    ///
    /// The old path was `<tmp>/elephc-link-dedup-<pid>` created with `create_dir_all`, which
    /// succeeds on a directory that already exists — so another local user could pre-create
    /// it with permissive access and plant symlinks inside.
    #[test]
    fn scratch_is_unpredictable_private_and_fresh() {
        use std::os::unix::fs::PermissionsExt;

        let first = create_private_scratch().expect("scratch must be creatable");
        let second = create_private_scratch().expect("scratch must be creatable");

        assert_ne!(first, second, "two scratch paths must not collide");
        assert!(
            !first.to_string_lossy().contains(&std::process::id().to_string()),
            "the name must not be derived from the pid: {first:?}"
        );

        for path in [&first, &second] {
            let metadata = std::fs::metadata(path).expect("scratch must exist");
            assert!(metadata.is_dir());
            assert_eq!(
                metadata.permissions().mode() & 0o777,
                0o700,
                "scratch must be owner-only: {path:?}"
            );
            assert_eq!(
                std::fs::read_dir(path).expect("scratch must be readable").count(),
                0,
                "scratch must be fresh and empty: {path:?}"
            );
        }

        let _ = std::fs::remove_dir_all(&first);
        let _ = std::fs::remove_dir_all(&second);
    }

    /// A destination that already exists — a planted symlink among them — is REFUSED rather
    /// than followed and truncated.
    #[test]
    fn an_existing_destination_is_refused_not_followed() {
        let scratch = create_private_scratch().expect("scratch must be creatable");
        let victim = scratch.join("victim.txt");
        std::fs::write(&victim, b"original").expect("victim must be writable");

        let planted = scratch.join("libbridge.a");
        std::os::unix::fs::symlink(&victim, &planted).expect("symlink must be creatable");

        let opened = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&planted);
        assert!(
            opened.is_err(),
            "create_new must refuse a pre-existing destination"
        );
        assert_eq!(
            std::fs::read(&victim).expect("victim must still be readable"),
            b"original",
            "the symlink target must be untouched"
        );

        let _ = std::fs::remove_dir_all(&scratch);
    }
}
