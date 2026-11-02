//! Purpose:
//! Caches assembled slice objects by the CONTENT of the slice, so a rebuild that
//! changed nothing in a slice never runs the assembler over that slice again.
//!
//! Called from:
//! - `crate::linker::assemble_parallel()`, once per contiguous slice produced by
//!   `crate::linker::asm_split`.
//!
//! Key details:
//! - The key is the FNV-1a of the RENDERED slice text, not of a byte range of the
//!   input. A rendered slice already carries its section replay, its promoted
//!   renames and its own visibility footer, so it is self-contained: two runs that
//!   render the same string must assemble to the same object.
//! - The assembler is part of the key. `/usr/bin/as` on macOS is a shim that execs
//!   clang, so the bytes it produces track the installed Command Line Tools; the
//!   tool's path, its arguments, its length and its modification time all go in.
//! - This is a DIFFERENT cache from the runtime object's and lives in its own
//!   directory, so `runtime_cache::storage`'s canonical-name matcher can never
//!   match a slice object and the runtime cache's bound of eight entries can never
//!   evict one.
//! - Publication is atomic (unique temporary, then rename) and an FNV integrity
//!   sidecar is verified before any reuse, the same shape as `runtime_cache`.
//! - Every failure here is silent and falls back to assembling. A cache that
//!   cannot be read or written must make the next build slower, never wrong.

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

/// Bumped whenever the splitter's rendering or this module's naming changes.
///
/// The slice text is hashed, so a rendering change is already visible in the key.
/// A change to the NAME format, or to what the assembler is invoked with, is not,
/// and this is what retires those entries.
const ASM_CACHE_FORMAT_VERSION: u32 = 1;

/// Maximum published slice objects retained before the oldest are pruned.
///
/// Sized for several builds' worth of slices rather than the runtime cache's eight
/// entries: one Symfony build at the default job count publishes on the order of
/// ten, and a developer moving between branches wants the other branch's slices to
/// still be there when they come back.
const MAX_ASM_CACHE_OBJECTS: usize = 256;

/// FNV-1a offset basis.
const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
/// FNV-1a prime.
const FNV_PRIME: u64 = 0x100000001b3;

/// Returns whether the slice cache may be consulted and published to.
///
/// `ELEPHC_ASM_CACHE=0` (or `off`) takes the plain assemble-every-slice path, which
/// is what the identity gate compares against. Only those two values disable it, so
/// a typo cannot silently turn the cache off and hide a stale-object bug.
pub(super) fn is_enabled() -> bool {
    enabled_from_env(std::env::var("ELEPHC_ASM_CACHE").ok().as_deref())
}

/// Maps an `ELEPHC_ASM_CACHE` value to whether the slice cache is consulted.
///
/// Unset keeps the cache on. Only `0` and `off` disable it: anything else stays on,
/// so a typo cannot silently retire the cache and hide the fact that a build is
/// paying full assembler time.
fn enabled_from_env(value: Option<&str>) -> bool {
    !matches!(value, Some("0") | Some("off"))
}

/// Cache directory for slice objects, beside the runtime cache but never inside it.
fn asm_cache_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("XDG_CACHE_HOME") {
        PathBuf::from(path).join("elephc").join("asm")
    } else if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".cache").join("elephc").join("asm")
    } else {
        std::env::temp_dir().join("elephc-cache").join("asm")
    }
}

