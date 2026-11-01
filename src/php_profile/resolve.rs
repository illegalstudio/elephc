//! Purpose:
//! Finds the PHP profile a project already declares, so `--php-version` stops being
//! something the user has to remember.
//!
//! Called from:
//! - `crate::cli::compile_config`, which owns the entry file path and fills in
//!   `php_version` / `php_version_provenance`.
//!
//! Key details:
//!
//! - EXACT PINS WIN. `composer.lock`'s `platform-overrides.php`,
//!   `composer.json`'s `config.platform.php`, and `.php-version` directly select a profile.
//!   A `require.php` constraint is consulted only when it excludes the newest maintained
//!   profile, in which case the newest admitted profile is selected. The local constraint
//!   parser follows Composer's range semantics instead of Cargo's.
//!
//! - NOTHING IS REQUIRED. Every source is optional at every level, and a project with no
//!   recognized manifest resolves to the default exactly as before. Compiling a lone `.php`
//!   file must never need project metadata.
//!
//! - THE SEARCH IS AN UPWARD WALK from the entry file's directory, first hit wins, the way
//!   project-oriented tools find their metadata. Within one directory the sources are tried in
//!   confidence order, so a lock pin beats a manifest pin beats a profile file.
//!
//! - A PIN OUTSIDE THE MAINTAINED RANGE IS CLAMPED, NOT IGNORED. A project pinning `8.1`
//!   cannot be emulated, but it is still saying "old", so the oldest maintained profile is
//!   the closest honest answer — and the clamp is REPORTED rather than applied silently,
//!   because the difference between what was asked for and what was used is exactly the kind
//!   of thing that must not be discovered later.

use std::path::{Path, PathBuf};

use crate::php_profile::Provenance;
use crate::web_prelude::PhpVersion;

/// The outcome of looking for a declared profile.
pub struct Resolved {
    /// The profile to compile with.
    pub profile: PhpVersion,
    /// Where it came from.
    pub provenance: Provenance,
    /// Anything the user should know about how the answer was reached — a clamped pin, or a
    /// source that could not be read. Empty in the ordinary case.
    pub notes: Vec<String>,
}

/// How a declared version string relates to the maintained profile set.
enum Pin {
    /// It names a maintained profile.
    Exact(PhpVersion),
    /// It is older than anything elephc maintains.
    TooOld,
    /// It is newer than anything elephc maintains.
    TooNew,
    /// It is not a `major.minor[.patch]` version at all.
    Unparsable,
}

/// Classifies a declared version string against the maintained profile set.
///
/// Only the major and minor components are considered: a profile is a language profile, so
/// `8.3.11` and `8.3` name the same one (see `PhpVersion::version_string` for the
/// patch-is-zero rule). A trailing stability or build suffix is ignored.
fn classify(raw: &str) -> Pin {
    let cleaned = raw.trim();
    let cleaned = cleaned.split(['-', '+']).next().unwrap_or(cleaned);
    let mut parts = cleaned.split('.');
    let (Some(major), Some(minor)) = (parts.next(), parts.next()) else {
        return Pin::Unparsable;
    };
    let (Ok(major), Ok(minor)) = (major.parse::<u32>(), minor.parse::<u32>()) else {
        return Pin::Unparsable;
    };
    let wanted = major * 10_000 + minor * 100;
    if let Some(profile) = PhpVersion::MAINTAINED
        .iter()
        .copied()
        .find(|profile| profile.version_id() == wanted)
    {
        return Pin::Exact(profile);
    }
    let oldest = PhpVersion::MAINTAINED[0];
    if wanted < oldest.version_id() {
        Pin::TooOld
    } else {
        Pin::TooNew
    }
}

/// Turns a classified pin into a profile, recording a note when the answer had to be moved.
fn apply(raw: &str, source: &str, notes: &mut Vec<String>) -> Option<PhpVersion> {
    let oldest = PhpVersion::MAINTAINED[0];
    let newest = PhpVersion::MAINTAINED[PhpVersion::MAINTAINED.len() - 1];
    match classify(raw) {
        Pin::Exact(profile) => Some(profile),
        Pin::TooOld => {
            notes.push(format!(
                "{source} pins PHP {raw}, which elephc does not maintain; using {}",
                oldest.spelling()
            ));
            Some(oldest)
        }
        Pin::TooNew => {
            notes.push(format!(
                "{source} pins PHP {raw}, which elephc does not maintain yet; using {}",
                newest.spelling()
            ));
            Some(newest)
        }
        Pin::Unparsable => {
            notes.push(format!("{source} has an unreadable PHP version '{raw}'; ignoring it"));
            None
        }
    }
}

/// What one read of a JSON document found.
///
/// Absent and unreadable documents are distinct internally even though both are ignored during
/// structural discovery; neither can safely be assumed to be intended project metadata.
enum Manifest {
    /// The file does not exist, or could not be read.
    Absent,
    /// The file exists but is not valid JSON and cannot be structurally identified.
    Unreadable,
    /// The parsed document.
    Parsed(serde_json::Value),
}

