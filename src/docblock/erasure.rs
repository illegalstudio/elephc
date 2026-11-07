//! Purpose:
//! Falls a source file back to its WRITTEN types when its adopted PHPDoc generics refuse the
//! program, so an annotation can never make php-valid code fail to compile.
//!
//! Called from:
//! - `crate::source::finalize_physical_program()` (adoption, or not, per physical file).
//! - `crate::pipeline` once a check round failed, before any class file is handed to the runtime.
//!
//! Key details:
//! - php never reads a docblock. Adopting `@template` / `@extends X<T>` as real generics is
//!   elephc's extension, and PHPStan vocabulary routinely says more than elephc can honour
//!   (Symfony's Config builders: `NodeBuilder<TParent>` constructed from `null`, invariant
//!   `ExprBuilder<…>` instantiations). Fixing each shape locally never ends; this is the general
//!   fallback the local tolerances sit in front of.
//! - A failed round names files: the error's own position, its enclosing declaration, and every
//!   class it mentions. Each such file that ADOPTED annotations is re-read without them on a
//!   re-run of the compiler, exactly like the backend's class-file deferral re-run
//!   (`autoload::deferral`): the list travels in `ELEPHC_DOCBLOCK_ERASED_FILES`.
//! - Erasure is per physical file and total: a half-adopted file could mention an instantiation
//!   its own declaration no longer provides.
//! - The erased set is remembered beside the deferral memo (`autoload::deferral_memo`, same key,
//!   `.erased` extension), each file under the hash of its bytes, so the next build of the same
//!   program erases up front instead of paying the failed round again.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use crate::errors::CompileError;
use crate::parser::ast::Program;

/// Newline-separated canonical paths read without PHPDoc generics on this run.
const ERASED_FILES_ENV: &str = "ELEPHC_DOCBLOCK_ERASED_FILES";

/// The diagnostic of the round that FIRST erased files, replayed if erasing does not help.
const ORIGINAL_ERROR_ENV: &str = "ELEPHC_DOCBLOCK_ORIGINAL_ERROR";

/// Counts erasure re-runs, so a build whose refusals keep moving still ends.
const ERASURE_ROUND_ENV: &str = "ELEPHC_DOCBLOCK_ERASURE_ROUND";

/// The most erasure re-runs one build takes before a refusal is reported as it is.
const MAX_ERASURE_ROUNDS: u32 = 8;

/// The physical files whose annotations were adopted on this run. Global rather than
/// thread-local: the autoload pass parses files on worker threads.
fn adopted() -> &'static Mutex<HashSet<PathBuf>> {
    static ADOPTED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    ADOPTED.get_or_init(|| Mutex::new(HashSet::new()))
}

/// The entry program, which keys the memo; set once per compile before any file is parsed.
static ENTRY: OnceLock<PathBuf> = OnceLock::new();

/// Names the entry program so the remembered erasures of its earlier builds apply.
pub fn set_entry(entry: &Path) {
    let _ = ENTRY.set(entry.to_path_buf());
}

/// The files this run reads without PHPDoc generics: the re-run's list, plus the remembered
/// files whose bytes are unchanged.
fn erased_files() -> &'static HashSet<PathBuf> {
    static ERASED: OnceLock<HashSet<PathBuf>> = OnceLock::new();
    ERASED.get_or_init(|| {
        let mut erased: HashSet<PathBuf> = std::env::var(ERASED_FILES_ENV)
            .map(|joined| {
                joined
                    .lines()
                    .filter(|line| !line.is_empty())
                    .map(PathBuf::from)
                    .collect()
            })
            .unwrap_or_default();
        erased.extend(remembered());
        erased
    })
}

/// The memo's files whose bytes still hash as they did when they were erased.
fn remembered() -> Vec<PathBuf> {
    let Some(memo) = ENTRY
        .get()
        .and_then(|entry| crate::autoload::deferral_memo::sibling_memo_path(entry, "erased"))
    else {
        return Vec::new();
    };
    let Ok(text) = std::fs::read_to_string(memo) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|line| {
            let (hash, path) = line.split_once('\t')?;
            let hash = u64::from_str_radix(hash, 16).ok()?;
            let path = PathBuf::from(path);
            (crate::autoload::deferral_memo::content_hash(&path)? == hash).then_some(path)
        })
        .collect()
}

/// Records this build's erased files for the next build, once the program checked.
pub fn store_memo() {
    let Some(memo) = ENTRY
        .get()
        .and_then(|entry| crate::autoload::deferral_memo::sibling_memo_path(entry, "erased"))
    else {
        return;
    };
    let mut files: Vec<&PathBuf> = erased_files().iter().collect();
    if files.is_empty() {
        let _ = std::fs::remove_file(&memo);
        return;
    }
    files.sort();
    let mut text = String::new();
    for path in files {
        if let Some(hash) = crate::autoload::deferral_memo::content_hash(path) {
            text.push_str(&format!("{hash:016x}\t{}\n", path.display()));
        }
    }
    if let Some(parent) = memo.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(memo, text);
}

fn canonical(path: &Path) -> PathBuf {
    crate::source::canonical(&path).unwrap_or_else(|_| path.to_path_buf())
}

/// Whether `source` carries a tag that can make a declaration generic.
///
/// Member annotations are adopted only beside a template (`members`), so a file without one of
/// these tags adopts nothing and is never worth erasing.
fn may_adopt(source: &str) -> bool {
    source.contains("@template") || source.contains("@extends") || source.contains("@implements")
}

