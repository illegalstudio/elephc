//! Purpose:
//! PHP-compatible XML attribute-name validation and DOM exceptions.
//!
//! Called from:
//! - `crate::dom_html_prelude::declarations()` prepends the exception and validation
//!   helpers before the HTML walker.
//!
//! Key details:
//! - XML 1.0 Name rules are checked without a regex or native-library dependency.
//! - UTF-8 decoding rejects overlong, surrogate and out-of-range sequences.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `DOMException` — transcribed from the PHP form.
fn decl_class_domexception() -> Stmt {
    class("DOMException")
        .extends("Exception")
        .build()
}

/// `__elephc_dom_escape_namespace` — transcribed from the PHP form.
fn decl_fn_elephc_dom_escape_namespace() -> Stmt {
    function("__elephc_dom_escape_namespace")
        .param("value", TypeExpr::Str)
        .returns(TypeExpr::Str)
        .body(vec![
            s_assign("value", e_call("str_replace", vec![e_str("&"), e_str("&amp;"), e_var("value")])),
            s_assign("value", e_call("str_replace", vec![e_str("<"), e_str("&lt;"), e_var("value")])),
            s_assign("value", e_call("str_replace", vec![e_str(">"), e_str("&gt;"), e_var("value")])),
            s_assign("value", e_call("str_replace", vec![e_str("\""), e_str("&quot;"), e_var("value")])),
            s_assign("value", e_call("str_replace", vec![e_str("\r"), e_str("&#13;"), e_var("value")])),
            s_assign("value", e_call("str_replace", vec![e_str("\n"), e_str("&#10;"), e_var("value")])),
            s_return(e_call("str_replace", vec![e_str("\t"), e_str("&#9;"), e_var("value")])),
        ])
        .build()
}

/// `__elephc_dom_valid_attribute_name` — transcribed from the PHP form.
fn decl_fn_elephc_dom_valid_attribute_name() -> Stmt {
    function("__elephc_dom_valid_attribute_name")
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_assign("length", e_call("strlen", vec![e_var("name")])),
            s_if(
                e_binop(e_var("length"), BinOp::StrictEq, e_int(0)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), None, vec![
                s_assign("lead", e_call("ord", vec![e_call("substr", vec![e_var("name"), e_var("i"), e_int(1)])])),
                s_assign("width", e_int(1)),
                s_assign("code", e_var("lead")),
                s_if(
                    e_binop(e_var("lead"), BinOp::GtEq, e_int(128)),
                    vec![
                        s_if(
                            e_binop(e_binop(e_var("lead"), BinOp::GtEq, e_int(194)), BinOp::And, e_binop(e_var("lead"), BinOp::LtEq, e_int(223))),
                            vec![
                                s_assign("width", e_int(2)),
                                s_assign("code", e_binop(e_var("lead"), BinOp::BitAnd, e_int(31))),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("lead"), BinOp::GtEq, e_int(224)), BinOp::And, e_binop(e_var("lead"), BinOp::LtEq, e_int(239))), vec![
                                s_assign("width", e_int(3)),
                                s_assign("code", e_binop(e_var("lead"), BinOp::BitAnd, e_int(15))),
                            ]),
                            (e_binop(e_binop(e_var("lead"), BinOp::GtEq, e_int(240)), BinOp::And, e_binop(e_var("lead"), BinOp::LtEq, e_int(244))), vec![
                                s_assign("width", e_int(4)),
                                s_assign("code", e_binop(e_var("lead"), BinOp::BitAnd, e_int(7))),
                            ]),
                        ],
                            Some(vec![
                            s_return(e_bool(false)),
                        ]),
                        ),
                        s_if(
                            e_binop(e_binop(e_var("i"), BinOp::Add, e_var("width")), BinOp::Gt, e_var("length")),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_for(Some(s_assign("j", e_int(1))), Some(e_binop(e_var("j"), BinOp::Lt, e_var("width"))), Some(s_expr(e_post_inc("j"))), vec![
                            s_assign("next", e_call("ord", vec![e_call("substr", vec![e_var("name"), e_binop(e_var("i"), BinOp::Add, e_var("j")), e_int(1)])])),
                            s_if(
                                e_binop(e_binop(e_var("next"), BinOp::Lt, e_int(128)), BinOp::Or, e_binop(e_var("next"), BinOp::Gt, e_int(191))),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("code", e_binop(e_binop(e_var("code"), BinOp::ShiftLeft, e_int(6)), BinOp::BitOr, e_binop(e_var("next"), BinOp::BitAnd, e_int(63)))),
                        ]),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("width"), BinOp::StrictEq, e_int(2)), BinOp::And, e_binop(e_var("code"), BinOp::Lt, e_int(128))), BinOp::Or, e_binop(e_binop(e_var("width"), BinOp::StrictEq, e_int(3)), BinOp::And, e_binop(e_var("code"), BinOp::Lt, e_int(2048)))), BinOp::Or, e_binop(e_binop(e_var("width"), BinOp::StrictEq, e_int(4)), BinOp::And, e_binop(e_var("code"), BinOp::Lt, e_int(65536)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(55296)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(57343)))), BinOp::Or, e_binop(e_var("code"), BinOp::Gt, e_int(1114111))),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ],
                    vec![],
                    None,
                ),
                s_assign("start", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::StrictEq, e_int(58)), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(95))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(90)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(97)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(122)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(192)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(214)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(216)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(246)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(248)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(767)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(880)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(893)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(895)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(8191)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(8204)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(8205)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(8304)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(8591)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(11264)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(12271)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(12289)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(55295)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(63744)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(64975)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65008)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(65533)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(65536)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(983039))))),
                s_if(
                    e_binop(e_not(e_var("start")), BinOp::And, e_binop(e_binop(e_var("i"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_not(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::StrictEq, e_int(45)), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(46))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(183))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(48)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(57)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(768)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(879)))), BinOp::Or, e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(8255)), BinOp::And, e_binop(e_var("code"), BinOp::LtEq, e_int(8256))))))),
                    vec![
                        s_return(e_bool(false)),
                    ],
                    vec![],
                    None,
                ),
                s_assign("i", e_binop(e_var("i"), BinOp::Add, e_var("width"))),
            ]),
            s_return(e_bool(true)),
        ])
        .build()
}

/// Builds this module's declarations, one helper per declaration above.
pub(super) fn declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_class_domexception(),
            decl_fn_elephc_dom_escape_namespace(),
            decl_fn_elephc_dom_valid_attribute_name(),
        ]
    })
}