/// Reads and parses a JSON manifest ONCE.
///
/// A malformed manifest is a state rather than an error: optional project metadata must not make
/// an otherwise standalone source file fail to compile.
///
/// The single read matters because one manifest can provide both an exact platform declaration
/// and a compatible-version constraint. Reading per query would duplicate I/O and parsing.
fn read_manifest(path: &Path) -> Manifest {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Manifest::Absent;
    };
    match serde_json::from_str(&text) {
        Ok(document) => Manifest::Parsed(document),
        Err(_) => Manifest::Unreadable,
    }
}

/// Reads direct JSON documents in deterministic path order.
fn read_json_manifests(dir: &Path) -> Vec<Manifest> {
    let mut paths = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file()
                && path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths
        .iter()
        .map(|path| read_manifest(path))
        .collect()
}

impl Manifest {
    /// Follows a dotted path of keys to a string value, if this manifest has one there.
    fn string_at(&self, keys: &[&str]) -> Option<&str> {
        let Self::Parsed(document) = self else {
            return None;
        };
        let mut node = document;
        for key in keys {
            node = node.get(key)?;
        }
        node.as_str()
    }
}

/// Looks for a declared profile in one directory, in confidence order.
fn resolve_in(dir: &Path, notes: &mut Vec<String>) -> Option<(PhpVersion, Provenance)> {
    let manifests = read_json_manifests(dir);
    for manifest in &manifests {
        if let Some(raw) = manifest.string_at(&["platform-overrides", "php"]) {
            if let Some(profile) = apply(raw, "lock manifest platform override", notes) {
                return Some((profile, Provenance::LockManifest));
            }
        }
    }

    for manifest in &manifests {
        if let Some(raw) = manifest.string_at(&["config", "platform", "php"]) {
            if let Some(profile) = apply(raw, "project manifest platform declaration", notes) {
                return Some((profile, Provenance::ProjectManifest));
            }
        }
    }

    let toolchain = dir.join(".php-version");
    if let Ok(raw) = std::fs::read_to_string(&toolchain) {
        if let Some(profile) = apply(raw.trim(), ".php-version", notes) {
            return Some((profile, Provenance::PhpVersionFile));
        }
    }

    // Last: the `require.php` CONSTRAINT, which is a range rather than a pin.
    //
    // It is honored ONLY WHEN IT NARROWS — when the newest profile it admits is not the one
    // that would have been chosen anyway. `"^8.2"` admits everything through the newest, so
    // it says nothing elephc did not already assume and changes nothing; `"~8.3.0"` excludes
    // everything above 8.3, which is a deliberate statement worth following.
    //
    // That restriction is what makes reading a range defensible at all. Picking a point
    // inside one is a judgement call, and this makes the call only in the case where every
    // reasonable reading agrees: the project has explicitly ruled newer PHP out.
    for manifest in &manifests {
        if let Some(raw) = manifest.string_at(&["require", "php"]) {
            let newest = PhpVersion::MAINTAINED[PhpVersion::MAINTAINED.len() - 1];
            if let Some(admitted) = crate::php_profile::constraint::newest_admitted(raw) {
                if admitted.version_id() < newest.version_id() {
                    return Some((admitted, Provenance::ProjectConstraint));
                }
            }
        }
    }

    None
}