/// Applies the file's PHPDoc generics unless an earlier round of this build erased them.
pub fn apply_for_file(program: Program, source: &str, path: &Path) -> Program {
    if !may_adopt(source) {
        return program;
    }
    let path = canonical(path);
    if erased_files().contains(&path) {
        return program;
    }
    adopted().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(path);
    super::apply(program, source)
}

/// The adopting files a failed check round implicates, minus those already erased.
///
/// An error implicates the file it sits in, the file declaring its enclosing declaration, and
/// the file declaring every class it names — a generic call site (`$env->getExtension(...)` in
/// Twig's `core.php`) is refused for a template declared elsewhere (`Environment.php`).
pub fn implicated_files(
    errors: &CompileError,
    class_files: &HashMap<String, String>,
) -> Vec<PathBuf> {
    let adopted = adopted().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    if adopted.is_empty() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = Vec::new();
    let mut consider = |path: PathBuf| {
        if adopted.contains(&path) && !erased_files().contains(&path) && !out.contains(&path) {
            out.push(path);
        }
    };
    for error in errors.flatten() {
        if let Some(path) = error.span.source_path() {
            consider(canonical(Path::new(&path)));
        }
        if let Some(path) = &error.file {
            consider(canonical(Path::new(path)));
        }
        let names = error
            .declaration
            .iter()
            .cloned()
            .chain(class_names_in(&error.message));
        for name in names {
            if let Some(path) = crate::autoload::deferral::class_file_path(class_files, &name) {
                consider(canonical(Path::new(path)));
            }
        }
    }
    out
}

/// Every class-like name a diagnostic spells: quoted (`'Ns\Box'`, `'Ns\Box<int>::get'`) or
/// written with type arguments (`ExprBuilder<Ns\Node>`).
fn class_names_in(message: &str) -> Vec<String> {
    let is_name_char = |c: char| c.is_ascii_alphanumeric() || c == '_' || c == '\\';
    let mut names = Vec::new();
    for quoted in message.split('\'').skip(1).step_by(2) {
        let end = quoted.find(['<', ':', '(', ' ']).unwrap_or(quoted.len());
        if end > 0 {
            names.push(quoted[..end].to_string());
        }
    }
    for (index, _) in message.match_indices('<') {
        let start = message[..index]
            .rfind(|c: char| !is_name_char(c))
            .map_or(0, |at| at + 1);
        if start < index {
            names.push(message[start..index].to_string());
        }
        // The arguments are classes too: `ExprBuilder<Ns\NodeDefinition>`.
        let rest = &message[index + 1..];
        let end = rest.find(|c: char| !is_name_char(c)).unwrap_or(rest.len());
        if end > 0 {
            names.push(rest[..end].to_string());
        }
    }
    names
}

/// Re-runs the compiler with `files` read without PHPDoc generics, if the round limit allows.
///
/// Returns only when no re-run happened (the limit was reached, or the executable could not be
/// started); the caller then reports the refusal as it is.
pub fn rerun_erasing(files: &[PathBuf], refusal: &CompileError) {
    if files.is_empty() {
        return;
    }
    let round: u32 = std::env::var(ERASURE_ROUND_ENV)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);
    if round >= MAX_ERASURE_ROUNDS {
        return;
    }
    let all: Vec<String> = erased_files()
        .iter()
        .chain(files.iter())
        .map(|path| path.display().to_string())
        .collect();
    crate::progress::clear();
    for path in files {
        eprintln!(
            "note: '{}' is compiled with its written types: its PHPDoc generics refuse the program",
            path.display()
        );
    }
    let Ok(executable) = std::env::current_exe() else {
        return;
    };
    let mut command = std::process::Command::new(executable);
    command
        .args(std::env::args_os().skip(1))
        .env(ERASED_FILES_ENV, all.join("\n"))
        .env(ERASURE_ROUND_ENV, (round + 1).to_string());
    // The first refusal is the one the program's author wrote against: if the written types fail
    // too (`id<int>("seven")` is refused either way), it is the diagnostic worth reading.
    if std::env::var_os(ORIGINAL_ERROR_ENV).is_none() {
        command.env(ORIGINAL_ERROR_ENV, crate::errors::render_plain(refusal));
    }
    // Replaces this process, as the deferral re-run does, so two compiled programs are never
    // resident at once.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let _ = command.exec();
    }
    if let Ok(status) = command.status() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

/// Prints the refusal that first triggered erasure, when this run erased files and still fails.
///
/// Returns whether it printed, in which case the caller skips its own (secondary) diagnostic.
pub fn report_original_instead() -> bool {
    let Ok(original) = std::env::var(ORIGINAL_ERROR_ENV) else {
        return false;
    };
    if erased_files().is_empty() {
        return false;
    }
    crate::progress::clear();
    eprintln!("{original}");
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_quoted_and_instantiated_classes() {
        let names = class_names_in(
            "Method 'Ns\\Norm<Ns\\Node>::before' return type expects Ns\\Expr<Ns\\Node>, got x",
        );
        for expected in ["Ns\\Norm", "Ns\\Node", "Ns\\Expr"] {
            assert!(names.iter().any(|name| name == expected), "{expected} in {names:?}");
        }
    }

    #[test]
    fn a_file_without_generic_tags_adopts_nothing() {
        assert!(!may_adopt("<?php /** @param int $x */ function f($x) {}"));
        assert!(may_adopt("<?php /** @template T */ class Box {}"));
    }
}
