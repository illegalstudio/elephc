//! Purpose:
//! Defines the per-file source profile: the language mode selected from a physical input path
//! plus the `declare(strict_types=1)` state that file opted into.
//! Centralizes `.lfc` classification so every file loader agrees on tag and strict-mode semantics.
//!
//! Called from:
//! - `crate::pipeline::compile()` for the entry source.
//! - `crate::resolver` and `crate::autoload` for additional physical source files.
//!
//! Key details:
//! - Only `.lfc` opts into tagless elephc source; every other path preserves tagged-PHP behavior.
//! - Classification is ASCII case-insensitive and never changes output-path naming.
//! - `strict_types` is a *per-file* PHP directive. It is stamped onto every `Stmt` created while
//!   one physical file is parsed (`crate::parser::ast::Stmt::strict_types`) and therefore survives
//!   include/autoload merging into the single flat program the type checker sees. Statement
//!   rewriting passes must re-install the profile they read off the statement they are rebuilding,
//!   which is why `with_parse_mode`/`scoped_parse_mode` take the whole `SourceProfile` instead of
//!   the mode alone: a rebuild that dropped the flag would silently downgrade a strict file to
//!   PHP's coercive parameter binding.

use std::cell::Cell;
use std::collections::HashSet;
use std::path::Path;

use crate::errors::CompileError;
use crate::parser::ast::Program;

/// Parser-only declaration name carrying one physical source's halt byte offset.
pub(crate) const HALT_OFFSET_SENTINEL: &str = "\0elephc.compiler_halt_offset\0";
/// Prefix of PHP's hidden per-file halt-offset constant name.
pub(crate) const HALT_OFFSET_MANGLED_PREFIX: &str = "\0__COMPILER_HALT_OFFSET__\0";

/// Returns whether a declaration name is PHP's hidden physical-file halt metadata.
pub(crate) fn is_mangled_halt_offset_name(name: &str) -> bool {
    name.starts_with(HALT_OFFSET_MANGLED_PREFIX)
}

/// Language profile selected for one physical source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SourceMode {
    /// Tagged PHP-compatible source that requires the normal `<?php` opening tag.
    Php,
    /// Tagless elephc source with every elephc extension available.
    Lfc,
    /// Compiler-generated source that is never subject to the user strict-PHP audit.
    Internal,
}

/// Everything one physical source file contributes to the AST nodes parsed from it.
///
/// `mode` comes from the file's path and is known before parsing starts; `strict_types` comes
/// from a `declare(strict_types=1)` directive and is only known once the parser has read the
/// file's first statement. Both are stamped onto every `Stmt` the file produces, so the merged
/// program still answers "which file was this written in" for the two questions that need it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SourceProfile {
    /// Language profile selected from the physical path.
    pub mode: SourceMode,
    /// Whether the file declared `strict_types=1`.
    pub strict_types: bool,
}

impl SourceProfile {
    /// Builds the profile a physical file starts parsing with: its path-derived mode and PHP's
    /// default coercive typing, which only a `declare(strict_types=1)` directive changes.
    pub fn new(mode: SourceMode) -> Self {
        Self {
            mode,
            strict_types: false,
        }
    }
}

thread_local! {
    /// Source mode inherited by AST nodes created during one parser invocation.
    static CURRENT_PARSE_MODE: Cell<SourceMode> = const { Cell::new(SourceMode::Internal) };

    /// `strict_types` state inherited by AST nodes created during one parser invocation.
    static CURRENT_STRICT_TYPES: Cell<bool> = const { Cell::new(false) };
}

impl SourceMode {
    /// Classifies a physical path, treating only a case-insensitive `.lfc` suffix as tagless.
    pub fn from_path(path: &Path) -> Self {
        if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("lfc"))
        {
            Self::Lfc
        } else {
            Self::Php
        }
    }

    /// Returns whether this source mode requires a user-written `<?php` opening tag.
    pub fn requires_open_tag(self) -> bool {
        matches!(self, Self::Php)
    }

    /// Returns whether an invocation-level strict-PHP request applies to this source.
    pub fn strict_php_is_effective(self, requested: bool) -> bool {
        requested && matches!(self, Self::Php)
    }
}

