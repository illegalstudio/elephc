//! Purpose:
//! Injects the PHP `var_export()` standard-library function (written in elephc-PHP)
//! that renders a parsable representation of a scalar or array value, matching the
//! interpreter's layout: `'…'`-quoted strings, `true`/`false`/`NULL` keywords, and
//! the indented `array ( … )` form with `key => value,` entries.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   before name resolution, so a user `var_export(...)` call resolves to the
//!   injected function through the normal pipeline (functions, recursion, arrays,
//!   string builtins) with no dedicated codegen or runtime helper.
//!
//! Key details:
//! - Implemented as a prelude rather than a runtime walker because the recursive,
//!   string-building format reuses ordinary PHP control flow; this keeps it correct
//!   on every supported target with no per-target assembly.
//! - Pay-for-use: injected only when `detect::program_references_var_export` finds a
//!   call or a `"var_export"` string (covering `function_exists`/callable forms), and
//!   never when the program already declares its own `var_export` (so user
//!   definitions win and there is no redeclaration conflict).
//! - Floats render with the interpreter's `serialize_precision = -1` semantics: the
//!   shortest decimal string that round-trips back to the same `double`, formatted
//!   with PHP's decimal/scientific layout (`1.0`, `0.3333333333333333`, `1.0E+17`,
//!   `1.0E-6`). `__elephc_var_export_float` finds the shortest precision by probing
//!   `sprintf("%.{p}e", ...)` until `(float)` of the result equals the input, then
//!   rebuilds the digit string per PHP's exponent thresholds — independent of the
//!   default `(string)`/`echo` precision used elsewhere.
//! - The ext/date object family renders through its php-src serialization shape as
//!   `\Class::__set_state(array(...))`, including nested DatePeriod objects.
//!   Other objects retain the generic visible-property renderer.
//! - The `$return` flag is FLAG-AWARE at the call site, mirroring `print_r`: `name_resolver`
//!   retargets a literal-flag call at [`RENDER_HELPER`] (`: string`) or [`ECHO_HELPER`]
//!   (prints, returns `null`), and only a runtime flag keeps the two-mode `var_export` body
//!   whose `string|null` return type then genuinely describes both outcomes.

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_float, e_index, e_instance_of, e_int, e_method_call,
    e_neg, e_null, e_post_inc, e_str, e_ternary, e_var, function, internal_declarations,
    s_assign, s_break, s_continue, s_echo, s_expr, s_for, s_foreach, s_if, s_return, t_array,
    t_mixed,
};

mod detect;

/// Builds the `var_export` prelude: the public entry point plus the internal helpers that
/// render a value to parsable text, single-quote-escape a string, render a float at
/// `serialize_precision = -1`, and render one object property. The helpers are prefixed so
/// they cannot collide with user code, and `var_export` itself is injected only when the
/// user does not define their own.
///
/// TRANSCRIBED from the PHP this module used to carry as a raw string, and re-transcribed
/// when that PHP grew object and enum-case rendering. Interpolated strings such as
/// `"%.{$p}e"` are CONCATENATIONS here because that is what the lexer turns them into —
/// they have no AST node of their own.
/// `__elephc_var_export_escape` — transcribed from the PHP form.
fn decl_fn_elephc_var_export_escape() -> Stmt {
    function("__elephc_var_export_escape")
        .param("s", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_return(e_call("str_replace", vec![e_str("'"), e_str("\\'"), e_call("str_replace", vec![e_str("\\"), e_str("\\\\"), e_var("s")])])),
        ])
        .build()
}

