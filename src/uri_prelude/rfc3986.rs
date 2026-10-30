//! Purpose:
//! Builds `Uri\Rfc3986\Uri` from the synthetic AST, including its computed `__debugInfo()`
//! and the private opaque slot that owns each object's native handle.
//!
//! Called from:
//! - `crate::uri_prelude::uri_declarations`.
//!
//! Key details:
//! - Declarations are built in `SourceMode::Php`, not internal mode: `var_dump()` must call
//!   the computed `__debugInfo()` of these classes, which compiler-internal classes skip.
//! - State lives in a private opaque slot that every property walker skips, so the objects
//!   expose no properties to `(array)`, `json_encode()`, or `var_export()`.

use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::span::Span;
use crate::synthetic_class::*;

/// Builds one URI prelude declaration.
pub(super) fn rfc3986_declaration() -> Stmt {
    s_namespace("Uri\\Rfc3986", vec![
        class("Uri").span(Span::new(1, 1))
            .final_()
            .private_prop("__elephc_uri_handle", t_nullable(TypeExpr::Int), Some(e_null()))
            .private_static_prop("__elephc_charge", t_array(), Some(e_array(vec![])))
            .method(
                method("parse")
                    .static_()
                    .param("uri", TypeExpr::Str)
                    .param_default("baseUrl", t_mixed(), e_null())
                    .returns(t_nullable(t_class("static")))
                    .body(vec![
                        s_try(vec![
                            s_return(e_new_self(vec![e_var("uri"), e_var("baseUrl")])),
                        ], vec![
                            (vec!["\\Uri\\InvalidUriException"], None, vec![
                                s_return(e_null()),
                            ]),
                        ], None),
                    ]),
            )
            .method(
                method("__construct")
                    .param("uri", TypeExpr::Str)
                    .param_default("baseUrl", t_mixed(), e_null())
                    .body(vec![
                        s_if(
                            e_binop(e_this_prop("__elephc_uri_handle"), BinOp::StrictNotEq, e_null()),
                            vec![
                                s_throw(e_new_fq("Error", vec![e_binop(e_str("Cannot modify readonly object of class "), BinOp::Concat, e_self_class())])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("base", e_int(0)),
                        s_if(
                            e_binop(e_var("baseUrl"), BinOp::StrictNotEq, e_null()),
                            vec![
                                s_if(
                                    e_not(e_instance_of(e_var("baseUrl"), "\\Uri\\Rfc3986\\Uri")),
                                    vec![
                                        s_throw(e_new_fq("TypeError", vec![e_str("Uri\\Rfc3986\\Uri::__construct(): Argument #2 ($baseUrl) must be of type ?Uri\\Rfc3986\\Uri")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("base", e_null_coalesce(e_prop(e_var("baseUrl"), "__elephc_uri_handle"), e_int(0))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("handle", e_call("\\elephc_uri_parse", vec![e_int(0), e_call("\\bin2hex", vec![e_var("uri")]), e_var("base")])),
                        s_if(
                            e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_throw(e_new_fq("Uri\\InvalidUriException", vec![e_self_call("__elephc_message", vec![])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "__elephc_uri_handle", e_var("handle")),
                        s_expr(e_self_call("__elephc_set_charge", vec![e_this(), e_var("handle")])),
                    ]),
            )
            .method(
                method("__destruct")
                    .body(vec![
                        s_if(
                            e_binop(e_this_prop("__elephc_uri_handle"), BinOp::StrictNotEq, e_null()),
                            vec![
                                s_expr(e_call("\\elephc_uri_free", vec![e_this_prop("__elephc_uri_handle")])),
                                s_prop_assign(e_this(), "__elephc_uri_handle", e_null()),
                                s_expr(e_self_call("__elephc_drop_charge", vec![e_this()])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("__clone")
                    .body(vec![
                        s_if(
                            e_binop(e_this_prop("__elephc_uri_handle"), BinOp::StrictNotEq, e_null()),
                            vec![
                                s_prop_assign(e_this(), "__elephc_uri_handle", e_call("\\elephc_uri_clone", vec![e_this_prop("__elephc_uri_handle")])),
                                s_expr(e_self_call("__elephc_set_charge", vec![e_this(), e_this_prop("__elephc_uri_handle")])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
            )
            .method(
                method("getScheme")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(0), e_int(1)])),
                    ]),
            )
            .method(
                method("getRawScheme")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(0), e_int(0)])),
                    ]),
            )
            .method(
                method("withScheme")
                    .param("scheme", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(0), e_var("scheme")])),
                    ]),
            )
            .method(
                method("getUserInfo")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(3), e_int(1)])),
                    ]),
            )
            .method(
                method("getRawUserInfo")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(3), e_int(0)])),
                    ]),
            )
            .method(
                method("withUserInfo")
                    .param("userinfo", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(3), e_var("userinfo")])),
                    ]),
            )
            .method(
                method("getUsername")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(1), e_int(1)])),
                    ]),
            )
            .method(
                method("getRawUsername")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(1), e_int(0)])),
                    ]),
            )
            .method(
                method("getPassword")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(2), e_int(1)])),
                    ]),
            )
            .method(
                method("getRawPassword")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(2), e_int(0)])),
                    ]),
            )
            .method(
                method("getHost")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(4), e_int(1)])),
                    ]),
            )
            .method(
                method("getRawHost")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(4), e_int(0)])),
                    ]),
            )
            .method(
                method("withHost")
                    .param("host", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(4), e_var("host")])),
                    ]),
            )
            .method(
                method("getPort")
                    .returns(t_nullable(TypeExpr::Int))
                    .body(vec![
                        s_assign("port", e_call("\\elephc_uri_port", vec![e_null_coalesce(e_this_prop("__elephc_uri_handle"), e_int(0))])),
                        s_return(e_ternary(e_binop(e_var("port"), BinOp::StrictEq, e_neg(e_int(1))), e_null(), e_var("port"))),
                    ]),
            )
            .method(
                method("withPort")
                    .param("port", t_nullable(TypeExpr::Int))
                    .returns(t_class("static"))
                    .body(vec![
                        s_assign("handle", e_call("\\elephc_uri_with_port", vec![e_null_coalesce(e_this_prop("__elephc_uri_handle"), e_int(0)), e_ternary(e_binop(e_var("port"), BinOp::StrictEq, e_null()), e_int(0), e_var("port")), e_ternary(e_binop(e_var("port"), BinOp::StrictEq, e_null()), e_int(1), e_int(0))])),
                        s_if(
                            e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_throw(e_new_fq("Uri\\InvalidUriException", vec![e_self_call("__elephc_message", vec![])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_method_call(e_this(), "__elephc_rebind", vec![e_var("handle")])),
                    ]),
            )
            .method(
                method("getPath")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_null_coalesce(e_method_call(e_this(), "__elephc_component", vec![e_int(6), e_int(1)]), e_str(""))),
                    ]),
            )
            .method(
                method("getRawPath")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_null_coalesce(e_method_call(e_this(), "__elephc_component", vec![e_int(6), e_int(0)]), e_str(""))),
                    ]),
            )
            .method(
                method("withPath")
                    .param("path", TypeExpr::Str)
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(6), e_var("path")])),
                    ]),
            )
            .method(
                method("getQuery")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(7), e_int(1)])),
                    ]),
            )
            .method(
                method("getRawQuery")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(7), e_int(0)])),
                    ]),
            )
            .method(
                method("withQuery")
                    .param("query", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(7), e_var("query")])),
                    ]),
            )
            .method(
                method("getFragment")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(8), e_int(1)])),
                    ]),
            )
            .method(
                method("getRawFragment")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(8), e_int(0)])),
                    ]),
            )
            .method(
                method("withFragment")
                    .param("fragment", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(8), e_var("fragment")])),
                    ]),
            )
            .method(
                method("equals")
                    .param("uri", t_mixed())
                    .param_default("comparisonMode", t_class("\\Uri\\UriComparisonMode"), e_class_const("\\Uri\\UriComparisonMode", "ExcludeFragment"))
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_if(
                            e_not(e_instance_of(e_var("uri"), "\\Uri\\Rfc3986\\Uri")),
                            vec![
                                s_throw(e_new_fq("TypeError", vec![e_str("Uri\\Rfc3986\\Uri::equals(): Argument #1 ($uri) must be of type Uri\\Rfc3986\\Uri")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("result", e_call("\\elephc_uri_equals", vec![e_null_coalesce(e_this_prop("__elephc_uri_handle"), e_int(0)), e_null_coalesce(e_prop(e_var("uri"), "__elephc_uri_handle"), e_int(0)), e_ternary(e_binop(e_var("comparisonMode"), BinOp::StrictEq, e_class_const("\\Uri\\UriComparisonMode", "IncludeFragment")), e_int(1), e_int(0))])),
                        s_return(e_binop(e_var("result"), BinOp::StrictNotEq, e_int(0))),
                    ]),
            )
            .method(
                method("toString")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_to_string", vec![e_int(1), e_bool(false)])),
                    ]),
            )
            .method(
                method("toRawString")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_to_string", vec![e_int(0), e_bool(false)])),
                    ]),
            )
            .method(
                method("resolve")
                    .param("uri", TypeExpr::Str)
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_new_self(vec![e_var("uri"), e_this()])),
                    ]),
            )
            .method(
                method("__serialize")
                    .returns(t_array())
                    .body(vec![
                        s_return(e_array(vec![e_array_assoc(vec![(e_str("uri"), e_method_call(e_this(), "toRawString", vec![]))]), e_array(vec![])])),
                    ]),
            )
            .method(
                method("__unserialize")
                    .param("data", t_array())
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_binop(e_this_prop("__elephc_uri_handle"), BinOp::StrictNotEq, e_null()),
                            vec![
                                s_throw(e_new_fq("Exception", vec![e_binop(e_binop(e_str("Invalid serialization data for "), BinOp::Concat, e_self_class()), BinOp::Concat, e_str(" object"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_call("\\count", vec![e_var("data")]), BinOp::StrictNotEq, e_int(2)),
                            vec![
                                s_throw(e_new_fq("Exception", vec![e_binop(e_binop(e_str("Invalid serialization data for "), BinOp::Concat, e_self_class()), BinOp::Concat, e_str(" object"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("state", e_null_coalesce(e_index(e_var("data"), e_int(0)), e_null())),
                        s_if(
                            e_binop(e_binop(e_binop(e_not(e_call("\\is_array", vec![e_var("state")])), BinOp::Or, e_binop(e_call("\\count", vec![e_var("state")]), BinOp::StrictNotEq, e_int(1))), BinOp::Or, e_not(e_call("\\array_key_exists", vec![e_str("uri"), e_var("state")]))), BinOp::Or, e_not(e_call("\\is_string", vec![e_index(e_var("state"), e_str("uri"))]))),
                            vec![
                                s_throw(e_new_fq("Exception", vec![e_binop(e_binop(e_str("Invalid serialization data for "), BinOp::Concat, e_self_class()), BinOp::Concat, e_str(" object"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("handle", e_call("\\elephc_uri_parse", vec![e_int(0), e_call("\\bin2hex", vec![e_index(e_var("state"), e_str("uri"))]), e_int(0)])),
                        s_if(
                            e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_throw(e_new_fq("Exception", vec![e_binop(e_binop(e_str("Invalid serialization data for "), BinOp::Concat, e_self_class()), BinOp::Concat, e_str(" object"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("properties", e_null_coalesce(e_index(e_var("data"), e_int(1)), e_null())),
                        s_if(
                            e_binop(e_not(e_call("\\is_array", vec![e_var("properties")])), BinOp::Or, e_binop(e_call("\\count", vec![e_var("properties")]), BinOp::StrictNotEq, e_int(0))),
                            vec![
                                s_expr(e_call("\\elephc_uri_free", vec![e_var("handle")])),
                                s_throw(e_new_fq("Exception", vec![e_binop(e_binop(e_str("Invalid serialization data for "), BinOp::Concat, e_self_class()), BinOp::Concat, e_str(" object"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "__elephc_uri_handle", e_var("handle")),
                        s_expr(e_self_call("__elephc_set_charge", vec![e_this(), e_var("handle")])),
                    ]),
            )
            .method(
                method("__debugInfo")
                    .returns(t_array())
                    .body(vec![
                        s_return(e_array_assoc(vec![(e_str("scheme"), e_method_call(e_this(), "getRawScheme", vec![])), (e_str("username"), e_method_call(e_this(), "getRawUsername", vec![])), (e_str("password"), e_method_call(e_this(), "getRawPassword", vec![])), (e_str("host"), e_method_call(e_this(), "getRawHost", vec![])), (e_str("port"), e_method_call(e_this(), "getPort", vec![])), (e_str("path"), e_method_call(e_this(), "getRawPath", vec![])), (e_str("query"), e_method_call(e_this(), "getRawQuery", vec![])), (e_str("fragment"), e_method_call(e_this(), "getRawFragment", vec![]))])),
                    ]),
            )
            .method(
                method("__elephc_component")
                    .private()
                    .param("component", TypeExpr::Int)
                    .param("mode", TypeExpr::Int)
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_assign("hex", e_call("\\elephc_uri_component", vec![e_null_coalesce(e_this_prop("__elephc_uri_handle"), e_int(0)), e_var("component"), e_var("mode")])),
                        s_return(e_ternary(e_binop(e_var("hex"), BinOp::StrictEq, e_null()), e_null(), e_cast(CastType::String, e_call("\\hex2bin", vec![e_var("hex")])))),
                    ]),
            )
            .method(
                method("__elephc_with")
                    .private()
                    .param("component", TypeExpr::Int)
                    .param("value", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_assign("handle", e_call("\\elephc_uri_with", vec![e_null_coalesce(e_this_prop("__elephc_uri_handle"), e_int(0)), e_var("component"), e_ternary(e_binop(e_var("value"), BinOp::StrictEq, e_null()), e_str(""), e_call("\\bin2hex", vec![e_var("value")])), e_ternary(e_binop(e_var("value"), BinOp::StrictEq, e_null()), e_int(1), e_int(0))])),
                        s_if(
                            e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_throw(e_new_fq("Uri\\InvalidUriException", vec![e_self_call("__elephc_message", vec![])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_method_call(e_this(), "__elephc_rebind", vec![e_var("handle")])),
                    ]),
            )
            .method(
                method("__elephc_rebind")
                    .private()
                    .param("handle", TypeExpr::Int)
                    .returns(t_class("static"))
                    .body(vec![
                        // Clear the handle before cloning: `__clone()` then has nothing to
                        // duplicate, and the clone receives the new handle directly below.
                        // The class is final and `__clone()` runs no user code, so restoring the
                        // original handle immediately after the clone cannot be observed.
                        s_assign("saved", e_this_prop("__elephc_uri_handle")),
                        s_prop_assign(e_this(), "__elephc_uri_handle", e_null()),
                        s_assign("copy", e_clone(e_this())),
                        s_prop_assign(e_this(), "__elephc_uri_handle", e_var("saved")),
                        s_prop_assign(e_var("copy"), "__elephc_uri_handle", e_var("handle")),
                        s_expr(e_self_call("__elephc_set_charge", vec![e_var("copy"), e_var("handle")])),
                        s_return(e_var("copy")),
                    ]),
            )
            .method(
                method("__elephc_to_string")
                    .private()
                    .param("mode", TypeExpr::Int)
                    .param("excludeFragment", TypeExpr::Bool)
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_assign("hex", e_call("\\elephc_uri_to_string", vec![e_null_coalesce(e_this_prop("__elephc_uri_handle"), e_int(0)), e_var("mode"), e_ternary(e_var("excludeFragment"), e_int(1), e_int(0))])),
                        s_if(
                            e_binop(e_var("hex"), BinOp::StrictEq, e_null()),
                            vec![
                                s_throw(e_new_fq("Uri\\UriError", vec![e_binop(e_binop(e_str("Cannot recompose "), BinOp::Concat, e_self_class()), BinOp::Concat, e_str(" to a string"))])),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_cast(CastType::String, e_call("\\hex2bin", vec![e_var("hex")]))),
                    ]),
            )
            .method(
                method("__elephc_message")
                    .private()
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_assign("hex", e_call("\\elephc_uri_error_message", vec![])),
                        s_return(e_ternary(e_binop(e_var("hex"), BinOp::StrictEq, e_null()), e_str("The specified URI is malformed"), e_cast(CastType::String, e_call("\\hex2bin", vec![e_var("hex")])))),
                    ]),
            )
            .method(
                method("__elephc_set_charge")
                    .private()
                    .static_()
                    .param("object", t_class("object"))
                    .param("handle", t_nullable(TypeExpr::Int))
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_if(
                            e_binop(e_var("handle"), BinOp::StrictEq, e_null()),
                            vec![s_return_void()],
                            vec![],
                            None,
                        ),
                        s_assign("handle", e_cast(CastType::Int, e_var("handle"))),
                        s_assign("units", e_call("\\elephc_uri_charge_units", vec![e_var("handle")])),
                        s_self_static_prop_array_assign(
                            "__elephc_charge",
                            e_cast(CastType::String, e_call("\\spl_object_id", vec![e_var("object")])),
                            e_call("\\array_fill", vec![e_int(0), e_var("units"), e_int(0)]),
                        ),
                    ]),
            )
            .method(
                method("__elephc_drop_charge")
                    .private()
                    .static_()
                    .param("object", t_class("object"))
                    .returns(TypeExpr::Void)
                    .body(vec![
                        s_typed_assign(t_array(), "map", e_self_static_prop("__elephc_charge")),
                        s_expr(e_call("unset", vec![e_index(e_var("map"), e_cast(CastType::String, e_call("\\spl_object_id", vec![e_var("object")])))])),
                        s_self_static_prop_assign("__elephc_charge", e_var("map")),
                    ]),
            )
            .build()
    ])
}