/// Returns whether Composer discovery should inspect a path as PHP/LFC source.
///
/// Physical includes remain PHP-compatible regardless of suffix, but Composer's
/// directory walkers intentionally discover only files named `.php` or `.lfc`.
pub fn is_composer_source_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("php") || extension.eq_ignore_ascii_case("lfc")
        })
}

/// Removes the recognized source suffix from one Composer-relative path component.
pub fn composer_source_stem(component: &str) -> String {
    Path::new(component)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or(component)
        .to_string()
}

/// RAII guard restoring the parser's previous source profile on drop.
pub(crate) struct ParseModeGuard {
    previous: SourceProfile,
}

impl Drop for ParseModeGuard {
    /// Restores the parser source profile active before the nested parse.
    fn drop(&mut self) {
        CURRENT_PARSE_MODE.with(|cell| cell.set(self.previous.mode));
        CURRENT_STRICT_TYPES.with(|cell| cell.set(self.previous.strict_types));
    }
}

/// Runs `f` while parser-created AST nodes inherit `profile`.
pub(crate) fn with_parse_mode<T>(profile: SourceProfile, f: impl FnOnce() -> T) -> T {
    let _guard = scoped_parse_mode(profile);
    f()
}

/// Installs one parser/source reconstruction profile until the returned guard is dropped.
pub(crate) fn scoped_parse_mode(profile: SourceProfile) -> ParseModeGuard {
    let mode = CURRENT_PARSE_MODE.with(|cell| cell.replace(profile.mode));
    let strict_types = CURRENT_STRICT_TYPES.with(|cell| cell.replace(profile.strict_types));
    ParseModeGuard {
        previous: SourceProfile { mode, strict_types },
    }
}

/// Returns the source mode assigned to AST nodes created at the current parse site.
pub(crate) fn current_parse_mode() -> SourceMode {
    CURRENT_PARSE_MODE.with(Cell::get)
}

/// Returns the `strict_types` state assigned to AST nodes created at the current parse site.
pub(crate) fn current_strict_types() -> bool {
    CURRENT_STRICT_TYPES.with(Cell::get)
}

/// Records that the file currently being parsed declared `strict_types=<enabled>`.
///
/// PHP requires the directive to be a file's very first statement, so every statement created
/// after this call belongs to the same file and inherits the flag. The enclosing
/// `ParseModeGuard` resets it when the file's parse ends, which is what keeps the directive from
/// leaking into an included file or back out to the includer.
pub(crate) fn declare_strict_types(enabled: bool) {
    CURRENT_STRICT_TYPES.with(|cell| cell.set(enabled));
}

/// Applies path-dependent post-parse processing shared by every physical source loader.
///
/// Magic constants retain the real path, strict PHP audits the unfiltered physical
/// program, and conditional compilation runs last so inactive extension syntax in a
/// PHP file cannot evade the audit.
pub fn finalize_physical_program(
    program: Program,
    path: &Path,
    mode: SourceMode,
    defines: &HashSet<String>,
) -> Result<Program, CompileError> {
    let (program, halt_offset) = extract_halt_offset(program);
    let program = match halt_offset {
        Some(offset) => {
            crate::magic_constants::substitute_halt_compiler_constants(program, offset, path)
        }
        None => program,
    };
    let program = crate::magic_constants::substitute_file_and_scope_constants(program, path);
    crate::strict_php::check_file_with_mode(&program, &path.display().to_string(), mode)?;
    Ok(crate::conditional::apply(program, defines))
}