/// `__elephc_var_export_float` — transcribed from the PHP form.
fn decl_fn_elephc_var_export_float() -> Stmt {
    function("__elephc_var_export_float")
        .param("f", TypeExpr::Float)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_nan", vec![e_var("f")]),
                vec![
                    s_return(e_str("NAN")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_infinite", vec![e_var("f")]),
                vec![
                    s_return(e_ternary(e_binop(e_var("f"), BinOp::Lt, e_int(0)), e_str("-INF"), e_str("INF"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("f"), BinOp::StrictEq, e_float(0.0)),
                vec![
                    s_return(e_ternary(e_binop(e_index(e_cast(CastType::String, e_var("f")), e_int(0)), BinOp::StrictEq, e_str("-")), e_str("-0.0"), e_str("0.0"))),
                ],
                vec![],
                None,
            ),
            s_assign("s", e_str("")),
            s_for(Some(s_assign("p", e_int(0))), Some(e_binop(e_var("p"), BinOp::LtEq, e_int(16))), Some(s_expr(e_post_inc("p"))), vec![
                s_assign("s", e_call("sprintf", vec![e_binop(e_binop(e_str("%."), BinOp::Concat, e_var("p")), BinOp::Concat, e_str("e")), e_var("f")])),
                s_if(
                    e_binop(e_cast(CastType::Float, e_var("s")), BinOp::StrictEq, e_var("f")),
                    vec![
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
            ]),
            s_assign("start", e_ternary(e_binop(e_index(e_var("s"), e_int(0)), BinOp::StrictEq, e_str("-")), e_int(1), e_int(0))),
            s_assign("neg", e_binop(e_var("start"), BinOp::StrictEq, e_int(1))),
            s_assign("epos", e_call("strpos", vec![e_var("s"), e_str("e")])),
            s_assign("exp", e_cast(CastType::Int, e_call("substr", vec![e_var("s"), e_binop(e_var("epos"), BinOp::Add, e_int(1))]))),
            s_assign("digits", e_call("str_replace", vec![e_str("."), e_str(""), e_call("substr", vec![e_var("s"), e_var("start"), e_binop(e_var("epos"), BinOp::Sub, e_var("start"))])])),
            s_assign("ndigits", e_call("strlen", vec![e_var("digits")])),
            s_assign("decpt", e_binop(e_var("exp"), BinOp::Add, e_int(1))),
            s_if(
                e_binop(e_binop(e_var("decpt"), BinOp::Lt, e_neg(e_int(3))), BinOp::Or, e_binop(e_var("decpt"), BinOp::Gt, e_int(17))),
                vec![
                    s_assign("out", e_index(e_var("digits"), e_int(0))),
                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_ternary(e_binop(e_var("ndigits"), BinOp::Gt, e_int(1)), e_binop(e_str("."), BinOp::Concat, e_call("substr", vec![e_var("digits"), e_int(1)])), e_str(".0")))),
                    s_assign("e", e_binop(e_var("decpt"), BinOp::Sub, e_int(1))),
                    s_assign("out", e_binop(e_binop(e_binop(e_var("out"), BinOp::Concat, e_str("E")), BinOp::Concat, e_ternary(e_binop(e_var("e"), BinOp::GtEq, e_int(0)), e_str("+"), e_str("-"))), BinOp::Concat, e_call("abs", vec![e_var("e")]))),
                ],
                vec![],
                Some(vec![
                s_if(
                    e_binop(e_var("decpt"), BinOp::LtEq, e_int(0)),
                    vec![
                        s_assign("out", e_binop(e_binop(e_str("0."), BinOp::Concat, e_call("str_repeat", vec![e_str("0"), e_neg(e_var("decpt"))])), BinOp::Concat, e_var("digits"))),
                    ],
                    vec![],
                    Some(vec![
                    s_if(
                        e_binop(e_var("decpt"), BinOp::GtEq, e_var("ndigits")),
                        vec![
                            s_assign("out", e_binop(e_binop(e_var("digits"), BinOp::Concat, e_call("str_repeat", vec![e_str("0"), e_binop(e_var("decpt"), BinOp::Sub, e_var("ndigits"))])), BinOp::Concat, e_str(".0"))),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("out", e_binop(e_binop(e_call("substr", vec![e_var("digits"), e_int(0), e_var("decpt")]), BinOp::Concat, e_str(".")), BinOp::Concat, e_call("substr", vec![e_var("digits"), e_var("decpt")]))),
                    ]),
                    ),
                ]),
                ),
            ]),
            ),
            s_return(e_binop(e_ternary(e_var("neg"), e_str("-"), e_str("")), BinOp::Concat, e_var("out"))),
        ])
        .build()
}

/// Returns the ext/date object-family test shared by recursive state rendering.
fn date_instance_condition(variable: &str) -> crate::parser::ast::Expr {
    [
        "DateTime",
        "DateTimeImmutable",
        "DateTimeZone",
        "DateInterval",
        "DatePeriod",
    ]
    .into_iter()
    .map(|class_name| e_instance_of(e_var(variable), class_name))
    .reduce(|left, right| e_binop(left, BinOp::Or, right))
    .expect("date instance family is non-empty")
}

/// `__elephc_var_export_date_object` — renders one ext/date `__serialize()` state recursively.
fn decl_fn_elephc_var_export_date_object() -> Stmt {
    function("__elephc_var_export_date_object")
        .param("value", t_mixed())
        .param("state", t_array())
        .param("indent", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_var("indent")])),
            s_assign(
                "out",
                e_binop(
                    e_binop(e_str("\\"), BinOp::Concat, e_call("get_class", vec![e_var("value")])),
                    BinOp::Concat,
                    e_str("::__set_state(array(\n"),
                ),
            ),
            s_foreach(
                e_var("state"),
                Some("k"),
                "v",
                vec![
                    s_if(
                        e_call("is_int", vec![e_var("k")]),
                        vec![s_assign("key", e_cast(CastType::String, e_var("k")))],
                        vec![],
                        Some(vec![s_assign(
                            "key",
                            e_binop(
                                e_binop(
                                    e_str("'"),
                                    BinOp::Concat,
                                    e_call("__elephc_var_export_escape", vec![e_var("k")]),
                                ),
                                BinOp::Concat,
                                e_str("'"),
                            ),
                        )]),
                    ),
                    s_assign(
                        "out",
                        e_binop(
                            e_binop(
                                e_binop(
                                    e_binop(e_var("out"), BinOp::Concat, e_var("pad")),
                                    BinOp::Concat,
                                    e_str("   "),
                                ),
                                BinOp::Concat,
                                e_var("key"),
                            ),
                            BinOp::Concat,
                            e_str(" => "),
                        ),
                    ),
                    s_if(
                        e_binop(
                            e_call("is_array", vec![e_var("v")]),
                            BinOp::Or,
                            date_instance_condition("v"),
                        ),
                        vec![s_assign(
                            "out",
                            e_binop(
                                e_binop(
                                    e_binop(
                                        e_binop(e_var("out"), BinOp::Concat, e_str("\n")),
                                        BinOp::Concat,
                                        e_var("pad"),
                                    ),
                                    BinOp::Concat,
                                    e_str("  "),
                                ),
                                BinOp::Concat,
                                e_call(
                                    "__elephc_var_export_str",
                                    vec![
                                        e_var("v"),
                                        e_binop(e_var("indent"), BinOp::Add, e_int(2)),
                                    ],
                                ),
                            ),
                        )],
                        vec![],
                        Some(vec![s_assign(
                            "out",
                            e_binop(
                                e_var("out"),
                                BinOp::Concat,
                                e_call(
                                    "__elephc_var_export_str",
                                    vec![
                                        e_var("v"),
                                        e_binop(e_var("indent"), BinOp::Add, e_int(2)),
                                    ],
                                ),
                            ),
                        )]),
                    ),
                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str(",\n"))),
                ],
            ),
            s_return(e_binop(
                e_binop(e_var("out"), BinOp::Concat, e_var("pad")),
                BinOp::Concat,
                e_str("))"),
            )),
        ])
        .build()
}

