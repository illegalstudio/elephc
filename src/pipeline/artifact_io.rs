//! Purpose:
//! One safe-creation policy for every file the compiler generates: refuse a symbolic-link
//! destination, and replace an existing artifact atomically rather than truncating it in
//! place.
//!
//! Called from:
//! - `crate::pipeline::compile()`, once, to validate every destination before any work runs.
//! - `crate::pipeline::backend` and `crate::source_map` for each artifact they write.
//!
//! Key details:
//! - Output paths are derived from the SOURCE FILENAME, so a checked-out tree controlled by
//!   someone else — an untrusted pull request, a shared build directory — chooses them. A
//!   symlink named `main.s`, `main.map`, `libmain.h` or `main.key` next to `main.php` used to
//!   make the compiler truncate and overwrite the symlink's target with the compiling user's
//!   permissions (issue #888).
//! - The check runs ONCE, up front, for every artifact including the ones an external
//!   assembler or linker writes, because refusing before any tool runs is the only way to
//!   cover destinations this process does not open itself.
//! - Writes this process performs go through [`write_artifact`], which creates a private
//!   temporary file in the destination's own directory and renames it over the target.
//!   `rename(2)` replaces a symlink rather than following it, so the window between the
//!   up-front check and the write is closed for these paths rather than merely narrowed.

use std::io;
use std::path::{Path, PathBuf};

use super::output::OutputPaths;

/// Which generated files the selected command will actually produce.
///
/// The validation below refuses a destination before any work runs, so it must describe the
/// run that is about to happen and no more: `--check` and `--emit-ir` produce none of these
/// files, and `--emit-asm` stops after the assembly. Checking a path the command will never
/// write turns an unrelated symlink or directory sitting at that name into a refusal of a
/// perfectly valid command.
pub(super) struct ArtifactPlan {
    assembly: bool,
    source_map: bool,
    /// The object file, the binary or library, its C header, and the probe-key sidecar —
    /// everything downstream of the assembler, which one flag decides together.
    linked: bool,
    probe_key: bool,
}

impl ArtifactPlan {
    /// Derives the plan from the flags that decide where `compile()` stops.
    pub(super) fn for_run(
        check_only: bool,
        emit_ir: bool,
        emit_asm: bool,
        emit_source_map: bool,
        probe: bool,
    ) -> Self {
        // Both return before the backend stage runs, so nothing in `OutputPaths` is written.
        if check_only || emit_ir {
            return Self {
                assembly: false,
                source_map: false,
                linked: false,
                probe_key: false,
            };
        }
        let linked = !emit_asm;
        Self {
            assembly: true,
            source_map: emit_source_map,
            linked,
            probe_key: linked && probe,
        }
    }
}

/// Rejects every generated-artifact destination that is a symbolic link, or that exists as
/// something other than a regular file.
///
/// Returns the offending path and the reason, for a diagnostic the caller renders.
///
/// EXISTING REGULAR FILES ARE ALLOWED, and deliberately: recompiling over yesterday's
/// `main.s` is the normal case. What is refused is a destination whose write would land
/// somewhere else (a symlink) or would not be a file write at all (a directory, a FIFO, a
/// device node).
pub(super) fn reject_unsafe_destinations(
    paths: &OutputPaths,
    plan: &ArtifactPlan,
) -> Result<(), String> {
    let mut destinations: Vec<&Path> = Vec::new();
    if plan.assembly {
        destinations.push(paths.asm.as_path());
    }
    if plan.source_map {
        destinations.push(paths.source_map.as_path());
    }
    if plan.linked {
        destinations.push(paths.obj.as_path());
        destinations.push(paths.bin.as_path());
        if let Some(header) = paths.header.as_deref() {
            destinations.push(header);
        }
    }
    // The probe-key sidecar is derived from the binary path at write time rather than stored
    // in `OutputPaths`; it is an output-path symlink target like any other.
    let sidecar = paths.bin.with_extension("key");
    if plan.probe_key {
        destinations.push(sidecar.as_path());
    }

    for destination in destinations {
        reject_unsafe_destination(destination)?;
    }
    Ok(())
}

