//! Purpose:
//! File-system side of `elephc test`: which files are test files, where the project root
//! is, and the virtual entry source that requires every test file.
//!
//! Called from:
//! - `crate::test_command::run_inner`.
//!
//! Key details:
//! - A test file is a regular file named `*Test.php` or `*Test.lfc` (PHPUnit's default
//!   suffix), found by a sorted recursive walk that skips dot-directories, so the order of
//!   the run is the order of the paths and is stable across machines. A directory is
//!   visited once by its canonical path: a symlink back into the tree (or a second link to
//!   the same directory) is not walked again, so a loop cannot recurse forever.
//! - The project root is the nearest directory, starting at the test path itself when it is
//!   a directory, that holds a `composer.json`; without one, the working directory.
//! - Required paths are absolute and single-quoted in the entry, so the entry's virtual
//!   location never affects how the files are found.

use std::path::{Path, PathBuf};

/// Returns every test file under `path` in sorted order; a file path is taken as is.
pub(super) fn collect_test_files(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        return Ok(vec![absolute(path)?]);
    }
    let mut files = Vec::new();
    let mut visited = std::collections::HashSet::new();
    walk(path, &mut files, &mut visited)?;
    files.sort();
    files.dedup();
    Ok(files)
}

/// Recursively collects test files below `dir`, visiting entries in name order and each
/// directory once by its canonical path.
fn walk(
    dir: &Path,
    files: &mut Vec<PathBuf>,
    visited: &mut std::collections::HashSet<PathBuf>,
) -> Result<(), String> {
    let canonical = absolute(dir)?;
    if !visited.insert(canonical) {
        return Ok(());
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(dir)
        .map_err(|error| format!("cannot read '{}': {error}", dir.display()))?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<_, _>>()
        .map_err(|error| format!("cannot read '{}': {error}", dir.display()))?;
    entries.sort();
    for entry in entries {
        let name = entry
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if entry.is_dir() {
            if !name.starts_with('.') {
                walk(&entry, files, visited)?;
            }
        } else if is_test_file(name) {
            files.push(absolute(&entry)?);
        }
    }
    Ok(())
}

/// PHPUnit's default naming: the file name ends in `Test` before the source extension.
pub(super) fn is_test_file(name: &str) -> bool {
    name.ends_with("Test.php") || name.ends_with("Test.lfc")
}

/// Canonicalizes `path` so the entry's `require_once` lines are absolute.
fn absolute(path: &Path) -> Result<PathBuf, String> {
    path.canonicalize()
        .map_err(|error| format!("cannot resolve '{}': {error}", path.display()))
}

/// The nearest ancestor of the test path (itself included when it is a directory) that
/// holds a `composer.json`; the working directory when none does.
pub(super) fn project_root(test_path: &Path, cwd: &Path) -> PathBuf {
    let start = if test_path.is_dir() {
        test_path.to_path_buf()
    } else {
        test_path.parent().map(Path::to_path_buf).unwrap_or_else(|| cwd.to_path_buf())
    };
    let start = start.canonicalize().unwrap_or(start);
    start
        .ancestors()
        .find(|dir| dir.join("composer.json").is_file())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cwd.to_path_buf())
}

/// The virtual entry: one `require_once` per test file. Discovery appends the driver later,
/// once the classes these files declare, and the ones they autoload, are all in the AST.
pub(super) fn entry_source(files: &[PathBuf]) -> String {
    let mut source = String::new();
    for file in files {
        source.push_str(&format!("require_once '{}';\n", php_single_quoted(&file.display().to_string())));
    }
    source
}

/// Escapes `text` for a single-quoted PHP string: only `\` and `'` are special there.
fn php_single_quoted(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for test-file discovery, project-root resolution and entry synthesis.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Each test builds its own tree under the system temp directory and removes it.

    use super::*;

    /// Creates a unique scratch directory for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "elephc_test_command_{name}_{}_{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    /// Only `*Test.php` / `*Test.lfc` regular files count, in sorted path order, and
    /// dot-directories are skipped.
    #[test]
    fn collects_test_files_in_sorted_order() {
        let dir = scratch("collect");
        std::fs::create_dir_all(dir.join("Unit/.hidden")).unwrap();
        for file in ["Unit/BetaTest.php", "Unit/AlphaTest.lfc", "Unit/Helper.php", "Unit/.hidden/ZetaTest.php", "ZTopTest.php", "notes.txt"] {
            std::fs::write(dir.join(file), "").unwrap();
        }
        let found: Vec<String> = collect_test_files(&dir)
            .expect("walk")
            .into_iter()
            .map(|path| path.strip_prefix(dir.canonicalize().unwrap()).unwrap().display().to_string())
            .collect();
        assert_eq!(found, vec!["Unit/AlphaTest.lfc", "Unit/BetaTest.php", "ZTopTest.php"]);
        let single = collect_test_files(&dir.join("Unit/Helper.php")).expect("single file");
        assert_eq!(single.len(), 1, "an explicit file is taken as is");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A symlink back to an ancestor is visited once: the walk terminates and each test file
    /// is listed once.
    #[test]
    fn a_symlink_loop_is_walked_once() {
        let dir = scratch("symlink_loop");
        std::fs::create_dir_all(dir.join("Unit")).unwrap();
        std::fs::write(dir.join("Unit/LoopTest.php"), "").unwrap();
        std::os::unix::fs::symlink(&dir, dir.join("Unit/back")).unwrap();
        let found = collect_test_files(&dir).expect("walk terminates");
        assert_eq!(found.len(), 1, "{found:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The root is the nearest ancestor with a `composer.json`, starting at the test
    /// directory itself; without one the working directory is used.
    #[test]
    fn resolves_the_project_root_from_composer_json() {
        let dir = scratch("root");
        std::fs::create_dir_all(dir.join("project/tests/Unit")).unwrap();
        std::fs::write(dir.join("project/composer.json"), "{}").unwrap();
        let cwd = dir.join("elsewhere");
        let expected = dir.join("project").canonicalize().unwrap();
        assert_eq!(project_root(&dir.join("project/tests/Unit"), &cwd), expected);
        assert_eq!(project_root(&dir.join("project/tests"), &cwd), expected);
        std::fs::write(dir.join("project/tests/Unit/composer.json"), "{}").unwrap();
        assert_eq!(
            project_root(&dir.join("project/tests/Unit"), &cwd),
            dir.join("project/tests/Unit").canonicalize().unwrap(),
            "the nearest manifest wins"
        );
        std::fs::create_dir_all(dir.join("bare")).unwrap();
        assert_eq!(project_root(&dir.join("bare"), &cwd), cwd, "no manifest: the working directory");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The entry requires every file by its absolute, single-quoted path.
    #[test]
    fn entry_requires_every_file() {
        let files = vec![PathBuf::from("/p/tests/AlphaTest.php"), PathBuf::from("/p/it's/BetaTest.lfc")];
        assert_eq!(
            entry_source(&files),
            "require_once '/p/tests/AlphaTest.php';\nrequire_once '/p/it\\'s/BetaTest.lfc';\n"
        );
    }
}
