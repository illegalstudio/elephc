//! Purpose:
//! Injects the PHP `http_build_query()` standard-library function, built as AST (written in
//! elephc-PHP terms), that renders an array or object as a URL-encoded query string.
//!
//! Called from:
//! - `crate::pipeline::compile()` via `inject_if_used`, before name resolution, so a user
//!   `http_build_query(...)` (or a namespaced caller falling back to the global name)
//!   resolves to the injected function through the normal pipeline.
//!
//! Key details:
//! - A prelude rather than a runtime walker: the recursion over nested arrays and objects,
//!   key bracketing and per-scalar formatting are ordinary PHP control flow over the existing
//!   `urlencode` / `rawurlencode` / `get_object_vars` builtins, so every supported target gets
//!   the same semantics with no new per-target assembly.
//! - PHP 8 semantics: `null` (and resource) values are skipped, `true`/`false` render `1`/`0`,
//!   floats render like `(string)`, nested keys become `name%5Bkey%5D`, `$numeric_prefix` is
//!   prepended RAW to top-level integer keys only, `$arg_separator` defaults to `&`
//!   (`arg_separator.output`'s default), and `PHP_QUERY_RFC3986` selects `rawurlencode`
//!   (every other encoding type uses `urlencode`). Empty nested arrays contribute nothing.
//!   Objects contribute the properties `get_object_vars()` returns from global scope, i.e.
//!   their public ones. A non-array, non-object `$data` raises PHP's TypeError.
//! - Every helper returns `(string)` of its result. A bare string return gives the function an
//!   `Unknown` return-alias summary, and the caller then pins (and leaks) each string argument
//!   it passes; the cast proves the result is a fresh copy.
//! - Pay-for-use: injected only when the program references `http_build_query` (a call, a
//!   callable string, `function_exists`, or an `eval()` fragment) and does not declare its own.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_concat, e_concat_all, e_const, e_int, e_new, e_not,
    e_null, e_str, e_ternary, e_var, function, internal_declarations, s_assign, s_continue,
    s_foreach, s_if, s_return, s_throw, t_mixed, t_nullable,
};

/// The PHP-visible function this prelude declares.
const PUBLIC_NAME: &str = "http_build_query";
/// Percent-encodes one key or value with the selected encoding.
const ENCODE_HELPER: &str = "__elephc_http_build_query_encode";
/// Renders one array level (recursively) as `name=value` pairs.
const PAIRS_HELPER: &str = "__elephc_http_build_query_pairs";
/// Names a rejected `$data` value the way PHP's TypeError does.
const TYPE_HELPER: &str = "__elephc_http_build_query_type";

/// `$a === $b`.
fn strict_eq(left: crate::parser::ast::Expr, right: crate::parser::ast::Expr) -> crate::parser::ast::Expr {
    e_binop(left, BinOp::StrictEq, right)
}

/// `__elephc_http_build_query_encode(string $text, int $encoding): string`.
fn encode_decl() -> Stmt {
    function(ENCODE_HELPER)
        .param("text", TypeExpr::Str)
        .param("encoding", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                strict_eq(e_var("encoding"), e_int(2)),
                vec![s_return(e_cast(
                    CastType::String,
                    e_call("rawurlencode", vec![e_var("text")]),
                ))],
                vec![],
                None,
            ),
            s_return(e_cast(
                CastType::String,
                e_call("urlencode", vec![e_var("text")]),
            )),
        ])
        .build()
}

/// `__elephc_http_build_query_type(mixed $value): string` — PHP's `zend_zval_value_name()`
/// spelling for the scalar values `http_build_query()` rejects.
fn type_decl() -> Stmt {
    let arm = |predicate: &str, name: &str| {
        s_if(
            e_call(predicate, vec![e_var("value")]),
            vec![s_return(e_str(name))],
            vec![],
            None,
        )
    };
    function(TYPE_HELPER)
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            arm("is_string", "string"),
            arm("is_int", "int"),
            arm("is_float", "float"),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![s_return(e_ternary(e_var("value"), e_str("true"), e_str("false")))],
                vec![],
                None,
            ),
            s_if(
                strict_eq(e_var("value"), e_null()),
                vec![s_return(e_str("null"))],
                vec![],
                None,
            ),
            s_return(e_str("resource")),
        ])
        .build()
}