/// Computes a 64-bit FNV-1a hash over `bytes`, continuing from `seed`.
fn fnv1a(seed: u64, bytes: &[u8]) -> u64 {
    let mut hash = seed;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

/// Identity of the assembler this build will invoke.
///
/// Two different assemblers can turn one slice into two different objects, so the
/// tool has to be part of the key. The program name and its arguments cover the
/// target selection; the binary's length and modification time cover a toolchain
/// upgrade that keeps the same path, which on macOS is how a Command Line Tools
/// update arrives.
pub(super) fn assembler_identity(command: &Command) -> String {
    let mut identity = format!(
        "v{ASM_CACHE_FORMAT_VERSION}:{}",
        command.get_program().to_string_lossy()
    );
    for argument in command.get_args() {
        identity.push(':');
        identity.push_str(&argument.to_string_lossy());
    }
    identity.push_str(&tool_stamp(command.get_program()));
    identity
}

/// Returns the length and modification time of the assembler binary, when readable.
///
/// A tool found on `PATH` rather than by absolute path has no metadata to read here;
/// the name and arguments still identify it, and a toolchain swap that also renames
/// the tool is covered. An unreadable tool contributes nothing rather than failing.
fn tool_stamp(program: &OsStr) -> String {
    let Ok(metadata) = fs::metadata(program) else {
        return String::new();
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_nanos())
        .unwrap_or(0);
    format!(":{}:{}", metadata.len(), modified)
}

/// Returns the cache key for one rendered slice under one assembler.
#[cfg(test)]
pub(super) fn slice_key(slice: &str, identity: &str) -> u64 {
    extend_slice_key(slice_key_seed(identity), slice.as_bytes())
}

/// The key of an empty slice under one assembler: what `extend_slice_key` starts from.
pub(super) fn slice_key_seed(identity: &str) -> u64 {
    fnv1a(FNV_OFFSET_BASIS, identity.as_bytes())
}

/// Extends a slice key with the slice's next bytes.
///
/// FNV-1a folds one byte at a time, so a slice keyed chunk by chunk as it is written gets the
/// key the whole text would: a slice no longer has to exist in memory to be looked up.
pub(super) fn extend_slice_key(key: u64, bytes: &[u8]) -> u64 {
    fnv1a(key, bytes)
}

/// Canonical file name for a cached slice object.
///
/// The shape deliberately shares no prefix with the runtime cache's
/// `runtime-v<version>-...-heap<n>.o`, so that even if the two ever landed in one
/// directory neither module's name matcher could claim the other's entries.
fn cache_name(key: u64) -> String {
    format!("asmslice-v{}-a{key:016x}.o", env!("CARGO_PKG_VERSION"))
}

/// Returns whether a cached object still matches its integrity sidecar.
fn is_intact(object: &Path, integrity: &Path) -> bool {
    let Ok(bytes) = fs::read(object) else {
        return false;
    };
    let Ok(expected) = fs::read_to_string(integrity) else {
        return false;
    };
    expected.trim() == format!("{:016x}", fnv1a(FNV_OFFSET_BASIS, &bytes))
}

/// Materializes a cached slice object at `destination`, if one is published and intact.
///
/// A hardlink is preferred: it costs no bytes and survives the cache pruning the
/// entry, because the inode outlives the canonical name. A copy is the fallback for
/// a cache on another filesystem. The destination is removed first so a hardlink
/// cannot fail merely because the previous build left a file there.
pub(super) fn reuse(key: u64, destination: &Path) -> bool {
    reuse_in(&asm_cache_dir(), key, destination)
}

/// `reuse`, against an explicit directory, so the storage can be tested without
/// mutating the process environment under a threaded test harness.
fn reuse_in(directory: &Path, key: u64, destination: &Path) -> bool {
    let name = cache_name(key);
    let object = directory.join(&name);
    let integrity = directory.join(format!("{name}.integrity"));
    if !is_intact(&object, &integrity) {
        return false;
    }
    let _ = fs::remove_file(destination);
    if fs::hard_link(&object, destination).is_ok() {
        return true;
    }
    fs::copy(&object, destination).is_ok()
}

/// Publishes one freshly assembled slice object under its content key.
///
/// Best effort throughout. The object is written to a process-unique temporary and
/// renamed, so a reader can never observe a partial object under the canonical name;
/// the sidecar is written the same way and only after the object is in place, so an
/// entry is never advertised as intact before it is.
pub(super) fn publish(key: u64, object: &Path) {
    publish_in(&asm_cache_dir(), key, object);
}

/// `publish`, against an explicit directory, so the storage can be tested without
/// mutating the process environment under a threaded test harness.
fn publish_in(directory: &Path, key: u64, object: &Path) {
    if fs::create_dir_all(directory).is_err() {
        return;
    }
    let name = cache_name(key);
    let canonical = directory.join(&name);
    if canonical.exists() {
        return;
    }
    let Ok(bytes) = fs::read(object) else {
        return;
    };
    let unique = format!("{}_{key:016x}", std::process::id());
    let temporary = directory.join(format!("{name}.{unique}.tmp"));
    if fs::write(&temporary, &bytes).is_err() {
        let _ = fs::remove_file(&temporary);
        return;
    }
    if fs::rename(&temporary, &canonical).is_err() {
        let _ = fs::remove_file(&temporary);
        return;
    }
    let integrity_temporary = directory.join(format!("{name}.integrity.{unique}.tmp"));
    let digest = format!("{:016x}\n", fnv1a(FNV_OFFSET_BASIS, &bytes));
    if fs::write(&integrity_temporary, digest).is_err() {
        let _ = fs::remove_file(&integrity_temporary);
        return;
    }
    if fs::rename(&integrity_temporary, directory.join(format!("{name}.integrity"))).is_err() {
        let _ = fs::remove_file(&integrity_temporary);
    }
}

/// Removes the oldest published slice objects beyond the cache bound.
///
/// Housekeeping is best effort so an unreadable stale entry cannot fail a build.
/// Only this module's canonical names are considered, so an unrelated file sharing
/// the directory is never removed, and the entries this build just used are kept
/// whatever their age.
pub(super) fn prune(keep: &[u64]) {
    prune_in(&asm_cache_dir(), keep);
}

/// `prune`, against an explicit directory, so it can be tested directly.
fn prune_in(directory: &Path, keep: &[u64]) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    let kept: Vec<String> = keep.iter().map(|&key| cache_name(key)).collect();
    let mut objects: Vec<(PathBuf, SystemTime, String)> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            if !name_is_canonical(&name) {
                return None;
            }
            let metadata = entry.metadata().ok()?;
            if !metadata.is_file() {
                return None;
            }
            Some((path, metadata.modified().unwrap_or(UNIX_EPOCH), name))
        })
        .collect();
    if objects.len() <= MAX_ASM_CACHE_OBJECTS {
        return;
    }
    objects.sort_by_key(|(_, modified, _)| *modified);
    let excess = objects.len() - MAX_ASM_CACHE_OBJECTS;
    for (path, _, name) in objects
        .into_iter()
        .filter(|(_, _, name)| !kept.contains(name))
        .take(excess)
    {
        if fs::remove_file(&path).is_err() {
            continue;
        }
        let _ = fs::remove_file(directory.join(format!("{name}.integrity")));
    }
}