/// Rejects one destination that is a symlink or a non-regular existing file.
fn reject_unsafe_destination(path: &Path) -> Result<(), String> {
    // `symlink_metadata` does NOT follow the final component, which is the whole point:
    // `metadata` would report the TARGET's kind and happily call a symlink-to-a-regular-file
    // a regular file.
    //
    // ONLY `NotFound` MEANS "the write will create it". A permission error, an I/O error or
    // an `ELOOP` says the destination could not be inspected — treating those as absence
    // would let exactly the paths that cannot be vouched for through the check.
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "refusing to write the generated file '{}': its destination could not be \
                 inspected ({error})",
                path.display()
            ))
        }
    };
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "refusing to write the generated file '{}': it is a symbolic link, and writing \
             through it would overwrite its target",
            path.display()
        ));
    }
    if !metadata.file_type().is_file() {
        return Err(format!(
            "refusing to write the generated file '{}': it exists and is not a regular file",
            path.display()
        ));
    }
    Ok(())
}

/// Writes one generated artifact, replacing any existing regular file atomically.
///
/// Creates a private temporary file in the DESTINATION'S OWN DIRECTORY — so the rename is
/// within one filesystem and therefore atomic — with `O_CREAT | O_EXCL`, writes the bytes,
/// then renames it over the destination.
///
/// `rename(2)` REPLACES a symlink at the destination rather than following it, so a symlink
/// planted after [`reject_unsafe_destinations`] ran cannot redirect this write either. That
/// is what makes the up-front check a diagnostic rather than the safety mechanism for the
/// paths this process writes itself.
pub(crate) fn write_artifact(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_staged(path, bytes, None)
}

/// Writes one generated artifact that must never be readable by anyone but its owner.
///
/// On Unix, the permissions are applied to the STAGED file before the rename — so the bytes
/// are never reachable at the destination's well-known name under a permissive umask, not
/// even for the instant between creating the file and a `set_permissions` call afterwards.
/// The probe key is the monitoring HMAC credential and the `.key` path is derived from the
/// binary's, so on a shared build host that instant repeats on every rebuild.
///
/// `mode(0o600)` is an upper bound, not an assignment: the umask can only clear further
/// bits, so the file is never MORE permissive than this. Windows ACLs cannot be represented by
/// Rust's portable file-permission API; fail closed there rather than silently publishing a
/// credential under an inherited ACL we did not inspect or set.
pub(crate) fn write_private_artifact(path: &Path, bytes: &[u8]) -> io::Result<()> {
    #[cfg(unix)]
    {
        write_staged(path, bytes, Some(0o600))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, bytes);
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "private artifacts require an explicit owner-only ACL on this platform",
        ))
    }
}

