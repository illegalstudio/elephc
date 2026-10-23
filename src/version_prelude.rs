//! Purpose:
//! Injects PHP version/environment functions and the shared INI restore wrapper.
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
//!   wrappers around configuration, and a real declaration is what makes
//!   `function_exists('zend_version')` report `true`.
//! - `php_sapi_name()` returns the `PHP_SAPI` CONSTANT rather than a second baked literal,
//!   so the two can never drift; `PHP_SAPI` itself is baked from `web_prelude::sapi_name`
//!   by `codegen_support::prescan::collect_constants`.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_array, e_assign, e_binop, e_bool, e_call, e_cast, e_concat, e_concat_all, e_const,
    e_index, e_int, e_new_fq, e_not, e_null, e_post_inc, e_str, e_this, e_this_prop, e_var, function,
    internal_declarations, method, s_array_push, s_assign, s_expr, s_for, s_foreach, s_if,
    s_prop_array_assign, s_prop_array_push, s_return, s_return_void, s_throw, t_array, t_mixed,
};
use crate::web_prelude::PhpVersion;

/// `zend_version()`: the Zend Engine version string for the compile target.
///
/// Reference PHP 8.5.6 reports `4.5.6`. elephc reports `4.<minor>.0` — see
/// `PhpVersion::zend_version` for why the patch component is `0` and why claiming a Zend
/// Engine version at all is a *language-profile* statement rather than an engine identity.
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

/// Restores shared Core and mbstring directives while leaving immutable or unknown settings unchanged.
fn ini_restore_decl() -> Stmt {
    function("ini_restore")
        .param("option", TypeExpr::Str)
        .returns(TypeExpr::Void)
        .body(vec![crate::shared_ini_prelude::directive(elephc_builtin_contract::mbstring_abi::ini::INI_RESTORE), s_assign(
            "option",
            e_cast(CastType::String, e_var("option")),
        )])
        .build()
}

/// `__elephc_dynamic_constant()`: the runtime lookup behind a non-literal `constant($name)`.
///
/// Looks the name up in the flat global-constant inventory (`get_defined_constants(false)`),
/// strips a leading `\` the way PHP's global lookup does, and raises `\Error` for an unknown
/// name — PHP's own `Undefined constant "X"` behaviour. Class constants (`Foo::BAR`) are not in
/// this inventory and raise the same `\Error`.
fn dynamic_constant_decl() -> Stmt {
    function("__elephc_dynamic_constant")
        .param("name", TypeExpr::Str)
        .returns(t_mixed())
        .body(vec![
            s_assign(
                "name",
                e_call("ltrim", vec![e_var("name"), e_str("\\")]),
            ),
            s_assign("all", e_call("get_defined_constants", vec![e_bool(false)])),
            s_if(
                e_not(e_call(
                    "array_key_exists",
                    vec![e_var("name"), e_var("all")],
                )),
                vec![s_throw(e_new_fq(
                    "Error",
                    vec![e_concat_all(vec![
                        e_str("Undefined constant \""),
                        e_var("name"),
                        e_str("\""),
                    ])],
                ))],
                vec![],
                None,
            ),
            s_return(e_index(e_var("all"), e_var("name"))),
        ])
        .build()
}

/// `random_bytes()`: `$length` cryptographically secure random bytes.
///
/// Composed from `random_int()` (a CSPRNG) one byte at a time via `chr()`, so no new runtime
/// emitter is needed. A non-positive length raises PHP's `ValueError`.
fn random_bytes_decl() -> Stmt {
    function("random_bytes")
        .param("length", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("length"), BinOp::Lt, e_int(1)),
                vec![s_throw(e_new_fq(
                    "ValueError",
                    vec![e_str(
                        "random_bytes(): Argument #1 ($length) must be greater than 0",
                    )],
                ))],
                vec![],
                None,
            ),
            s_assign("bytes", e_str("")),
            s_for(
                Some(s_assign("i", e_int(0))),
                Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))),
                Some(s_expr(e_post_inc("i"))),
                vec![s_assign(
                    "bytes",
                    e_concat(
                        e_var("bytes"),
                        e_call("chr", vec![e_call("random_int", vec![e_int(0), e_int(255)])]),
                    ),
                )],
            ),
            s_return(e_var("bytes")),
        ])
        .build()
}

