//! Purpose:
//! Resolves static Composer autoload mappings and supported SPL registration patterns.
//! Prefixes Composer `autoload.files` and inlines class files discovered by the AOT autoload registry.
//!
//! Called from:
//! - `crate::pipeline::compile()`
//!
//! Key details:
//! - Runtime autoload callbacks cannot run in native binaries; supported rules are interpreted at compile time.
//! - Composer files execute before the entry program while class-triggered files splice before first use.

mod alias;
mod index;
mod interpret;
mod polyfill_prune;
mod registry;
mod rule;
mod walk;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use registry::Registry;

use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::Program;
use crate::parser::ast::Stmt;
use crate::span::Span;

use walk::{collect_declared_fqns, collect_reference_points};

/// Built-in class-like names that exist in every PHP environment (e.g. `Exception`,
/// `stdClass`, `Iterator`). Seeded into the declared FQN set so references to these
/// types are never treated as autoload demands.
const BUILTIN_CLASS_LIKE_NAMES: &[&str] = &[
    "ArrayAccess",
    "AppendIterator",
    "ArrayIterator",
    "ArrayObject",
    "BadFunctionCallException",
    "BadMethodCallException",
    "CachingIterator",
    "CallbackFilterIterator",
    "Countable",
    "DomainException",
    "EmptyIterator",
    "Error",
    "Exception",
    "Fiber",
    "FiberError",
    "Generator",
    "InternalIterator",
    "InvalidArgumentException",
    "Iterator",
    "IteratorAggregate",
    "IteratorIterator",
    "JsonException",
    "JsonSerializable",
    "LengthException",
    "LimitIterator",
    "LogicException",
    "MultipleIterator",
    "NoRewindIterator",
    "OutOfBoundsException",
    "OutOfRangeException",
    "OuterIterator",
    "OverflowException",
    "ParentIterator",
    "RangeException",
    "RecursiveArrayIterator",
    "RecursiveCallbackFilterIterator",
    "RecursiveFilterIterator",
    "RecursiveIterator",
    "RecursiveIteratorIterator",
    "ReflectionAttribute",
    "ReflectionClass",
    "ReflectionMethod",
    "ReflectionProperty",
    "RuntimeException",
    "SeekableIterator",
    "SortDirection",
    "SplDoublyLinkedList",
    "SplFixedArray",
    "SplObserver",
    "SplQueue",
    "SplStack",
    "SplSubject",
    "Stringable",
    "Throwable",
    "Traversable",
    "TypeError",
    "UnderflowException",
    "UnexpectedValueException",
    "ValueError",
    "stdClass",
];

/// Run the autoload pass over a fully resolver+name_resolver-processed
/// program. For every canonical class reference that isn't declared in
/// the program, look it up first in the composer.json PSR-4 index and
/// then in the user-registered closure rules; parse the referenced file,
/// run resolver+name_resolver on it, and append. Iterate until stable.
///
/// Returns the program plus any `CompileWarning`s collected along the way:
/// `autoload.files` helpers that fail to parse or read are skipped with a
/// warning rather than aborting the build (they are always-included but
/// frequently unreferenced by the app), while a class the program actually
/// references must still load or the call returns `Err`.
pub fn run(
    mut program: Program,
    base_dir: &Path,
    registry: &Registry,
) -> Result<(Program, Vec<CompileWarning>), CompileError> {
    if registry.is_empty() {
        return Ok((program, Vec::new()));
    }
    let mut warnings: Vec<CompileWarning> = Vec::new();
    let mut included: HashSet<PathBuf> = HashSet::new();
    const MAX_ITERATIONS: usize = 64;

    // -- prefix always-included files first --
    // composer.json's `autoload.files` declares files that must always be
    // included. Prefix them in Composer order so their top-level statements
    // execute before the entry program.
    let mut prefix: Program = Vec::new();
    for path in registry.always_included_files() {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if included.insert(canonical.clone()) {
            // `autoload.files` helpers are always-included but often unreferenced by
            // the app. Tolerate an unparseable or unreadable helper by skipping it and
            // recording a warning rather than aborting the whole build, so one
            // unsupported construct in an unused helper cannot kill compilation.
            // Strict include resolution (`false`): a helper's top-level statements run
            // eagerly at startup, so an unresolvable dynamic include must surface as an
            // error that becomes a skip here, not a degraded stub that would fatal at boot.
            match load_autoloaded_file(&canonical, base_dir, false) {
                Ok(stmts) => prefix.extend(stmts),
                Err(e) => warnings.push(CompileWarning::new(
                    Span::dummy(),
                    &format!(
                        "Autoload: skipped autoload.files helper '{}': {}",
                        canonical.display(),
                        e.message
                    ),
                )),
            }
        }
    }
    if !prefix.is_empty() {
        prefix.extend(program);
        program = prefix;
    }

    // Remove PHP polyfill redefinition guards for functions elephc provides. The
    // guarded wrapper bodies are never materialized, so dropping them keeps the
    // classes they delegate to (e.g. the 97 KB `DeepClone` polyfill) out of the
    // reference graph collected below.
    program = polyfill_prune::prune_provided_function_polyfills(program);

    // Drop definition guards for optional `autoload.files` helpers the program never calls
    // (e.g. Symfony's `u()`/`b()`/`s()` and `dump()`/`dd()`). Their bodies construct heavy
    // classes (`UnicodeString`, `ByteString`, `VarDumper`) that would otherwise be dragged into
    // the closure purely by the unused helper, not the program's actual reachable code.
    program = polyfill_prune::prune_unused_optional_helpers(program);

    for _ in 0..MAX_ITERATIONS {
        let mut declared = collect_declared_fqns(&program);
        seed_builtin_declared_fqns(&mut declared);
        let reference_points = collect_reference_points(&program);
        let mut insertions: Vec<(usize, Program)> = Vec::new();
        for (stmt_idx, fqn) in reference_points {
            if declared.contains(&fqn) {
                continue;
            }
            if let Some(path) = resolve_class(&fqn, registry) {
                let canonical = path.canonicalize().unwrap_or(path);
                if included.insert(canonical.clone()) {
                    // Referenced classes must load or the program is broken: a class the
                    // app actually uses cannot be tolerated-away like an unreferenced
                    // `autoload.files` helper, so a load failure here is a hard error.
                    // Lenient include resolution (`true`): a dynamic include inside a class
                    // method is lazy and may never run, so an unresolvable one degrades to a
                    // runtime-fatal stub instead of failing the whole compile.
                    let loaded = load_autoloaded_file(&canonical, base_dir, true)?;
                    insertions.push((stmt_idx, loaded));
                }
            }
        }
        if insertions.is_empty() {
            break;
        }
        let mut offset = 0usize;
        for (stmt_idx, loaded) in insertions {
            let insert_at = stmt_idx + offset;
            offset += loaded.len();
            program.splice(insert_at..insert_at, loaded);
        }
    }
    Ok((program, warnings))
}

