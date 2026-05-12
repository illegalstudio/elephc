//! Static autoload via composer.json PSR-4.
//!
//! Composer's `autoload.psr-4` section maps each namespace prefix to one or
//! more directories on disk. PSR-4 turns `App\Foo\Bar` into
//! `<dir>/Foo/Bar.php`. Because elephc is an AOT compiler, we cannot run
//! `spl_autoload_register` callbacks at runtime — instead we **pre-resolve**
//! every PSR-4 mapping at compile time: build an index of FQN → file path,
//! and after the resolver/name-resolver passes, walk the AST for class
//! references that aren't yet declared and inline the corresponding file.
//!
//! This matches `composer dump-autoload --classmap-authoritative` semantically.

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

use walk::{collect_declared_fqns, collect_referenced_fqns};

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

    // -- splice always-included files first --
    // composer.json's `autoload.files` declares files that must always be
    // included. Splice them up front so any classes/functions they declare
    // are present before the iterative class-reference loop begins.
    for path in registry.always_included_files() {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if included.insert(canonical.clone()) {
            program = splice_autoloaded_file(program, &canonical, base_dir)?;
        }
    }

    for _ in 0..MAX_ITERATIONS {
        let declared = collect_declared_fqns(&program);
        let referenced = collect_referenced_fqns(&program);
        let mut new_paths: Vec<PathBuf> = Vec::new();
        for fqn in &referenced {
            if declared.contains(fqn) {
                continue;
            }
            if let Some(path) = resolve_class(fqn, registry) {
                let canonical = path.canonicalize().unwrap_or(path);
                if included.insert(canonical.clone()) {
                    new_paths.push(canonical);
                }
            }
        }
        if new_paths.is_empty() {
            break;
        }
        for path in new_paths {
            program = splice_autoloaded_file(program, &path, base_dir)?;
        }
    }
    Ok(program)
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

/// Parse, resolve includes, and name-resolve a single file, then append the
/// resulting statements to `program`. Shared by PSR-4 lookups and (later)
/// closure-rule resolutions.
pub(super) fn splice_autoloaded_file(
    mut program: Program,
    path: &Path,
    base_dir: &Path,
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
    let resolved = crate::resolver::resolve(parsed, path.parent().unwrap_or(base_dir))?;
    let canonicalized: Vec<Stmt> = crate::name_resolver::resolve(resolved)?;
    // name_resolver has already flattened namespace nodes and canonicalized
    // declarations, so we splice the statements directly into the top-level
    // program.
    program.extend(canonicalized);
    Ok(program)
}
