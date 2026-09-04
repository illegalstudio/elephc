//! Purpose:
//! Assembles the complete mysqli prelude PHP source from its fragments
//! (constants, exception, connection, result, statement, and the procedural
//! aliases), applying `--php-version` gates. TEST-ONLY: the compiler builds the
//! surface as AST (`build::mysqli_declarations`); this text is the oracle that
//! build is compared against, node by node, for every profile.
//!
//! Called from:
//! - `crate::mysqli_prelude::parsed_prelude_for_version` (the oracle's parsed
//!   side) and `assembled_source_for_version` (the transcription driver).
//!
//! Key details:
//! - Fragments are plain PHP bodies without a `<?php` header; this module owns
//!   the single header, so concatenation order is free (the prelude carries only
//!   hoisted declarations) — but the BUILT aggregator follows the same order so
//!   the oracle can zip the two programs.
//! - Version gates are source rewrites at assembly time, mirroring
//!   `pdo_prelude::prelude_source_for_version`: PHP 8.0 flips the baked
//!   `mysqli_report` default from `ERROR|STRICT` (3) to `OFF` (0). Each gate has
//!   a conditional twin in `build/`; the tests below assert the gates on the
//!   built AST, and the oracle proves the text agrees.

use crate::php_version::PhpVersion;

/// Returns the complete mysqli prelude source for one PHP compatibility version.
pub(super) fn source_for_version(php_version: PhpVersion) -> String {
    let mut source = String::from("<?php\n");
    source.push_str(super::constants::SRC);
    source.push_str(super::exception::SRC);
    source.push_str(super::connection::SRC);
    source.push_str(super::result::SRC);
    source.push_str(super::statement::SRC);
    source.push_str(super::procedural::SRC);
    if php_version < PhpVersion::Php82 {
        // mysqli::execute_query / mysqli_execute_query are PHP 8.2+.
        remove_version_block(
            &mut source,
            "    // -- elephc PHP >= 8.2 mysqli execute_query begin --",
            "    // -- elephc PHP >= 8.2 mysqli execute_query end --",
        );
        remove_version_block(
            &mut source,
            "// -- elephc PHP >= 8.2 mysqli execute_query begin --",
            "// -- elephc PHP >= 8.2 mysqli execute_query end --",
        );
    }
    if php_version < PhpVersion::Php81 {
        // PHP 8.0's default mysqli_report mode is MYSQLI_REPORT_OFF; 8.1+
        // defaults to MYSQLI_REPORT_ERROR | MYSQLI_REPORT_STRICT.
        source = source.replace(
            "public static int $reportMode = 3;",
            "public static int $reportMode = 0;",
        );
        // mysqli_result::fetch_column and its procedural alias are PHP 8.1+.
        remove_version_block(
            &mut source,
            "    // -- elephc PHP >= 8.1 mysqli fetch_column begin --",
            "    // -- elephc PHP >= 8.1 mysqli fetch_column end --",
        );
        remove_version_block(
            &mut source,
            "// -- elephc PHP >= 8.1 mysqli fetch_column begin --",
            "// -- elephc PHP >= 8.1 mysqli fetch_column end --",
        );
    }
    source
}