/// Lower any top-level literal `class_alias()` calls left after another
/// expansion pass, such as resolver includes or autoloaded files.
pub fn collect_aliases(program: Program) -> Program {
    alias::collect_aliases(program)
}

/// Inserts PHP's built-in class-like names into `declared` so that references
/// to types like `Exception`, `stdClass`, and `Iterator` are never treated as
/// autoload demands. Called at the start of each autoload iteration.
fn seed_builtin_declared_fqns(declared: &mut HashSet<String>) {
    for name in BUILTIN_CLASS_LIKE_NAMES {
        declared.insert((*name).to_string());
    }
}

/// Try the resolution chain in order: composer.json PSR-4 first, then each
/// user-registered closure rule. Returns the first rule that produces a
/// path matching an existing file on disk.
fn resolve_class(fqn: &str, registry: &Registry) -> Option<PathBuf> {
    if let Some(path) = registry.psr4().lookup(fqn) {
        return Some(path.to_path_buf());
    }
    for rule in registry.rules() {
        if let Some(path) = interpret::resolve(rule, fqn) {
            if path.is_file() {
                return Some(path);
            }
        }
    }
    None
}

/// Load, parse, and resolve a single autoloaded PHP file, returning its statements.
///
/// `lenient_includes` selects include-resolution strictness for this file. It is `true` only
/// for lazily-referenced class files: such a file's dynamic `include`/`require` typically sits
/// inside a method that may never run for the program being built (e.g. a polyfill that
/// `require`s a data table by a computed path), so an unresolvable runtime-dynamic path is
/// degraded to a runtime-fatal stub rather than failing compilation. It is `false` for
/// always-included `autoload.files` helpers, whose top-level statements execute eagerly at
/// startup: a degraded stub there would fatal immediately, so those keep the strict behavior
/// and an unresolvable include surfaces as an error the caller turns into a tolerant skip.
fn load_autoloaded_file(
    path: &Path,
    base_dir: &Path,
    lenient_includes: bool,
) -> Result<Program, CompileError> {
    let content = std::fs::read_to_string(path).map_err(|e| {
        CompileError::new(
            Span::dummy(),
            &format!("Autoload: cannot read '{}': {}", path.display(), e),
        )
    })?;
    let file_label = path.display().to_string();
    let tokens = crate::lexer::tokenize(&content).map_err(|e| e.with_file(file_label.clone()))?;
    let parsed = crate::parser::parse(&tokens).map_err(|e| e.with_file(file_label.clone()))?;
    let parsed = crate::magic_constants::substitute_file_and_scope_constants(parsed, path);
    let include_base = path.parent().unwrap_or(base_dir);
    let resolved = if lenient_includes {
        crate::resolver::resolve_lenient_includes(parsed, include_base)?
    } else {
        crate::resolver::resolve(parsed, include_base)?
    };
    let resolved = alias::collect_aliases(resolved);
    let canonicalized: Vec<Stmt> = crate::name_resolver::resolve(resolved)?;
    // name_resolver has already flattened namespace nodes and canonicalized
    // declarations, so we splice the statements directly into the top-level
    // program.
    Ok(canonicalized)
}