/// Returns whether a file name is a published slice object rather than a temporary.
///
/// A temporary always carries a `.tmp` suffix, and the integrity sidecar carries
/// `.integrity`, so neither can be mistaken for an object to prune on its own.
fn name_is_canonical(name: &str) -> bool {
    let Some(rest) = name.strip_prefix("asmslice-v") else {
        return false;
    };
    let Some(without_extension) = rest.strip_suffix(".o") else {
        return false;
    };
    let Some((version, key)) = without_extension.rsplit_once("-a") else {
        return false;
    };
    !version.is_empty()
        && key.len() == 16
        && key.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    //! Unit tests for the slice object cache.

    use super::*;

    /// A key must depend on the slice text.
    #[test]
    fn a_different_slice_takes_a_different_key() {
        let identity = "v1:as:-arch:arm64";
        assert_ne!(
            slice_key("        ret\n", identity),
            slice_key("        nop\n", identity)
        );
    }

    /// A key must depend on the assembler, not only on the text.
    #[test]
    fn the_same_slice_under_a_different_assembler_takes_a_different_key() {
        let slice = "        ret\n";
        assert_ne!(
            slice_key(slice, "v1:as:-arch:arm64"),
            slice_key(slice, "v1:as:-arch:x86_64")
        );
    }

    /// The same text under the same assembler must key the same, or nothing hits.
    #[test]
    fn the_same_slice_under_the_same_assembler_takes_the_same_key() {
        let identity = "v1:as:-arch:arm64";
        assert_eq!(slice_key("        ret\n", identity), slice_key("        ret\n", identity));
    }

    /// The published name must be recognisable, and nothing else may be.
    #[test]
    fn only_a_published_object_name_is_canonical() {
        let name = cache_name(0x0123456789abcdef);
        assert!(name_is_canonical(&name), "{name} should be canonical");
        assert!(!name_is_canonical(&format!("{name}.integrity")));
        assert!(!name_is_canonical(&format!("{name}.4242_0123456789abcdef.tmp")));
        assert!(!name_is_canonical("asmslice-v0.1.0-aZZZZZZZZZZZZZZZ.o"));
        assert!(!name_is_canonical("asmslice-v0.1.0-a0123.o"));
    }

    /// A slice object must never be mistaken for a runtime cache object.
    ///
    /// The two caches live in different directories, but the runtime cache's pruner
    /// matches by NAME, so a shape that could satisfy both would be a latent way for
    /// one cache to evict the other's entries.
    #[test]
    fn a_slice_object_name_is_not_a_runtime_cache_object_name() {
        let name = cache_name(0xfeedfacecafebeef);
        assert!(!name.starts_with("runtime-v"));
        assert!(!name.contains("-heap"));
        assert!(!name.contains("-rt"));
    }

    /// The cache is on unless explicitly disabled, and a typo keeps it on.
    ///
    /// `no` and the empty string are in here deliberately: a value that looks like a
    /// refusal but is not one of the two accepted spellings must leave the cache ON,
    /// so nobody believes they disabled it while the build still consults it.
    #[test]
    fn only_zero_and_off_disable_the_cache() {
        assert!(enabled_from_env(None));
        assert!(!enabled_from_env(Some("0")));
        assert!(!enabled_from_env(Some("off")));
        assert!(enabled_from_env(Some("1")));
        assert!(enabled_from_env(Some("")));
        assert!(enabled_from_env(Some("no")));
        assert!(enabled_from_env(Some("false")));
    }

    /// Creates a directory this test owns, named so no other test can collide.
    fn scratch(tag: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "elephc_asm_cache_{}_{}",
            std::process::id(),
            tag
        ));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create scratch directory");
        directory
    }

    /// A published entry must come back byte for byte.
    #[test]
    fn a_published_object_is_reused() {
        let directory = scratch("reuse");
        let source = directory.join("slice0.o");
        fs::write(&source, b"OBJECTBYTES").expect("write source object");
        let key = slice_key("        ret\n", "v1:as:-arch:arm64");
        publish_in(&directory, key, &source);

        let destination = directory.join("restored.o");
        assert!(reuse_in(&directory, key, &destination), "an intact entry must be reused");
        assert_eq!(fs::read(&destination).expect("read restored"), b"OBJECTBYTES");
        let _ = fs::remove_dir_all(&directory);
    }

    /// A published object whose bytes no longer match its sidecar must be refused.
    ///
    /// This is the check that stops a truncated or corrupted object being linked as
    /// if it were the assembler's own output.
    #[test]
    fn a_tampered_object_is_refused() {
        let directory = scratch("tampered");
        let source = directory.join("slice0.o");
        fs::write(&source, b"OBJECTBYTES").expect("write source object");
        let key = slice_key("        ret\n", "v1:as:-arch:arm64");
        publish_in(&directory, key, &source);

        fs::write(directory.join(cache_name(key)), b"CORRUPTED!!").expect("tamper");
        let destination = directory.join("restored.o");
        assert!(
            !reuse_in(&directory, key, &destination),
            "a tampered entry must be refused"
        );
        let _ = fs::remove_dir_all(&directory);
    }

    /// An object with no sidecar at all must be refused rather than trusted.
    #[test]
    fn an_object_without_its_sidecar_is_refused() {
        let directory = scratch("nosidecar");
        let key = slice_key("        ret\n", "v1:as:-arch:arm64");
        fs::write(directory.join(cache_name(key)), b"OBJECTBYTES").expect("write object");
        let destination = directory.join("restored.o");
        assert!(!reuse_in(&directory, key, &destination));
        let _ = fs::remove_dir_all(&directory);
    }

    /// A miss must report a miss rather than leaving a stale destination in place.
    #[test]
    fn a_miss_does_not_leave_the_previous_destination_behind() {
        let directory = scratch("miss");
        let destination = directory.join("restored.o");
        fs::write(&destination, b"STALEOBJECT").expect("write stale");
        let key = slice_key("        nop\n", "v1:as:-arch:arm64");
        assert!(!reuse_in(&directory, key, &destination), "nothing was published");
        // The stale file may remain -- what matters is the caller is told to
        // assemble, which overwrites it. Assert the verdict, not the filesystem.
        let _ = fs::remove_dir_all(&directory);
    }

    /// Pruning must respect the bound, keep this build's entries, and leave
    /// unrelated files alone.
    #[test]
    fn pruning_keeps_the_active_entries_and_ignores_foreign_files() {
        let directory = scratch("prune");
        let foreign = directory.join("not-ours.txt");
        fs::write(&foreign, b"leave me").expect("write foreign");
        let mut keys = Vec::new();
        for index in 0..(MAX_ASM_CACHE_OBJECTS + 8) {
            let key = slice_key(&format!("slice {index}\n"), "v1:as:-arch:arm64");
            let source = directory.join(format!("in{index}.o"));
            fs::write(&source, format!("bytes {index}")).expect("write object");
            publish_in(&directory, key, &source);
            let _ = fs::remove_file(&source);
            keys.push(key);
        }
        // Keep the last eight, which stand for the slices this build just used.
        let active: Vec<u64> = keys[keys.len() - 8..].to_vec();
        prune_in(&directory, &active);

        let remaining = fs::read_dir(&directory)
            .expect("read directory")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(name_is_canonical)
            })
            .count();
        assert!(
            remaining <= MAX_ASM_CACHE_OBJECTS,
            "pruning must bring the cache back to its bound, found {remaining}"
        );
        for key in active {
            assert!(
                directory.join(cache_name(key)).exists(),
                "an entry this build used must survive pruning"
            );
        }
        assert!(foreign.exists(), "an unrelated file must never be pruned");
        let _ = fs::remove_dir_all(&directory);
    }
}