/// `uniqid()`: PHP's time-based unique id, composed from `gettimeofday()` and `sprintf`.
///
/// php-src formats the current seconds and microseconds as `%08x%05x`, so the 13-hex-digit
/// body is byte-for-byte what php produces. `$more_entropy` appends a `.%08x` derived from
/// the same clock; it exists for uniqueness, not cryptographic quality, and no fixture
/// asserts its exact value.
fn uniqid_decl() -> Stmt {
    function("uniqid")
        .param_default("prefix", TypeExpr::Str, e_str(""))
        .param_default("more_entropy", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("tv", e_call("gettimeofday", vec![])),
            s_if(
                e_var("more_entropy"),
                vec![s_return(e_concat(
                    e_var("prefix"),
                    e_call(
                        "sprintf",
                        vec![
                            e_str("%08x%05x.%08x"),
                            e_index(e_var("tv"), e_str("sec")),
                            e_index(e_var("tv"), e_str("usec")),
                            e_binop(
                                e_index(e_var("tv"), e_str("usec")),
                                BinOp::BitXor,
                                e_index(e_var("tv"), e_str("sec")),
                            ),
                        ],
                    ),
                ))],
                vec![],
                None,
            ),
            s_return(e_concat(
                e_var("prefix"),
                e_call(
                    "sprintf",
                    vec![
                        e_str("%08x%05x"),
                        e_index(e_var("tv"), e_str("sec")),
                        e_index(e_var("tv"), e_str("usec")),
                    ],
                ),
            )),
        ])
        .build()
}

/// `WeakMap`: the SPL object-keyed map, injected like the other version-surface declarations.
///
/// elephc has no weak references, so the map is backed by two parallel arrays — keys and
/// values — and resolves a key by object IDENTITY (`===`). That is exact for the map's own
/// `offsetSet`/`offsetGet`/`offsetExists`/`offsetUnset`/`count` semantics; the one divergence
/// is that a stored key object is held by the array rather than weakly, so it is not collected
/// while it remains a key. A program that never mentions `WeakMap` carries nothing.
fn weakmap_decl() -> Stmt {
    class("WeakMap")
        .implements("ArrayAccess")
        .implements("Countable")
        .private_prop("__elephc_keys", t_array(), Some(e_array(vec![])))
        .private_prop("__elephc_values", t_array(), Some(e_array(vec![])))
        .method(
            method("offsetSet")
                .param("offset", t_mixed())
                .param("value", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_foreach(
                        e_this_prop("__elephc_keys"),
                        Some("__elephc_i"),
                        "__elephc_k",
                        vec![s_if(
                            e_binop(e_var("__elephc_k"), BinOp::StrictEq, e_var("offset")),
                            vec![
                                s_prop_array_assign(
                                    e_this(),
                                    "__elephc_values",
                                    e_var("__elephc_i"),
                                    e_var("value"),
                                ),
                                s_return_void(),
                            ],
                            vec![],
                            None,
                        )],
                    ),
                    s_prop_array_push(e_this(), "__elephc_keys", e_var("offset")),
                    s_prop_array_push(e_this(), "__elephc_values", e_var("value")),
                ]),
        )
        .method(
            method("offsetGet")
                .param("offset", t_mixed())
                .returns(t_mixed())
                .body(vec![
                    s_foreach(
                        e_this_prop("__elephc_keys"),
                        Some("__elephc_i"),
                        "__elephc_k",
                        vec![s_if(
                            e_binop(e_var("__elephc_k"), BinOp::StrictEq, e_var("offset")),
                            vec![s_return(e_index(
                                e_this_prop("__elephc_values"),
                                e_var("__elephc_i"),
                            ))],
                            vec![],
                            None,
                        )],
                    ),
                    s_return(e_null()),
                ]),
        )
        .method(
            method("offsetExists")
                .param("offset", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_foreach(
                        e_this_prop("__elephc_keys"),
                        None,
                        "__elephc_k",
                        vec![s_if(
                            e_binop(e_var("__elephc_k"), BinOp::StrictEq, e_var("offset")),
                            vec![s_return(e_bool(true))],
                            vec![],
                            None,
                        )],
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("offsetUnset")
                .param("offset", t_mixed())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("__elephc_next_keys", e_array(vec![])),
                    s_assign("__elephc_next_values", e_array(vec![])),
                    s_foreach(
                        e_this_prop("__elephc_keys"),
                        Some("__elephc_i"),
                        "__elephc_k",
                        vec![s_if(
                            e_binop(e_var("__elephc_k"), BinOp::StrictNotEq, e_var("offset")),
                            vec![
                                s_array_push("__elephc_next_keys", e_var("__elephc_k")),
                                s_array_push(
                                    "__elephc_next_values",
                                    e_index(e_this_prop("__elephc_values"), e_var("__elephc_i")),
                                ),
                            ],
                            vec![],
                            None,
                        )],
                    ),
                    s_expr(e_assign(
                        e_this_prop("__elephc_keys"),
                        e_var("__elephc_next_keys"),
                    )),
                    s_expr(e_assign(
                        e_this_prop("__elephc_values"),
                        e_var("__elephc_next_values"),
                    )),
                ]),
        )
        .method(
            method("count")
                .returns(TypeExpr::Int)
                .body(vec![s_return(e_call(
                    "count",
                    vec![e_this_prop("__elephc_keys")],
                ))]),
        )
        .build()
}