/// `__elephc_http_build_query_pairs(mixed $data, string $prefix, string $numeric_prefix,
/// string $separator, int $encoding): string`.
///
/// `$prefix === ""` marks the top level, where integer keys take the raw numeric prefix and
/// string keys are encoded; nested levels wrap the encoded key in `%5B…%5D`.
fn pairs_decl() -> Stmt {
    let encode = |value| e_call(ENCODE_HELPER, vec![value, e_var("encoding")]);
    let loop_body = vec![
        s_if(
            e_binop(
                strict_eq(e_var("value"), e_null()),
                BinOp::Or,
                e_call("is_resource", vec![e_var("value")]),
            ),
            vec![s_continue(1)],
            vec![],
            None,
        ),
        s_if(
            strict_eq(e_var("prefix"), e_str("")),
            vec![s_if(
                e_call("is_int", vec![e_var("key")]),
                vec![s_assign("name", e_concat(e_var("numeric_prefix"), e_var("key")))],
                vec![],
                Some(vec![s_assign("name", encode(e_var("key")))]),
            )],
            vec![],
            Some(vec![s_assign(
                "name",
                e_concat_all(vec![
                    e_var("prefix"),
                    e_str("%5B"),
                    encode(e_cast(CastType::String, e_var("key"))),
                    e_str("%5D"),
                ]),
            )]),
        ),
        s_if(
            e_call("is_object", vec![e_var("value")]),
            vec![s_assign("value", e_call("get_object_vars", vec![e_var("value")]))],
            vec![],
            None,
        ),
        s_if(
            e_call("is_array", vec![e_var("value")]),
            vec![
                s_assign(
                    "part",
                    e_call(
                        PAIRS_HELPER,
                        vec![
                            e_var("value"),
                            e_var("name"),
                            e_str(""),
                            e_var("separator"),
                            e_var("encoding"),
                        ],
                    ),
                ),
                s_if(
                    strict_eq(e_var("part"), e_str("")),
                    vec![s_continue(1)],
                    vec![],
                    None,
                ),
            ],
            vec![
                (
                    strict_eq(e_var("value"), e_bool(true)),
                    vec![s_assign("part", e_concat(e_var("name"), e_str("=1")))],
                ),
                (
                    strict_eq(e_var("value"), e_bool(false)),
                    vec![s_assign("part", e_concat(e_var("name"), e_str("=0")))],
                ),
            ],
            Some(vec![s_assign(
                "part",
                e_concat_all(vec![
                    e_var("name"),
                    e_str("="),
                    encode(e_cast(CastType::String, e_var("value"))),
                ]),
            )]),
        ),
        s_if(
            strict_eq(e_var("out"), e_str("")),
            vec![s_assign("out", e_var("part"))],
            vec![],
            Some(vec![s_assign(
                "out",
                e_concat_all(vec![e_var("out"), e_var("separator"), e_var("part")]),
            )]),
        ),
    ];
    function(PAIRS_HELPER)
        .param("data", t_mixed())
        .param("prefix", TypeExpr::Str)
        .param("numeric_prefix", TypeExpr::Str)
        .param("separator", TypeExpr::Str)
        .param("encoding", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("out", e_str("")),
            s_foreach(e_var("data"), Some("key"), "value", loop_body),
            s_return(e_cast(CastType::String, e_var("out"))),
        ])
        .build()
}