/// Stages, writes, syncs and renames one artifact, optionally with an explicit creation mode.
fn write_staged(path: &Path, bytes: &[u8], mode: Option<u32>) -> io::Result<()> {
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::OpenOptionsExt;

    let directory = path.parent().unwrap_or(Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("artifact");
    let temporary = unique_temporary_path(directory, file_name);

    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    if let Some(mode) = mode {
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    let mut file = options.open(&temporary)?;
    let written = file.write_all(bytes).and_then(|()| file.sync_all());
    drop(file);
    if let Err(error) = written {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        let _ = std::fs::remove_file(&temporary);
        return Err(error);
    }
    Ok(())
}

/// Builds an unpredictable sibling path for the staged write.
///
/// The pid alone would be guessable, and a guessable staging name in a shared directory is
/// the same defect this module exists to fix — so the process id is mixed with a
/// monotonically increasing counter and the current time, and `O_EXCL` at the open is what
/// actually decides the race.
fn unique_temporary_path(directory: &Path, file_name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|since| since.subsec_nanos() as u64)
        .unwrap_or(0);
    let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
    directory.join(format!(
        ".{}.elephc-{}-{}-{}.tmp",
        file_name,
        std::process::id(),
        nanos,
        unique
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an empty scratch directory for one test.
    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elephc-artifact-io-{}-{}-{:?}",
            tag,
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch must be creatable");
        dir
    }

    /// Issue #888: a symlink at a generated-artifact path is refused, and its target is left
    /// alone.
    #[cfg(unix)]
    #[test]
    fn a_symlink_destination_is_refused() {
        let dir = scratch("symlink");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, b"original").expect("victim must be writable");
        let planted = dir.join("main.s");
        std::os::unix::fs::symlink(&victim, &planted).expect("symlink must be creatable");

        let error = reject_unsafe_destination(&planted).expect_err("a symlink must be refused");
        assert!(error.contains("symbolic link"), "diagnostic was {error:?}");
        assert_eq!(
            std::fs::read(&victim).expect("victim must be readable"),
            b"original"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A destination that exists as a DIRECTORY (or any non-regular file) is refused too.
    #[test]
    fn a_non_regular_destination_is_refused() {
        let dir = scratch("nonregular");
        let occupied = dir.join("main.map");
        std::fs::create_dir(&occupied).expect("directory must be creatable");

        let error =
            reject_unsafe_destination(&occupied).expect_err("a directory must be refused");
        assert!(error.contains("not a regular file"), "diagnostic was {error:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A missing destination and an existing REGULAR file are both accepted: creating an
    /// artifact and recompiling over one are the normal cases.
    #[test]
    fn missing_and_regular_destinations_are_accepted() {
        let dir = scratch("regular");
        let fresh = dir.join("libmain.h");
        reject_unsafe_destination(&fresh).expect("a missing destination must be accepted");

        std::fs::write(&fresh, b"old").expect("artifact must be writable");
        reject_unsafe_destination(&fresh).expect("an existing regular file must be accepted");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// `write_artifact` replaces an existing regular file and leaves no staging file behind.
    #[test]
    fn writing_replaces_an_existing_artifact_and_cleans_up() {
        let dir = scratch("replace");
        let artifact = dir.join("main.key");
        std::fs::write(&artifact, b"stale").expect("artifact must be writable");

        write_artifact(&artifact, b"fresh").expect("write must succeed");

        assert_eq!(
            std::fs::read(&artifact).expect("artifact must be readable"),
            b"fresh"
        );
        let leftovers: Vec<_> = std::fs::read_dir(&dir)
            .expect("scratch must be readable")
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "staging files left behind: {leftovers:?}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A destination that cannot be INSPECTED is refused, not waved through.
    ///
    /// Treating every `symlink_metadata` failure as absence let a path whose kind is
    /// unknown — a permission error, an I/O error, a symlink loop — reach the write or the
    /// external tool unchecked. `ELOOP` is the one such failure a test can produce
    /// portably, and it is also the most pointed: a symlink cycle is precisely a
    /// destination that is a symlink.
    #[cfg(unix)]
    #[test]
    fn an_uninspectable_destination_is_refused() {
        let dir = scratch("eloop");
        let looped = dir.join("main.s");
        std::os::unix::fs::symlink(&looped, &looped).expect("self-link must be creatable");

        let error =
            reject_unsafe_destination(&looped).expect_err("an unreadable destination is refused");
        assert!(
            error.contains("could not be inspected") || error.contains("symbolic link"),
            "diagnostic was {error:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The probe key is created owner-only BEFORE it reaches its well-known name, so a
    /// permissive umask never exposes the monitoring credential even briefly.
    #[cfg(unix)]
    #[test]
    fn a_private_artifact_is_never_group_or_world_readable() {
        use std::os::unix::fs::PermissionsExt;

        let dir = scratch("private");
        let sidecar = dir.join("main.key");

        write_private_artifact(&sidecar, b"0123456789abcdef").expect("write must succeed");

        let mode = std::fs::metadata(&sidecar)
            .expect("key must exist")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode & 0o077, 0, "key is reachable by others: {mode:o}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A command validates the destinations it will WRITE and no others.
    ///
    /// `--check` and `--emit-ir` return before the backend runs, and `--emit-asm` stops
    /// after the assembly, so an unrelated symlink at `main`, `main.o` or `main.key` must
    /// not refuse any of them.
    #[cfg(unix)]
    #[test]
    fn validation_covers_only_the_artifacts_the_command_produces() {
        let dir = scratch("plan");
        let paths = OutputPaths {
            asm: dir.join("main.s"),
            obj: dir.join("main.o"),
            bin: dir.join("main"),
            source_map: dir.join("main.map"),
            header: None,
        };
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, b"original").expect("victim must be writable");
        for planted in ["main.o", "main", "main.key"] {
            std::os::unix::fs::symlink(&victim, dir.join(planted))
                .expect("symlink must be creatable");
        }

        let check = ArtifactPlan::for_run(true, false, false, true, true);
        reject_unsafe_destinations(&paths, &check)
            .expect("--check writes nothing and must not be refused");
        let ir = ArtifactPlan::for_run(false, true, false, true, true);
        reject_unsafe_destinations(&paths, &ir)
            .expect("--emit-ir writes none of these and must not be refused");
        let asm = ArtifactPlan::for_run(false, false, true, false, true);
        reject_unsafe_destinations(&paths, &asm)
            .expect("--emit-asm writes only the assembly and must not be refused");

        // The full build does reach all of them, and is refused.
        let full = ArtifactPlan::for_run(false, false, false, true, true);
        reject_unsafe_destinations(&paths, &full)
            .expect_err("a full build must still refuse the planted symlinks");

        // A build without monitoring writes no key, so a symlink at `main.key` alone is
        // not its problem: remove the two it does write and it must pass.
        std::fs::remove_file(dir.join("main.o")).expect("planted link must be removable");
        std::fs::remove_file(dir.join("main")).expect("planted link must be removable");
        let unmonitored = ArtifactPlan::for_run(false, false, false, true, false);
        reject_unsafe_destinations(&paths, &unmonitored)
            .expect("a build with no probe key must ignore a symlink at the key path");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Even if a symlink appears AFTER the up-front check, the write replaces the link
    /// itself rather than following it — `rename(2)`'s own guarantee.
    #[cfg(unix)]
    #[test]
    fn writing_replaces_a_symlink_instead_of_following_it() {
        let dir = scratch("rename");
        let victim = dir.join("victim.txt");
        std::fs::write(&victim, b"original").expect("victim must be writable");
        let planted = dir.join("main.s");
        std::os::unix::fs::symlink(&victim, &planted).expect("symlink must be creatable");

        write_artifact(&planted, b"generated").expect("write must succeed");

        assert_eq!(
            std::fs::read(&victim).expect("victim must be readable"),
            b"original",
            "the symlink target must be untouched"
        );
        assert!(
            !std::fs::symlink_metadata(&planted)
                .expect("destination must exist")
                .file_type()
                .is_symlink(),
            "the symlink itself must have been replaced"
        );
        assert_eq!(
            std::fs::read(&planted).expect("artifact must be readable"),
            b"generated"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Windows must not turn a private probe credential into a file with an
    /// unchecked inherited ACL just to keep monitoring compilation moving.
    #[cfg(not(unix))]
    #[test]
    fn private_artifacts_fail_closed_without_owner_only_acl_support() {
        let error = write_private_artifact(Path::new("monitor.key"), b"secret")
            .expect_err("private artifact creation must not weaken access control");
        assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    }
}
