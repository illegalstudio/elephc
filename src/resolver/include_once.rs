//! Purpose:
//! Generates stable labels used to track `include_once` and `require_once` execution.
//! Converts file paths into assembly-safe identifiers for resolver/runtime coordination.
//!
//! Called from:
//! - `crate::resolver::engine_includes::resolve_include_stmt()`.
//!
//! Key details:
//! - Labels must be deterministic for a canonical path so repeated includes share one guard.

use std::cell::RefCell;
use std::path::{Component, Path, PathBuf};

thread_local! {
    /// The directory identities are taken relative to: the main source's canonical directory.
    static IDENTITY_ROOT: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Sets the directory include identities are measured from, for the current compilation.
///
/// The labels and include-variant names below are hashed into emitted symbols and data. Hashing
/// a canonical ABSOLUTE path made two checkouts of one project emit different modules; hashing
/// the path relative to the main source's directory keeps them identical while the resolver
/// still keeps canonical paths for its own deduplication.
#[allow(dead_code)] // called by the binary crate's pipeline
pub(crate) fn set_identity_root(root: &Path) {
    let root = if root.as_os_str().is_empty() { Path::new(".") } else { root };
    let canonical = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    IDENTITY_ROOT.with(|cell| *cell.borrow_mut() = Some(canonical));
}

/// The compilation-local spelling of `path`: relative to the identity root when one is set
/// (`../` steps included for a file outside it), otherwise the path unchanged.
pub(crate) fn identity_path(path: &Path) -> String {
    IDENTITY_ROOT.with(|cell| {
        let root = cell.borrow();
        let Some(root) = root.as_deref() else {
            return path.to_string_lossy().into_owned();
        };
        relative_to(path, root).to_string_lossy().into_owned()
    })
}

/// `path` expressed relative to `root`; both are expected to be canonical absolute paths.
fn relative_to(path: &Path, root: &Path) -> PathBuf {
    let path_parts: Vec<Component> = path.components().collect();
    let root_parts: Vec<Component> = root.components().collect();
    let common = path_parts
        .iter()
        .zip(root_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();
    // Nothing shared but the filesystem root: a relative spelling would just be the absolute
    // path in disguise, so keep it as is.
    if common <= 1 {
        return path.to_path_buf();
    }
    let mut relative = PathBuf::new();
    for _ in common..root_parts.len() {
        relative.push("..");
    }
    for part in &path_parts[common..] {
        relative.push(part.as_os_str());
    }
    relative
}

/// Computes a deterministic assembly-safe label for `include_once`/`require_once` guards.
///
/// Uses FNV-1a hashing to convert a file path into a unique 64-bit identifier,
/// formatted as `_include_once_{hash:016x}`. The same canonical path always
/// produces the same label, enabling deduplication of repeated includes.
///
/// # Arguments
/// * `path` - The file path to hash.
///
/// # Returns
/// A `String` label safe for use as an assembly symbol name.
/// `pub(crate)` rather than `pub(super)`: the web prelude matches the resolved preload's
/// guard by this label, which is the only exact way to tell it from a `require_once` the
/// user wrote at the top of their own file.
pub(crate) fn include_once_label(path: &Path) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in identity_path(path).as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("_include_once_{hash:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The label hashes the path relative to the identity root, so a project moved to another
    /// directory keeps its labels.
    #[test]
    fn labels_do_not_depend_on_the_directory_the_project_lives_in() {
        let first = std::env::temp_dir().join(format!("elephc_label_a_{}", std::process::id()));
        let second = std::env::temp_dir().join(format!("elephc_label_b_{}", std::process::id()));
        for dir in [&first, &second] {
            std::fs::create_dir_all(dir.join("sub")).unwrap();
            std::fs::write(dir.join("sub").join("lib.php"), "<?php").unwrap();
        }
        set_identity_root(&first);
        let one = include_once_label(&std::fs::canonicalize(first.join("sub/lib.php")).unwrap());
        set_identity_root(&second);
        let two = include_once_label(&std::fs::canonicalize(second.join("sub/lib.php")).unwrap());
        assert_eq!(one, two);
        // A different file under the same root keeps a different label.
        std::fs::write(second.join("sub").join("other.php"), "<?php").unwrap();
        let other = include_once_label(&std::fs::canonicalize(second.join("sub/other.php")).unwrap());
        assert_ne!(two, other);
        // A file outside the root is spelled with `..` steps rather than its absolute path.
        let outside = std::fs::canonicalize(first.join("sub/lib.php")).unwrap();
        assert_eq!(identity_path(&outside).split('/').next(), Some(".."));
        for dir in [&first, &second] {
            let _ = std::fs::remove_dir_all(dir);
        }
    }
}
