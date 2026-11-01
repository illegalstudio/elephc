//! Purpose:
//! Builds the ext/standard functions elephc has no other implementation of, as AST-built
//! prelude declarations injected when a program names them (`uniqid`, `stripcslashes`,
//! `htmlspecialchars_decode`, `utf8_encode`/`utf8_decode`, `str_increment`/`str_decrement`,
//! `md5_file`/`sha1_file`).
//!
//! Called from:
//! - `crate::backend_gap_prelude::inject_if_used`, which prepends the declarations of every
//!   function in [`FUNCTIONS`] the program references and does not declare itself.
//!
//! Key details:
//! - Each body transcribes the php-src 8.5 algorithm (`ext/standard/html.c` `get_next_char` and
//!   `traverse_for_entities`, `string.c` `php_stripcslashes`, `str_increment`/`str_decrement`,
//!   `uniqid.c`) and was checked against php itself with a differential oracle before being
//!   transcribed (`synthetic_class::transcribe`), never parsed from PHP text at compile time.
//! - Diagnostics go through `__elephc_internal_warning`, whose message prefix selects the php
//!   level: `utf8_*` raise php's internal E_DEPRECATED, a directory read the E_NOTICE php's stream
//!   layer gives, a missing file the E_WARNING.
//! - `uniqid()` keeps the previous stamp in a function static and re-reads the clock until it
//!   changes, as php does; `$more_entropy` appends `%.8F` of a value in [0, 10).

use crate::parser::ast::{BinOp, CastType, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_bool, e_call, e_cast, e_const, e_index, e_int, e_neg, e_new, e_not, e_post_inc, e_str, e_ternary, e_var, function, php_semantics_declarations, s_array_assign, s_assign, s_break, s_continue, s_do_while, s_expr, s_for, s_if, s_return, s_static, s_throw, s_while, t_union,
};

/// The php functions this prelude declares, each with its builder.
pub(crate) const FUNCTIONS: &[(&str, fn() -> Stmt)] = &[
    ("uniqid", decl_fn_uniqid),
    ("stripcslashes", decl_fn_stripcslashes),
    ("htmlspecialchars_decode", decl_fn_htmlspecialchars_decode),
    ("utf8_encode", decl_fn_utf8_encode),
    ("utf8_decode", decl_fn_utf8_decode),
    ("str_increment", decl_fn_str_increment),
    ("str_decrement", decl_fn_str_decrement),
    ("md5_file", decl_fn_md5_file),
    ("sha1_file", decl_fn_sha1_file),
];

/// Returns the declarations of `names` (entries of [`FUNCTIONS`]) plus the helpers they call.
pub(crate) fn declarations(names: &[&str]) -> Program {
    let needs_reader = names.iter().any(|name| *name == "md5_file" || *name == "sha1_file");
    let needs_byte_class = names.iter().any(|name| {
        matches!(*name, "stripcslashes" | "htmlspecialchars_decode" | "str_increment" | "str_decrement")
    });
    php_semantics_declarations(|| {
        let mut declarations: Vec<Stmt> = FUNCTIONS
            .iter()
            .filter(|(name, _)| names.contains(name))
            .map(|(_, build)| build())
            .collect();
        if needs_reader {
            declarations.push(decl_fn_elephc_read_file_for());
        }
        if needs_byte_class {
            declarations.push(decl_fn_elephc_bytes_in());
        }
        declarations
    })
}

/// `uniqid` — transcribed from the PHP form.
fn decl_fn_uniqid() -> Stmt {
    function("uniqid")
        .param_default("prefix", TypeExpr::Str, e_str(""))
        .param_default("more_entropy", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Str)
        .body(vec![
            s_static("previous", e_str("")),
            s_do_while(vec![
                s_assign("now", e_call("explode", vec![e_str(" "), e_call("microtime", vec![])])),
                s_assign("stamp", e_call("sprintf", vec![e_str("%08x%05x"), e_call("intval", vec![e_index(e_var("now"), e_int(1))]), e_call("intval", vec![e_call("substr", vec![e_index(e_var("now"), e_int(0)), e_int(2), e_int(6)])])])),
            ], e_binop(e_var("stamp"), BinOp::StrictEq, e_var("previous"))),
            s_assign("previous", e_var("stamp")),
            s_if(
                e_var("more_entropy"),
                vec![
                    s_return(e_binop(e_binop(e_var("prefix"), BinOp::Concat, e_var("stamp")), BinOp::Concat, e_call("sprintf", vec![e_str("%.8F"), e_binop(e_binop(e_call("mt_rand", vec![e_int(0), e_int(2147483646)]), BinOp::Div, e_int(2147483647)), BinOp::Mul, e_int(10))]))),
                ],
                vec![],
                None,
            ),
            s_return(e_binop(e_var("prefix"), BinOp::Concat, e_var("stamp"))),
        ])
        .build()
}

