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
mod registry;
mod rule;
mod walk;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

pub use registry::Registry;

use crate::errors::CompileError;
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
pub fn run(
    mut program: Program,
    base_dir: &Path,
    registry: &Registry,
) -> Result<Program, CompileError> {
    if registry.is_empty() {
        return Ok(program);
    }
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
            prefix.extend(load_autoloaded_file(&canonical, base_dir)?);
        }
    }
    if !prefix.is_empty() {
        prefix.extend(program);
        program = prefix;
    }

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
                    let loaded = load_autoloaded_file(&canonical, base_dir)?;
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
    Ok(program)
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
fn load_autoloaded_file(path: &Path, base_dir: &Path) -> Result<Program, CompileError> {
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
    let resolved = crate::resolver::resolve(parsed, path.parent().unwrap_or(base_dir))?;
    let resolved = alias::collect_aliases(resolved);
    let canonicalized: Vec<Stmt> = crate::name_resolver::resolve(resolved)?;
    // name_resolver has already flattened namespace nodes and canonicalized
    // declarations, so we splice the statements directly into the top-level
    // program.
    Ok(canonicalized)
}
