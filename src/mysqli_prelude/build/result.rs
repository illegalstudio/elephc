//! Purpose:
//! Builds the `mysqli_result` class (plus its foreach iterator) as AST. A result
//! OWNS its rows: `mysqli::query` drains every row through `elephc_pdo_step`
//! into PHP arrays and finalizes the statement before the result is handed out,
//! so a later query on the same connection can never invalidate an earlier
//! result (`data_seek`, `num_rows`, `foreach` all keep working).
//!
//! Called from:
//! - `crate::mysqli_prelude::build::mysqli_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from `mysqli_prelude::result::SRC` (`synthetic_class::transcribe`);
//!   the oracle `built_declarations_match_the_php_for_every_version` compares the
//!   built classes against that PHP for every profile.
//! - Rows are stored positionally; assoc/both shapes are derived from the
//!   captured column-name list at fetch time (duplicate names: last wins, like
//!   PHP). Field metadata (`name`, `table`, native type, flags, length) is
//!   captured at drain time because the statement is finalized immediately.
//! - `fetch_*` return `null` when the cursor is exhausted (`fetch_column`
//!   returns `false`), matching php-src.
//! - `fetch_column` is PHP 8.1+; the method is a version conditional here and its
//!   procedural alias is gated in `build.rs`.
//! - `getIterator()` yields `fetch_assoc()` rows from position 0 (PHP's
//!   `mysqli_result` foreach is assoc with integer keys).

use crate::php_version::PhpVersion;
use crate::parser::ast::{BinOp, CastType, TypeExpr, Stmt};
use crate::synthetic_class::{
    class,
    e_array,
    e_assign,
    e_binop,
    e_bool,
    e_call,
    e_cast,
    e_dyn_prop,
    e_index,
    e_int,
    e_method_call,
    e_new,
    e_new_dynamic,
    e_null,
    e_post_inc,
    e_spread,
    e_str,
    e_this,
    e_this_prop,
    e_var,
    method,
    s_array_assign,
    s_array_push,
    s_assign,
    s_break,
    s_expr,
    s_for,
    s_if,
    s_prop_assign,
    s_return,
    s_throw,
    s_while,
    t_array,
    t_class,
    t_mixed,
    t_nullable,
};