/// `stripcslashes` — transcribed from the PHP form.
fn decl_fn_stripcslashes() -> Stmt {
    function("stripcslashes")
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("out", e_str("")),
            s_assign("n", e_call("strlen", vec![e_var("string")])),
            s_assign("i", e_int(0)),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("n")), vec![
                s_assign("c", e_index(e_var("string"), e_var("i"))),
                s_if(
                    e_binop(e_binop(e_var("c"), BinOp::StrictNotEq, e_str("\\")), BinOp::Or, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::GtEq, e_var("n"))),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("c"))),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                s_assign("c", e_index(e_var("string"), e_var("i"))),
                s_assign("simple", e_call("strpos", vec![e_str("ntravbf\\"), e_var("c")])),
                s_if(
                    e_binop(e_var("simple"), BinOp::StrictNotEq, e_bool(false)),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_index(e_str("\n\t\r\u{7}\u{b}\u{8}\u{c}\\"), e_var("simple")))),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("x")), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::Add, e_int(1)), BinOp::Lt, e_var("n"))), BinOp::And, e_call("__elephc_bytes_in", vec![e_index(e_var("string"), e_binop(e_var("i"), BinOp::Add, e_int(1))), e_str("0123456789abcdefABCDEF")])),
                    vec![
                        s_assign("digits", e_index(e_var("string"), e_binop(e_var("i"), BinOp::Add, e_int(1)))),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(2))),
                        s_if(
                            e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("n")), BinOp::And, e_call("__elephc_bytes_in", vec![e_index(e_var("string"), e_var("i")), e_str("0123456789abcdefABCDEF")])),
                            vec![
                                s_assign("digits", e_binop(e_var("digits"), BinOp::Concat, e_index(e_var("string"), e_var("i")))),
                                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("chr", vec![e_call("hexdec", vec![e_var("digits")])]))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("digits", e_str("")),
                s_while(e_binop(e_binop(e_binop(e_binop(e_var("i"), BinOp::Lt, e_var("n")), BinOp::And, e_binop(e_call("strlen", vec![e_var("digits")]), BinOp::Lt, e_int(3))), BinOp::And, e_binop(e_index(e_var("string"), e_var("i")), BinOp::GtEq, e_str("0"))), BinOp::And, e_binop(e_index(e_var("string"), e_var("i")), BinOp::LtEq, e_str("7"))), vec![
                    s_assign("digits", e_binop(e_var("digits"), BinOp::Concat, e_index(e_var("string"), e_var("i")))),
                    s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                ]),
                s_if(
                    e_binop(e_var("digits"), BinOp::StrictNotEq, e_str("")),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("chr", vec![e_binop(e_call("octdec", vec![e_var("digits")]), BinOp::BitAnd, e_int(255))]))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("c"))),
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_var("out")),
        ])
        .build()
}

