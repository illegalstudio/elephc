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

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};
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

/// A refusal no annotation can have caused: the class a declaration extends does not exist.
const MISSING_PARENT_REFUSAL: &str = "Unknown class referenced during inheritance flattening";

/// The most erasure re-runs one build takes before a refusal is reported as it is.
const MAX_ERASURE_ROUNDS: u32 = 8;

/// The physical files whose annotations were adopted on this run. Global rather than
/// thread-local: the autoload pass parses files on worker threads.
fn adopted() -> &'static Mutex<HashSet<PathBuf>> {
    static ADOPTED: OnceLock<Mutex<HashSet<PathBuf>>> = OnceLock::new();
    ADOPTED.get_or_init(|| Mutex::new(HashSet::default()))
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
    let paths_of = |error: &CompileError, names: bool| -> Vec<PathBuf> {
        let mut paths = Vec::new();
        paths.extend(error.span.source_path().map(|path| canonical(Path::new(&path))));
        paths.extend(error.file.as_ref().map(|path| canonical(Path::new(path))));
        let declared = error.declaration.iter().cloned();
        let named = names.then(|| class_names_in(&error.message)).into_iter().flatten();
        for name in declared.chain(named) {
            if let Some(path) = crate::autoload::deferral::class_file_path(class_files, &name) {
                paths.push(canonical(Path::new(path)));
            }
        }
        paths
    };
    // A parent that nothing provides stays missing with the written types too, so a file refused
    // for one leaves the program whatever else the round says about it. Erasing it re-ran the
    // whole compiler for nothing: PHPUnit's `TestCase` is absent from a production vendor tree,
    // and two templated `Test/` classes of symfony/validator cost every first build a seventh of
    // its time.
    let flattened = errors.flatten();
    let leaving: HashSet<PathBuf> = flattened
        .iter()
        .filter(|error| error.message.contains(MISSING_PARENT_REFUSAL))
        .flat_map(|error| paths_of(error, false))
        .collect();
    let mut out: Vec<PathBuf> = Vec::new();
    for error in flattened.iter().filter(|error| !error.message.contains(MISSING_PARENT_REFUSAL)) {
        for path in paths_of(error, true) {
            if adopted.contains(&path)
                && !leaving.contains(&path)
                && !erased_files().contains(&path)
                && !out.contains(&path)
            {
                out.push(path);
            }
        }
    }
    with_dependents(out)
}

/// Every class-like name `source` declares (`class X`, `interface X`, `trait X`).
///
/// A token scan, not a parse: a name spelled in prose is picked up too, which only makes the
/// closure below erase one more file, never fewer.
fn declared_names(source: &str) -> Vec<String> {
    let mut words = source.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'));
    let mut names = Vec::new();
    while let Some(word) = words.next() {
        if matches!(word, "class" | "interface" | "trait") {
            if let Some(name) = words.next().filter(|name| !name.is_empty()) {
                names.push(name.to_string());
            }
        }
    }
    names
}

/// Adds to `files` every other adopting file that writes type arguments on a class they declare.
///
/// Erasing a file turns its templates into ordinary classes, so a file elsewhere that still
/// writes `Repository<T>` or `@extends Base<T>` against one is refused in the NEXT round, which
/// erases one more file and re-runs the whole compiler: Symfony's Demo needed eight rounds for
/// Doctrine's schema classes alone and still ended refused. The closure is taken in one go
/// instead, to a fixpoint, so the re-run compiles every file that depends on an erased one with
/// its written types too. Over-erasing only gives up annotations php never read.
fn with_dependents(files: Vec<PathBuf>) -> Vec<PathBuf> {
    if files.is_empty() {
        return files;
    }
    let mut candidates: Vec<PathBuf> = adopted()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .cloned()
        .collect();
    candidates.sort();
    let mut sources: HashMap<PathBuf, String> = HashMap::default();
    let mut read = |path: &PathBuf| -> String {
        sources
            .entry(path.clone())
            .or_insert_with(|| std::fs::read_to_string(path).unwrap_or_default())
            .clone()
    };
    let mut out = files;
    let mut done = 0;
    while done < out.len() {
        let names = declared_names(&read(&out[done]));
        done += 1;
        if names.is_empty() {
            continue;
        }
        for path in &candidates {
            if out.contains(path) || erased_files().contains(path) {
                continue;
            }
            let text = read(path);
            if names.iter().any(|name| writes_arguments_on(&text, name)) {
                out.push(path.clone());
            }
        }
    }
    out
}

/// Whether `text` writes type arguments on `name`: `name<`, as a whole word.
fn writes_arguments_on(text: &str, name: &str) -> bool {
    let pattern = format!("{name}<");
    text.match_indices(&pattern).any(|(at, _)| {
        !text[..at]
            .chars()
            .next_back()
            .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
    })
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

    /// A missing parent is refused with the written types too, so it erases nothing.
    #[test]
    fn a_missing_parent_implicates_no_file() {
        let path = std::env::temp_dir().join(format!("elephc-erasure-{}.php", std::process::id()));
        std::fs::write(&path, "<?php").unwrap();
        let path = canonical(&path);
        adopted().lock().unwrap().insert(path.clone());
        let refusal = |message: &str| {
            let mut error = CompileError::new(crate::span::Span::dummy(), message);
            error.file = Some(path.display().to_string());
            error
        };
        let missing = refusal("Unknown class referenced during inheritance flattening: A\\B");
        assert!(implicated_files(&missing, &HashMap::default()).is_empty());
        let generic = refusal("Method 'Box<int>::get' return type expects int, got string");
        assert_eq!(implicated_files(&generic, &HashMap::default()), vec![path.clone()]);
        let mut both = missing;
        both.related.push(generic);
        assert!(implicated_files(&both, &HashMap::default()).is_empty());
        adopted().lock().unwrap().remove(&path);
        std::fs::remove_file(&path).unwrap();
    }

    /// Erasing a template's file takes in one go every adopting file writing arguments on it, and
    /// theirs in turn, so the re-run does not discover them one whole compile at a time.
    #[test]
    fn erasing_a_class_erases_its_dependents_to_a_fixpoint() {
        let dir = std::env::temp_dir().join(format!("elephc-erasure-closure-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let write = |name: &str, text: &str| {
            let path = dir.join(name);
            std::fs::write(&path, text).unwrap();
            canonical(&path)
        };
        let base = write("base.php", "<?php /** @template T */ class Base {}");
        let mid = write("mid.php", "<?php /** @template T @extends Base<T> */ class Mid extends Base {}");
        let leaf = write("leaf.php", "<?php /** @param Mid<int> $m */ class Leaf {}");
        let near_miss = write("near.php", "<?php /** @template T @extends OtherBase<T> */ class Near {}");
        let unrelated = write("unrelated.php", "<?php /** @template T */ class Alone {}");
        for path in [&base, &mid, &leaf, &near_miss, &unrelated] {
            adopted().lock().unwrap().insert(path.clone());
        }
        let closure = with_dependents(vec![base.clone()]);
        assert_eq!(closure, vec![base.clone(), mid.clone(), leaf.clone()]);
        for path in [&base, &mid, &leaf, &near_miss, &unrelated] {
            adopted().lock().unwrap().remove(path);
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_file_without_generic_tags_adopts_nothing() {
        assert!(!may_adopt("<?php /** @param int $x */ function f($x) {}"));
        assert!(may_adopt("<?php /** @template T */ class Box {}"));
    }
}