/// `__elephc_var_export_prop` — transcribed from the PHP form.
fn decl_fn_elephc_var_export_prop() -> Stmt {
    function("__elephc_var_export_prop")
        .param("owner", t_mixed())
        .param("index", TypeExpr::Int)
        .param("indent", TypeExpr::Int)
        .param("pad", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("pv", e_call("__elephc_object_prop_value", vec![e_var("owner"), e_var("index")])),
            s_if(
                e_binop(e_call("is_array", vec![e_var("pv")]), BinOp::Or, e_call("is_object", vec![e_var("pv")])),
                vec![
                    s_return(e_binop(e_binop(e_binop(e_str("\n"), BinOp::Concat, e_var("pad")), BinOp::Concat, e_str("  ")), BinOp::Concat, e_call("__elephc_var_export_str", vec![e_var("pv"), e_binop(e_var("indent"), BinOp::Add, e_int(2))]))),
                ],
                vec![],
                None,
            ),
            s_return(e_call("__elephc_var_export_str", vec![e_var("pv"), e_binop(e_var("indent"), BinOp::Add, e_int(2))])),
        ])
        .build()
}

/// `__elephc_var_export_str` — transcribed from the PHP form.
fn decl_fn_elephc_var_export_str() -> Stmt {
    function("__elephc_var_export_str")
        .param("value", t_mixed())
        .param("indent", TypeExpr::Int)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_call("is_int", vec![e_var("value")]),
                vec![
                    s_return(e_cast(CastType::String, e_var("value"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_float", vec![e_var("value")]),
                vec![
                    s_return(e_call("__elephc_var_export_float", vec![e_cast(CastType::Float, e_var("value"))])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_bool", vec![e_var("value")]),
                vec![
                    s_return(e_ternary(e_var("value"), e_str("true"), e_str("false"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_null", vec![e_var("value")]),
                vec![
                    s_return(e_str("NULL")),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_string", vec![e_var("value")]),
                vec![
                    s_assign("text", e_cast(CastType::String, e_var("value"))),
                    s_return(e_binop(e_binop(e_str("'"), BinOp::Concat, e_call("__elephc_var_export_escape", vec![e_var("text")])), BinOp::Concat, e_str("'"))),
                ],
                vec![],
                None,
            ),
            s_if(
                e_call("is_array", vec![e_var("value")]),
                vec![
                    s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_var("indent")])),
                    s_assign("out", e_str("array (\n")),
                    s_foreach(e_var("value"), Some("k"), "v", vec![
                        s_if(
                            e_call("is_int", vec![e_var("k")]),
                            vec![
                                s_assign("key", e_cast(CastType::String, e_var("k"))),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("keytext", e_cast(CastType::String, e_var("k"))),
                            s_assign("key", e_binop(e_binop(e_str("'"), BinOp::Concat, e_call("__elephc_var_export_escape", vec![e_var("keytext")])), BinOp::Concat, e_str("'"))),
                        ]),
                        ),
                        s_assign("out", e_binop(e_binop(e_binop(e_binop(e_var("out"), BinOp::Concat, e_var("pad")), BinOp::Concat, e_str("  ")), BinOp::Concat, e_var("key")), BinOp::Concat, e_str(" => "))),
                        s_if(
                            e_binop(e_call("is_array", vec![e_var("v")]), BinOp::Or, e_call("is_object", vec![e_var("v")])),
                            vec![
                                s_assign("out", e_binop(e_binop(e_binop(e_binop(e_var("out"), BinOp::Concat, e_str("\n")), BinOp::Concat, e_var("pad")), BinOp::Concat, e_str("  ")), BinOp::Concat, e_call("__elephc_var_export_str", vec![e_var("v"), e_binop(e_var("indent"), BinOp::Add, e_int(2))]))),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("__elephc_var_export_str", vec![e_var("v"), e_binop(e_var("indent"), BinOp::Add, e_int(2))]))),
                        ]),
                        ),
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str(",\n"))),
                    ]),
                    s_assign("out", e_binop(e_binop(e_var("out"), BinOp::Concat, e_var("pad")), BinOp::Concat, e_str(")"))),
                    s_return(e_var("out")),
                ],
                vec![],
                None,
            ),
            s_if(
                date_instance_condition("value"),
                vec![s_return(e_call(
                    "__elephc_var_export_date_object",
                    vec![
                        e_var("value"),
                        e_method_call(e_var("value"), "__serialize", vec![]),
                        e_var("indent"),
                    ],
                ))],
                vec![],
                None,
            ),
            s_if(
                e_call("is_object", vec![e_var("value")]),
                vec![
                    s_assign("class", e_call("get_class", vec![e_var("value")])),
                    s_assign("pad", e_call("str_repeat", vec![e_str(" "), e_var("indent")])),
                    s_if(
                        e_call("__elephc_object_is_enum", vec![e_var("value")]),
                        vec![
                            s_assign("cases", e_call("__elephc_object_prop_count", vec![e_var("value")])),
                            s_for(Some(s_assign("c", e_int(0))), Some(e_binop(e_var("c"), BinOp::Lt, e_var("cases"))), Some(s_expr(e_post_inc("c"))), vec![
                                s_if(
                                    e_binop(e_call("__elephc_object_prop_name", vec![e_var("value"), e_var("c")]), BinOp::StrictEq, e_str("name")),
                                    vec![
                                        s_return(e_binop(e_binop(e_binop(e_str("\\"), BinOp::Concat, e_var("class")), BinOp::Concat, e_str("::")), BinOp::Concat, e_call("__elephc_object_prop_value", vec![e_var("value"), e_var("c")]))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_return(e_binop(e_str("\\"), BinOp::Concat, e_var("class"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("class"), BinOp::StrictEq, e_str("stdClass")),
                        vec![
                            s_assign("out", e_str("(object) array(\n")),
                            s_assign("close", e_str(")")),
                        ],
                        vec![],
                        Some(vec![
                        s_assign("out", e_binop(e_binop(e_str("\\"), BinOp::Concat, e_var("class")), BinOp::Concat, e_str("::__set_state(array(\n"))),
                        s_assign("close", e_str("))")),
                    ]),
                    ),
                    s_assign("count", e_call("__elephc_object_prop_count", vec![e_var("value")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("name", e_call("__elephc_object_prop_name", vec![e_var("value"), e_var("i")])),
                        s_if(
                            e_binop(e_var("name"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("out", e_binop(e_binop(e_binop(e_binop(e_binop(e_var("out"), BinOp::Concat, e_var("pad")), BinOp::Concat, e_str("   ")), BinOp::Concat, e_str("'")), BinOp::Concat, e_call("__elephc_var_export_escape", vec![e_var("name")])), BinOp::Concat, e_str("' => "))),
                        s_assign("out", e_binop(e_binop(e_var("out"), BinOp::Concat, e_call("__elephc_var_export_prop", vec![e_var("value"), e_var("i"), e_var("indent"), e_var("pad")])), BinOp::Concat, e_str(",\n"))),
                    ]),
                    s_return(e_binop(e_binop(e_var("out"), BinOp::Concat, e_var("pad")), BinOp::Concat, e_var("close"))),
                ],
                vec![],
                None,
            ),
            s_return(e_str("")),
        ])
        .build()
}

/// `__elephc_var_export_echo` — transcribed from the PHP form.
fn decl_fn_elephc_var_export_echo() -> Stmt {
    function("__elephc_var_export_echo")
        .param("value", t_mixed())
        .body(vec![
            s_echo(e_call("__elephc_var_export_str", vec![e_var("value"), e_int(0)])),
            s_return(e_null()),
        ])
        .build()
}

/// `var_export` — transcribed from the PHP form.
fn decl_fn_var_export() -> Stmt {
    function("var_export")
        .param("value", t_mixed())
        .param_default("return", TypeExpr::Bool, e_bool(false))
        .body(vec![
            s_assign("rendered", e_call("__elephc_var_export_str", vec![e_var("value"), e_int(0)])),
            s_if(
                e_var("return"),
                vec![
                    s_return(e_var("rendered")),
                ],
                vec![],
                None,
            ),
            s_echo(e_var("rendered")),
            s_return(e_null()),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn var_export_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_fn_elephc_var_export_escape(),
            decl_fn_elephc_var_export_float(),
            decl_fn_elephc_var_export_date_object(),
            decl_fn_elephc_var_export_prop(),
            decl_fn_elephc_var_export_str(),
            decl_fn_elephc_var_export_echo(),
            decl_fn_var_export(),
        ]
    })
}

/// Name of the prelude helper that RENDERS a value to its parsable text and returns it.
///
/// Declared `: string`, so `crate::name_resolver` can retarget `var_export($v, true)` at it and
/// get PHP's `string` return type without contradicting what the callee actually returns. Its
/// presence in the resolved symbol table also doubles as the "the elephc prelude owns
/// `var_export`" marker — `inject_if_used` declares it only when it injects.
pub const RENDER_HELPER: &str = "__elephc_var_export_str";

/// Name of the prelude helper that PRINTS a value and returns `null`, the echo-mode contract of
/// `var_export($v)` / `var_export($v, false)` on reference PHP 8.5.6.
///
/// Left unhinted deliberately: elephc spells PHP `null` as `PhpType::Void`, which a lone
/// `return null;` infers exactly, while a `: void` hint would reject the assignment
/// `$r = var_export($v);` that PHP allows.
pub const ECHO_HELPER: &str = "__elephc_var_export_echo";

/// Prepends the `var_export` prelude when the program references `var_export` and does
/// not declare its own, so unrelated binaries pay nothing and a user definition is not
/// clobbered. The prelude is hoisted function declarations only, so prepending does not
/// change top-level execution order.
pub fn inject_if_used(
    program: Program,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !detect::program_references_var_export(&program)
        || detect::program_declares_var_export(&program)
    {
        return program;
    }
    let mut combined = var_export_declarations();
    inventory.record_program("var_export", &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::StmtKind;

    /// The surface is fixed: four helpers, the echo helper, then the entry point. The two
    /// helper NAMES are load-bearing — `name_resolver` retargets a literal-flag
    /// `var_export($v, true)` at `RENDER_HELPER` and `var_export($v)` at `ECHO_HELPER`, and
    /// `RENDER_HELPER`'s presence doubles as the "the prelude owns var_export" marker.
    #[test]
    fn declares_the_helpers_the_name_resolver_retargets() {
        let declared: Vec<String> = var_export_declarations()
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            declared,
            vec![
                "__elephc_var_export_escape",
                "__elephc_var_export_float",
                "__elephc_var_export_date_object",
                // Renders ONE object property. Its own function rather than a loop body so the
                // boxed property value stays a short-lived local instead of a loop-carried one.
                "__elephc_var_export_prop",
                RENDER_HELPER,
                ECHO_HELPER,
                "var_export",
            ]
        );
    }

    /// `RENDER_HELPER` must stay `: string` and `ECHO_HELPER` must stay UNHINTED.
    ///
    /// Neither is cosmetic. `name_resolver` retargets `var_export($v, true)` at the render
    /// helper and takes PHP's `string` return from its hint, so dropping it would contradict
    /// what the callee returns. The echo helper is deliberately unhinted because elephc spells
    /// PHP `null` as `PhpType::Void`, which a lone `return null;` infers exactly, while a
    /// `: void` hint would reject `$r = var_export($v);` — which PHP allows.
    #[test]
    fn the_retarget_helpers_keep_their_declared_returns() {
        let mut render_return = None;
        let mut echo_return = None;
        let mut echo_declared = false;
        for stmt in var_export_declarations() {
            let StmtKind::FunctionDecl {
                name, return_type, ..
            } = &stmt.kind
            else {
                continue;
            };
            if name == RENDER_HELPER {
                render_return = Some(return_type.clone());
            }
            if name == ECHO_HELPER {
                echo_declared = true;
                echo_return = Some(return_type.clone());
            }
        }
        assert_eq!(render_return, Some(Some(TypeExpr::Str)));
        assert!(echo_declared, "the echo helper must be declared");
        assert_eq!(echo_return, Some(None), "the echo helper stays unhinted");
    }

    /// `var_export`'s second parameter defaults to `false`, so `var_export($v)` is echo mode.
    #[test]
    fn the_entry_point_defaults_to_echo_mode() {
        let entry = var_export_declarations()
            .into_iter()
            .find(|stmt| matches!(&stmt.kind, StmtKind::FunctionDecl { name, .. } if name == "var_export"))
            .expect("var_export must be declared");
        let StmtKind::FunctionDecl { params, .. } = &entry.kind else {
            unreachable!("filtered above");
        };
        assert_eq!(params.len(), 2);
        assert_eq!(params[1].0, "return");
        assert_eq!(
            params[1].2.as_ref().map(|expr| &expr.kind),
            Some(&crate::parser::ast::ExprKind::BoolLiteral(false))
        );
    }
}
