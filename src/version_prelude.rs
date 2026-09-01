//! Purpose:
//! Injects the three PHP version/environment-surface functions that carry no runtime state
//! and are therefore pure compile-time answers: `zend_version()`, `php_sapi_name()` and
//! `ini_restore()`.
//!
//! Called from:
//! - `crate::pipeline::compile()`, after the OPcache/web preludes and before name
//!   resolution, so a namespaced caller still resolves to the injected declaration.
//!
//! Key details:
//! - Pay-for-use per function, with a per-function redeclaration guard, exactly like the
//!   OPcache prelude: a program that never mentions `zend_version` carries nothing, and a
//!   program that declares its own `ini_restore` keeps it. The reference-detection walk is
//!   shared with the OPcache prelude (`opcache_prelude::detect`) rather than duplicated.
//! - These are declared PHP FUNCTIONS, not builtins, for the same reason `ini_get` /
//!   `ini_set` / `ini_get_all` are (see `opcache_prelude::build::cli_ini_get_decl`): they are
//!   fixed projections of compile-time configuration, and a real declaration is what makes
//!   `function_exists('zend_version')` report `true`.
//! - `php_sapi_name()` returns the `PHP_SAPI` CONSTANT rather than a second baked literal,
//!   so the two can never drift; `PHP_SAPI` itself is baked from `web_prelude::sapi_name`
//!   by `codegen_support::prescan::collect_constants`.

use crate::parser::ast::{CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_cast, e_const, e_str, e_var, function, internal_declarations, s_assign,
};
use crate::web_prelude::PhpVersion;

/// `zend_version()`: the Zend Engine version string for the compile target.
///
/// The default profile reports the frozen php-src oracle's `4.5.10-dev`; older profiles retain
/// their `4.<minor>.0` spelling. See `PhpVersion::zend_version` for the shared source of truth.
/// The version is baked in as a literal at injection time.
fn zend_version_decl(php_version: PhpVersion) -> Stmt {
    function("zend_version")
        .returns(TypeExpr::Str)
        .returning(e_str(php_version.zend_version()))
        .build()
}

/// `php_sapi_name()`: the SAPI name for the compile mode (`cli`, or `cli-server` under
/// `--web`).
///
/// Reference PHP declares this `string|false`; elephc always knows its own mode, so the
/// declaration is `string` and the `false` arm is unreachable — a narrowing, not a
/// divergence in observed values. The value is read from `PHP_SAPI` so the constant and the
/// function are one source of truth (`web_prelude::sapi_name`).
fn php_sapi_name_decl() -> Stmt {
    function("php_sapi_name")
        .returns(TypeExpr::Str)
        .returning(e_const("PHP_SAPI"))
        .build()
}

/// `ini_restore()`: restores a directive to its startup value — a NO-OP in elephc.
///
/// Reference PHP resets the directive to the value it had at startup and returns `void`
/// (verified on 8.5.6: `var_dump(ini_restore('precision'))` prints `NULL`).
///
/// In elephc every INI value is baked into the binary at compile time and NOTHING can change
/// it at runtime: `ini_set()` already reports failure (`false`) for every key it is asked to
/// set, precisely because the compiled value cannot move (see
/// `opcache_prelude::build::cli_ini_set_decl` and the `--web` `ini_set` in `web_prelude`). A
/// directive is therefore *always already* at its startup value, which makes "restore it to
/// the startup value" a no-op that is not an approximation but the exact outcome: after
/// `ini_restore($k)`, `ini_get($k)` returns what it returned before, which is what reference
/// PHP guarantees too whenever no `ini_set` succeeded in between. Because no `ini_set` can
/// ever succeed here, that condition always holds.
///
/// The parameter is consumed into a discarded local so the checker does not flag it unused,
/// mirroring how the CLI `ini_set` wrapper consumes `$value`.
fn ini_restore_decl() -> Stmt {
    function("ini_restore")
        .param("option", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![s_assign(
            "option",
            e_cast(CastType::String, e_var("option")),
        )])
        .build()
}

/// Prepends the version-surface functions this program actually references.
///
/// Each function is independently detected and independently guarded: a program that
/// references `php_sapi_name` but not `zend_version` gets only the former, and a program that
/// declares its own `ini_restore` gets none of ours for that name. Returns the program
/// unchanged when nothing is needed, so unrelated binaries pay nothing.
///
/// Injection is hoisted function declarations only, so prepending cannot change top-level
/// execution order.
pub fn inject_if_used(
    program: Program,
    php_version: PhpVersion,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let selected: Vec<&str> = ["zend_version", "php_sapi_name", "ini_restore"]
        .into_iter()
        .filter(|name| {
            crate::opcache_prelude::detect::program_references(&program, name)
                && !crate::opcache_prelude::detect::program_declares(&program, name)
        })
        .collect();
    if selected.is_empty() {
        return program;
    }
    let mut combined = version_declarations(&selected, php_version);
    inventory.record_program("version", &combined);
    combined.extend(program);
    combined
}

