//! Purpose:
//! Builds `Uri\WhatWg\Url`, its validation enums, and its exception classes from the
//! synthetic AST.
//!
//! Called from:
//! - `crate::uri_prelude::uri_declarations`.
//!
//! Key details:
//! - Declarations are built in `SourceMode::Php`, not internal mode: `var_dump()` must call
//!   the computed `__debugInfo()` of these classes, which compiler-internal classes skip.
//! - State lives in a private opaque slot that every property walker skips, so the objects
//!   expose no properties to `(array)`, `json_encode()`, or `var_export()`.

use crate::parser::ast::{BinOp, CastType, EnumCaseDecl, Stmt, StmtKind, TypeExpr};
use crate::span::Span;
use crate::synthetic_class::*;

/// Builds one URI prelude declaration.
pub(super) fn whatwg_declaration() -> Stmt {
    s_namespace("Uri\\WhatWg", vec![
        Stmt::new(StmtKind::EnumDecl { name: "UrlValidationErrorType".to_string(), generics: None, backing_type: None, cases: vec![EnumCaseDecl { name: "DomainToAscii".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "DomainToUnicode".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "DomainInvalidCodePoint".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "HostInvalidCodePoint".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4EmptyPart".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4TooManyParts".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4NonNumericPart".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4NonDecimalPart".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4OutOfRangePart".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv6Unclosed".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv6InvalidCompression".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv6TooManyPieces".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv6MultipleCompression".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv6InvalidCodePoint".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv6TooFewPieces".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4InIpv6TooManyPieces".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4InIpv6InvalidCodePoint".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4InIpv6OutOfRangePart".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "Ipv4InIpv6TooFewParts".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "InvalidUrlUnit".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "SpecialSchemeMissingFollowingSolidus".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "MissingSchemeNonRelativeUrl".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "InvalidReverseSoldius".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "InvalidCredentials".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "HostMissing".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "PortOutOfRange".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "PortInvalid".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "FileInvalidWindowsDriveLetter".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }, EnumCaseDecl { name: "FileInvalidWindowsDriveLetterHost".to_string(), value: None, span: Span::dummy(), attributes: Vec::new() }], implements: Vec::new(), trait_uses: Vec::new(), methods: Vec::new(), constants: Vec::new() }, Span::dummy()),
        class("UrlValidationError").span(Span::new(1, 1))
            .final_()
            .readonly_prop("context", TypeExpr::Str)
            .readonly_prop("type", t_class("\\Uri\\WhatWg\\UrlValidationErrorType"))
            .readonly_prop("failure", TypeExpr::Bool)
            .method(
                method("__construct")
                    .param("context", TypeExpr::Str)
                    .param("type", t_class("\\Uri\\WhatWg\\UrlValidationErrorType"))
                    .param("failure", TypeExpr::Bool)
                    .body(vec![
                        s_if(
                            e_call("isset", vec![e_this_prop("context")]),
                            vec![
                                s_throw(e_new_fq("Error", vec![e_str("Cannot modify readonly property Uri\\WhatWg\\UrlValidationError::$context")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "context", e_var("context")),
                        s_prop_assign(e_this(), "type", e_var("type")),
                        s_prop_assign(e_this(), "failure", e_var("failure")),
                    ]),
            )
            .build(),
        class("InvalidUrlException").span(Span::new(1, 1))
            .extends("\\Uri\\InvalidUriException")
            .readonly_prop("errors", TypeExpr::Array(Box::new(t_mixed())))
            .method(
                method("__construct")
                    .param_default("message", TypeExpr::Str, e_str(""))
                    .param_default("errors", TypeExpr::Array(Box::new(t_mixed())), e_array(vec![]))
                    .param_default("code", TypeExpr::Int, e_int(0))
                    .param_default("previous", t_nullable(t_class("\\Throwable")), e_null())
                    .body(vec![
                        s_assign("currentCode", e_method_call(e_this(), "getCode", vec![])),
                        s_assign("currentPrevious", e_method_call(e_this(), "getPrevious", vec![])),
                        s_expr(e_parent_call("__construct", vec![e_var("message"), e_ternary(e_binop(e_var("code"), BinOp::StrictNotEq, e_int(0)), e_var("code"), e_var("currentCode")), e_null_coalesce(e_var("previous"), e_var("currentPrevious"))])),
                        s_if(
                            e_not(e_self_call("__elephc_is_error_list", vec![e_var("errors")])),
                            vec![
                                s_throw(e_new_fq("ValueError", vec![e_str("Uri\\WhatWg\\InvalidUrlException::__construct(): Argument #2 ($errors) must be a list of Uri\\WhatWg\\UrlValidationError")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_call("isset", vec![e_this_prop("errors")]),
                            vec![
                                s_throw(e_new_fq("Error", vec![e_str("Cannot modify readonly property Uri\\WhatWg\\InvalidUrlException::$errors")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_prop_assign(e_this(), "errors", e_var("errors")),
                    ]),
            )
            .method(
                method("__elephc_is_error_list")
                    .private()
                    .static_()
                    .param("errors", t_array())
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_if(
                            e_not(e_call("\\array_is_list", vec![e_var("errors")])),
                            vec![
                                s_return(e_bool(false)),
                            ],
                            vec![],
                            None,
                        ),
                        s_foreach(e_var("errors"), None, "error", vec![
                            s_if(
                                e_not(e_instance_of(e_var("error"), "\\Uri\\WhatWg\\UrlValidationError")),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                        ]),
                        s_return(e_bool(true)),
                    ]),
            )
            .build(),
        class("Url").span(Span::new(1, 1))
            .final_()
            .private_prop("__elephc_uri_handle", t_nullable(TypeExpr::Int), Some(e_null()))
            .private_static_prop("__elephc_charge", t_array(), Some(e_array(vec![])))
            .method(
                method("parse")
                    .static_()
                    .param("uri", TypeExpr::Str)
                    .param_default("baseUrl", t_mixed(), e_null())
                    .param_by_ref_default("errors", None, e_null())
                    .returns(t_nullable(t_class("static")))
                    .body(vec![
                        s_try(vec![
                            s_return(e_new_self(vec![e_var("uri"), e_var("baseUrl"), e_var("errors")])),
                        ], vec![
                            (vec!["\\Uri\\WhatWg\\InvalidUrlException"], Some("e"), vec![
                                s_assign("errors", e_prop(e_var("e"), "errors")),
                                s_return(e_null()),
                            ]),
                        ], None),
                    ]),
            )
            .method(
                method("__construct")
                    .param("uri", TypeExpr::Str)
                    .param_default("baseUrl", t_mixed(), e_null())
                    .param_by_ref_default("softErrors", None, e_null())
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
                                    e_not(e_instance_of(e_var("baseUrl"), "\\Uri\\WhatWg\\Url")),
                                    vec![
                                        s_throw(e_new_fq("TypeError", vec![e_str("Uri\\WhatWg\\Url::__construct(): Argument #2 ($baseUrl) must be of type ?Uri\\WhatWg\\Url")])),
                                    ],
                                    vec![],
                                    None,
                                ),
                                s_assign("base", e_null_coalesce(e_prop(e_var("baseUrl"), "__elephc_uri_handle"), e_int(0))),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("handle", e_call("\\elephc_uri_parse", vec![e_int(1), e_call("\\bin2hex", vec![e_var("uri")]), e_var("base")])),
                        s_if(
                            e_binop(e_var("handle"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_throw(e_new_fq("Uri\\WhatWg\\InvalidUrlException", vec![e_self_call("__elephc_message", vec![]), e_self_call("__elephc_errors", vec![])])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("warnings", e_self_call("__elephc_errors", vec![])),
                        s_assign("softErrors", e_var("warnings")),
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
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_null_coalesce(e_method_call(e_this(), "__elephc_component", vec![e_int(0), e_int(1)]), e_str(""))),
                    ]),
            )
            .method(
                method("withScheme")
                    .param("scheme", TypeExpr::Str)
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(0), e_var("scheme")])),
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
                method("withUsername")
                    .param("username", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(1), e_var("username")])),
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
                method("withPassword")
                    .param("password", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(2), e_var("password")])),
                    ]),
            )
            .method(
                method("getAsciiHost")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(4), e_int(1)])),
                    ]),
            )
            .method(
                method("getUnicodeHost")
                    .returns(t_nullable(TypeExpr::Str))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_component", vec![e_int(4), e_int(2)])),
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
                                s_throw(e_new_fq("Uri\\WhatWg\\InvalidUrlException", vec![e_self_call("__elephc_message", vec![]), e_self_call("__elephc_errors", vec![])])),
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
                method("withFragment")
                    .param("fragment", t_nullable(TypeExpr::Str))
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_with", vec![e_int(8), e_var("fragment")])),
                    ]),
            )
            .method(
                method("equals")
                    .param("url", t_mixed())
                    .param_default("comparisonMode", t_class("\\Uri\\UriComparisonMode"), e_class_const("\\Uri\\UriComparisonMode", "ExcludeFragment"))
                    .returns(TypeExpr::Bool)
                    .body(vec![
                        s_if(
                            e_not(e_instance_of(e_var("url"), "\\Uri\\WhatWg\\Url")),
                            vec![
                                s_throw(e_new_fq("TypeError", vec![e_str("Uri\\WhatWg\\Url::equals(): Argument #1 ($url) must be of type Uri\\WhatWg\\Url")])),
                            ],
                            vec![],
                            None,
                        ),
                        s_assign("result", e_call("\\elephc_uri_equals", vec![e_null_coalesce(e_this_prop("__elephc_uri_handle"), e_int(0)), e_null_coalesce(e_prop(e_var("url"), "__elephc_uri_handle"), e_int(0)), e_ternary(e_binop(e_var("comparisonMode"), BinOp::StrictEq, e_class_const("\\Uri\\UriComparisonMode", "IncludeFragment")), e_int(1), e_int(0))])),
                        s_return(e_binop(e_var("result"), BinOp::StrictNotEq, e_int(0))),
                    ]),
            )
            .method(
                method("toAsciiString")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_to_string", vec![e_int(1), e_bool(false)])),
                    ]),
            )
            .method(
                method("toUnicodeString")
                    .returns(TypeExpr::Str)
                    .body(vec![
                        s_return(e_method_call(e_this(), "__elephc_to_string", vec![e_int(2), e_bool(false)])),
                    ]),
            )
            .method(
                method("resolve")
                    .param("uri", TypeExpr::Str)
                    .param_by_ref_default("softErrors", None, e_null())
                    .returns(t_class("static"))
                    .body(vec![
                        s_return(e_new_self(vec![e_var("uri"), e_this(), e_var("softErrors")])),
                    ]),
            )
            .method(
                method("__serialize")
                    .returns(t_array())
                    .body(vec![
                        s_return(e_array(vec![e_array_assoc(vec![(e_str("uri"), e_method_call(e_this(), "toAsciiString", vec![]))]), e_array(vec![])])),
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
                        s_assign("handle", e_call("\\elephc_uri_parse", vec![e_int(1), e_call("\\bin2hex", vec![e_index(e_var("state"), e_str("uri"))]), e_int(0)])),
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
                        s_return(e_array_assoc(vec![(e_str("scheme"), e_method_call(e_this(), "getScheme", vec![])), (e_str("username"), e_method_call(e_this(), "getUsername", vec![])), (e_str("password"), e_method_call(e_this(), "getPassword", vec![])), (e_str("host"), e_method_call(e_this(), "getAsciiHost", vec![])), (e_str("port"), e_method_call(e_this(), "getPort", vec![])), (e_str("path"), e_method_call(e_this(), "getPath", vec![])), (e_str("query"), e_method_call(e_this(), "getQuery", vec![])), (e_str("fragment"), e_method_call(e_this(), "getFragment", vec![]))])),
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
                                s_throw(e_new_fq("Uri\\WhatWg\\InvalidUrlException", vec![e_self_call("__elephc_message", vec![]), e_self_call("__elephc_errors", vec![])])),
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
                method("__elephc_errors")
                    .private()
                    .static_()
                    .returns(t_array())
                    .body(vec![
                        s_assign("errors", e_array(vec![])),
                        s_assign("count", e_call("\\elephc_uri_error_count", vec![])),
                        s_for(Some(s_assign("index", e_int(0))), Some(e_binop(e_var("index"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("index"))), vec![
                            s_assign("context", e_call("\\elephc_uri_error_context", vec![e_var("index")])),
                            s_array_push("errors", e_new_fq("Uri\\WhatWg\\UrlValidationError", vec![e_ternary(e_binop(e_var("context"), BinOp::StrictEq, e_null()), e_str(""), e_cast(CastType::String, e_call("\\hex2bin", vec![e_var("context")]))), e_self_call("__elephc_type", vec![e_call("\\elephc_uri_error_type", vec![e_var("index")])]), e_binop(e_call("\\elephc_uri_error_failure", vec![e_var("index")]), BinOp::StrictNotEq, e_int(0))])),
                        ]),
                        s_return(e_var("errors")),
                    ]),
            )
            .method(
                method("__elephc_type")
                    .private()
                    .static_()
                    .param("code", TypeExpr::Int)
                    .returns(t_class("\\Uri\\WhatWg\\UrlValidationErrorType"))
                    .body(vec![
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "DomainToAscii")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "DomainToUnicode")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(2)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "DomainInvalidCodePoint")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(3)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "HostInvalidCodePoint")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(4)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4EmptyPart")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(5)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4TooManyParts")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(6)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4NonNumericPart")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(7)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4NonDecimalPart")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(8)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4OutOfRangePart")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(9)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv6Unclosed")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(10)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv6InvalidCompression")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(11)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv6TooManyPieces")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(12)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv6MultipleCompression")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(13)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv6InvalidCodePoint")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(14)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv6TooFewPieces")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(15)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4InIpv6TooManyPieces")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(16)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4InIpv6InvalidCodePoint")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(17)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4InIpv6OutOfRangePart")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(18)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "Ipv4InIpv6TooFewParts")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(19)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "InvalidUrlUnit")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(20)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "SpecialSchemeMissingFollowingSolidus")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(21)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "MissingSchemeNonRelativeUrl")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(22)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "InvalidReverseSoldius")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(23)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "InvalidCredentials")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(24)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "HostMissing")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(25)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "PortOutOfRange")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(26)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "PortInvalid")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(27)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "FileInvalidWindowsDriveLetter")),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("code"), BinOp::StrictEq, e_int(28)),
                            vec![
                                s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "FileInvalidWindowsDriveLetterHost")),
                            ],
                            vec![],
                            None,
                        ),
                        s_return(e_class_const("\\Uri\\WhatWg\\UrlValidationErrorType", "InvalidUrlUnit")),
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