/// `http_build_query(mixed $data, string $numeric_prefix = "", ?string $arg_separator = null,
/// int $encoding_type = PHP_QUERY_RFC1738): string`.
fn public_decl() -> Stmt {
    function("http_build_query")
        .param("data", t_mixed())
        .param_default("numeric_prefix", TypeExpr::Str, e_str(""))
        .param_default("arg_separator", t_nullable(TypeExpr::Str), e_null())
        .param_default("encoding_type", TypeExpr::Int, e_const("PHP_QUERY_RFC1738"))
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_object", vec![e_var("data")]),
                vec![s_assign("data", e_call("get_object_vars", vec![e_var("data")]))],
                vec![(
                    e_not(e_call("is_array", vec![e_var("data")])),
                    vec![s_throw(e_new(
                        "TypeError",
                        vec![e_concat_all(vec![
                            e_str("http_build_query(): Argument #1 ($data) must be of type array, "),
                            e_call(TYPE_HELPER, vec![e_var("data")]),
                            e_str(" given"),
                        ])],
                    ))],
                )],
                None,
            ),
            s_assign("separator", e_str("&")),
            s_if(
                e_not(strict_eq(e_var("arg_separator"), e_null())),
                vec![s_assign("separator", e_var("arg_separator"))],
                vec![],
                None,
            ),
            s_return(e_cast(
                CastType::String,
                e_call(
                    PAIRS_HELPER,
                    vec![
                        e_var("data"),
                        e_str(""),
                        e_var("numeric_prefix"),
                        e_var("separator"),
                        e_var("encoding_type"),
                    ],
                ),
            )),
        ])
        .build()
}

/// Builds the `http_build_query` prelude: the public function plus its internal helpers.
pub(crate) fn http_build_query_declarations() -> Program {
    internal_declarations(|| vec![encode_decl(), type_decl(), pairs_decl(), public_decl()])
}

/// Prepends the prelude when the program references `http_build_query` and does not
/// declare its own. Returns the program unchanged otherwise, so unrelated binaries pay
/// nothing.
///
/// Injection is hoisted function declarations only, so prepending cannot change top-level
/// execution order. Runs under `crate::compiler_stack::with_compiler_stack` because the usage
/// scan walks the whole program.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        if !crate::opcache_prelude::detect::program_references(&program, PUBLIC_NAME)
            || crate::opcache_prelude::detect::program_declares(&program, PUBLIC_NAME)
        {
            return program;
        }
        let mut combined = http_build_query_declarations();
        inventory.record_program("http_build_query", &combined);
        combined.extend(program);
        combined
    })
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for pay-for-use selection in the `http_build_query` prelude.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - Assertions are on the injected declarations, so a detector that stops matching fails
    //!   here rather than at link time.

    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses a PHP fixture before prelude injection.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Counts the free-function declarations named `expected` in a program.
    fn declarations(program: &Program, expected: &str) -> usize {
        program
            .iter()
            .filter(|stmt| {
                matches!(
                    &stmt.kind,
                    StmtKind::FunctionDecl { name, .. } if name.eq_ignore_ascii_case(expected)
                )
            })
            .count()
    }

    /// Injects with a throwaway declaration inventory.
    fn inject(source: &str) -> Program {
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        inject_if_used(parse(source), &mut inventory)
    }

    /// A program that never mentions the function carries none of the prelude.
    #[test]
    fn unrelated_program_gets_nothing() {
        let injected = inject("<?php echo urlencode('a b');");
        assert_eq!(declarations(&injected, PUBLIC_NAME), 0);
        assert_eq!(declarations(&injected, PAIRS_HELPER), 0);
    }

    /// A call, and the string-literal form `function_exists()` uses, both inject every helper.
    #[test]
    fn call_and_string_references_inject_the_prelude() {
        for source in [
            "<?php echo http_build_query(['a' => 1]);",
            "<?php var_dump(function_exists('http_build_query'));",
        ] {
            let injected = inject(source);
            for name in [PUBLIC_NAME, ENCODE_HELPER, TYPE_HELPER, PAIRS_HELPER] {
                assert_eq!(declarations(&injected, name), 1, "{source}: {name}");
            }
        }
    }

    /// A user declaration of the same name wins; the prelude copy is not injected.
    #[test]
    fn user_declaration_is_not_clobbered() {
        let injected =
            inject("<?php function http_build_query($d) { return 'mine'; } echo http_build_query([]);");
        assert_eq!(declarations(&injected, PUBLIC_NAME), 1);
        assert_eq!(declarations(&injected, PAIRS_HELPER), 0);
    }
}
