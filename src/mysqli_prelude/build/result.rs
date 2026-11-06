//! Purpose:
//! Builds the `mysqli_result` class (and its foreach iterator) as AST. A result
//! OWNS its rows, so a later query can never invalidate an earlier result.
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::result::SRC` (`synthetic_class::transcribe`);
//!   the oracle compares the built class against that PHP for every profile.
//!   `fetch_column` (8.1+) is the one version conditional.

use crate::php_version::PhpVersion;
use crate::parser::ast::{BinOp, CastType, TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    e_array,
    e_array_assoc,
    e_assign,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_const,
    e_dyn_prop,
    e_index,
    e_int,
    e_method_call,
    e_neg,
    e_new,
    e_not,
    e_null,
    e_null_coalesce,
    e_post_inc,
    e_prop,
    e_static_call,
    e_static_prop,
    e_str,
    e_ternary,
    e_this,
    e_this_prop,
    e_var,
    method,
    s_array_assign,
    s_array_push,
    s_assign,
    s_break,
    s_continue,
    s_expr,
    s_for,
    s_if,
    s_prop_array_push,
    s_prop_assign,
    s_return,
    s_return_void,
    s_static_prop_assign,
    s_throw,
    s_while,
    t_array,
    t_class,
    t_mixed,
    t_nullable,
    t_union,
};

/// `mysqli_result` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_result(php_version: PhpVersion) -> Stmt {
    class("mysqli_result")
        .implements("IteratorAggregate")
        .prop("current_field", TypeExpr::Int, Some(e_int(0)))
        .prop("field_count", TypeExpr::Int, Some(e_int(0)))
        .prop("lengths", t_nullable(t_array()), Some(e_null()))
        .prop("num_rows", t_union(vec![TypeExpr::Int, TypeExpr::Str]), Some(e_int(0)))
        .prop("type", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("fromFactory", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("cells", t_array(), Some(e_array(vec![])))
        .private_prop("names", t_array(), Some(e_array(vec![])))
        .private_prop("metaTables", t_array(), Some(e_array(vec![])))
        .private_prop("metaNativeTypes", t_array(), Some(e_array(vec![])))
        .private_prop("metaFlags", t_array(), Some(e_array(vec![])))
        .private_prop("metaLens", t_array(), Some(e_array(vec![])))
        .private_prop("metaDecimals", t_array(), Some(e_array(vec![])))
        .private_prop("rowCount", TypeExpr::Int, Some(e_int(0)))
        .private_prop("pos", TypeExpr::Int, Some(e_int(0)))
        .private_prop("streamStmt", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("streamStep", TypeExpr::Int, Some(e_int(0)))
        .private_prop("streamLink", t_nullable(t_class("mysqli")), Some(e_null()))
        .private_prop("streamKinds", t_array(), Some(e_array(vec![])))
        .private_prop("streamRows", TypeExpr::Int, Some(e_int(0)))
        .private_prop("streamStale", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("streamReportLink", t_nullable(t_class("mysqli")), Some(e_null()))
        .private_prop("statId", TypeExpr::Int, Some(e_neg(e_int(1))))
        .private_prop("statBinary", TypeExpr::Bool, Some(e_bool(false)))
        .private_prop("statKeys", t_array(), Some(e_array(vec![])))
        .private_prop("statFreed", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .param("mysql", t_class("mysqli"))
                .param_default("result_mode", TypeExpr::Int, e_int(0))
                .body(vec![
                    s_if(
                        e_static_prop("mysqli_result", "fromFactory"),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_method_call(e_var("mysql"), "__elephcConn", vec![]), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli object is not fully initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("result_mode"), BinOp::NotEq, e_int(0)), BinOp::And, e_binop(e_var("result_mode"), BinOp::NotEq, e_int(1))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::__construct(): Argument #2 ($result_mode) must be either MYSQLI_STORE_RESULT or MYSQLI_USE_RESULT")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_source", e_ternary(e_binop(e_var("result_mode"), BinOp::Eq, e_int(1)), e_method_call(e_var("mysql"), "use_result", vec![]), e_method_call(e_var("mysql"), "store_result", vec![]))),
                    s_if(
                        e_binop(e_var("_source"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_var("_source"), "__elephcCopyInto", vec![e_this(), e_var("result_mode")])),
                ]),
        )
        .method(
            method("__elephcFromDrain")
                .private()
                .static_()
                .param("cells", t_array())
                .param("rowCount", TypeExpr::Int)
                .param("names", t_array())
                .param("tables", t_array())
                .param("nativeTypes", t_array())
                .param("flags", t_array())
                .param("lens", t_array())
                .param("decimals", t_array())
                .returns(t_class("mysqli_result"))
                .body(vec![
                    s_static_prop_assign("mysqli_result", "fromFactory", e_bool(true)),
                    s_assign("_placeholder", e_new("mysqli", vec![])),
                    s_expr(e_method_call(e_var("_placeholder"), "__elephcDetachHandle", vec![])),
                    s_assign("_result", e_new("mysqli_result", vec![e_var("_placeholder")])),
                    s_static_prop_assign("mysqli_result", "fromFactory", e_bool(false)),
                    s_prop_assign(e_var("_result"), "cells", e_var("cells")),
                    s_prop_assign(e_var("_result"), "rowCount", e_var("rowCount")),
                    s_prop_assign(e_var("_result"), "names", e_var("names")),
                    s_prop_assign(e_var("_result"), "metaTables", e_var("tables")),
                    s_prop_assign(e_var("_result"), "metaNativeTypes", e_var("nativeTypes")),
                    s_prop_assign(e_var("_result"), "metaFlags", e_var("flags")),
                    s_prop_assign(e_var("_result"), "metaLens", e_var("lens")),
                    s_prop_assign(e_var("_result"), "metaDecimals", e_var("decimals")),
                    s_prop_assign(e_var("_result"), "num_rows", e_var("rowCount")),
                    s_prop_assign(e_var("_result"), "field_count", e_call("count", vec![e_var("names")])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("__elephcFromStream")
                .private()
                .static_()
                .param("link", t_class("mysqli"))
                .param("stmt", TypeExpr::Int)
                .param("firstStep", TypeExpr::Int)
                .param("names", t_array())
                .param("tables", t_array())
                .param("nativeTypes", t_array())
                .param("flags", t_array())
                .param("lens", t_array())
                .param("decimals", t_array())
                .param("kinds", t_array())
                .returns(t_class("mysqli_result"))
                .body(vec![
                    s_static_prop_assign("mysqli_result", "fromFactory", e_bool(true)),
                    s_assign("_placeholder", e_new("mysqli", vec![])),
                    s_expr(e_method_call(e_var("_placeholder"), "__elephcDetachHandle", vec![])),
                    s_assign("_result", e_new("mysqli_result", vec![e_var("_placeholder")])),
                    s_static_prop_assign("mysqli_result", "fromFactory", e_bool(false)),
                    s_prop_assign(e_var("_result"), "names", e_var("names")),
                    s_prop_assign(e_var("_result"), "metaTables", e_var("tables")),
                    s_prop_assign(e_var("_result"), "metaNativeTypes", e_var("nativeTypes")),
                    s_prop_assign(e_var("_result"), "metaFlags", e_var("flags")),
                    s_prop_assign(e_var("_result"), "metaLens", e_var("lens")),
                    s_prop_assign(e_var("_result"), "metaDecimals", e_var("decimals")),
                    s_prop_assign(e_var("_result"), "field_count", e_call("count", vec![e_var("names")])),
                    s_prop_assign(e_var("_result"), "type", e_int(1)),
                    s_prop_assign(e_var("_result"), "num_rows", e_int(0)),
                    s_prop_assign(e_var("_result"), "streamStmt", e_var("stmt")),
                    s_prop_assign(e_var("_result"), "streamStep", e_var("firstStep")),
                    s_prop_assign(e_var("_result"), "streamLink", e_var("link")),
                    s_prop_assign(e_var("_result"), "streamKinds", e_var("kinds")),
                    s_prop_assign(e_var("_result"), "statId", e_method_call(e_var("link"), "__elephcStatId", vec![])),
                    s_return(e_var("_result")),
                ]),
        )
        .method(
            method("__elephcCopyInto")
                .private()
                .param("target", t_class("mysqli_result"))
                .param("mode", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_var("target"), "cells", e_this_prop("cells")),
                    s_prop_assign(e_var("target"), "names", e_this_prop("names")),
                    s_prop_assign(e_var("target"), "metaTables", e_this_prop("metaTables")),
                    s_prop_assign(e_var("target"), "metaNativeTypes", e_this_prop("metaNativeTypes")),
                    s_prop_assign(e_var("target"), "metaFlags", e_this_prop("metaFlags")),
                    s_prop_assign(e_var("target"), "metaLens", e_this_prop("metaLens")),
                    s_prop_assign(e_var("target"), "metaDecimals", e_this_prop("metaDecimals")),
                    s_prop_assign(e_var("target"), "rowCount", e_this_prop("rowCount")),
                    s_prop_assign(e_var("target"), "field_count", e_this_prop("field_count")),
                    s_prop_assign(e_var("target"), "type", e_var("mode")),
                    s_prop_assign(e_var("target"), "num_rows", e_ternary(e_binop(e_var("mode"), BinOp::Eq, e_int(1)), e_int(0), e_this_prop("rowCount"))),
                    s_prop_assign(e_var("target"), "streamStmt", e_this_prop("streamStmt")),
                    s_prop_assign(e_var("target"), "streamStep", e_this_prop("streamStep")),
                    s_prop_assign(e_var("target"), "streamLink", e_this_prop("streamLink")),
                    s_prop_assign(e_var("target"), "streamKinds", e_this_prop("streamKinds")),
                    s_prop_assign(e_var("target"), "streamRows", e_this_prop("streamRows")),
                    s_prop_assign(e_var("target"), "streamStale", e_this_prop("streamStale")),
                    s_prop_assign(e_var("target"), "statId", e_this_prop("statId")),
                    s_prop_assign(e_var("target"), "statBinary", e_this_prop("statBinary")),
                    s_prop_assign(e_var("target"), "statKeys", e_this_prop("statKeys")),
                    s_prop_assign(e_var("target"), "statFreed", e_this_prop("statFreed")),
                    s_prop_assign(e_this(), "streamStmt", e_neg(e_int(1))),
                    s_prop_assign(e_this(), "streamLink", e_null()),
                    s_prop_assign(e_this(), "statId", e_neg(e_int(1))),
                ]),
        )
        .method(
            method("__elephcCellKind")
                .private()
                .static_()
                .param("native", TypeExpr::Str)
                .param("flags", TypeExpr::Int)
                .param("intFloatNative", TypeExpr::Bool)
                .param("binary", TypeExpr::Bool)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("BIT")),
                        vec![
                            s_return(e_ternary(e_binop(e_var("binary"), BinOp::Or, e_var("intFloatNative")), e_int(3), e_int(4))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_integer", e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("native"), BinOp::StrictEq, e_str("TINY")), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("SHORT"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("LONG"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("LONGLONG"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("INT24"))), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("YEAR")))),
                    s_if(
                        e_binop(e_var("_integer"), BinOp::And, e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(64)), BinOp::NotEq, e_int(0))),
                        vec![
                            s_return(e_ternary(e_var("binary"), e_int(5), e_int(0))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("binary"), BinOp::Or, e_not(e_var("intFloatNative"))),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_var("_integer"),
                        vec![
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("native"), BinOp::StrictEq, e_str("FLOAT")), BinOp::Or, e_binop(e_var("native"), BinOp::StrictEq, e_str("DOUBLE"))),
                        vec![
                            s_return(e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("__elephcConvertCell")
                .private()
                .static_()
                .param("value", t_mixed())
                .param("kind", TypeExpr::Int)
                .param("length", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("value"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("kind"), BinOp::Eq, e_int(0))),
                        vec![
                            s_return(e_var("value")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::Eq, e_int(1)),
                        vec![
                            s_assign("_int", e_cast(CastType::Int, e_var("value"))),
                            s_return(e_ternary(e_binop(e_cast(CastType::String, e_var("_int")), BinOp::StrictEq, e_cast(CastType::String, e_var("value"))), e_var("_int"), e_var("value"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::Eq, e_int(2)),
                        vec![
                            s_return(e_cast(CastType::Float, e_var("value"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("kind"), BinOp::Eq, e_int(5)),
                        vec![
                            s_return(e_call("str_pad", vec![e_cast(CastType::String, e_var("value")), e_var("length"), e_str("0"), e_const("STR_PAD_LEFT")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_bytes", e_cast(CastType::String, e_var("value"))),
                    s_assign("_bits", e_int(0)),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_call("strlen", vec![e_var("_bytes")]))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_bits", e_binop(e_binop(e_var("_bits"), BinOp::ShiftLeft, e_int(8)), BinOp::BitOr, e_call("ord", vec![e_index(e_var("_bytes"), e_var("_i"))]))),
                    ]),
                    s_if(
                        e_binop(e_var("_bits"), BinOp::Lt, e_int(0)),
                        vec![
                            s_assign("_half", e_binop(e_binop(e_var("_bits"), BinOp::ShiftRight, e_int(1)), BinOp::BitAnd, e_int(9223372036854775807))),
                            s_assign("_tens", e_call("intdiv", vec![e_var("_half"), e_int(5)])),
                            s_assign("_unit", e_binop(e_binop(e_int(2), BinOp::Mul, e_binop(e_var("_half"), BinOp::Sub, e_binop(e_var("_tens"), BinOp::Mul, e_int(5)))), BinOp::Add, e_binop(e_var("_bits"), BinOp::BitAnd, e_int(1)))),
                            s_return(e_binop(e_cast(CastType::String, e_var("_tens")), BinOp::Concat, e_cast(CastType::String, e_var("_unit")))),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_ternary(e_binop(e_var("kind"), BinOp::Eq, e_int(3)), e_var("_bits"), e_cast(CastType::String, e_var("_bits")))),
                ]),
        )
        .method(
            method("__elephcCountFor")
                .private()
                .param("link", t_class("mysqli"))
                .param("binary", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "statId", e_method_call(e_var("link"), "__elephcStatId", vec![])),
                    s_prop_assign(e_this(), "statBinary", e_var("binary")),
                ]),
        )
        .method(
            method("statKey")
                .private()
                .param("index", TypeExpr::Int)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_call("count", vec![e_this_prop("statKeys")]), BinOp::Eq, e_int(0)),
                        vec![
                            s_assign("_text", e_array_assoc(vec![(e_str("DECIMAL"), e_str("proto_text_fetched_decimal")), (e_str("TINY"), e_str("proto_text_fetched_tinyint")), (e_str("SHORT"), e_str("proto_text_fetched_short")), (e_str("LONG"), e_str("proto_text_fetched_int")), (e_str("FLOAT"), e_str("proto_text_fetched_float")), (e_str("DOUBLE"), e_str("proto_text_fetched_double")), (e_str("NULL"), e_str("proto_text_fetched_null")), (e_str("TIMESTAMP"), e_str("proto_text_fetched_timestamp")), (e_str("LONGLONG"), e_str("proto_text_fetched_bigint")), (e_str("INT24"), e_str("proto_text_fetched_int24")), (e_str("DATE"), e_str("proto_text_fetched_date")), (e_str("TIME"), e_str("proto_text_fetched_time")), (e_str("DATETIME"), e_str("proto_text_fetched_datetime")), (e_str("YEAR"), e_str("proto_text_fetched_year")), (e_str("NEWDATE"), e_str("proto_text_fetched_date")), (e_str("VARCHAR"), e_str("proto_text_fetched_string")), (e_str("BIT"), e_str("proto_text_fetched_bit")), (e_str("NEWDECIMAL"), e_str("proto_text_fetched_decimal")), (e_str("ENUM"), e_str("proto_text_fetched_set")), (e_str("SET"), e_str("proto_text_fetched_geometry")), (e_str("JSON"), e_str("proto_text_fetched_blob")), (e_str("TINY_BLOB"), e_str("proto_text_fetched_enum")), (e_str("MEDIUM_BLOB"), e_str("proto_text_fetched_enum")), (e_str("LONG_BLOB"), e_str("proto_text_fetched_enum")), (e_str("BLOB"), e_str("proto_text_fetched_enum")), (e_str("VAR_STRING"), e_str("proto_text_fetched_string")), (e_str("STRING"), e_str("proto_text_fetched_string")), (e_str("GEOMETRY"), e_str("proto_text_fetched_other"))])),
                            s_assign("_binary", e_array_assoc(vec![(e_str("DECIMAL"), e_str("proto_binary_fetched_float")), (e_str("TINY"), e_str("proto_binary_fetched_short")), (e_str("SHORT"), e_str("proto_binary_fetched_int24")), (e_str("LONG"), e_str("proto_binary_fetched_bigint")), (e_str("FLOAT"), e_str("proto_binary_fetched_double")), (e_str("DOUBLE"), e_str("proto_binary_fetched_date")), (e_str("NULL"), e_str("proto_binary_fetched_bit")), (e_str("TIMESTAMP"), e_str("proto_binary_fetched_string")), (e_str("LONGLONG"), e_str("proto_binary_fetched_decimal")), (e_str("INT24"), e_str("proto_binary_fetched_int")), (e_str("DATE"), e_str("proto_binary_fetched_year")), (e_str("TIME"), e_str("proto_binary_fetched_datetime")), (e_str("DATETIME"), e_str("proto_binary_fetched_timestamp")), (e_str("YEAR"), e_str("proto_binary_fetched_time")), (e_str("NEWDATE"), e_str("proto_binary_fetched_year")), (e_str("VARCHAR"), e_str("proto_binary_fetched_json")), (e_str("BIT"), e_str("proto_binary_fetched_tinyint")), (e_str("NEWDECIMAL"), e_str("proto_binary_fetched_float")), (e_str("ENUM"), e_str("proto_binary_fetched_enum")), (e_str("SET"), e_str("proto_binary_fetched_set")), (e_str("TINY_BLOB"), e_str("proto_binary_fetched_blob")), (e_str("MEDIUM_BLOB"), e_str("proto_binary_fetched_blob")), (e_str("LONG_BLOB"), e_str("proto_binary_fetched_blob")), (e_str("BLOB"), e_str("proto_binary_fetched_blob")), (e_str("VAR_STRING"), e_str("proto_binary_fetched_json")), (e_str("STRING"), e_str("proto_binary_fetched_json")), (e_str("GEOMETRY"), e_str("proto_binary_fetched_geometry"))])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_assign("_native", e_cast(CastType::String, e_index(e_this_prop("metaNativeTypes"), e_var("_i")))),
                                s_prop_array_push(e_this(), "statKeys", e_ternary(e_this_prop("statBinary"), e_null_coalesce(e_index(e_var("_binary"), e_var("_native")), e_str("proto_binary_fetched_other")), e_null_coalesce(e_index(e_var("_text"), e_var("_native")), e_str("proto_binary_fetched_null")))),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_cast(CastType::String, e_index(e_this_prop("statKeys"), e_var("index")))),
                ]),
        )
        .method(
            method("statBytes")
                .private()
                .param("index", TypeExpr::Int)
                .param("value", t_mixed())
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_native", e_cast(CastType::String, e_index(e_this_prop("metaNativeTypes"), e_var("index")))),
                    s_if(
                        e_binop(e_var("_native"), BinOp::StrictEq, e_str("BIT")),
                        vec![
                            s_assign("_bytes", e_call("intdiv", vec![e_binop(e_cast(CastType::Int, e_index(e_this_prop("metaLens"), e_var("index"))), BinOp::Add, e_int(7)), e_int(8)])),
                            s_return(e_ternary(e_this_prop("statBinary"), e_binop(e_var("_bytes"), BinOp::Add, e_int(1)), e_var("_bytes"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_not(e_this_prop("statBinary")), BinOp::Or, e_call("is_string", vec![e_var("value")])),
                        vec![
                            s_return(e_call("strlen", vec![e_cast(CastType::String, e_var("value"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_native"), BinOp::StrictEq, e_str("TINY")),
                        vec![
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_native"), BinOp::StrictEq, e_str("SHORT")), BinOp::Or, e_binop(e_var("_native"), BinOp::StrictEq, e_str("YEAR"))),
                        vec![
                            s_return(e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("_native"), BinOp::StrictEq, e_str("INT24")), BinOp::Or, e_binop(e_var("_native"), BinOp::StrictEq, e_str("LONG"))), BinOp::Or, e_binop(e_var("_native"), BinOp::StrictEq, e_str("FLOAT"))),
                        vec![
                            s_return(e_int(4)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("_native"), BinOp::StrictEq, e_str("LONGLONG")), BinOp::Or, e_binop(e_var("_native"), BinOp::StrictEq, e_str("DOUBLE"))),
                        vec![
                            s_return(e_int(8)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("strlen", vec![e_cast(CastType::String, e_var("value"))])),
                ]),
        )
        .method(
            method("countRow")
                .private()
                .param("row", t_array())
                .param("buffered", TypeExpr::Bool)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_id", e_this_prop("statId")),
                    s_if(
                        e_binop(e_var("_id"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_var("buffered"),
                        vec![
                            s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_ternary(e_this_prop("statBinary"), e_str("rows_fetched_from_client_ps_buffered"), e_str("rows_fetched_from_client_normal_buffered")), e_int(1), e_bool(false)])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("rows_fetched_from_server_normal"), e_int(1), e_bool(true)])),
                        s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("rows_fetched_from_client_normal_unbuffered"), e_int(1), e_bool(true)])),
                    ]),
                    ),
                    s_assign("_bytes", e_int(0)),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_value", e_index(e_var("row"), e_var("_i"))),
                        s_if(
                            e_binop(e_var("_value"), BinOp::StrictEq, e_null()),
                            vec![
                                s_if(
                                    e_this_prop("statBinary"),
                                    vec![
                                        s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("proto_binary_fetched_bit"), e_int(1), e_bool(true)])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_continue(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_method_call(e_this(), "statKey", vec![e_var("_i")]), e_int(1), e_bool(true)])),
                        s_assign("_bytes", e_binop(e_var("_bytes"), BinOp::Add, e_method_call(e_this(), "statBytes", vec![e_var("_i"), e_var("_value")]))),
                    ]),
                    s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_ternary(e_this_prop("statBinary"), e_str("bytes_received_real_data_ps"), e_str("bytes_received_real_data_normal")), e_var("_bytes"), e_bool(true)])),
                ]),
        )
        .method(
            method("countFree")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("_id", e_this_prop("statId")),
                    s_if(
                        e_binop(e_binop(e_var("_id"), BinOp::Lt, e_int(0)), BinOp::Or, e_this_prop("statFreed")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "statFreed", e_bool(true)),
                    s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("explicit_free_result"), e_int(1), e_bool(true)])),
                    s_prop_assign(e_this(), "statId", e_neg(e_int(1))),
                ]),
        )
        .method(
            method("fetch_row")
                .returns(t_nullable(t_array()))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_this_prop("streamStmt"), BinOp::GtEq, e_int(0)), BinOp::Or, e_binop(e_this_prop("type"), BinOp::Eq, e_int(1))),
                        vec![
                            s_return(e_method_call(e_this(), "streamRow", vec![])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("rowCount")),
                        vec![
                            s_prop_assign(e_this(), "lengths", e_null()),
                            s_if(
                                e_binop(e_this_prop("type"), BinOp::Eq, e_int(1)),
                                vec![
                                    s_prop_assign(e_this(), "num_rows", e_this_prop("rowCount")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_base", e_binop(e_this_prop("pos"), BinOp::Mul, e_this_prop("field_count"))),
                    s_prop_assign(e_this(), "pos", e_binop(e_this_prop("pos"), BinOp::Add, e_int(1))),
                    s_assign("_row", e_array(vec![])),
                    s_assign("_lengths", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_value", e_index(e_this_prop("cells"), e_binop(e_var("_base"), BinOp::Add, e_var("_i")))),
                        s_array_push("_row", e_var("_value")),
                        s_if(
                            e_binop(e_var("_value"), BinOp::StrictEq, e_null()),
                            vec![
                                s_array_push("_lengths", e_int(0)),
                            ],
                            vec![],
                            Some(vec![
                            s_array_push("_lengths", e_call("strlen", vec![e_cast(CastType::String, e_var("_value"))])),
                        ]),
                        ),
                    ]),
                    s_prop_assign(e_this(), "lengths", e_var("_lengths")),
                    s_expr(e_method_call(e_this(), "countRow", vec![e_var("_row"), e_bool(true)])),
                    s_return(e_var("_row")),
                ]),
        )
        .method(
            method("fetch_assoc")
                .returns(t_nullable(t_array()))
                .body(vec![
                    s_assign("_row", e_method_call(e_this(), "fetch_row", vec![])),
                    s_if(
                        e_binop(e_var("_row"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_assoc", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_assign("_assoc", e_cast(CastType::String, e_index(e_this_prop("names"), e_var("_i"))), e_index(e_var("_row"), e_var("_i"))),
                    ]),
                    s_return(e_var("_assoc")),
                ]),
        )
        .method(
            method("fetch_array")
                .param_default("mode", TypeExpr::Int, e_int(3))
                .returns(t_nullable(t_array()))
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("mode"), BinOp::NotEq, e_int(1)), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(2))), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(3))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::fetch_array(): Argument #1 ($mode) must be one of MYSQLI_NUM, MYSQLI_ASSOC, or MYSQLI_BOTH")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_row", e_method_call(e_this(), "fetch_row", vec![])),
                    s_if(
                        e_binop(e_var("_row"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("mode"), BinOp::Eq, e_int(2)),
                        vec![
                            s_return(e_var("_row")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_out", e_array(vec![])),
                    s_if(
                        e_binop(e_var("mode"), BinOp::Eq, e_int(3)),
                        vec![
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_array_assign("_out", e_var("_i"), e_index(e_var("_row"), e_var("_i"))),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_assign("_out", e_cast(CastType::String, e_index(e_this_prop("names"), e_var("_i"))), e_index(e_var("_row"), e_var("_i"))),
                    ]),
                    s_return(e_var("_out")),
                ]),
        )
        .method(
            method("fetch_object")
                .param_default("class", TypeExpr::Str, e_str("stdClass"))
                .param_default("constructor_args", t_array(), e_array(vec![]))
                .returns(t_mixed())
                .body(vec![
                    s_assign("_isStd", e_binop(e_call("strtolower", vec![e_var("class")]), BinOp::StrictEq, e_str("stdclass"))),
                    s_if(
                        e_binop(e_not(e_var("_isStd")), BinOp::And, e_call("__elephc_class_is_abstract", vec![e_var("class")])),
                        vec![
                            s_throw(e_new("Error", vec![e_binop(e_binop(e_str("Class "), BinOp::Concat, e_var("class")), BinOp::Concat, e_str(" cannot be instantiated"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_row", e_method_call(e_this(), "fetch_row", vec![])),
                    s_if(
                        e_binop(e_var("_row"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_var("_isStd"),
                        vec![
                            s_assign("_std", e_new("stdClass", vec![])),
                            s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                                s_assign("_n", e_cast(CastType::String, e_index(e_this_prop("names"), e_var("_i")))),
                                s_expr(e_assign(e_dyn_prop(e_var("_std"), e_var("_n")), e_index(e_var("_row"), e_var("_i")))),
                            ]),
                            s_return(e_var("_std")),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_object", e_method_call(e_this(), "assignRow", vec![e_call("__elephc_new_without_constructor", vec![e_var("class")]), e_var("_row")])),
                    s_if(
                        e_call("__elephc_class_has_constructor", vec![e_var("class")]),
                        vec![
                            s_expr(e_call("call_user_func_array", vec![e_array(vec![e_var("_object"), e_str("__construct")]), e_var("constructor_args")])),
                        ],
                        vec![
                        (e_binop(e_call("count", vec![e_var("constructor_args")]), BinOp::NotEq, e_int(0)), vec![
                            // PHP 8.3 turned this Exception into a ValueError.
                            s_throw(e_new(if php_version >= PhpVersion::Php83 { "ValueError" } else { "Exception" }, vec![e_binop(e_binop(e_str("mysqli_result::fetch_object(): Argument #2 ($constructor_args) must be empty when the specified class ("), BinOp::Concat, e_var("class")), BinOp::Concat, e_str(") does not have a constructor"))])),
                        ]),
                    ],
                        None,
                    ),
                    s_return(e_var("_object")),
                ]),
        )
        .method(
            method("assignRow")
                .private()
                .param("object", t_mixed())
                .param("row", t_array())
                .returns(t_mixed())
                .body(vec![
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_n", e_cast(CastType::String, e_index(e_this_prop("names"), e_var("_i")))),
                        s_expr(e_assign(e_dyn_prop(e_var("object"), e_var("_n")), e_index(e_var("row"), e_var("_i")))),
                    ]),
                    s_return(e_var("object")),
                ]),
        )
        .method(
            method("fetch_all")
                .param_default("mode", TypeExpr::Int, e_int(2))
                .returns(t_array())
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_binop(e_var("mode"), BinOp::NotEq, e_int(1)), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(2))), BinOp::And, e_binop(e_var("mode"), BinOp::NotEq, e_int(3))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::fetch_all(): Argument #1 ($mode) must be one of MYSQLI_NUM, MYSQLI_ASSOC, or MYSQLI_BOTH")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_all", e_array(vec![])),
                    s_while(e_bool(true), vec![
                        s_assign("_row", e_method_call(e_this(), "fetch_array", vec![e_var("mode")])),
                        s_if(
                            e_binop(e_var("_row"), BinOp::StrictEq, e_null()),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_array_push("_all", e_var("_row")),
                    ]),
                    s_return(e_var("_all")),
                ]),
        )
        // `mysqli_result::fetch_column` is PHP 8.1+; under 8.0 the method is ABSENT.
        .when(php_version >= PhpVersion::Php81, |class| {
            class.method(method("fetch_column")
                    .param_default("column", TypeExpr::Int, e_int(0))
                    .returns(t_mixed())
                    .body(vec![
                        s_if(
                            e_binop(e_var("column"), BinOp::Lt, e_int(0)),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("mysqli_result::fetch_column(): Argument #1 ($column) must be greater than or equal to 0")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("column"), BinOp::GtEq, e_this_prop("field_count")),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("mysqli_result::fetch_column(): Argument #1 ($column) must be less than the number of fields for this result set")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("_row", e_method_call(e_this(), "fetch_row", vec![])),
                        s_if(
                            e_binop(e_var("_row"), BinOp::StrictEq, e_null()),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_index(e_var("_row"), e_var("column"))),
                    ]),
        )
        })
        .method(
            method("data_seek")
                .param("offset", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("offset"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::data_seek(): Argument #1 ($offset) must be greater than or equal to 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("type"), BinOp::Eq, e_int(1)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("mysqli_result::data_seek() cannot be used in MYSQLI_USE_RESULT mode")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("offset"), BinOp::GtEq, e_this_prop("rowCount")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "pos", e_var("offset")),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("fetch_field_direct")
                .param("index", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_var("index"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::fetch_field_direct(): Argument #1 ($index) must be greater than or equal to 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("index"), BinOp::GtEq, e_this_prop("field_count")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::fetch_field_direct(): Argument #1 ($index) must be less than the number of fields for this result set")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "fieldObject", vec![e_var("index")])),
                ]),
        )
        .method(
            method("fetch_field")
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("current_field"), BinOp::GtEq, e_this_prop("field_count")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_field", e_method_call(e_this(), "fieldObject", vec![e_this_prop("current_field")])),
                    s_prop_assign(e_this(), "current_field", e_binop(e_this_prop("current_field"), BinOp::Add, e_int(1))),
                    s_return(e_var("_field")),
                ]),
        )
        .method(
            method("fetch_fields")
                .returns(t_array())
                .body(vec![
                    s_assign("_fields", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_fields", e_method_call(e_this(), "fieldObject", vec![e_var("_i")])),
                    ]),
                    s_return(e_var("_fields")),
                ]),
        )
        .method(
            method("field_seek")
                .param("index", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("index"), BinOp::Lt, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::field_seek(): Argument #1 ($index) must be greater than or equal to 0")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("index"), BinOp::GtEq, e_this_prop("field_count")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("mysqli_result::field_seek(): Argument #1 ($index) must be less than the number of fields for this result set")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "current_field", e_var("index")),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "discardStream", vec![])),
                    s_expr(e_method_call(e_this(), "countFree", vec![])),
                    s_prop_assign(e_this(), "cells", e_array(vec![])),
                    s_prop_assign(e_this(), "rowCount", e_int(0)),
                    s_prop_assign(e_this(), "num_rows", e_int(0)),
                    s_prop_assign(e_this(), "pos", e_int(0)),
                    s_prop_assign(e_this(), "lengths", e_null()),
                ]),
        )
        .method(
            method("free")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "close", vec![])),
                ]),
        )
        .method(
            method("free_result")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_expr(e_method_call(e_this(), "close", vec![])),
                ]),
        )
        .method(
            method("getIterator")
                .returns(t_class("Iterator"))
                .body(vec![
                    s_return(e_new("__ElephcMysqliResultIterator", vec![e_this()])),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_expr(e_method_call(e_this(), "discardStream", vec![])),
                    s_expr(e_method_call(e_this(), "countFree", vec![])),
                ]),
        )
        .method(
            method("streamRow")
                .private()
                .returns(t_nullable(t_array()))
                .body(vec![
                    s_if(
                        e_binop(e_method_call(e_this(), "streamDetachedIfStale", vec![]), BinOp::Or, e_this_prop("streamStale")),
                        vec![
                            s_assign("_link", e_this_prop("streamReportLink")),
                            s_if(
                                e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_expr(e_method_call(e_var("_link"), "__elephcReportGiven", vec![e_str("Commands out of sync; you can't run this command now"), e_int(2014), e_str("HY000")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("streamStmt"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_this_prop("streamStep"), BinOp::Eq, e_int(2))),
                        vec![
                            s_prop_assign(e_this(), "streamStep", e_call("elephc_pdo_step", vec![e_this_prop("streamStmt")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("streamStmt"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_this_prop("streamStep"), BinOp::Lt, e_int(0))),
                        vec![
                            s_assign("_link", e_this_prop("streamLink")),
                            s_if(
                                e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                                vec![s_expr(e_method_call(e_var("_link"), "__elephcAdoptBridgeError", vec![]))],
                                vec![],
                                None,
                            ),
                            s_expr(e_method_call(e_this(), "endStream", vec![])),
                            s_prop_assign(e_this(), "lengths", e_null()),
                            s_prop_assign(e_this(), "num_rows", e_this_prop("streamRows")),
                            s_if(
                                e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                                vec![s_expr(e_method_call(e_var("_link"), "__elephcReportError", vec![]))],
                                vec![],
                                None,
                            ),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_this_prop("streamStmt"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_this_prop("streamStep"), BinOp::NotEq, e_int(1))),
                        vec![
                            s_expr(e_method_call(e_this(), "endStream", vec![])),
                            s_prop_assign(e_this(), "lengths", e_null()),
                            s_prop_assign(e_this(), "num_rows", e_this_prop("streamRows")),
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_row", e_array(vec![])),
                    s_assign("_lengths", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_value", e_static_call("mysqli_result", "__elephcConvertCell", vec![e_method_call(e_this(), "columnValue", vec![e_var("_i")]), e_cast(CastType::Int, e_index(e_this_prop("streamKinds"), e_var("_i"))), e_cast(CastType::Int, e_index(e_this_prop("metaLens"), e_var("_i")))])),
                        s_array_push("_row", e_var("_value")),
                        s_if(
                            e_binop(e_var("_value"), BinOp::StrictEq, e_null()),
                            vec![
                                s_array_push("_lengths", e_int(0)),
                            ],
                            vec![],
                            Some(vec![
                            s_array_push("_lengths", e_call("strlen", vec![e_cast(CastType::String, e_var("_value"))])),
                        ]),
                        ),
                    ]),
                    s_prop_assign(e_this(), "streamRows", e_binop(e_this_prop("streamRows"), BinOp::Add, e_int(1))),
                    s_prop_assign(e_this(), "streamStep", e_int(2)),
                    s_prop_assign(e_this(), "lengths", e_var("_lengths")),
                    s_expr(e_method_call(e_this(), "countRow", vec![e_var("_row"), e_bool(false)])),
                    s_return(e_var("_row")),
                ]),
        )
        .method(
            method("discardStream")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("streamStmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_method_call(e_this(), "streamDetachedIfStale", vec![]),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_id", e_this_prop("statId")),
                    s_if(
                        e_binop(e_binop(e_var("_id"), BinOp::GtEq, e_int(0)), BinOp::And, e_binop(e_this_prop("streamStep"), BinOp::NotEq, e_int(0))),
                        vec![
                            s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("flushed_normal_sets"), e_int(1), e_bool(true)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_while(e_bool(true), vec![
                        s_if(
                            e_binop(e_this_prop("streamStep"), BinOp::Eq, e_int(2)),
                            vec![
                                s_prop_assign(e_this(), "streamStep", e_call("elephc_pdo_step", vec![e_this_prop("streamStmt")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_this_prop("streamStep"), BinOp::NotEq, e_int(1)),
                            vec![
                                s_break(1),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "streamRows", e_binop(e_this_prop("streamRows"), BinOp::Add, e_int(1))),
                        s_prop_assign(e_this(), "streamStep", e_int(2)),
                        s_if(
                            e_binop(e_var("_id"), BinOp::GtEq, e_int(0)),
                            vec![
                                s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("rows_fetched_from_server_normal"), e_int(1), e_bool(true)])),
                                s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("rows_fetched_from_client_normal_unbuffered"), e_int(1), e_bool(true)])),
                                s_expr(e_static_call("mysqli", "__elephcStatFor", vec![e_var("_id"), e_str("rows_skipped_normal"), e_int(1), e_bool(true)])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_expr(e_method_call(e_this(), "endStream", vec![])),
                ]),
        )
        .method(
            method("streamDetachedIfStale")
                .private()
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("streamStmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_link", e_this_prop("streamLink")),
                    s_if(
                        e_binop(e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()), BinOp::And, e_method_call(e_var("_link"), "__elephcOwnsStream", vec![e_this_prop("streamStmt")])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "streamStmt", e_neg(e_int(1))),
                    s_prop_assign(e_this(), "streamLink", e_null()),
                    s_prop_assign(e_this(), "streamReportLink", e_var("_link")),
                    s_prop_assign(e_this(), "streamStale", e_bool(true)),
                    s_prop_assign(e_this(), "lengths", e_null()),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("endStream")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("streamStmt"), BinOp::Lt, e_int(0)),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_stmt", e_this_prop("streamStmt")),
                    s_prop_assign(e_this(), "streamStmt", e_neg(e_int(1))),
                    s_assign("_link", e_this_prop("streamLink")),
                    s_prop_assign(e_this(), "streamLink", e_null()),
                    s_if(
                        e_binop(e_var("_link"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_method_call(e_var("_link"), "__elephcStreamFinished", vec![e_var("_stmt"), e_this_prop("streamRows")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("columnValue")
                .private()
                .param("index", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("_type", e_call("elephc_pdo_column_type", vec![e_this_prop("streamStmt"), e_var("index")])),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(1)),
                        vec![
                            s_return(e_call("elephc_pdo_column_int", vec![e_this_prop("streamStmt"), e_var("index")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(2)),
                        vec![
                            s_return(e_call("elephc_pdo_column_double", vec![e_this_prop("streamStmt"), e_var("index")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("_type"), BinOp::Eq, e_int(5)),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_len", e_call("elephc_pdo_column_data_len", vec![e_this_prop("streamStmt"), e_var("index")])),
                    s_if(
                        e_binop(e_var("_len"), BinOp::Gt, e_int(0)),
                        vec![
                            s_return(e_call("__elephc_ptr_read_string", vec![e_call("elephc_pdo_column_data_ptr", vec![e_this_prop("streamStmt"), e_var("index")]), e_var("_len")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_str("")),
                ]),
        )
        .method(
            method("fieldObject")
                .private()
                .param("index", TypeExpr::Int)
                .returns(t_class("stdClass"))
                .body(vec![
                    s_assign("_type", e_method_call(e_this(), "mysqliTypeFromNative", vec![e_cast(CastType::String, e_index(e_this_prop("metaNativeTypes"), e_var("index")))])),
                    s_assign("_field", e_new("stdClass", vec![])),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("name")), e_cast(CastType::String, e_index(e_this_prop("names"), e_var("index"))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("orgname")), e_cast(CastType::String, e_index(e_this_prop("names"), e_var("index"))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("table")), e_cast(CastType::String, e_index(e_this_prop("metaTables"), e_var("index"))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("orgtable")), e_cast(CastType::String, e_index(e_this_prop("metaTables"), e_var("index"))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("def")), e_str(""))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("db")), e_str(""))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("catalog")), e_str("def"))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("max_length")), e_int(0))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("length")), e_cast(CastType::Int, e_index(e_this_prop("metaLens"), e_var("index"))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("charsetnr")), e_int(0))),
                    s_assign("_flags", e_cast(CastType::Int, e_index(e_this_prop("metaFlags"), e_var("index")))),
                    s_assign("_length", e_cast(CastType::Int, e_index(e_this_prop("metaLens"), e_var("index")))),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("_type"), BinOp::LtEq, e_int(9)), BinOp::And, e_binop(e_binop(e_binop(e_var("_type"), BinOp::NotEq, e_int(7)), BinOp::Or, e_binop(e_var("_length"), BinOp::Eq, e_int(14))), BinOp::Or, e_binop(e_var("_length"), BinOp::Eq, e_int(8)))), BinOp::Or, e_binop(e_var("_type"), BinOp::Eq, e_int(13))),
                        vec![
                            s_assign("_flags", e_binop(e_var("_flags"), BinOp::BitOr, e_int(32768))),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("flags")), e_var("_flags"))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("type")), e_var("_type"))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("decimals")), e_cast(CastType::Int, e_index(e_this_prop("metaDecimals"), e_var("index"))))),
                    s_return(e_var("_field")),
                ]),
        )
        .method(
            method("mysqliTypeFromNative")
                .private()
                .param("native", TypeExpr::Str)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("DECIMAL")),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("TINY")),
                        vec![
                            s_return(e_int(1)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("SHORT")),
                        vec![
                            s_return(e_int(2)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("LONG")),
                        vec![
                            s_return(e_int(3)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("FLOAT")),
                        vec![
                            s_return(e_int(4)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("DOUBLE")),
                        vec![
                            s_return(e_int(5)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("NULL")),
                        vec![
                            s_return(e_int(6)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("TIMESTAMP")),
                        vec![
                            s_return(e_int(7)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("LONGLONG")),
                        vec![
                            s_return(e_int(8)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("INT24")),
                        vec![
                            s_return(e_int(9)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("DATE")),
                        vec![
                            s_return(e_int(10)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("TIME")),
                        vec![
                            s_return(e_int(11)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("DATETIME")),
                        vec![
                            s_return(e_int(12)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("YEAR")),
                        vec![
                            s_return(e_int(13)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("NEWDATE")),
                        vec![
                            s_return(e_int(14)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("VARCHAR")),
                        vec![
                            s_return(e_int(15)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("BIT")),
                        vec![
                            s_return(e_int(16)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("VECTOR")),
                        vec![
                            s_return(e_int(242)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("JSON")),
                        vec![
                            s_return(e_int(245)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("NEWDECIMAL")),
                        vec![
                            s_return(e_int(246)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("ENUM")),
                        vec![
                            s_return(e_int(247)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("SET")),
                        vec![
                            s_return(e_int(248)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("TINY_BLOB")),
                        vec![
                            s_return(e_int(249)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("MEDIUM_BLOB")),
                        vec![
                            s_return(e_int(250)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("LONG_BLOB")),
                        vec![
                            s_return(e_int(251)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("BLOB")),
                        vec![
                            s_return(e_int(252)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("VAR_STRING")),
                        vec![
                            s_return(e_int(253)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("STRING")),
                        vec![
                            s_return(e_int(254)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("native"), BinOp::StrictEq, e_str("GEOMETRY")),
                        vec![
                            s_return(e_int(255)),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_int(254)),
                ]),
        )
        .build()
}

/// `__ElephcMysqliResultIterator` — transcribed from the PHP form.
pub(super) fn decl_class_elephcmysqliresultiterator() -> Stmt {
    class("__ElephcMysqliResultIterator")
        .final_()
        .implements("Iterator")
        .private_prop("result", t_class("mysqli_result"), None)
        .private_prop("row", t_mixed(), None)
        .private_prop("position", TypeExpr::Int, None)
        .method(
            method("__construct")
                .param("result", t_class("mysqli_result"))
                .body(vec![
                    s_prop_assign(e_this(), "result", e_var("result")),
                    s_prop_assign(e_this(), "row", e_null()),
                    s_prop_assign(e_this(), "position", e_int(0)),
                ]),
        )
        .method(
            method("rewind")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_prop(e_this_prop("result"), "type"), BinOp::Eq, e_int(0)),
                        vec![
                            s_expr(e_method_call(e_this_prop("result"), "data_seek", vec![e_int(0)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "row", e_method_call(e_this_prop("result"), "fetch_assoc", vec![])),
                    s_prop_assign(e_this(), "position", e_int(0)),
                ]),
        )
        .method(
            method("valid")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_this_prop("row"), BinOp::StrictNotEq, e_null())),
                ]),
        )
        .method(
            method("current")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_this_prop("row")),
                ]),
        )
        .method(
            method("key")
                .returns(t_mixed())
                .body(vec![
                    s_return(e_this_prop("position")),
                ]),
        )
        .method(
            method("next")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "row", e_method_call(e_this_prop("result"), "fetch_assoc", vec![])),
                    s_prop_assign(e_this(), "position", e_binop(e_this_prop("position"), BinOp::Add, e_int(1))),
                ]),
        )
        .build()
}