/// Removes the parser's terminal halt sentinel and returns its byte offset.
fn extract_halt_offset(program: Program) -> (Program, Option<i64>) {
    let mut halt_offset = None;
    let mut executable = Vec::with_capacity(program.len());
    for stmt in program {
        match &stmt.kind {
            crate::parser::ast::StmtKind::ConstDecl { name, value }
                if name == HALT_OFFSET_SENTINEL =>
            {
                if let crate::parser::ast::ExprKind::IntLiteral(offset) = value.kind {
                    halt_offset = Some(offset);
                }
            }
            _ => executable.push(stmt),
        }
    }
    (executable, halt_offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies `.lfc` classification is case-insensitive while other suffixes remain PHP mode.
    #[test]
    fn classifies_only_lfc_as_tagless() {
        assert_eq!(SourceMode::from_path(Path::new("main.lfc")), SourceMode::Lfc);
        assert_eq!(SourceMode::from_path(Path::new("main.LFC")), SourceMode::Lfc);
        assert_eq!(SourceMode::from_path(Path::new("main.php")), SourceMode::Php);
        assert_eq!(SourceMode::from_path(Path::new("bootstrap.inc")), SourceMode::Php);
        assert_eq!(SourceMode::from_path(Path::new("script")), SourceMode::Php);
    }

    /// Verifies Composer discovery accepts only the two physical source suffixes.
    #[test]
    fn classifies_composer_source_paths() {
        assert!(is_composer_source_path(Path::new("src/App.php")));
        assert!(is_composer_source_path(Path::new("src/App.LFC")));
        assert!(!is_composer_source_path(Path::new("src/App.inc")));
        assert_eq!(composer_source_stem("App.lfc"), "App");
    }

    /// Verifies a nested file parse starts coercive and cannot leak its `strict_types` state
    /// back to the includer, which is what makes the directive per-file after include merging.
    #[test]
    fn nested_parse_scopes_strict_types_to_one_file() {
        with_parse_mode(SourceProfile::new(SourceMode::Php), || {
            assert!(!current_strict_types());
            declare_strict_types(true);
            assert!(current_strict_types());

            // An `include`d file parses inside the includer's scope and must start coercive.
            with_parse_mode(SourceProfile::new(SourceMode::Php), || {
                assert!(!current_strict_types());
                declare_strict_types(true);
            });
            assert!(current_strict_types());

            with_parse_mode(SourceProfile::new(SourceMode::Php), || {
                assert!(!current_strict_types());
            });
            assert!(current_strict_types());
        });
        assert!(!current_strict_types());
    }

    /// Verifies a statement-rewriting pass re-installing a statement's profile restores both the
    /// language mode and the `strict_types` flag, so a rebuilt node keeps its file's binding
    /// rules instead of silently reverting to coercive.
    #[test]
    fn reinstalling_a_profile_restores_both_fields() {
        let strict = SourceProfile {
            mode: SourceMode::Php,
            strict_types: true,
        };
        with_parse_mode(strict, || {
            assert_eq!(current_parse_mode(), SourceMode::Php);
            assert!(current_strict_types());
        });
        assert_eq!(current_parse_mode(), SourceMode::Internal);
        assert!(!current_strict_types());
    }

    /// Verifies strict PHP applies only to PHP-mode user source.
    #[test]
    fn strict_php_is_source_mode_aware() {
        assert!(SourceMode::Php.strict_php_is_effective(true));
        assert!(!SourceMode::Php.strict_php_is_effective(false));
        assert!(!SourceMode::Lfc.strict_php_is_effective(true));
        assert!(!SourceMode::Internal.strict_php_is_effective(true));
    }

    /// Verifies physical finalization rewrites public offset reads and retains PHP's hidden key.
    #[test]
    fn finalizes_halt_offset_as_file_local_metadata() {
        let source = "<?php echo __COMPILER_HALT_OFFSET__; __HALT_COMPILER();DATA";
        let tokens = crate::lexer::tokenize(source).expect("halt source tokenizes");
        let parsed = crate::parser::parse(&tokens).expect("halt source parses");
        let path = Path::new("/tmp/elephc-halt-source-test.php");
        let finalized = finalize_physical_program(
            parsed,
            path,
            SourceMode::Php,
            &HashSet::new(),
        )
        .expect("halt source finalizes");
        assert!(matches!(
            &finalized[0].kind,
            crate::parser::ast::StmtKind::ConstDecl { name, value }
                if name == "\0__COMPILER_HALT_OFFSET__\0/tmp/elephc-halt-source-test.php"
                    && value.kind == crate::parser::ast::ExprKind::IntLiteral(55)
        ));
        assert!(matches!(
            &finalized[1].kind,
            crate::parser::ast::StmtKind::Echo(expr)
                if expr.kind == crate::parser::ast::ExprKind::IntLiteral(55)
        ));
    }
}