/// `htmlspecialchars_decode` — transcribed from the PHP form.
fn decl_fn_htmlspecialchars_decode() -> Stmt {
    function("htmlspecialchars_decode")
        .param("string", TypeExpr::Str)
        .param_default("flags", TypeExpr::Int, e_binop(e_binop(e_const("ENT_QUOTES"), BinOp::BitOr, e_const("ENT_SUBSTITUTE")), BinOp::BitOr, e_const("ENT_HTML401")))
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("double", e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::StrictNotEq, e_int(0))),
            s_assign("single", e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(1)), BinOp::StrictNotEq, e_int(0))),
            s_assign("apos", e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(48)), BinOp::StrictNotEq, e_int(0))),
            s_assign("out", e_str("")),
            s_assign("n", e_call("strlen", vec![e_var("string")])),
            s_assign("i", e_int(0)),
            s_while(e_binop(e_var("i"), BinOp::Lt, e_var("n")), vec![
                s_assign("amp", e_call("strpos", vec![e_var("string"), e_str("&"), e_var("i")])),
                s_if(
                    e_binop(e_var("amp"), BinOp::StrictEq, e_bool(false)),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("substr", vec![e_var("string"), e_var("i")]))),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("substr", vec![e_var("string"), e_var("i"), e_binop(e_var("amp"), BinOp::Sub, e_var("i"))]))),
                s_assign("i", e_var("amp")),
                s_if(
                    e_binop(e_binop(e_var("n"), BinOp::Sub, e_var("i")), BinOp::Lt, e_int(4)),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("substr", vec![e_var("string"), e_var("i")]))),
                        s_break(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("code", e_neg(e_int(1))),
                s_assign("end", e_neg(e_int(1))),
                s_if(
                    e_binop(e_index(e_var("string"), e_binop(e_var("i"), BinOp::Add, e_int(1))), BinOp::StrictEq, e_str("#")),
                    vec![
                        s_assign("j", e_binop(e_var("i"), BinOp::Add, e_int(2))),
                        s_assign("hex", e_binop(e_binop(e_index(e_var("string"), e_var("j")), BinOp::StrictEq, e_str("x")), BinOp::Or, e_binop(e_index(e_var("string"), e_var("j")), BinOp::StrictEq, e_str("X")))),
                        s_if(
                            e_var("hex"),
                            vec![
                                s_assign("j", e_binop(e_var("j"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("digits", e_str("")),
                        s_while(e_binop(e_binop(e_var("j"), BinOp::Lt, e_var("n")), BinOp::And, e_ternary(e_var("hex"), e_call("__elephc_bytes_in", vec![e_index(e_var("string"), e_var("j")), e_str("0123456789abcdefABCDEF")]), e_call("__elephc_bytes_in", vec![e_index(e_var("string"), e_var("j")), e_str("0123456789")]))), vec![
                            s_assign("digits", e_binop(e_var("digits"), BinOp::Concat, e_index(e_var("string"), e_var("j")))),
                            s_assign("j", e_binop(e_var("j"), BinOp::Add, e_int(1))),
                        ]),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("digits"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_var("j"), BinOp::Lt, e_var("n"))), BinOp::And, e_binop(e_index(e_var("string"), e_var("j")), BinOp::StrictEq, e_str(";"))),
                            vec![
                                s_assign("value", e_ternary(e_var("hex"), e_call("hexdec", vec![e_var("digits")]), e_call("intval", vec![e_var("digits")]))),
                                s_if(
                                    e_binop(e_binop(e_binop(e_binop(e_binop(e_var("value"), BinOp::StrictEq, e_int(38)), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(60))), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(62))), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(34))), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(39))),
                                    vec![
                                        s_assign("code", e_var("value")),
                                        s_assign("end", e_var("j")),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("semi", e_call("strpos", vec![e_var("string"), e_str(";"), e_binop(e_var("i"), BinOp::Add, e_int(1))])),
                    s_if(
                        e_binop(e_var("semi"), BinOp::StrictNotEq, e_bool(false)),
                        vec![
                            s_assign("name", e_call("substr", vec![e_var("string"), e_binop(e_var("i"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("semi"), BinOp::Sub, e_var("i")), BinOp::Sub, e_int(1))])),
                            s_if(
                                e_binop(e_var("name"), BinOp::StrictEq, e_str("amp")),
                                vec![
                                    s_assign("code", e_int(38)),
                                ],
                                vec![
                                (e_binop(e_var("name"), BinOp::StrictEq, e_str("lt")), vec![
                                    s_assign("code", e_int(60)),
                                ]),
                                (e_binop(e_var("name"), BinOp::StrictEq, e_str("gt")), vec![
                                    s_assign("code", e_int(62)),
                                ]),
                                (e_binop(e_var("name"), BinOp::StrictEq, e_str("quot")), vec![
                                    s_assign("code", e_int(34)),
                                ]),
                                (e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("apos")), BinOp::And, e_var("apos")), vec![
                                    s_assign("code", e_int(39)),
                                ]),
                            ],
                                None,
                            ),
                            s_assign("end", e_var("semi")),
                        ],
                        vec![],
                        None,
                    ),
                ]),
                ),
                s_if(
                    e_binop(e_binop(e_binop(e_var("code"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::StrictEq, e_int(39)), BinOp::And, e_not(e_var("single")))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::StrictEq, e_int(34)), BinOp::And, e_not(e_var("double")))),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("&"))),
                        s_assign("i", e_binop(e_var("i"), BinOp::Add, e_int(1))),
                        s_continue(1),
                    ],
                    vec![],
                    None,
                ),
                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("chr", vec![e_var("code")]))),
                s_assign("i", e_binop(e_var("end"), BinOp::Add, e_int(1))),
            ]),
            s_return(e_var("out")),
        ])
        .build()
}