/// `get_debug_type()`: the diagnostic type name, `null`/`bool`/`int`/`float`/`string`/`array`,
/// a class name, or `resource (<type>)`. Transcribed as a real PHP function (`is_*` dispatch)
/// so it is available everywhere, and injected pay-for-use like the rest of this prelude.
fn get_debug_type_decl() -> Stmt {
    function("get_debug_type")
        .param("value", t_mixed())
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_null", vec![e_var("value")]),
                vec![s_return(e_str("null"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![s_return(e_str("bool"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![s_return(e_str("int"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![s_return(e_str("float"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![s_return(e_str("string"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![s_return(e_str("array"))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![s_return(e_call("get_class", vec![e_var("value")]))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_resource", vec![e_var("value")]),
                vec![s_return(e_concat(
                    e_str("resource ("),
                    e_concat(
                        e_call("get_resource_type", vec![e_var("value")]),
                        e_str(")"),
                    ),
                ))],
                vec![],
                None,
            ),
            s_return(e_str("unknown")),
        ])
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
///
/// Runs under `crate::compiler_stack::with_compiler_stack` (issue #1149): its usage scan walks the
/// whole program, so an embedder calling it without the CLI driver's wrapper gets the same stack
/// budget.
pub fn inject_if_used(
    program: Program,
    php_version: PhpVersion,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| {
        inject_if_used_on_compiler_stack(program, php_version, inventory)
    })
}

/// The body of [`inject_if_used`], run on the stack `with_compiler_stack` sized.
fn inject_if_used_on_compiler_stack(
    program: Program,
    php_version: PhpVersion,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let selected: Vec<&str> = [
        "zend_version",
        "php_sapi_name",
        "ini_restore",
        "uniqid",
        "random_bytes",
        "constant",
        "WeakMap",
        "get_debug_type",
    ]
        .into_iter()
        .filter(|name| {
            crate::opcache_prelude::detect::program_references(&program, name)
                && !crate::opcache_prelude::detect::program_declares(&program, name)
                && !crate::opcache_prelude::detect::program_declares_class(&program, name)
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
        if selected.contains(&"uniqid") {
            declarations.push(uniqid_decl());
        }
        if selected.contains(&"random_bytes") {
            declarations.push(random_bytes_decl());
        }
        if selected.contains(&"constant") {
            declarations.push(dynamic_constant_decl());
        }
        if selected.contains(&"WeakMap") {
            declarations.push(weakmap_decl());
        }
        if selected.contains(&"get_debug_type") {
            declarations.push(get_debug_type_decl());
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

    /// Returns whether an injected program declares a class.
    fn declares_class(program: &Program, expected: &str) -> bool {
        program.iter().any(|stmt| {
            matches!(
                &stmt.kind,
                StmtKind::ClassDecl { name, .. } if name.eq_ignore_ascii_case(expected)
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
            (PhpVersion::Php85, "4.5.0"),
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

    /// `new WeakMap` pulls in the class declaration; an unrelated program carries none, and
    /// a user-declared `WeakMap` class is never clobbered.
    #[test]
    fn weakmap_is_injected_only_when_referenced() {
        let injected = inject_for_test(parse("<?php $m = new WeakMap();"), PhpVersion::Php85);
        assert!(declares_class(&injected, "WeakMap"));

        let injected = inject_for_test(parse("<?php echo 1;"), PhpVersion::Php85);
        assert!(!declares_class(&injected, "WeakMap"));

        let source = "<?php class WeakMap { public $a = 1; } $m = new WeakMap(); echo $m->a;";
        let injected = inject_for_test(parse(source), PhpVersion::Php85);
        let count = injected
            .iter()
            .filter(|stmt| {
                matches!(
                    &stmt.kind,
                    StmtKind::ClassDecl { name, .. } if name.eq_ignore_ascii_case("WeakMap")
                )
            })
            .count();
        assert_eq!(count, 1, "the user declaration must be the only one");
    }

    /// `get_debug_type` is injected only when referenced.
    #[test]
    fn get_debug_type_is_injected_only_when_referenced() {
        let injected = inject_for_test(parse("<?php echo get_debug_type(1);"), PhpVersion::Php85);
        assert!(declares(&injected, "get_debug_type"));

        let injected = inject_for_test(parse("<?php echo 1;"), PhpVersion::Php85);
        assert!(!declares(&injected, "get_debug_type"));
    }
}
