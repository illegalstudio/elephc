//! Purpose:
//! Runs the interpreter parser over every PHP file of a real source tree and reports each refusal.
//!
//! Called from:
//! - `cargo test -p elephc-magician source_tree_sweep -- --ignored --nocapture` with
//!   `ELEPHC_PARSER_SWEEP_ROOTS` set to a colon-separated list of directories.
//!
//! Key details:
//! - Every file goes through `crate::parser::parse_source_file()`, the entry
//!   `crate::interpreter::include_exec` uses for a runtime include, so a refusal here is a refusal
//!   of the real bridge path.
//! - Only files that `php -n -l` accepts count as refusals; a file PHP itself rejects is
//!   reported separately and never counted against the parser.

use crate::parser::parse_source_file;
use std::path::{Path, PathBuf};

/// The colon-separated list of directories the sweep walks.
const SWEEP_ROOTS_ENV: &str = "ELEPHC_PARSER_SWEEP_ROOTS";

/// The PHP binary used as the oracle for which files are valid PHP.
const SWEEP_PHP_ENV: &str = "ELEPHC_PARSER_SWEEP_PHP";

/// One refused file with the diagnostic the interpreter parser produced for it.
struct Refusal {
    path: PathBuf,
    line: i64,
    token: String,
    reason: String,
}

/// Parses every PHP file under the configured roots and fails naming each refusal.
///
/// The sweep is `#[ignore]`d because it needs a checked-out application tree; it is committed so
/// a later campaign can re-run it against the same or another vendor directory without
/// rebuilding a throwaway harness.
#[test]
#[ignore = "needs ELEPHC_PARSER_SWEEP_ROOTS pointing at a real PHP source tree"]
fn source_tree_sweep_reports_every_interpreter_parser_refusal() {
    let Ok(roots) = std::env::var(SWEEP_ROOTS_ENV) else {
        panic!("{SWEEP_ROOTS_ENV} must name at least one directory to sweep");
    };
    let mut files = Vec::new();
    for root in roots.split(':').filter(|root| !root.is_empty()) {
        collect_php_files(Path::new(root), &mut files);
    }
    files.sort();
    assert!(!files.is_empty(), "the swept roots contain no PHP file");

    let mut refusals = Vec::new();
    for path in &files {
        if let Some(refusal) = refuse_file(path) {
            refusals.push(refusal);
        }
    }

    let php = std::env::var(SWEEP_PHP_ENV).unwrap_or_else(|_| "php".to_string());
    let mut valid = Vec::new();
    let mut invalid = Vec::new();
    for refusal in refusals {
        if php_accepts(&php, &refusal.path) {
            valid.push(refusal);
        } else {
            invalid.push(refusal);
        }
    }

    println!("SWEEP files={} refused={}", files.len(), valid.len());
    for refusal in &invalid {
        println!("PHPINVALID\t{}", refusal.path.display());
    }
    for refusal in &valid {
        println!(
            "REFUSE\t{}\t{}\t{}\t{}",
            refusal.reason,
            refusal.token,
            refusal.line,
            refusal.path.display()
        );
    }
    assert!(
        valid.is_empty(),
        "the interpreter parser refuses {} file(s) that php -n -l accepts",
        valid.len()
    );
}

/// Appends every `.php` file under one directory to `files`.
fn collect_php_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_php_files(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "php") {
            files.push(path);
        }
    }
}

/// Parses one file the way a runtime include does and returns its refusal.
fn refuse_file(path: &Path) -> Option<Refusal> {
    let bytes = std::fs::read(path).ok()?;
    let diagnostic = parse_source_file(&bytes).err()?;
    Some(Refusal {
        path: path.to_path_buf(),
        line: diagnostic.line(),
        token: diagnostic.token().to_string(),
        reason: format!("{:?}", diagnostic.error()),
    })
}

/// Returns true when `php -n -l` accepts the file.
fn php_accepts(php: &str, path: &Path) -> bool {
    std::process::Command::new(php)
        .arg("-n")
        .arg("-l")
        .arg(path)
        .output()
        .is_ok_and(|output| output.status.success())
}