/// `utf8_encode` — transcribed from the PHP form.
fn decl_fn_utf8_encode() -> Stmt {
    function("utf8_encode")
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_expr(e_call("__elephc_internal_warning", vec![e_str("Deprecated: Function utf8_encode() is deprecated since 8.2, visit the php.net documentation for various alternatives\n")])),
            s_assign("out", e_str("")),
            s_assign("n", e_call("strlen", vec![e_var("string")])),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("n"))), Some(s_expr(e_post_inc("i"))), vec![
                s_assign("o", e_call("ord", vec![e_index(e_var("string"), e_var("i"))])),
                s_if(
                    e_binop(e_var("o"), BinOp::Lt, e_int(128)),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_index(e_var("string"), e_var("i")))),
                    ],
                    vec![],
                    Some(vec![
                    s_assign("out", e_binop(e_binop(e_var("out"), BinOp::Concat, e_call("chr", vec![e_binop(e_int(192), BinOp::BitOr, e_binop(e_var("o"), BinOp::ShiftRight, e_int(6)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("o"), BinOp::BitAnd, e_int(63)))]))),
                ]),
                ),
            ]),
            s_return(e_var("out")),
        ])
        .build()
}

/// `utf8_decode` — transcribed from the PHP form.
fn decl_fn_utf8_decode() -> Stmt {
    function("utf8_decode")
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_expr(e_call("__elephc_internal_warning", vec![e_str("Deprecated: Function utf8_decode() is deprecated since 8.2, visit the php.net documentation for various alternatives\n")])),
            s_assign("out", e_str("")),
            s_assign("n", e_call("strlen", vec![e_var("string")])),
            s_assign("pos", e_int(0)),
            s_while(e_binop(e_var("pos"), BinOp::Lt, e_var("n")), vec![
                s_assign("c", e_call("ord", vec![e_index(e_var("string"), e_var("pos"))])),
                s_assign("avail", e_binop(e_var("n"), BinOp::Sub, e_var("pos"))),
                s_assign("b1", e_ternary(e_binop(e_var("avail"), BinOp::Gt, e_int(1)), e_call("ord", vec![e_index(e_var("string"), e_binop(e_var("pos"), BinOp::Add, e_int(1)))]), e_int(0))),
                s_assign("b2", e_ternary(e_binop(e_var("avail"), BinOp::Gt, e_int(2)), e_call("ord", vec![e_index(e_var("string"), e_binop(e_var("pos"), BinOp::Add, e_int(2)))]), e_int(0))),
                s_assign("b3", e_ternary(e_binop(e_var("avail"), BinOp::Gt, e_int(3)), e_call("ord", vec![e_index(e_var("string"), e_binop(e_var("pos"), BinOp::Add, e_int(3)))]), e_int(0))),
                s_assign("t1", e_binop(e_binop(e_var("b1"), BinOp::GtEq, e_int(128)), BinOp::And, e_binop(e_var("b1"), BinOp::LtEq, e_int(191)))),
                s_assign("t2", e_binop(e_binop(e_var("b2"), BinOp::GtEq, e_int(128)), BinOp::And, e_binop(e_var("b2"), BinOp::LtEq, e_int(191)))),
                s_assign("t3", e_binop(e_binop(e_var("b3"), BinOp::GtEq, e_int(128)), BinOp::And, e_binop(e_var("b3"), BinOp::LtEq, e_int(191)))),
                s_assign("l1", e_binop(e_binop(e_var("b1"), BinOp::Lt, e_int(128)), BinOp::Or, e_binop(e_binop(e_var("b1"), BinOp::GtEq, e_int(194)), BinOp::And, e_binop(e_var("b1"), BinOp::LtEq, e_int(244))))),
                s_assign("l2", e_binop(e_binop(e_var("b2"), BinOp::Lt, e_int(128)), BinOp::Or, e_binop(e_binop(e_var("b2"), BinOp::GtEq, e_int(194)), BinOp::And, e_binop(e_var("b2"), BinOp::LtEq, e_int(244))))),
                s_assign("l3", e_binop(e_binop(e_var("b3"), BinOp::Lt, e_int(128)), BinOp::Or, e_binop(e_binop(e_var("b3"), BinOp::GtEq, e_int(194)), BinOp::And, e_binop(e_var("b3"), BinOp::LtEq, e_int(244))))),
                s_if(
                    e_binop(e_var("c"), BinOp::Lt, e_int(128)),
                    vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_call("chr", vec![e_var("c")]))),
                        s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(1))),
                    ],
                    vec![
                    (e_binop(e_var("c"), BinOp::Lt, e_int(194)), vec![
                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                        s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(1))),
                    ]),
                    (e_binop(e_var("c"), BinOp::Lt, e_int(224)), vec![
                        s_if(
                            e_binop(e_var("avail"), BinOp::Lt, e_int(2)),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                                s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![
                            (e_not(e_var("t1")), vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                                s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_ternary(e_var("l1"), e_int(1), e_int(2)))),
                            ]),
                        ],
                            Some(vec![
                            s_assign("code", e_binop(e_binop(e_binop(e_var("c"), BinOp::BitAnd, e_int(31)), BinOp::ShiftLeft, e_int(6)), BinOp::BitOr, e_binop(e_var("b1"), BinOp::BitAnd, e_int(63)))),
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_ternary(e_binop(e_var("code"), BinOp::Gt, e_int(255)), e_str("?"), e_call("chr", vec![e_var("code")])))),
                            s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(2))),
                        ]),
                        ),
                    ]),
                    (e_binop(e_var("c"), BinOp::Lt, e_int(240)), vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_var("avail"), BinOp::Lt, e_int(3)), BinOp::Or, e_not(e_var("t1"))), BinOp::Or, e_not(e_var("t2"))),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                                s_if(
                                    e_binop(e_binop(e_var("avail"), BinOp::Lt, e_int(2)), BinOp::Or, e_var("l1")),
                                    vec![
                                        s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(1))),
                                    ],
                                    vec![
                                    (e_binop(e_binop(e_var("avail"), BinOp::Lt, e_int(3)), BinOp::Or, e_var("l2")), vec![
                                        s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(2))),
                                    ]),
                                ],
                                    Some(vec![
                                    s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(3))),
                                ]),
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                            s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(3))),
                        ]),
                        ),
                    ]),
                    (e_binop(e_var("c"), BinOp::Lt, e_int(245)), vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("avail"), BinOp::Lt, e_int(4)), BinOp::Or, e_not(e_var("t1"))), BinOp::Or, e_not(e_var("t2"))), BinOp::Or, e_not(e_var("t3"))),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                                s_if(
                                    e_binop(e_binop(e_var("avail"), BinOp::Lt, e_int(2)), BinOp::Or, e_var("l1")),
                                    vec![
                                        s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(1))),
                                    ],
                                    vec![
                                    (e_binop(e_binop(e_var("avail"), BinOp::Lt, e_int(3)), BinOp::Or, e_var("l2")), vec![
                                        s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(2))),
                                    ]),
                                    (e_binop(e_binop(e_var("avail"), BinOp::Lt, e_int(4)), BinOp::Or, e_var("l3")), vec![
                                        s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(3))),
                                    ]),
                                ],
                                    Some(vec![
                                    s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(4))),
                                ]),
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                            s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(4))),
                        ]),
                        ),
                    ]),
                ],
                    Some(vec![
                    s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("?"))),
                    s_assign("pos", e_binop(e_var("pos"), BinOp::Add, e_int(1))),
                ]),
                ),
            ]),
            s_return(e_var("out")),
        ])
        .build()
}