/// Removes one inclusive source fragment delimited by stable version-gate
/// comments. Panics when either marker is missing because a renamed prelude
/// marker must fail compiler tests loudly instead of silently exposing a
/// method in the wrong PHP version (same contract as the PDO prelude's helper).
fn remove_version_block(source: &mut String, begin: &str, end: &str) {
    let start = source
        .find(begin)
        .unwrap_or_else(|| panic!("missing mysqli prelude version-gate marker: {begin}"));
    let relative_end = source[start..]
        .find(end)
        .unwrap_or_else(|| panic!("missing mysqli prelude version-gate marker: {end}"));
    let mut finish = start + relative_end + end.len();
    if source.as_bytes().get(finish) == Some(&b'\n') {
        finish += 1;
    }
    source.replace_range(start..finish, "");
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the mysqli prelude version gates: every supported PHP
    //! version's assembled text tokenizes/parses, and the gated members
    //! (`execute_query`, `fetch_column`, the `mysqli_report` default) follow the
    //! version on the BUILT surface the compiler injects.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The gate assertions inspect `build::mysqli_declarations`, not the text:
    //!   the oracle in `mysqli_prelude::oracle_tests` already proves text and
    //!   build agree, and the build is what ships.

    use super::*;
    use crate::mysqli_prelude::build::mysqli_declarations;
    use crate::parser::ast::{ExprKind, Program, StmtKind};

    /// Every supported PHP version's assembled source tokenizes and parses.
    #[test]
    fn every_version_source_tokenizes_and_parses() {
        for version in PhpVersion::ALL {
            let source = source_for_version(version);
            let tokens = crate::lexer::tokenize(&source)
                .unwrap_or_else(|e| panic!("{version:?} prelude must tokenize: {e:?}"));
            crate::parser::parse_internal(&tokens)
                .unwrap_or_else(|e| panic!("{version:?} prelude must parse: {e:?}"));
        }
    }

    /// `execute_query` (method and procedural alias) exists from PHP 8.2 only.
    #[test]
    fn execute_query_is_version_gated() {
        let php81 = mysqli_declarations(PhpVersion::Php81);
        assert!(!class_has_method(&php81, "mysqli", "execute_query"));
        assert!(!has_function(&php81, "mysqli_execute_query"));
        let php82 = mysqli_declarations(PhpVersion::Php82);
        assert!(class_has_method(&php82, "mysqli", "execute_query"));
        assert!(has_function(&php82, "mysqli_execute_query"));
    }

    /// `fetch_column` (method and procedural alias) exists from PHP 8.1 only.
    #[test]
    fn fetch_column_is_version_gated() {
        let php80 = mysqli_declarations(PhpVersion::Php80);
        assert!(!class_has_method(&php80, "mysqli_result", "fetch_column"));
        assert!(!has_function(&php80, "mysqli_fetch_column"));
        let php81 = mysqli_declarations(PhpVersion::Php81);
        assert!(class_has_method(&php81, "mysqli_result", "fetch_column"));
        assert!(has_function(&php81, "mysqli_fetch_column"));
    }

    /// PHP 8.0 bakes `mysqli_report` default OFF; 8.1+ bakes ERROR|STRICT.
    #[test]
    fn report_mode_default_follows_php_version() {
        assert_eq!(report_mode_default(&mysqli_declarations(PhpVersion::Php80)), 0);
        assert_eq!(report_mode_default(&mysqli_declarations(PhpVersion::Php81)), 3);
        assert_eq!(report_mode_default(&mysqli_declarations(PhpVersion::Php85)), 3);
    }

    /// Whether the built program declares `class_name::method_name`.
    fn class_has_method(program: &Program, class_name: &str, method_name: &str) -> bool {
        program.iter().any(|stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, methods, .. } if name == class_name => {
                methods.iter().any(|method| method.name == method_name)
            }
            _ => false,
        })
    }

    /// Whether the built program declares a top-level function `function_name`.
    fn has_function(program: &Program, function_name: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == function_name)
        })
    }

    /// The integer literal `mysqli::$reportMode` is declared with.
    fn report_mode_default(program: &Program) -> i64 {
        program
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, properties, .. } if name == "mysqli" => properties
                    .iter()
                    .find(|property| property.is_static && property.name == "reportMode")
                    .map(|property| match &property.default {
                        Some(expr) => match &expr.kind {
                            ExprKind::IntLiteral(value) => *value,
                            other => panic!("$reportMode default is not an int: {other:?}"),
                        },
                        None => panic!("$reportMode has no default"),
                    }),
                _ => None,
            })
            .expect("the built prelude declares mysqli::$reportMode")
    }
}
