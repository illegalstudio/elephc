//! Purpose:
//! Re-exports the syntax crate's source profile and applies physical-file finalization.
//!
//! Called from:
//! - `crate::pipeline::compile()` for the entry source.
//! - `crate::resolver` and `crate::autoload` for additional physical source files.
//!
//! Key details:
//! - Only `.lfc` opts into tagless elephc source; every other path preserves tagged-PHP behavior.
//! - `strict_types` is a per-file directive stamped on every parsed statement.

use std::collections::HashSet;
use std::path::Path;

use crate::errors::CompileError;
use crate::parser::ast::Program;

/// Returns the canonical spelling of one physical source path.
///
/// Falls back to the given spelling when the path no longer resolves (for example a deleted
/// temporary), so a diagnostic still names something.
pub(crate) fn canonical_source_path(path: &std::path::Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

/// Returns the canonical physical file of one span.
///
/// Runtime diagnostics must name the same file as `_script_source_file` and `__FILE__`, which
/// use the canonical entry path even when the CLI named it through a symlink.
pub(crate) fn canonical_span_source_path(span: crate::span::Span) -> Option<String> {
    span.source_path()
        .map(|path| canonical_source_path(std::path::Path::new(&path)))
}

#[allow(unused_imports)]
pub use elephc_chopper::syntax::source::{
    composer_source_stem, current_parse_mode, current_strict_types, declare_strict_types,
    is_composer_source_path, scoped_parse_mode, with_parse_mode, SourceMode, SourceProfile,
};

/// Applies path-dependent post-parse processing shared by every physical source loader.
///
/// Magic constants retain the real path, strict PHP audits the unfiltered physical
/// program, and conditional compilation runs after it so inactive extension syntax in a
/// PHP file cannot evade the audit. `goto` lowering closes the sequence, so no later pass
/// ever sees a jump.
pub fn finalize_physical_program(
    program: Program,
    source: &str,
    path: &Path,
    mode: SourceMode,
    defines: &HashSet<String>,
) -> Result<Program, CompileError> {
    let program = crate::magic_constants::substitute_file_and_scope_constants(program, path);
    crate::strict_php::check_file_with_mode(&program, &path.display().to_string(), mode)?;
    // Generic doc-comment annotations are applied AFTER the strict audit, and that order is the
    // whole point of the surface: a `@template` file is valid PHP — the annotations are
    // comments — so `--strict-php` must keep accepting it. Applying them first made the audit
    // see the injected `<T>` and `array<T>` and reject a file php-src parses happily.
    //
    // Per physical file, because the lexer discards comments and they are recovered from the
    // source text by line: that only means anything while the file is still its own, before
    // include resolution splices every file into one program without rebasing line numbers.
    let program = crate::docblock::apply(program, source);
    let program = crate::conditional::apply(program, defines);
    // Last, on the statements that survive conditional compilation: a label inside an inactive
    // `ifdef` branch does not exist, and one inside the active branch belongs to the enclosing
    // statement list once the branch is spliced in.
    crate::goto_lowering::lower_program(program, &path.display().to_string())
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

    /// Verifies a nested file parse starts coercive and cannot leak its `strict_types` state.
    #[test]
    fn nested_parse_scopes_strict_types_to_one_file() {
        with_parse_mode(SourceProfile::new(SourceMode::Php), || {
            assert!(!current_strict_types());
            declare_strict_types(true);
            assert!(current_strict_types());

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

    /// Verifies a statement-rewriting pass re-installing a profile restores both fields.
    #[test]
    fn reinstalling_a_profile_restores_both_fields() {
        let strict = SourceProfile { mode: SourceMode::Php, strict_types: true };
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
}