/// `str_increment` — transcribed from the PHP form.
fn decl_fn_str_increment() -> Stmt {
    function("str_increment")
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("string"), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("str_increment(): Argument #1 ($string) must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("__elephc_bytes_in", vec![e_var("string"), e_str("0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")])),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("str_increment(): Argument #1 ($string) must be composed only of alphanumeric ASCII characters")])),
                ],
                vec![],
                None,
            ),
            s_assign("i", e_binop(e_call("strlen", vec![e_var("string")]), BinOp::Sub, e_int(1))),
            s_while(e_binop(e_var("i"), BinOp::GtEq, e_int(0)), vec![
                s_assign("c", e_index(e_var("string"), e_var("i"))),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("z")),
                    vec![
                        s_array_assign("string", e_var("i"), e_str("a")),
                    ],
                    vec![
                    (e_binop(e_var("c"), BinOp::StrictEq, e_str("Z")), vec![
                        s_array_assign("string", e_var("i"), e_str("A")),
                    ]),
                    (e_binop(e_var("c"), BinOp::StrictEq, e_str("9")), vec![
                        s_array_assign("string", e_var("i"), e_str("0")),
                    ]),
                ],
                    Some(vec![
                    s_array_assign("string", e_var("i"), e_call("chr", vec![e_binop(e_call("ord", vec![e_var("c")]), BinOp::Add, e_int(1))])),
                    s_return(e_var("string")),
                ]),
                ),
                s_assign("i", e_binop(e_var("i"), BinOp::Sub, e_int(1))),
            ]),
            s_assign("first", e_index(e_var("string"), e_int(0))),
            s_assign("carry", e_ternary(e_binop(e_var("first"), BinOp::StrictEq, e_str("a")), e_str("a"), e_ternary(e_binop(e_var("first"), BinOp::StrictEq, e_str("A")), e_str("A"), e_str("1")))),
            s_return(e_binop(e_var("carry"), BinOp::Concat, e_var("string"))),
        ])
        .build()
}

