//! Purpose:
//! Structural discovery of managed native catalog artifacts for the codegen harness: the
//! cache root the production resolver uses, and the newest `lib/` directory of one
//! package that holds every archive a fixture needs to link.
//!
//! Called from:
//! - `crate::support::curl_native` (curl and its four dependencies) and
//!   `crate::support::xml_native` (libxml2 + the Elephc shim).
//!
//! Key details:
//! - Deliberately NOT the production resolver: the harness has no project manifest, so it
//!   walks `artifacts/<package>/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>`
//!   and picks the newest `(version, recipe revision)`; a mismatch can only fail a test
//!   link, never ship anything.
//! - `scripts/ci/libxml2_lib_dir.sh` is the POSIX-sh twin of this walk for CI steps that
//!   need the directory in the shell.

use std::path::{Path, PathBuf};

use super::target;

/// Resolves the managed native cache's `artifacts/` root using the same environment
/// precedence as `elephc::native_deps`' `CacheLayout::from_environment`.
pub(crate) fn native_cache_artifacts_root() -> Option<PathBuf> {
    let root = if let Some(explicit) = std::env::var_os("ELEPHC_NATIVE_CACHE")
        .filter(|value| !value.is_empty())
    {
        PathBuf::from(explicit)
    } else if let Some(xdg) = std::env::var_os("XDG_CACHE_HOME").filter(|value| !value.is_empty()) {
        PathBuf::from(xdg).join("elephc/native")
    } else {
        PathBuf::from(std::env::var_os("HOME").filter(|value| !value.is_empty())?)
            .join(".cache/elephc/native")
    };
    let artifacts = root.join("artifacts");
    artifacts.is_dir().then_some(artifacts)
}

/// Walks `artifacts/<package>/<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>`
/// and returns the NEWEST — highest `(version, recipe revision)` — `lib/` directory that
/// contains every expected archive for the harness target.
///
/// The version/recipe/source/abi/toolchain components are content-addressed and vary per
/// machine, so they are enumerated rather than reconstructed; the TARGET component is
/// matched exactly, so a macOS cache can never satisfy a Linux fixture.
///
/// NEWEST WINS, AND THAT IS LOAD-BEARING RATHER THAN TIDY. Neither a catalog version bump
/// nor a recipe revision bump deletes the artifact the previous one built (`elephc native
/// prune` does, on request), so a developer's cache accumulates them: the day `curl` went
/// to revision 2 (HTTP/2 + SCP/SFTP + the full protocol set), every existing cache held
/// both `8.21.0/r1/` and `8.21.0/r2/`. `read_dir` order is unspecified, so taking the
/// first match found would link the HTTP/1.1-only archive on some runs and the current one
/// on others, and a fixture asserting a revision-2 protocol would fail for a reason nothing
/// in its own output explains.
///
/// BOTH AXES MATTER, not just the revision. `8.21.0/r2` and a future `8.22.0/r1` are
/// siblings under the same package, and ordering on the revision alone would pick the
/// STALE `r2` of the old version — the identical silent-staleness bug one directory level
/// up. Versions therefore compare as dotted numeric tuples (so `8.22.0 > 8.21.0`, and
/// `10.47 > 9.x` rather than sorting as text), with the revision as the tiebreak.
pub(crate) fn find_package_library_dir(
    artifacts: &Path,
    package: &str,
    archives: &[&str],
) -> Option<PathBuf> {
    let target_dir_name = target().as_str();
    let mut level = vec![artifacts.join(package)];
    // version -> recipe -> source sha -> target -> abi -> toolchain fingerprint
    for depth in 0..6 {
        let mut next = Vec::new();
        for dir in level {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    continue;
                }
                // Depth 3 is the target component; anything built for another target is
                // not a candidate for this harness run.
                if depth == 3 && entry.file_name() != *target_dir_name {
                    continue;
                }
                next.push(entry.path());
            }
        }
        level = next;
    }
    // Descending by (version, recipe revision), then by the whole path, so the choice is
    // both current and reproducible across runs on one machine.
    let package_root = artifacts.join(package);
    level.sort_by(|left, right| {
        version_and_revision_of(right, &package_root)
            .cmp(&version_and_revision_of(left, &package_root))
            .then_with(|| right.cmp(left))
    });
    level.into_iter().find_map(|dir| {
        let lib = dir.join("lib");
        archives
            .iter()
            .all(|archive| lib.join(archive).is_file())
            .then_some(lib)
    })
}

/// Reads the `<version>` and `r<N>` recipe-revision components out of one enumerated
/// artifact directory, as a sort key.
///
/// Returns `None` for either half of a path that does not have the expected shape, which
/// sorts it below every well-formed candidate rather than letting an unparseable directory
/// win. The version is a `Vec<u64>` of its dot-separated parts so it compares numerically
/// (`8.9.0 < 8.21.0`, which byte order gets backwards); a part that is not a plain number
/// makes the whole version unusable rather than silently comparing as `0`.
fn version_and_revision_of(
    dir: &Path,
    package_root: &Path,
) -> (Option<Vec<u64>>, Option<u32>) {
    let Ok(relative) = dir.strip_prefix(package_root) else {
        return (None, None);
    };
    // `<version>/r<recipe>/<source-sha>/<target>/<abi>/<toolchain>`
    let mut components = relative.components();
    let version = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .and_then(|text| text.split('.').map(|part| part.parse().ok()).collect());
    let revision = components
        .next()
        .and_then(|component| component.as_os_str().to_str())
        .and_then(|text| text.strip_prefix('r'))
        .and_then(|text| text.parse().ok());
    (version, revision)
}
