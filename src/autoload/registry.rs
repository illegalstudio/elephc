//! Compile-time autoload state.
//!
//! The Registry is the single source of truth for everything the autoload
//! pass needs: the static composer.json PSR-4 index, the user-registered
//! closure rules (extracted from `spl_autoload_register` calls), and the
//! current value of `spl_autoload_extensions`. It is built once after name
//! resolution and consumed by `autoload::run`.

use std::path::{Path, PathBuf};

use crate::errors::CompileWarning;
use crate::parser::ast::Program;

use super::index::AutoloadIndex;
use super::rule::{collect_register_calls, AutoloadRule};

/// PHP's spl_autoload_extensions default. Read by `spl_autoload_extensions()`
/// when no value has been set yet.
pub const DEFAULT_AUTOLOAD_EXTENSIONS: &str = ".inc,.php";

/// All compile-time autoload state. Owned by `main.rs` for the duration
/// of one compilation.
pub struct Registry {
    psr4: AutoloadIndex,
    rules: Vec<AutoloadRule>,
    extensions: String,
    warnings: Vec<CompileWarning>,
}

impl Registry {
    /// Build the registry by reading composer.json from `project_root` and
    /// scanning `program` for `spl_autoload_register` callsites. Returns
    /// the registry plus the program with consumed register sites stripped
    /// so the runtime stub doesn't see closure bodies containing
    /// non-foldable `require_once` statements.
    pub fn build(project_root: &Path, program: Program) -> (Self, Program) {
        let psr4 = AutoloadIndex::from_project_root(project_root);
        let (program, rules, warnings) = collect_register_calls(program);
        // Synthesise alias subclasses after closure collection so the
        // alias decls don't get confused with autoloader sources.
        let program = super::alias::collect_aliases(program);
        let registry = Registry {
            psr4,
            rules,
            extensions: DEFAULT_AUTOLOAD_EXTENSIONS.to_string(),
            warnings,
        };
        (registry, program)
    }

    pub fn psr4(&self) -> &AutoloadIndex {
        &self.psr4
    }

    /// Files listed under `autoload.files` (or `autoload-dev.files`) in
    /// any composer.json the index visited. They must always be inlined
    /// at compile time, regardless of which classes the program
    /// references.
    pub fn always_included_files(&self) -> &[PathBuf] {
        self.psr4.files()
    }

    pub fn rules(&self) -> &[AutoloadRule] {
        &self.rules
    }

    #[allow(dead_code)] // consumed by spl_autoload_extensions runtime helper
    pub fn extensions(&self) -> &str {
        &self.extensions
    }

    /// True when the registry has nothing to contribute — no PSR-4 mappings
    /// and no closure rules. Used by `run` to short-circuit when the program
    /// has no autoload to do.
    pub fn is_empty(&self) -> bool {
        self.psr4.is_empty() && self.rules.is_empty()
    }

    /// Number of registered closure rules, surfaced to runtime by
    /// `spl_autoload_functions()`.
    #[allow(dead_code)] // consumed by spl_autoload_functions codegen in a follow-up commit
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Compile-time warnings produced by the rule collector — typically
    /// `spl_autoload_register` calls whose closure was rejected because
    /// of `use(...)` captures or other constraints we can't reason
    /// about. main.rs prints these alongside type-checker warnings.
    pub fn warnings(&self) -> &[CompileWarning] {
        &self.warnings
    }
}