/// Builds the declarations for exactly the named functions, in the canonical order.
pub(crate) fn version_declarations(selected: &[&str], php_version: PhpVersion) -> Program {
    internal_declarations(|| {
        let mut declarations = Vec::new();
        if selected.contains(&"zend_version") {
            declarations.push(zend_version_decl(php_version));
        }
        if selected.contains(&"php_sapi_name") {
            declarations.push(php_sapi_name_decl());
        }
        if selected.contains(&"ini_restore") {
            declarations.push(ini_restore_decl());
        }
        declarations
    })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for pay-for-use selection and version baking in the version prelude.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Assertions are on the injected DECLARATIONS, so a template that stops parsing or a
    //!   detector that stops matching fails here rather than at link time.

    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses a PHP fixture before version-prelude injection.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Injects the version prelude with a throwaway declaration inventory.
    fn inject_for_test(program: Program, php_version: PhpVersion) -> Program {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        inject_if_used(program, php_version, &mut inventory)
    }

    /// Returns whether an injected program declares a free function.
    fn declares(program: &Program, expected: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::FunctionDecl { name, .. } if name.eq_ignore_ascii_case(expected)
            )
        })
    }

    /// A program that references nothing carries no version-prelude declaration.
    #[test]
    fn unrelated_program_gets_nothing() {
        let program = parse("<?php echo 1;");
        let injected = inject_for_test(program, PhpVersion::Php85);
        assert!(!declares(&injected, "zend_version"));
        assert!(!declares(&injected, "php_sapi_name"));
        assert!(!declares(&injected, "ini_restore"));
    }

    /// Each function is injected independently of the others.
    #[test]
    fn each_function_is_injected_independently() {
        let injected = inject_for_test(parse("<?php echo zend_version();"), PhpVersion::Php85);
        assert!(declares(&injected, "zend_version"));
        assert!(!declares(&injected, "php_sapi_name"));
        assert!(!declares(&injected, "ini_restore"));

        let injected = inject_for_test(parse("<?php ini_restore('x');"), PhpVersion::Php85);
        assert!(declares(&injected, "ini_restore"));
        assert!(!declares(&injected, "zend_version"));
    }

    /// A user declaration of the same name wins; the prelude copy is not injected.
    #[test]
    fn user_declaration_is_not_clobbered() {
        let source = "<?php function ini_restore(string $o): void {} ini_restore('x');";
        let injected = inject_for_test(parse(source), PhpVersion::Php85);
        let count = injected
            .iter()
            .filter(|stmt| {
                matches!(
                    &stmt.kind,
                    StmtKind::FunctionDecl { name, .. } if name.eq_ignore_ascii_case("ini_restore")
                )
            })
            .count();
        assert_eq!(count, 1, "the user declaration must be the only one");
    }

    /// The baked Zend version follows the compile-target profile.
    ///
    /// Asserts on the LITERAL the declaration returns, not on the source text that used to
    /// carry it: the version reaches the program as a `StringLiteral` in the built body, so a
    /// profile that stopped being baked in fails here.
    #[test]
    fn zend_version_body_follows_profile() {
        for (profile, expected) in [
            (PhpVersion::Php82, "4.2.0"),
            (PhpVersion::Php83, "4.3.0"),
            (PhpVersion::Php84, "4.4.0"),
            (PhpVersion::Php85, "4.5.10-dev"),
        ] {
            let injected = inject_for_test(parse("<?php echo zend_version();"), profile);
            assert!(declares(&injected, "zend_version"));

            let baked = injected
                .iter()
                .find_map(|stmt| match &stmt.kind {
                    StmtKind::FunctionDecl { name, body, .. } if name == "zend_version" => {
                        body.first().and_then(|first| match &first.kind {
                            StmtKind::Return(Some(expr)) => match &expr.kind {
                                crate::parser::ast::ExprKind::StringLiteral(value) => {
                                    Some(value.clone())
                                }
                                _ => None,
                            },
                            _ => None,
                        })
                    }
                    _ => None,
                })
                .expect("zend_version must return a literal");
            assert_eq!(baked, expected, "{profile:?} must bake {expected}");
        }
    }

    /// A string-literal reference (the `function_exists` / callable form) still injects.
    #[test]
    fn string_literal_reference_injects() {
        let injected = inject_for_test(
            parse("<?php var_dump(function_exists('php_sapi_name'));"),
            PhpVersion::Php85,
        );
        assert!(declares(&injected, "php_sapi_name"));
    }
}
