//! Purpose:
//! The libxml2 2.9.13 HTML4 parser (`__ElephcHtmlParser`): Latin-1 / meta-charset decoding,
//! tokenizer, auto-close and implied html/head/body/p, script CDATA, doctype, comments and PIs,
//! the NOIMPLIED / NODEFDTD / NOBLANKS options, and the fold of its flat node table into the
//! tree arrays `loadHTML` materializes.
//!
//! Called from:
//! - `super::contribute`, which registers the class under `Part::Html`.
//!
//! Key details:
//! - Transcribed from the PHP model with `synthetic_class::transcribe_split`. The model keeps its
//!   nodes in parallel arrays (`kind`, `nm`, `vl`, `at`, `nx`, `pv`, `fc`, `lc`), node 0 being the
//!   document, so building a node never mutates a nested array.
//! - Differential-tested against `php -n` over fuzzed tag soup; the one known gap is documented
//!   in `super`.

#![allow(clippy::all)]

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `__ElephcHtmlParser` — transcribed from the PHP form.
fn decl_class_elephchtmlparser() -> Stmt {
    class("__ElephcHtmlParser")
        .final_()
        .prop("kind", t_array(), Some(e_array(vec![])))
        .prop("nm", t_array(), Some(e_array(vec![])))
        .prop("vl", t_array(), Some(e_array(vec![])))
        .prop("at", t_array(), Some(e_array(vec![])))
        .prop("nx", t_array(), Some(e_array(vec![])))
        .prop("pv", t_array(), Some(e_array(vec![])))
        .prop("fc", t_array(), Some(e_array(vec![])))
        .prop("lc", t_array(), Some(e_array(vec![])))
        .prop("dtdId", TypeExpr::Int, Some(e_neg(e_int(1))))
        .prop("latin1End", TypeExpr::Int, Some(e_int(0)))
        .prop("s", TypeExpr::Str, Some(e_str("")))
        .prop("pos", TypeExpr::Int, Some(e_int(0)))
        .prop("len", TypeExpr::Int, Some(e_int(0)))
        .prop("options", TypeExpr::Int, Some(e_int(0)))
        .prop("names", t_array(), Some(e_array(vec![])))
        .prop("stk", t_array(), Some(e_array(vec![])))
        .prop("nodeTop", TypeExpr::Int, Some(e_int(0)))
        .prop("depth", TypeExpr::Int, Some(e_int(0)))
        .prop("seen", TypeExpr::Int, Some(e_int(0)))
        .prop("closeMap", t_array(), Some(e_array(vec![])))
        .prop("voidTags", t_array(), Some(e_array(vec![])))
        .prop("entities", t_array(), Some(e_array(vec![])))
        .prop("booleanAttrs", t_array(), Some(e_array(vec![])))
        .prop("allowPcdata", t_array(), Some(e_array(vec![])))
        .prop("keepBlanks", TypeExpr::Bool, Some(e_bool(true)))
        .method(
            method("__construct")
                .param("source", TypeExpr::Str)
                .param("options", TypeExpr::Int)
                .body(vec![
                    s_expr(e_method_call(e_this(), "newNode", vec![e_int(0), e_str("#document"), e_str("")])),
                    s_prop_assign(e_this(), "options", e_var("options")),
                    s_prop_assign(e_this(), "keepBlanks", e_binop(e_binop(e_var("options"), BinOp::BitAnd, e_int(256)), BinOp::StrictEq, e_int(0))),
                    s_prop_assign(e_this(), "s", e_self_call("decodeSource", vec![e_var("source")])),
                    s_if(
                        e_binop(e_self_call("hasHighByte", vec![e_var("source")]), BinOp::And, e_not(e_call("str_starts_with", vec![e_var("source"), e_str("<?xml")]))),
                        vec![
                            s_assign("cut", e_self_call("metaUtf8End", vec![e_var("source")])),
                            s_prop_assign(e_this(), "latin1End", e_ternary(e_binop(e_var("cut"), BinOp::Lt, e_int(0)), e_call("strlen", vec![e_this_prop("s")]), e_call("strlen", vec![e_self_call("latin1ToUtf8", vec![e_call("substr", vec![e_var("source"), e_int(0), e_var("cut")])])]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "len", e_call("strlen", vec![e_this_prop("s")])),
                ]),
        )
        .method(
            method("utf8")
                .static_()
                .param("code", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_var("code"), BinOp::Lt, e_int(128)),
                        vec![
                            s_return(e_call("chr", vec![e_var("code")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("code"), BinOp::Lt, e_int(2048)),
                        vec![
                            s_return(e_binop(e_call("chr", vec![e_binop(e_int(192), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("code"), BinOp::Lt, e_int(65536)),
                        vec![
                            s_return(e_binop(e_binop(e_call("chr", vec![e_binop(e_int(224), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(12)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_binop(e_binop(e_call("chr", vec![e_binop(e_int(240), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(18)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(12)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)), BinOp::BitAnd, e_int(63)))])), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))]))),
                ]),
        )
        .method(
            method("decodeSource")
                .static_()
                .param("source", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_not(e_self_call("hasHighByte", vec![e_var("source")])),
                        vec![
                            s_return(e_var("source")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("str_starts_with", vec![e_var("source"), e_str("<?xml")]),
                        vec![
                            s_return(e_var("source")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("cut", e_self_call("metaUtf8End", vec![e_var("source")])),
                    s_if(
                        e_binop(e_var("cut"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_self_call("latin1ToUtf8", vec![e_var("source")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_self_call("latin1ToUtf8", vec![e_call("substr", vec![e_var("source"), e_int(0), e_var("cut")])]), BinOp::Concat, e_call("substr", vec![e_var("source"), e_var("cut")]))),
                ]),
        )
        .method(
            method("hasHighByte")
                .static_()
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_binop(e_call("ord", vec![e_index(e_var("text"), e_var("i"))]), BinOp::GtEq, e_int(128)),
                            vec![
                                s_return(e_bool(true)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("metaUtf8End")
                .static_()
                .param("source", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("lower", e_call("strtolower", vec![e_var("source")])),
                    s_assign("from", e_int(0)),
                    s_while(e_bool(true), vec![
                        s_assign("at", e_call("strpos", vec![e_var("lower"), e_str("<meta"), e_var("from")])),
                        s_if(
                            e_binop(e_var("at"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_return(e_neg(e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("end", e_call("strpos", vec![e_var("lower"), e_str(">"), e_var("at")])),
                        s_if(
                            e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                            vec![
                                s_assign("end", e_call("strlen", vec![e_var("lower")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("next", e_call("substr", vec![e_var("lower"), e_binop(e_var("at"), BinOp::Add, e_int(5)), e_int(1)])),
                        s_assign("tag", e_call("substr", vec![e_var("lower"), e_var("at"), e_binop(e_var("end"), BinOp::Sub, e_var("at"))])),
                        s_assign("charset", e_call("strpos", vec![e_var("tag"), e_str("charset")])),
                        s_if(
                            e_binop(e_binop(e_var("charset"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_binop(e_var("next"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_binop(e_binop(e_binop(e_binop(e_var("next"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("next"), BinOp::LtEq, e_str("9"))), BinOp::Or, e_binop(e_binop(e_var("next"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("next"), BinOp::LtEq, e_str("z")))), BinOp::Or, e_binop(e_var("next"), BinOp::StrictEq, e_str("_")))))),
                            vec![
                                s_assign("rest", e_call("ltrim", vec![e_call("substr", vec![e_var("tag"), e_binop(e_var("charset"), BinOp::Add, e_int(7))]), e_str(" \t\r\n")])),
                                s_if(
                                    e_binop(e_binop(e_var("rest"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_index(e_var("rest"), e_int(0)), BinOp::StrictEq, e_str("="))),
                                    vec![
                                        s_assign("rest", e_call("ltrim", vec![e_call("substr", vec![e_var("rest"), e_int(1)]), e_str(" \t\r\n\"'")])),
                                        s_if(
                                            e_binop(e_call("str_starts_with", vec![e_var("rest"), e_str("utf-8")]), BinOp::Or, e_call("str_starts_with", vec![e_var("rest"), e_str("utf8")])),
                                            vec![
                                                s_return(e_binop(e_var("end"), BinOp::Add, e_int(1))),
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
                            None,
                        ),
                        s_assign("from", e_binop(e_var("at"), BinOp::Add, e_int(5))),
                    ]),
                ]),
        )
        .method(
            method("latin1ToUtf8")
                .static_()
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_str("")),
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_assign("code", e_call("ord", vec![e_index(e_var("text"), e_var("i"))])),
                        s_if(
                            e_binop(e_var("code"), BinOp::Lt, e_int(128)),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_index(e_var("text"), e_var("i")))),
                            ],
                            vec![],
                            Some(vec![
                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_call("chr", vec![e_binop(e_int(192), BinOp::BitOr, e_binop(e_var("code"), BinOp::ShiftRight, e_int(6)))]), BinOp::Concat, e_call("chr", vec![e_binop(e_int(128), BinOp::BitOr, e_binop(e_var("code"), BinOp::BitAnd, e_int(63)))])))),
                        ]),
                        ),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .method(
            method("cur")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_ternary(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("s"), e_this_prop("pos")), e_str(""))),
                ]),
        )
        .method(
            method("nxt")
                .param("n", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_ternary(e_binop(e_binop(e_this_prop("pos"), BinOp::Add, e_var("n")), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_var("n"))), e_str(""))),
                ]),
        )
        .method(
            method("skipBlanks")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\r"))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                ]),
        )
        .method(
            method("isBlankChar")
                .param("c", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(" ")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\t"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\n"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("\r")))),
                ]),
        )
        .method(
            method("isLetter")
                .param("c", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("z"))), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("A")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("Z"))))),
                ]),
        )
        .method(
            method("startsWithNoCase")
                .param("word", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("strtolower", vec![e_call("substr", vec![e_this_prop("s"), e_this_prop("pos"), e_call("strlen", vec![e_var("word")])])]), BinOp::StrictEq, e_var("word"))),
                ]),
        )
        .method(
            method("topName")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("n", e_call("count", vec![e_this_prop("names")])),
                    s_return(e_ternary(e_binop(e_var("n"), BinOp::StrictEq, e_int(0)), e_null(), e_index(e_this_prop("names"), e_binop(e_var("n"), BinOp::Sub, e_int(1))))),
                ]),
        )
        .method(
            method("topId")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_ternary(e_binop(e_this_prop("nodeTop"), BinOp::StrictEq, e_int(0)), e_neg(e_int(1)), e_index(e_this_prop("stk"), e_binop(e_this_prop("nodeTop"), BinOp::Sub, e_int(1))))),
                ]),
        )
        .method(
            method("newNode")
                .param("kind", TypeExpr::Int)
                .param("name", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("id", e_call("count", vec![e_this_prop("kind")])),
                    s_prop_array_push(e_this(), "kind", e_var("kind")),
                    s_prop_array_push(e_this(), "nm", e_var("name")),
                    s_prop_array_push(e_this(), "vl", e_var("value")),
                    s_prop_array_push(e_this(), "at", e_array(vec![])),
                    s_prop_array_push(e_this(), "nx", e_neg(e_int(1))),
                    s_prop_array_push(e_this(), "pv", e_neg(e_int(1))),
                    s_prop_array_push(e_this(), "fc", e_neg(e_int(1))),
                    s_prop_array_push(e_this(), "lc", e_neg(e_int(1))),
                    s_return(e_var("id")),
                ]),
        )
        .method(
            method("appendTo")
                .param("parent", TypeExpr::Int)
                .param("id", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("last", e_index(e_this_prop("lc"), e_var("parent"))),
                    s_prop_array_assign(e_this(), "pv", e_var("id"), e_var("last")),
                    s_if(
                        e_binop(e_var("last"), BinOp::Lt, e_int(0)),
                        vec![
                            s_prop_array_assign(e_this(), "fc", e_var("parent"), e_var("id")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_array_assign(e_this(), "nx", e_var("last"), e_var("id")),
                    ]),
                    ),
                    s_prop_array_assign(e_this(), "lc", e_var("parent"), e_var("id")),
                ]),
        )
        .method(
            method("prependToDocument")
                .param("id", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("first", e_index(e_this_prop("fc"), e_int(0))),
                    s_prop_array_assign(e_this(), "nx", e_var("id"), e_var("first")),
                    s_if(
                        e_binop(e_var("first"), BinOp::Lt, e_int(0)),
                        vec![
                            s_prop_array_assign(e_this(), "lc", e_int(0), e_var("id")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_array_assign(e_this(), "pv", e_var("first"), e_var("id")),
                    ]),
                    ),
                    s_prop_array_assign(e_this(), "fc", e_int(0), e_var("id")),
                ]),
        )
        .method(
            method("parse")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "closeMap", e_static_call("__ElephcHtmlTables", "closeMap", vec![])),
                    s_prop_assign(e_this(), "voidTags", e_static_call("__ElephcHtmlTables", "voidTags", vec![])),
                    s_prop_assign(e_this(), "entities", e_static_call("__ElephcHtmlTables", "entityByName", vec![])),
                    s_prop_assign(e_this(), "booleanAttrs", e_static_call("__ElephcHtmlTables", "booleanAttrs", vec![])),
                    s_prop_assign(e_this(), "allowPcdata", e_static_call("__ElephcHtmlTables", "allowPcdata", vec![])),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_while(e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("-"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(3)]), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("?")))), vec![
                        s_if(
                            e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))),
                            vec![
                                s_expr(e_method_call(e_this(), "parseComment", vec![])),
                            ],
                            vec![],
                            Some(vec![
                            s_expr(e_method_call(e_this(), "parsePI", vec![])),
                        ]),
                        ),
                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    ]),
                    s_if(
                        e_binop(e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))), BinOp::And, e_method_call(e_this(), "startsWithNoCaseAt", vec![e_int(2), e_str("doctype")])),
                        vec![
                            s_expr(e_method_call(e_this(), "parseDoctype", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_while(e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("-"))), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(3)]), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("?")))), vec![
                        s_if(
                            e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))),
                            vec![
                                s_expr(e_method_call(e_this(), "parseComment", vec![])),
                            ],
                            vec![],
                            Some(vec![
                            s_expr(e_method_call(e_this(), "parsePI", vec![])),
                        ]),
                        ),
                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    ]),
                    s_expr(e_method_call(e_this(), "parseContent", vec![])),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_expr(e_method_call(e_this(), "autoCloseOnEnd", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(4)), BinOp::StrictEq, e_int(0)), BinOp::And, e_binop(e_this_prop("dtdId"), BinOp::Lt, e_int(0))),
                        vec![
                            s_assign("dtd", e_method_call(e_this(), "newNode", vec![e_int(6), e_str("html"), e_str("-//W3C//DTD HTML 4.0 Transitional//EN")])),
                            s_prop_array_assign(e_this(), "at", e_var("dtd"), e_array(vec![e_str("http://www.w3.org/TR/REC-html40/loose.dtd")])),
                            s_prop_assign(e_this(), "dtdId", e_var("dtd")),
                            s_expr(e_method_call(e_this(), "prependToDocument", vec![e_var("dtd")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("startsWithNoCaseAt")
                .param("offset", TypeExpr::Int)
                .param("word", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("strtolower", vec![e_call("substr", vec![e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_var("offset")), e_call("strlen", vec![e_var("word")])])]), BinOp::StrictEq, e_var("word"))),
                ]),
        )
        .method(
            method("saxStartElement")
                .param("name", TypeExpr::Str)
                .param("attrs", t_array())
                .param("valueless", t_array())
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("local", e_var("name")),
                    s_assign("colon", e_call("strpos", vec![e_var("name"), e_str(":")])),
                    s_if(
                        e_binop(e_binop(e_var("colon"), BinOp::StrictNotEq, e_bool(false)), BinOp::And, e_binop(e_var("colon"), BinOp::Gt, e_int(0))),
                        vec![
                            s_assign("local", e_call("substr", vec![e_var("name"), e_binop(e_var("colon"), BinOp::Add, e_int(1))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("local", e_ternary(e_binop(e_var("local"), BinOp::StrictEq, e_str("")), e_var("name"), e_var("local"))),
                    s_assign("element", e_method_call(e_this(), "newNode", vec![e_int(1), e_var("local"), e_str("")])),
                    s_assign("pairs", e_array(vec![])),
                    s_foreach(e_var("attrs"), Some("attrName"), "value", vec![
                        s_if(
                            e_call("isset", vec![e_index(e_var("valueless"), e_cast(CastType::String, e_var("attrName")))]),
                            vec![
                                s_array_push("pairs", e_array(vec![e_cast(CastType::String, e_var("attrName")), e_cast(CastType::String, e_var("value")), e_bool(true)])),
                            ],
                            vec![],
                            Some(vec![
                            s_array_push("pairs", e_array(vec![e_cast(CastType::String, e_var("attrName")), e_cast(CastType::String, e_var("value"))])),
                        ]),
                        ),
                    ]),
                    s_prop_array_assign(e_this(), "at", e_var("element"), e_var("pairs")),
                    s_assign("parent", e_method_call(e_this(), "topId", vec![])),
                    s_if(
                        e_binop(e_index(e_this_prop("fc"), e_int(0)), BinOp::Lt, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "appendTo", vec![e_int(0), e_var("element")])),
                        ],
                        vec![],
                        Some(vec![
                        s_if(
                            e_binop(e_var("parent"), BinOp::Lt, e_int(0)),
                            vec![
                                s_assign("parent", e_index(e_this_prop("fc"), e_int(0))),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_index(e_this_prop("kind"), e_var("parent")), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_expr(e_method_call(e_this(), "appendTo", vec![e_var("parent"), e_var("element")])),
                            ],
                            vec![],
                            Some(vec![
                            s_expr(e_method_call(e_this(), "appendTo", vec![e_int(0), e_var("element")])),
                        ]),
                        ),
                    ]),
                    ),
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("stk")]), BinOp::Gt, e_this_prop("nodeTop")),
                        vec![
                            s_prop_array_assign(e_this(), "stk", e_this_prop("nodeTop"), e_var("element")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_array_push(e_this(), "stk", e_var("element")),
                    ]),
                    ),
                    s_prop_assign(e_this(), "nodeTop", e_binop(e_this_prop("nodeTop"), BinOp::Add, e_int(1))),
                ]),
        )
        .method(
            method("saxEndElement")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "nodeTop", e_binop(e_this_prop("nodeTop"), BinOp::Sub, e_int(1))),
                ]),
        )
        .method(
            method("saxText")
                .param("kind", TypeExpr::Int)
                .param("name", TypeExpr::Str)
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("node", e_method_call(e_this(), "topId", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("node"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("text"), BinOp::StrictEq, e_str(""))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("last", e_index(e_this_prop("lc"), e_var("node"))),
                    s_if(
                        e_binop(e_binop(e_var("last"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_index(e_this_prop("kind"), e_var("last")), BinOp::StrictEq, e_var("kind"))),
                        vec![
                            s_prop_array_assign(e_this(), "vl", e_var("last"), e_binop(e_index(e_this_prop("vl"), e_var("last")), BinOp::Concat, e_var("text"))),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "appendTo", vec![e_var("node"), e_method_call(e_this(), "newNode", vec![e_var("kind"), e_var("name"), e_var("text")])])),
                ]),
        )
        .method(
            method("saxCharacters")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "saxText", vec![e_int(2), e_str("#text"), e_var("text")])),
                ]),
        )
        .method(
            method("saxCdata")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "saxText", vec![e_int(3), e_str("#cdata-section"), e_var("text")])),
                ]),
        )
        .method(
            method("saxAddMisc")
                .param("id", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("parent", e_method_call(e_this(), "topId", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("parent"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_index(e_this_prop("kind"), e_var("parent")), BinOp::StrictNotEq, e_int(1))),
                        vec![
                            s_expr(e_method_call(e_this(), "appendTo", vec![e_int(0), e_var("id")])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_this(), "appendTo", vec![e_var("parent"), e_var("id")])),
                    ]),
                    ),
                ]),
        )
        .method(
            method("tree")
                .param("id", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("children", e_array(vec![])),
                    s_assign("child", e_index(e_this_prop("fc"), e_var("id"))),
                    s_while(e_binop(e_var("child"), BinOp::GtEq, e_int(0)), vec![
                        s_array_push("children", e_method_call(e_this(), "tree", vec![e_var("child")])),
                        s_assign("child", e_index(e_this_prop("nx"), e_var("child"))),
                    ]),
                    s_assign("kind", e_index(e_this_prop("kind"), e_var("id"))),
                    s_assign("label", e_str("element")),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_assign("label", e_str("document")),
                        ],
                        vec![
                        (e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)), vec![
                            s_assign("label", e_str("text")),
                        ]),
                        (e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), vec![
                            s_assign("label", e_str("cdata")),
                        ]),
                        (e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), vec![
                            s_assign("label", e_str("comment")),
                        ]),
                        (e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)), vec![
                            s_assign("label", e_str("pi")),
                        ]),
                        (e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)), vec![
                            s_assign("label", e_str("doctype")),
                        ]),
                    ],
                        None,
                    ),
                    s_return(e_call("__elephc_dom_new_node", vec![e_var("label"), e_index(e_this_prop("nm"), e_var("id")), e_index(e_this_prop("vl"), e_var("id")), e_index(e_this_prop("at"), e_var("id")), e_var("children")])),
                ]),
        )
        .method(
            method("namePush")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("seen"), BinOp::Lt, e_int(3)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_prop_assign(e_this(), "seen", e_int(3)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("seen"), BinOp::Lt, e_int(10)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("body"))),
                        vec![
                            s_prop_assign(e_this(), "seen", e_int(10)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_array_push(e_this(), "names", e_var("name")),
                ]),
        )
        .method(
            method("namePop")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_call("array_pop", vec![e_this_prop("names")])),
                ]),
        )
        .method(
            method("checkAutoClose")
                .param("newTag", TypeExpr::Str)
                .param("oldTag", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_call("isset", vec![e_index(e_this_prop("closeMap"), e_binop(e_binop(e_var("oldTag"), BinOp::Concat, e_str(">")), BinOp::Concat, e_var("newTag")))])),
                ]),
        )
        .method(
            method("autoClose")
                .param("newTag", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), BinOp::And, e_method_call(e_this(), "checkAutoClose", vec![e_var("newTag"), e_index(e_this_prop("names"), e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Sub, e_int(1)))])), vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                    ]),
                ]),
        )
        .method(
            method("autoCloseOnEnd")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                    ]),
                ]),
        )
        .method(
            method("autoCloseOnClose")
                .param("newTag", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("priority", e_static_call("__ElephcHtmlTables", "endPriority", vec![e_var("newTag")])),
                    s_assign("i", e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Sub, e_int(1))),
                    s_while(e_binop(e_var("i"), BinOp::GtEq, e_int(0)), vec![
                        s_if(
                            e_binop(e_index(e_this_prop("names"), e_var("i")), BinOp::StrictEq, e_var("newTag")),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_static_call("__ElephcHtmlTables", "endPriority", vec![e_index(e_this_prop("names"), e_var("i"))]), BinOp::Gt, e_var("priority")),
                            vec![
                                s_return_void(),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_post_dec("i")),
                    ]),
                    s_if(
                        e_binop(e_var("i"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_while(e_binop(e_method_call(e_this(), "topName", vec![]), BinOp::StrictNotEq, e_var("newTag")), vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                    ]),
                ]),
        )
        .method(
            method("checkImplied")
                .param("newTag", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("options"), BinOp::BitAnd, e_int(8192)), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("newTag"), BinOp::StrictEq, e_str("html")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::LtEq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("html")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("html"), e_array(vec![]), e_array(vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("newTag"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::LtEq, e_int(1)), BinOp::And, e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("newTag"), BinOp::StrictEq, e_str("script")), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("style"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("meta"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("link"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("title"))), BinOp::Or, e_binop(e_var("newTag"), BinOp::StrictEq, e_str("base")))),
                        vec![
                            s_if(
                                e_binop(e_this_prop("seen"), BinOp::GtEq, e_int(3)),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("head")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("head"), e_array(vec![]), e_array(vec![])])),
                        ],
                        vec![
                        (e_binop(e_binop(e_binop(e_var("newTag"), BinOp::StrictNotEq, e_str("noframes")), BinOp::And, e_binop(e_var("newTag"), BinOp::StrictNotEq, e_str("frame"))), BinOp::And, e_binop(e_var("newTag"), BinOp::StrictNotEq, e_str("frameset"))), vec![
                            s_if(
                                e_binop(e_this_prop("seen"), BinOp::GtEq, e_int(10)),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_foreach(e_this_prop("names"), None, "open", vec![
                                s_if(
                                    e_binop(e_binop(e_var("open"), BinOp::StrictEq, e_str("body")), BinOp::Or, e_binop(e_var("open"), BinOp::StrictEq, e_str("head"))),
                                    vec![
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("body")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("body"), e_array(vec![]), e_array(vec![])])),
                        ]),
                    ],
                        None,
                    ),
                ]),
        )
        .method(
            method("checkParagraph")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("tag", e_method_call(e_this(), "topName", vec![])),
                    s_if(
                        e_binop(e_var("tag"), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "autoClose", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "checkImplied", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("p"), e_array(vec![]), e_array(vec![])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("tag"), BinOp::StrictEq, e_str("html")), BinOp::Or, e_binop(e_var("tag"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_expr(e_method_call(e_this(), "autoClose", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "checkImplied", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "namePush", vec![e_str("p")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_str("p"), e_array(vec![]), e_array(vec![])])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("parseContent")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("currentNode", e_method_call(e_this(), "topName", vec![])),
                    s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                    s_while(e_bool(true), vec![
                        s_assign("c", e_ternary(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), e_index(e_this_prop("s"), e_this_prop("pos")), e_str(""))),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("/"))),
                            vec![
                                s_assign("ended", e_method_call(e_this(), "parseEndTag", vec![])),
                                s_if(
                                    e_binop(e_var("ended"), BinOp::And, e_binop(e_binop(e_var("currentNode"), BinOp::StrictNotEq, e_null()), BinOp::Or, e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::StrictEq, e_int(0)))),
                                    vec![
                                        s_assign("currentNode", e_method_call(e_this(), "topName", vec![])),
                                        s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("top", e_method_call(e_this(), "topName", vec![])),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_var("top"), BinOp::StrictNotEq, e_null())), BinOp::And, e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_method_call(e_this(), "nxt", vec![e_int(1)])]), BinOp::Or, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(":")))),
                            vec![
                                s_assign("save", e_this_prop("pos")),
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                s_assign("peek", e_method_call(e_this(), "parseHtmlName", vec![])),
                                s_prop_assign(e_this(), "pos", e_var("save")),
                                s_if(
                                    e_binop(e_binop(e_var("peek"), BinOp::StrictNotEq, e_null()), BinOp::And, e_method_call(e_this(), "checkAutoClose", vec![e_var("peek"), e_var("top")])),
                                    vec![
                                        s_expr(e_method_call(e_this(), "autoClose", vec![e_var("peek")])),
                                        s_continue(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_var("depth"), BinOp::GtEq, e_call("count", vec![e_this_prop("names")]))), BinOp::And, e_binop(e_var("currentNode"), BinOp::StrictNotEq, e_var("top"))),
                            vec![
                                s_assign("currentNode", e_var("top")),
                                s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_binop(e_var("currentNode"), BinOp::StrictEq, e_str("script")), BinOp::Or, e_binop(e_var("currentNode"), BinOp::StrictEq, e_str("style")))),
                            vec![
                                s_expr(e_method_call(e_this(), "parseScript", vec![])),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("c"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_expr(e_method_call(e_this(), "autoCloseOnEnd", vec![])),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))), BinOp::And, e_method_call(e_this(), "startsWithNoCaseAt", vec![e_int(2), e_str("doctype")])),
                            vec![
                                s_expr(e_method_call(e_this(), "parseDoctype", vec![])),
                                s_assign("c", e_method_call(e_this(), "cur", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("c"), BinOp::StrictEq, e_str("")),
                            vec![
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("!"))),
                            vec![
                                s_if(
                                    e_binop(e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("-")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(3)]), BinOp::StrictEq, e_str("-"))),
                                    vec![
                                        s_expr(e_method_call(e_this(), "parseComment", vec![])),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_expr(e_method_call(e_this(), "skipBogusComment", vec![])),
                                ]),
                                ),
                            ],
                            vec![
                            (e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("?"))), vec![
                                s_expr(e_method_call(e_this(), "parsePI", vec![])),
                            ]),
                            (e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), vec![
                                s_expr(e_method_call(e_this(), "parseElement", vec![])),
                                s_assign("currentNode", e_method_call(e_this(), "topName", vec![])),
                                s_assign("depth", e_call("count", vec![e_this_prop("names")])),
                            ]),
                            (e_binop(e_var("c"), BinOp::StrictEq, e_str("&")), vec![
                                s_expr(e_method_call(e_this(), "parseReference", vec![])),
                            ]),
                        ],
                            Some(vec![
                            s_expr(e_method_call(e_this(), "parseCharData", vec![])),
                        ]),
                        ),
                    ]),
                ]),
        )
        .method(
            method("skipBogusComment")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("parseScript")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_if(
                            e_binop(e_binop(e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictEq, e_str("<")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("/"))), BinOp::And, e_method_call(e_this(), "isLetter", vec![e_method_call(e_this(), "nxt", vec![e_int(2)])])),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_expr(e_method_call(e_this(), "saxCdata", vec![e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])])),
                ]),
        )
        .method(
            method("parseComment")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("end", e_call("strpos", vec![e_this_prop("s"), e_str("-->"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(4))])),
                    s_if(
                        e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_prop_assign(e_this(), "pos", e_this_prop("len")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("comment", e_method_call(e_this(), "newNode", vec![e_int(4), e_str("#comment"), e_call("substr", vec![e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(4)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(4))])])),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(3))),
                    s_expr(e_method_call(e_this(), "saxAddMisc", vec![e_var("comment")])),
                ]),
        )
        .method(
            method("parseName")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("c", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_call("ord", vec![e_var("c")]), BinOp::GtEq, e_int(128))))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9")))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("."))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_call("ord", vec![e_var("c")]), BinOp::GtEq, e_int(128))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                    s_assign("name", e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])),
                    s_if(
                        e_binop(e_binop(e_var("start"), BinOp::Lt, e_this_prop("latin1End")), BinOp::And, e_self_call("hasHighByte", vec![e_var("name")])),
                        vec![
                            s_assign("first", e_int(0)),
                            s_while(e_binop(e_call("ord", vec![e_index(e_var("name"), e_var("first"))]), BinOp::Lt, e_int(128)), vec![
                                s_expr(e_post_inc("first")),
                            ]),
                            s_assign("name", e_call("substr", vec![e_var("name"), e_ternary(e_binop(e_var("first"), BinOp::StrictEq, e_int(0)), e_int(2), e_var("first"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("name")),
                ]),
        )
        .method(
            method("parsePI")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                    s_assign("target", e_method_call(e_this(), "parseName", vec![])),
                    s_if(
                        e_binop(e_var("target"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_var("target")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            s_expr(e_method_call(e_this(), "saxAddMisc", vec![e_method_call(e_this(), "newNode", vec![e_int(5), e_var("target"), e_str("")])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("data", e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_expr(e_method_call(e_this(), "saxAddMisc", vec![e_method_call(e_this(), "newNode", vec![e_int(5), e_var("target"), e_var("data")])])),
                ]),
        )
        .method(
            method("parseLiteral")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("quote", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("quote"), BinOp::StrictNotEq, e_str("\"")), BinOp::And, e_binop(e_var("quote"), BinOp::StrictNotEq, e_str("'"))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("end", e_call("strpos", vec![e_this_prop("s"), e_var("quote"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))])),
                    s_if(
                        e_binop(e_var("end"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_prop_assign(e_this(), "pos", e_this_prop("len")),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("value", e_call("substr", vec![e_this_prop("s"), e_binop(e_this_prop("pos"), BinOp::Add, e_int(1)), e_binop(e_binop(e_var("end"), BinOp::Sub, e_this_prop("pos")), BinOp::Sub, e_int(1))])),
                    s_prop_assign(e_this(), "pos", e_binop(e_var("end"), BinOp::Add, e_int(1))),
                    s_return(e_var("value")),
                ]),
        )
        .method(
            method("parseDoctype")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(9))),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("name", e_method_call(e_this(), "parseName", vec![])),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("public", e_null()),
                    s_assign("system", e_null()),
                    s_if(
                        e_method_call(e_this(), "startsWithNoCase", vec![e_str("system")]),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                            s_assign("system", e_method_call(e_this(), "parseLiteral", vec![])),
                        ],
                        vec![
                        (e_method_call(e_this(), "startsWithNoCase", vec![e_str("public")]), vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(6))),
                            s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                            s_assign("public", e_method_call(e_this(), "parseLiteral", vec![])),
                            s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                            s_if(
                                e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("\"")), BinOp::Or, e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("'"))),
                                vec![
                                    s_assign("system", e_method_call(e_this(), "parseLiteral", vec![])),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                    ],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_this_prop("dtdId"), BinOp::GtEq, e_int(0))),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("dtd", e_method_call(e_this(), "newNode", vec![e_int(6), e_var("name"), e_ternary(e_binop(e_var("public"), BinOp::StrictEq, e_null()), e_str(""), e_var("public"))])),
                    s_prop_array_assign(e_this(), "at", e_var("dtd"), e_array(vec![e_ternary(e_binop(e_var("system"), BinOp::StrictEq, e_null()), e_str(""), e_var("system"))])),
                    s_prop_assign(e_this(), "dtdId", e_var("dtd")),
                    s_expr(e_method_call(e_this(), "prependToDocument", vec![e_var("dtd")])),
                ]),
        )
        .method(
            method("areBlanks")
                .param("text", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("length", e_call("strlen", vec![e_var("text")])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_if(
                            e_not(e_method_call(e_this(), "isBlankChar", vec![e_index(e_var("text"), e_var("i"))])),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("len")),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str("<")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("name", e_method_call(e_this(), "topName", vec![])),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("html"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("body")), BinOp::And, e_binop(e_this_prop("dtdId"), BinOp::GtEq, e_int(0))), BinOp::And, e_binop(e_index(e_this_prop("vl"), e_this_prop("dtdId")), BinOp::StrictNotEq, e_str(""))),
                        vec![
                            s_assign("id", e_call("strtolower", vec![e_index(e_this_prop("vl"), e_this_prop("dtdId"))])),
                            s_if(
                                e_binop(e_binop(e_var("id"), BinOp::StrictEq, e_str("-//w3c//dtd html 4.01//en")), BinOp::Or, e_binop(e_var("id"), BinOp::StrictEq, e_str("-//w3c//dtd html 4//en"))),
                                vec![
                                    s_return(e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("node", e_method_call(e_this(), "topId", vec![])),
                    s_if(
                        e_binop(e_var("node"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("last", e_index(e_this_prop("lc"), e_var("node"))),
                    s_while(e_binop(e_binop(e_var("last"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_index(e_this_prop("kind"), e_var("last")), BinOp::StrictEq, e_int(4))), vec![
                        s_assign("last", e_index(e_this_prop("pv"), e_var("last"))),
                    ]),
                    s_if(
                        e_binop(e_var("last"), BinOp::Lt, e_int(0)),
                        vec![
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("allowPcdata"), e_var("name"))]),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![
                        (e_binop(e_binop(e_index(e_this_prop("kind"), e_var("last")), BinOp::StrictEq, e_int(2)), BinOp::Or, e_binop(e_index(e_this_prop("kind"), e_var("last")), BinOp::StrictEq, e_int(3))), vec![
                            s_return(e_bool(false)),
                        ]),
                        (e_call("isset", vec![e_index(e_this_prop("allowPcdata"), e_index(e_this_prop("nm"), e_var("last")))]), vec![
                            s_return(e_bool(false)),
                        ]),
                    ],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("parseCharData")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("buf", e_str("")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("<")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("&"))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("code", e_call("ord", vec![e_var("c")])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(32)), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(9))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(10))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(13))),
                            vec![
                                s_assign("buf", e_binop(e_var("buf"), BinOp::Concat, e_var("c"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_if(
                        e_binop(e_var("buf"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_method_call(e_this(), "areBlanks", vec![e_var("buf")]),
                        vec![
                            s_if(
                                e_this_prop("keepBlanks"),
                                vec![
                                    s_expr(e_method_call(e_this(), "saxCharacters", vec![e_var("buf")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                        s_expr(e_method_call(e_this(), "saxCharacters", vec![e_var("buf")])),
                    ]),
                    ),
                ]),
        )
        .method(
            method("parseCharRef")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("value", e_int(0)),
                    s_if(
                        e_binop(e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("x")), BinOp::Or, e_binop(e_method_call(e_this(), "nxt", vec![e_int(2)]), BinOp::StrictEq, e_str("X"))),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(3))),
                            s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(";"))), vec![
                                s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                                s_if(
                                    e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9"))),
                                    vec![
                                        s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(16)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(48)))),
                                    ],
                                    vec![
                                    (e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("a")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("f"))), vec![
                                        s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(16)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(87)))),
                                    ]),
                                    (e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("A")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("F"))), vec![
                                        s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(16)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(55)))),
                                    ]),
                                ],
                                    Some(vec![
                                    s_break(1),
                                ]),
                                ),
                                s_if(
                                    e_binop(e_var("value"), BinOp::Gt, e_int(1114112)),
                                    vec![
                                        s_assign("value", e_int(1114112)),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                            s_if(
                                e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(";")),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                        s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(";"))), vec![
                            s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                            s_if(
                                e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9"))),
                                vec![
                                    s_assign("value", e_binop(e_binop(e_var("value"), BinOp::Mul, e_int(10)), BinOp::Add, e_binop(e_call("ord", vec![e_var("c")]), BinOp::Sub, e_int(48)))),
                                ],
                                vec![],
                                Some(vec![
                                s_break(1),
                            ]),
                            ),
                            s_if(
                                e_binop(e_var("value"), BinOp::Gt, e_int(1114112)),
                                vec![
                                    s_assign("value", e_int(1114112)),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ]),
                        s_if(
                            e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(";")),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("value"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("value"), BinOp::GtEq, e_int(1114112))), BinOp::Or, e_binop(e_binop(e_binop(e_binop(e_var("value"), BinOp::Lt, e_int(32)), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_int(9))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_int(10))), BinOp::And, e_binop(e_var("value"), BinOp::StrictNotEq, e_int(13)))), BinOp::Or, e_binop(e_binop(e_var("value"), BinOp::GtEq, e_int(55296)), BinOp::And, e_binop(e_var("value"), BinOp::LtEq, e_int(57343)))), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(65534))), BinOp::Or, e_binop(e_var("value"), BinOp::StrictEq, e_int(65535))),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("value")),
                ]),
        )
        .method(
            method("parseEntityRef")
                .returns(t_array())
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_assign("name", e_method_call(e_this(), "parseName", vec![])),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_array(vec![e_null(), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(";")),
                        vec![
                            s_if(
                                e_call("isset", vec![e_index(e_this_prop("entities"), e_var("name"))]),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                    s_return(e_array(vec![e_var("name"), e_index(e_this_prop("entities"), e_var("name"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_array(vec![e_var("name"), e_null()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array(vec![e_var("name"), e_null()])),
                ]),
        )
        .method(
            method("parseReference")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("#")),
                        vec![
                            s_assign("value", e_method_call(e_this(), "parseCharRef", vec![])),
                            s_if(
                                e_binop(e_var("value"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_self_call("utf8", vec![e_var("value")])])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("ref", e_method_call(e_this(), "parseEntityRef", vec![])),
                    s_if(
                        e_binop(e_index(e_var("ref"), e_int(0)), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_str("&")])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_index(e_var("ref"), e_int(1)), BinOp::StrictEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                            s_expr(e_method_call(e_this(), "saxCharacters", vec![e_binop(e_str("&"), BinOp::Concat, e_index(e_var("ref"), e_int(0)))])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "checkParagraph", vec![])),
                    s_expr(e_method_call(e_this(), "saxCharacters", vec![e_self_call("utf8", vec![e_index(e_var("ref"), e_int(1))])])),
                ]),
        )
        .method(
            method("parseHtmlName")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("c", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("")), BinOp::Or, e_not(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("."))))),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("start", e_this_prop("pos")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_binop(e_method_call(e_this(), "isLetter", vec![e_var("c")]), BinOp::Or, e_binop(e_binop(e_var("c"), BinOp::GtEq, e_str("0")), BinOp::And, e_binop(e_var("c"), BinOp::LtEq, e_str("9")))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str(":"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("-"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("_"))), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("."))),
                            vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ],
                            vec![],
                            Some(vec![
                            s_break(1),
                        ]),
                        ),
                    ]),
                    s_return(e_call("strtolower", vec![e_call("substr", vec![e_this_prop("s"), e_var("start"), e_binop(e_this_prop("pos"), BinOp::Sub, e_var("start"))])])),
                ]),
        )
        .method(
            method("parseEndTag")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                    s_assign("name", e_method_call(e_this(), "parseHtmlName", vec![])),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictNotEq, e_str(">")),
                        vec![
                            s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("depth"), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_binop(e_binop(e_var("name"), BinOp::StrictEq, e_str("html")), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("body"))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_str("head")))),
                        vec![
                            s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Sub, e_int(1))),
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("found", e_bool(false)),
                    s_for(Some(s_assign("i", e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Sub, e_int(1)))), Some(e_binop(e_var("i"), BinOp::GtEq, e_int(0))), Some(s_expr(e_post_dec("i"))), vec![
                        s_if(
                            e_binop(e_index(e_this_prop("names"), e_var("i")), BinOp::StrictEq, e_var("name")),
                            vec![
                                s_assign("found", e_bool(true)),
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_if(
                        e_not(e_var("found")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "autoCloseOnClose", vec![e_var("name")])),
                    s_if(
                        e_binop(e_method_call(e_this(), "topName", vec![]), BinOp::StrictEq, e_var("name")),
                        vec![
                            s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                            s_expr(e_method_call(e_this(), "namePop", vec![])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(false)),
                ]),
        )
        .method(
            method("parseElement")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("failed", e_method_call(e_this(), "parseStartTag", vec![])),
                    s_assign("name", e_method_call(e_this(), "topName", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("failed"), BinOp::StrictEq, e_neg(e_int(1))), BinOp::Or, e_binop(e_var("name"), BinOp::StrictEq, e_null())),
                        vec![
                            s_if(
                                e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("/")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(">"))),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(2))),
                            s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                            s_expr(e_method_call(e_this(), "namePop", vec![])),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str(">")),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                        s_expr(e_method_call(e_this(), "namePop", vec![])),
                        s_return_void(),
                    ]),
                    ),
                    s_if(
                        e_call("isset", vec![e_index(e_this_prop("voidTags"), e_var("name"))]),
                        vec![
                            s_expr(e_method_call(e_this(), "saxEndElement", vec![])),
                            s_expr(e_method_call(e_this(), "namePop", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("parseStartTag")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_assign("name", e_method_call(e_this(), "parseHtmlName", vec![])),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_null()),
                        vec![
                            s_while(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                            s_return(e_neg(e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "autoClose", vec![e_var("name")])),
                    s_expr(e_method_call(e_this(), "checkImplied", vec![e_var("name")])),
                    s_assign("discard", e_bool(false)),
                    s_if(
                        e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::Gt, e_int(0)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("html"))),
                        vec![
                            s_assign("discard", e_bool(true)),
                            s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_call("count", vec![e_this_prop("names")]), BinOp::StrictNotEq, e_int(1)), BinOp::And, e_binop(e_var("name"), BinOp::StrictEq, e_str("head"))),
                        vec![
                            s_assign("discard", e_bool(true)),
                            s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Add, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("name"), BinOp::StrictEq, e_str("body")),
                        vec![
                            s_foreach(e_this_prop("names"), None, "open", vec![
                                s_if(
                                    e_binop(e_var("open"), BinOp::StrictEq, e_str("body")),
                                    vec![
                                        s_assign("discard", e_bool(true)),
                                        s_prop_assign(e_this(), "depth", e_binop(e_this_prop("depth"), BinOp::Add, e_int(1))),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    s_assign("attrs", e_array(vec![])),
                    s_assign("valueless", e_array(vec![])),
                    s_while(e_binop(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), BinOp::And, e_not(e_binop(e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictEq, e_str("/")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(">"))))), vec![
                        s_assign("attrName", e_method_call(e_this(), "parseHtmlName", vec![])),
                        s_if(
                            e_binop(e_var("attrName"), BinOp::StrictNotEq, e_null()),
                            vec![
                                s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                                s_assign("value", e_null()),
                                s_if(
                                    e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_str("=")),
                                    vec![
                                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                                        s_assign("value", e_method_call(e_this(), "parseAttValue", vec![])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_if(
                                    e_not(e_call("isset", vec![e_index(e_var("attrs"), e_var("attrName"))])),
                                    vec![
                                        s_if(
                                            e_binop(e_var("value"), BinOp::StrictEq, e_null()),
                                            vec![
                                                s_if(
                                                    e_call("isset", vec![e_index(e_this_prop("booleanAttrs"), e_var("attrName"))]),
                                                    vec![
                                                        s_array_assign("attrs", e_var("attrName"), e_var("attrName")),
                                                    ],
                                                    vec![],
                                                    Some(vec![
                                                    s_array_assign("attrs", e_var("attrName"), e_str("")),
                                                    s_array_assign("valueless", e_var("attrName"), e_bool(true)),
                                                ]),
                                                ),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_array_assign("attrs", e_var("attrName"), e_var("value")),
                                        ]),
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ],
                            vec![],
                            Some(vec![
                            s_while(e_binop(e_binop(e_binop(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), BinOp::And, e_not(e_method_call(e_this(), "isBlankChar", vec![e_index(e_this_prop("s"), e_this_prop("pos"))]))), BinOp::And, e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictNotEq, e_str(">"))), BinOp::And, e_not(e_binop(e_binop(e_index(e_this_prop("s"), e_this_prop("pos")), BinOp::StrictEq, e_str("/")), BinOp::And, e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str(">"))))), vec![
                                s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            ]),
                        ]),
                        ),
                        s_expr(e_method_call(e_this(), "skipBlanks", vec![])),
                    ]),
                    s_if(
                        e_not(e_var("discard")),
                        vec![
                            s_expr(e_method_call(e_this(), "namePush", vec![e_var("name")])),
                            s_expr(e_method_call(e_this(), "saxStartElement", vec![e_var("name"), e_var("attrs"), e_var("valueless")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_ternary(e_var("discard"), e_int(1), e_int(0))),
                ]),
        )
        .method(
            method("parseAttValue")
                .returns(t_nullable(TypeExpr::Str))
                .body(vec![
                    s_assign("c", e_method_call(e_this(), "cur", vec![])),
                    s_if(
                        e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str("\"")), BinOp::Or, e_binop(e_var("c"), BinOp::StrictEq, e_str("'"))),
                        vec![
                            s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                            s_assign("value", e_method_call(e_this(), "parseAttribute", vec![e_var("c")])),
                            s_if(
                                e_binop(e_method_call(e_this(), "cur", vec![]), BinOp::StrictEq, e_var("c")),
                                vec![
                                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_var("value")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "parseAttribute", vec![e_str("")])),
                ]),
        )
        .method(
            method("parseAttribute")
                .param("stop", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("out", e_str("")),
                    s_while(e_binop(e_this_prop("pos"), BinOp::Lt, e_this_prop("len")), vec![
                        s_assign("c", e_index(e_this_prop("s"), e_this_prop("pos"))),
                        s_if(
                            e_binop(e_binop(e_var("stop"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_var("c"), BinOp::StrictEq, e_var("stop"))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_binop(e_var("stop"), BinOp::StrictEq, e_str("")), BinOp::And, e_binop(e_binop(e_var("c"), BinOp::StrictEq, e_str(">")), BinOp::Or, e_method_call(e_this(), "isBlankChar", vec![e_var("c")]))),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("c"), BinOp::StrictEq, e_str("&")),
                            vec![
                                s_if(
                                    e_binop(e_method_call(e_this(), "nxt", vec![e_int(1)]), BinOp::StrictEq, e_str("#")),
                                    vec![
                                        s_assign("value", e_method_call(e_this(), "parseCharRef", vec![])),
                                        s_if(
                                            e_binop(e_var("value"), BinOp::StrictNotEq, e_int(0)),
                                            vec![
                                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_self_call("utf8", vec![e_var("value")]))),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ],
                                    vec![],
                                    Some(vec![
                                    s_assign("ref", e_method_call(e_this(), "parseEntityRef", vec![])),
                                    s_if(
                                        e_binop(e_index(e_var("ref"), e_int(0)), BinOp::StrictEq, e_null()),
                                        vec![
                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_str("&"))),
                                        ],
                                        vec![
                                        (e_binop(e_index(e_var("ref"), e_int(1)), BinOp::StrictEq, e_null()), vec![
                                            s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_binop(e_str("&"), BinOp::Concat, e_index(e_var("ref"), e_int(0))))),
                                        ]),
                                    ],
                                        Some(vec![
                                        s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_self_call("utf8", vec![e_index(e_var("ref"), e_int(1))]))),
                                    ]),
                                    ),
                                ]),
                                ),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("code", e_call("ord", vec![e_var("c")])),
                        s_if(
                            e_binop(e_binop(e_binop(e_binop(e_var("code"), BinOp::GtEq, e_int(32)), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(9))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(10))), BinOp::Or, e_binop(e_var("code"), BinOp::StrictEq, e_int(13))),
                            vec![
                                s_assign("out", e_binop(e_var("out"), BinOp::Concat, e_var("c"))),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    ]),
                    s_return(e_var("out")),
                ]),
        )
        .build()
}

/// The parser class declaration.
pub(super) fn declaration() -> Stmt {
    decl_class_elephchtmlparser()
}
