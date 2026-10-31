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

#[allow(unused_imports)]
pub use elephc_syntax::source::{
    composer_source_stem, current_parse_mode, current_strict_types, declare_strict_types,
    is_composer_source_path, scoped_parse_mode, with_parse_mode, SourceMode, SourceProfile,
};

/// Applies path-dependent post-parse processing shared by every physical source loader.
///
/// Magic constants retain the real path, strict PHP audits the unfiltered physical
/// program, and conditional compilation runs last so inactive extension syntax in a
/// PHP file cannot evade the audit.
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
    Ok(crate::conditional::apply(program, defines))
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