/// Resolves the profile for a compilation whose entry file is `entry`.
///
/// Walks upward from the entry file's directory to the filesystem root, taking the first
/// directory that declares anything. Returns the default profile — the newest maintained one
/// — when nothing does, which is the common case and costs nothing.
pub fn resolve(entry: &Path) -> Resolved {
    let mut notes = Vec::new();
    let start = entry
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let start = std::fs::canonicalize(&start).unwrap_or(start);

    let mut dir = Some(start.as_path());
    while let Some(current) = dir {
        if let Some((profile, provenance)) = resolve_in(current, &mut notes) {
            return Resolved {
                profile,
                provenance,
                notes,
            };
        }
        dir = current.parent();
    }

    Resolved {
        profile: PhpVersion::default(),
        provenance: Provenance::Default,
        notes,
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for profile resolution: each source, their precedence, the upward walk,
    //! clamping, and the guarantee that a bare `.php` file needs no manifest.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static DIR_ID: AtomicUsize = AtomicUsize::new(0);

    /// Creates an isolated temp dir unique across parallel test threads.
    fn temp_dir() -> PathBuf {
        let id = DIR_ID.fetch_add(1, Ordering::SeqCst);
        let dir = std::env::temp_dir().join(format!(
            "elephc_resolve_{}_{:?}_{}",
            std::process::id(),
            std::thread::current().id(),
            id
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Writes a file under `dir` and returns the directory.
    fn write(dir: &Path, name: &str, body: &str) {
        std::fs::write(dir.join(name), body).unwrap();
    }

    /// A lone `.php` file with no project files resolves to the default, silently.
    #[test]
    fn bare_file_resolves_to_the_default() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::Default);
        assert_eq!(resolved.profile, PhpVersion::default());
        assert!(resolved.notes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `config.platform.php` is honored, and the patch component is irrelevant.
    #[test]
    fn project_manifest_platform_pin_is_honored() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(
            &dir,
            "project.json",
            r#"{"config":{"platform":{"php":"8.3.11"}}}"#,
        );
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::ProjectManifest);
        assert_eq!(resolved.profile, PhpVersion::Php83);
        assert!(resolved.notes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The lockfile wins over the manifest: it records what was actually installed against.
    #[test]
    fn lock_pin_beats_manifest_pin() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(
            &dir,
            "project.json",
            r#"{"config":{"platform":{"php":"8.3"}}}"#,
        );
        write(&dir, "resolution.json", r#"{"platform-overrides":{"php":"8.4"}}"#);
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::LockManifest);
        assert_eq!(resolved.profile, PhpVersion::Php84);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `.php-version` is read when no manifest pin exists.
    #[test]
    fn php_version_file_is_honored() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(&dir, ".php-version", "8.3.14\n");
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::PhpVersionFile);
        assert_eq!(resolved.profile, PhpVersion::Php83);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A project-manifest pin beats `.php-version` in the same directory.
    #[test]
    fn project_manifest_pin_beats_php_version_file() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(&dir, ".php-version", "8.2");
        write(
            &dir,
            "project.json",
            r#"{"config":{"platform":{"php":"8.4"}}}"#,
        );
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::ProjectManifest);
        assert_eq!(resolved.profile, PhpVersion::Php84);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The search walks upward, so a file in `src/` finds the project root's pin.
    #[test]
    fn search_walks_upward_from_the_entry_file() {
        let dir = temp_dir();
        let nested = dir.join("src").join("deep");
        std::fs::create_dir_all(&nested).unwrap();
        write(&nested, "prog.php", "<?php echo 1;");
        write(
            &dir,
            "project.json",
            r#"{"config":{"platform":{"php":"8.3"}}}"#,
        );
        let resolved = resolve(&nested.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::ProjectManifest);
        assert_eq!(resolved.profile, PhpVersion::Php83);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `require.php` constraint that admits the newest profile changes NOTHING, and is
    /// therefore not reported as the source. `"^8.2"` says nothing elephc did not already
    /// assume, so claiming it decided the profile would be a false attribution.
    #[test]
    fn non_narrowing_constraint_leaves_the_default() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(&dir, "project.json", r#"{"require":{"php":"^8.2"}}"#);
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::Default);
        assert_eq!(resolved.profile, PhpVersion::default());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A `require.php` constraint that EXCLUDES newer PHP is a deliberate statement and is
    /// followed. This is the only case where a range is allowed to decide the profile.
    #[test]
    fn narrowing_constraint_is_honored() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(&dir, "project.json", r#"{"require":{"php":"~8.3.0"}}"#);
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::ProjectConstraint);
        assert_eq!(resolved.profile, PhpVersion::Php83);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An exact platform pin still outranks a narrowing constraint in the same manifest.
    #[test]
    fn platform_pin_beats_require_constraint() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(
            &dir,
            "project.json",
            r#"{"require":{"php":"~8.3.0"},"config":{"platform":{"php":"8.2"}}}"#,
        );
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::ProjectManifest);
        assert_eq!(resolved.profile, PhpVersion::Php82);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A constraint admitting no maintained profile leaves the default rather than inventing
    /// an answer.
    #[test]
    fn unsatisfiable_constraint_leaves_the_default() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(&dir, "project.json", r#"{"require":{"php":"~7.4.0"}}"#);
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::Default);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pin below the maintained range is clamped to the oldest profile AND reported.
    #[test]
    fn too_old_pin_is_clamped_and_reported() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(
            &dir,
            "project.json",
            r#"{"config":{"platform":{"php":"8.1.0"}}}"#,
        );
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.profile, PhpVersion::MAINTAINED[0]);
        assert_eq!(resolved.notes.len(), 1);
        assert!(resolved.notes[0].contains("8.1.0"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A pin above the maintained range is clamped to the newest profile AND reported.
    #[test]
    fn too_new_pin_is_clamped_and_reported() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(&dir, ".php-version", "9.0");
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(
            resolved.profile,
            PhpVersion::MAINTAINED[PhpVersion::MAINTAINED.len() - 1]
        );
        assert_eq!(resolved.notes.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An arbitrary malformed JSON document never fails the build or claims to be project metadata.
    #[test]
    fn malformed_json_is_ignored_not_fatal() {
        let dir = temp_dir();
        write(&dir, "prog.php", "<?php echo 1;");
        write(&dir, "project.json", "{ this is not json");
        let resolved = resolve(&dir.join("prog.php"));
        assert_eq!(resolved.provenance, Provenance::Default);
        assert!(resolved.notes.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