/// `str_decrement` — transcribed from the PHP form.
fn decl_fn_str_decrement() -> Stmt {
    function("str_decrement")
        .param("string", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_if(
                e_binop(e_var("string"), BinOp::StrictEq, e_str("")),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("str_decrement(): Argument #1 ($string) must not be empty")])),
                ],
                vec![],
                None,
            ),
            s_if(
                e_not(e_call("__elephc_bytes_in", vec![e_var("string"), e_str("0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")])),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("str_decrement(): Argument #1 ($string) must be composed only of alphanumeric ASCII characters")])),
                ],
                vec![],
                None,
            ),
            s_assign("original", e_var("string")),
            s_assign("len", e_call("strlen", vec![e_var("string")])),
            s_assign("i", e_binop(e_var("len"), BinOp::Sub, e_int(1))),
            s_assign("carry", e_bool(true)),
            s_while(e_binop(e_var("carry"), BinOp::And, e_binop(e_var("i"), BinOp::GtEq, e_int(0))), vec![
                s_assign("c", e_index(e_var("string"), e_var("i"))),
                s_if(
                    e_binop(e_var("c"), BinOp::StrictEq, e_str("a")),
                    vec![
                        s_array_assign("string", e_var("i"), e_str("z")),
                    ],
                    vec![
                    (e_binop(e_var("c"), BinOp::StrictEq, e_str("A")), vec![
                        s_array_assign("string", e_var("i"), e_str("Z")),
                    ]),
                    (e_binop(e_var("c"), BinOp::StrictEq, e_str("0")), vec![
                        s_array_assign("string", e_var("i"), e_str("9")),
                    ]),
                ],
                    Some(vec![
                    s_array_assign("string", e_var("i"), e_call("chr", vec![e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(1))])),
                    s_assign("carry", e_bool(false)),
                ]),
                ),
                s_assign("i", e_binop(e_var("i"), BinOp::Sub, e_int(1))),
            ]),
            s_if(
                e_binop(e_var("carry"), BinOp::Or, e_binop(e_binop(e_var("len"), BinOp::Gt, e_int(1)), BinOp::And, e_binop(e_index(e_var("string"), e_int(0)), BinOp::StrictEq, e_str("0")))),
                vec![
                    s_if(
                        e_binop(e_var("len"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_binop(e_str("str_decrement(): Argument #1 ($string) \""), BinOp::Concat, e_var("original")), BinOp::Concat, e_str("\" is out of decrement range"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("substr", vec![e_var("string"), e_int(1)])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("string")),
        ])
        .build()
}

/// `md5_file` — transcribed from the PHP form.
fn decl_fn_md5_file() -> Stmt {
    function("md5_file")
        .param("filename", TypeExpr::Str)
        .param_default("binary", TypeExpr::Bool, e_bool(false))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("status", e_call("__elephc_read_file_for", vec![e_str("md5_file"), e_var("filename")])),
            s_if(
                e_binop(e_var("status"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("status"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_return(e_call("md5", vec![e_str(""), e_var("binary")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("md5", vec![e_cast(CastType::String, e_call("file_get_contents", vec![e_var("filename")])), e_var("binary")])),
        ])
        .build()
}

/// `sha1_file` — transcribed from the PHP form.
fn decl_fn_sha1_file() -> Stmt {
    function("sha1_file")
        .param("filename", TypeExpr::Str)
        .param_default("binary", TypeExpr::Bool, e_bool(false))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
        .body(vec![
            s_assign("status", e_call("__elephc_read_file_for", vec![e_str("sha1_file"), e_var("filename")])),
            s_if(
                e_binop(e_var("status"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_var("status"), BinOp::StrictEq, e_int(2)),
                vec![
                    s_return(e_call("sha1", vec![e_str(""), e_var("binary")])),
                ],
                vec![],
                None,
            ),
            s_return(e_call("sha1", vec![e_cast(CastType::String, e_call("file_get_contents", vec![e_var("filename")])), e_var("binary")])),
        ])
        .build()
}

/// `__elephc_read_file_for` — transcribed from the PHP form.
fn decl_fn_elephc_read_file_for() -> Stmt {
    function("__elephc_read_file_for")
        .param("function", TypeExpr::Str)
        .param("filename", TypeExpr::Str)
        .returns(TypeExpr::Int)
        .body(vec![
            s_if(
                e_call("is_dir", vec![e_var("filename")]),
                vec![
                    s_expr(e_call("__elephc_internal_warning", vec![e_binop(e_binop(e_str("Notice: "), BinOp::Concat, e_var("function")), BinOp::Concat, e_str("(): Read of 8192 bytes failed with errno=21 Is a directory\n"))])),
                    s_return(e_int(2)),
                ],
                vec![],
                None,
            ),
            s_if(
                e_binop(e_not(e_call("is_file", vec![e_var("filename")])), BinOp::Or, e_not(e_call("is_readable", vec![e_var("filename")]))),
                vec![
                    s_assign("reason", e_ternary(e_call("file_exists", vec![e_var("filename")]), e_str("Permission denied"), e_str("No such file or directory"))),
                    s_expr(e_call("__elephc_internal_warning", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("Warning: "), BinOp::Concat, e_var("function")), BinOp::Concat, e_str("(")), BinOp::Concat, e_var("filename")), BinOp::Concat, e_str("): Failed to open stream: ")), BinOp::Concat, e_var("reason")), BinOp::Concat, e_str("\n"))])),
                    s_return(e_int(0)),
                ],
                vec![],
                None,
            ),
            s_return(e_int(1)),
        ])
        .build()
}

/// `__elephc_bytes_in` — transcribed from the PHP form.
fn decl_fn_elephc_bytes_in() -> Stmt {
    function("__elephc_bytes_in")
        .param("bytes", TypeExpr::Str)
        .param("set", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_binop(e_binop(e_var("bytes"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_call("strspn", vec![e_var("bytes"), e_var("set")]), BinOp::StrictEq, e_call("strlen", vec![e_var("bytes")])))),
        ])
        .build()
}