/// `mysqli_result` — transcribed from the PHP form.
pub(super) fn decl_class_mysqli_result(php_version: PhpVersion) -> Stmt {
    class("mysqli_result")
        .implements("IteratorAggregate")
        .prop("num_rows", TypeExpr::Int, Some(e_int(0)))
        .prop("field_count", TypeExpr::Int, Some(e_int(0)))
        // Column byte lengths of the row returned by the most recent fetch, or null before the
        // first fetch / after exhaustion (php-src's $lengths).
        .prop("lengths", t_nullable(t_array()), Some(e_null()))
        // Buffered cells and drain-time column metadata. Cells are stored FLAT in row-major order
        // (row * field_count + column) so every buffered value is a Mixed scalar — a fetch builds
        // a fresh row array, which keeps the checker's array element typing simple (no
        // nested-array Mixed unwraps). The bridge statement is already finalized when a result
        // exists, so everything a fetch needs lives here.
        .private_prop("cells", t_array(), Some(e_array(vec![])))
        .private_prop("names", t_array(), Some(e_array(vec![])))
        .private_prop("metaTables", t_array(), Some(e_array(vec![])))
        .private_prop("metaNativeTypes", t_array(), Some(e_array(vec![])))
        .private_prop("metaFlags", t_array(), Some(e_array(vec![])))
        .private_prop("metaLens", t_array(), Some(e_array(vec![])))
        .private_prop("metaDecimals", t_array(), Some(e_array(vec![])))
        .private_prop("rowCount", TypeExpr::Int, Some(e_int(0)))
        .private_prop("pos", TypeExpr::Int, Some(e_int(0)))
        .private_prop("fieldPos", TypeExpr::Int, Some(e_int(0)))
        // Internal factory used by mysqli::query's drain (not part of PHP's surface; a user never
        // constructs mysqli_result directly). `$cells` is the flat row-major cell list described
        // above.
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
                    s_assign("_result", e_new("mysqli_result", vec![])),
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
            method("fetch_row")
                .returns(t_nullable(t_array()))
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("pos"), BinOp::GtEq, e_this_prop("rowCount")),
                        vec![
                            s_prop_assign(e_this(), "lengths", e_null()),
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
                        // Duplicate column names: last wins, matching PHP's assoc shape.
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
                    s_assign("_row", e_method_call(e_this(), "fetch_row", vec![])),
                    s_if(
                        e_binop(e_var("_row"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_null()),
                        ],
                        vec![],
                        None,
                    ),
                    // Two straight-line returns rather than one reassigned local, same reason as
                    // the PDO prelude's hydrateClassOrStd: the dynamic-new result is Mixed, and
                    // rebinding a concrete `new stdClass()` into that local would unify Mixed with
                    // Object(stdClass) in a slot that is about to be dynamic-property-written.
                    // stdClass also never goes through dynamic allocation (php-src resolves it to
                    // zend_standard_class_def directly).
                    s_if(
                        e_binop(e_call("strtolower", vec![e_var("class")]), BinOp::StrictEq, e_str("stdclass")),
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
                    // Documented divergence: php-src assigns properties BEFORE calling the
                    // constructor; elephc constructs first (PDO::FETCH_PROPS_LATE order) so the
                    // dynamic instantiation goes through the one proven `new $class` path with
                    // constructor arguments.
                    s_assign("_object", e_new_dynamic(e_var("class"), vec![e_spread(e_var("constructor_args"))])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_assign("_n", e_cast(CastType::String, e_index(e_this_prop("names"), e_var("_i")))),
                        s_expr(e_assign(e_dyn_prop(e_var("_object"), e_var("_n")), e_index(e_var("_row"), e_var("_i")))),
                    ]),
                    s_return(e_var("_object")),
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
                    // A fresh `$_row` each iteration, narrowed by the `=== null` break, so it is
                    // never reassigned from the narrowed `array` back to `?array` (which the
                    // checker rejects inside the narrowed scope).
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
        // `mysqli_result::fetch_column` is PHP 8.1+; under 8.0 the method is ABSENT
        // (its ValueError text names the method, so a stub would misreport too).
        .when(php_version >= PhpVersion::Php81, |class| {
            class.method(
                method("fetch_column")
                    .param_default("column", TypeExpr::Int, e_int(0))
                    .returns(t_mixed())
                    .body(vec![
                        s_if(
                            e_binop(e_binop(e_var("column"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("column"), BinOp::GtEq, e_this_prop("field_count"))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_str("mysqli_result::fetch_column(): Argument #1 ($column) must be greater than or equal to 0 and less than the number of fields in this result set")])),
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
                        e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::GtEq, e_this_prop("rowCount"))),
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
                        e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::GtEq, e_this_prop("field_count"))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    // Unknown metadata the bridge does not expose stays 0 / "" (documented);
                    // name/orgname, table/orgtable, type, flags, and length are real.
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
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("flags")), e_cast(CastType::Int, e_index(e_this_prop("metaFlags"), e_var("index"))))),
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("type")), e_method_call(e_this(), "mysqliTypeFromNative", vec![e_cast(CastType::String, e_index(e_this_prop("metaNativeTypes"), e_var("index")))]))),
                    // The bridge's column_precision is MySQL's own wire "decimals" byte.
                    s_expr(e_assign(e_dyn_prop(e_var("_field"), e_str("decimals")), e_cast(CastType::Int, e_index(e_this_prop("metaDecimals"), e_var("index"))))),
                    s_return(e_var("_field")),
                ]),
        )
        .method(
            method("fetch_field")
                .returns(t_mixed())
                .body(vec![
                    s_if(
                        e_binop(e_this_prop("fieldPos"), BinOp::GtEq, e_this_prop("field_count")),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_field", e_method_call(e_this(), "fetch_field_direct", vec![e_this_prop("fieldPos")])),
                    s_prop_assign(e_this(), "fieldPos", e_binop(e_this_prop("fieldPos"), BinOp::Add, e_int(1))),
                    s_return(e_var("_field")),
                ]),
        )
        .method(
            method("fetch_fields")
                .returns(t_array())
                .body(vec![
                    s_assign("_fields", e_array(vec![])),
                    s_for(Some(s_assign("_i", e_int(0))), Some(e_binop(e_var("_i"), BinOp::Lt, e_this_prop("field_count"))), Some(s_expr(e_post_inc("_i"))), vec![
                        s_array_push("_fields", e_method_call(e_this(), "fetch_field_direct", vec![e_var("_i")])),
                    ]),
                    s_return(e_var("_fields")),
                ]),
        )
        // Moves the fetch_field() cursor. php returns the previous position; elephc returns
        // true/false (in-range), the mysqli_result::field_seek contract most code relies on.
        .method(
            method("field_seek")
                .param("index", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("index"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("index"), BinOp::Gt, e_this_prop("field_count"))),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "fieldPos", e_var("index")),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("field_tell")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_this_prop("fieldPos")),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Void)
                .body(vec![
                    // Drop the buffered cells; further fetches see an exhausted cursor.
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
        // php-src type_to_name_native reversed: the bridge reports MySQL's own wire-type names;
        // map them onto the MYSQLI_TYPE_* integers with MYSQLI_TYPE_STRING as the default for
        // anything unrecognized.
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
    // Iterator behind mysqli_result::getIterator(): assoc rows keyed by position, starting from
    // row 0 regardless of the fetch cursor (rewind() seeks).
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
                    s_expr(e_method_call(e_this_prop("result"), "data_seek", vec![e_int(0)])),
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
