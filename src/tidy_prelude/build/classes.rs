//! Purpose:
//! The `tidy` and `tidyNode` classes of the tidy prelude, over the bridge handle.
//!
//! Called from:
//! - `crate::tidy_prelude::build::tidy_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::tidy_prelude::fragments`; the oracle
//!   test compares both node by node.
//! - `tidyNode` is final with a private constructor and a minting guard; its readonly
//!   properties are computed eagerly, including the recursive child array.

use crate::parser::ast::*;
use crate::synthetic_class::*;

/// `tidy` — transcribed from the PHP form.
pub(super) fn decl_class_tidy() -> Stmt {
    class("tidy")
        .prop("errorBuffer", t_nullable(TypeExpr::Str), Some(e_null()))
        .prop("value", t_nullable(TypeExpr::Str), Some(e_null()))
        .private_static_prop("__elephc_handles", t_array(), Some(e_array(vec![])))
        .private_static_prop("__elephc_minting", TypeExpr::Bool, Some(e_bool(false)))
        .private_static_prop("__elephc_minted_handle", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .param_default("filename", t_nullable(TypeExpr::Str), e_null())
                .param_default("config", t_mixed(), e_null())
                .param_default("encoding", t_mixed(), e_null())
                .param_default("useIncludePath", TypeExpr::Bool, e_bool(false))
                .body(vec![
                    s_if(
                        e_self_static_prop("__elephc_minting"),
                        vec![
                            s_self_static_prop_array_assign("__elephc_handles", e_call("spl_object_id", vec![e_this()]), e_self_static_prop("__elephc_minted_handle")),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_id", e_call("spl_object_id", vec![e_this()])),
                    s_if(
                        e_call("isset", vec![e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))]),
                        vec![
                            s_assign("_existing", e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))),
                            s_if(
                                e_binop(e_var("_existing"), BinOp::StrictNotEq, e_int(0)),
                                vec![
                                    s_expr(e_call("elephc_tidy_release", vec![e_var("_existing")])),
                                    s_expr(e_call("unset", vec![e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("_handle", e_call("elephc_tidy_create", vec![])),
                    s_if(
                        e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Exception", vec![e_str("Could not create tidy document")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_self_static_prop_array_assign("__elephc_handles", e_call("spl_object_id", vec![e_this()]), e_var("_handle")),
                    s_expr(e_call("__elephc_tidy_load_default_config", vec![e_var("_handle")])),
                    s_if(
                        e_binop(e_var("filename"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("_contents", e_call("__elephc_tidy_read_file", vec![e_var("filename"), e_var("useIncludePath"), e_str("tidy::__construct()"), e_str("construct")])),
                            s_if(
                                e_not(e_call("__elephc_tidy_apply_config", vec![e_var("_handle"), e_var("config"), e_str("tidy::__construct()"), e_bool(true)])),
                                vec![
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_call("__elephc_tidy_parse_into", vec![e_var("_handle"), e_cast(CastType::String, e_var("_contents")), e_var("encoding"), e_str("tidy::__construct()"), e_bool(false)]),
                                vec![
                                    s_expr(e_call("__elephc_tidy_sync_properties", vec![e_this()])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_assign("_id", e_call("spl_object_id", vec![e_this()])),
                    s_if(
                        e_call("isset", vec![e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))]),
                        vec![
                            s_assign("_handle", e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))),
                            s_expr(e_call("unset", vec![e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))])),
                            s_if(
                                e_binop(e_var("_handle"), BinOp::StrictNotEq, e_int(0)),
                                vec![
                                    s_expr(e_call("elephc_tidy_release", vec![e_var("_handle")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__clone")
                .body(vec![
                    s_throw(e_new("Error", vec![e_binop(e_str("Trying to clone an uncloneable object of class "), BinOp::Concat, e_call("get_class", vec![e_this()]))])),
                ]),
        )
        .method(
            method("__toString")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("tidy_get_output", vec![e_this()])),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("errorBuffer"), e_this_prop("errorBuffer")), (e_str("value"), e_this_prop("value"))])),
                ]),
        )
        .method(
            method("__elephc_doc_handle")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("_id", e_call("spl_object_id", vec![e_this()])),
                    s_if(
                        e_not(e_call("isset", vec![e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))])),
                        vec![
                            s_assign("_handle", e_call("elephc_tidy_create", vec![])),
                            s_if(
                                e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_throw(e_new("Exception", vec![e_str("Could not create tidy document")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_self_static_prop_array_assign("__elephc_handles", e_var("_id"), e_var("_handle")),
                            s_expr(e_call("__elephc_tidy_load_default_config", vec![e_var("_handle")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_index(e_self_static_prop("__elephc_handles"), e_var("_id"))),
                ]),
        )
        .method(
            method("__elephc_wrap")
                .static_()
                .param("handle", TypeExpr::Int)
                .returns(t_class("tidy"))
                .body(vec![
                    s_self_static_prop_assign("__elephc_minted_handle", e_var("handle")),
                    s_self_static_prop_assign("__elephc_minting", e_bool(true)),
                    s_assign("_object", e_new_self(vec![])),
                    s_self_static_prop_assign("__elephc_minting", e_bool(false)),
                    s_return(e_var("_object")),
                ]),
        )
        .method(
            method("__elephc_to_int")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("__elephc_to_float")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_return(e_float(0.0)),
                ]),
        )
        .method(
            method("__elephc_to_number")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("__elephc_to_bool")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("getOpt")
                .param("option", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::Int, TypeExpr::Bool]))
                .body(vec![
                    s_return(e_call("__elephc_tidy_get_option", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("option"), e_str("tidy::getOpt()"), e_int(1)])),
                ]),
        )
        .method(
            method("cleanRepair")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_ok", e_binop(e_call("elephc_tidy_clean_repair", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])]), BinOp::StrictNotEq, e_int(0))),
                    s_if(
                        e_var("_ok"),
                        vec![
                            s_expr(e_call("__elephc_tidy_sync_properties", vec![e_this()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_ok")),
                ]),
        )
        .method(
            method("parseFile")
                .param("filename", TypeExpr::Str)
                .param_default("config", t_mixed(), e_null())
                .param_default("encoding", t_mixed(), e_null())
                .param_default("useIncludePath", TypeExpr::Bool, e_bool(false))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_contents", e_call("__elephc_tidy_read_file", vec![e_var("filename"), e_var("useIncludePath"), e_str("tidy::parseFile()"), e_str("parse")])),
                    s_if(
                        e_binop(e_var("_contents"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("__elephc_tidy_apply_config", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("config"), e_str("tidy::parseFile()"), e_bool(false)])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("__elephc_tidy_parse_into", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_cast(CastType::String, e_var("_contents")), e_var("encoding"), e_str("tidy::parseFile()"), e_bool(false)])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_tidy_sync_properties", vec![e_this()])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("parseString")
                .param("string", TypeExpr::Str)
                .param_default("config", t_mixed(), e_null())
                .param_default("encoding", t_mixed(), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("strlen", vec![e_var("string")]), BinOp::Gt, e_int(4294967295)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("tidy::parseString(): Argument #1 ($string) is too long")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("__elephc_tidy_apply_config", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("config"), e_str("tidy::parseString()"), e_bool(false)])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("__elephc_tidy_parse_into", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("string"), e_var("encoding"), e_str("tidy::parseString()"), e_bool(false)])),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("__elephc_tidy_sync_properties", vec![e_this()])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("repairString")
                .static_()
                .param("string", TypeExpr::Str)
                .param_default("config", t_mixed(), e_null())
                .param_default("encoding", t_mixed(), e_null())
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_return(e_call("__elephc_tidy_repair_string", vec![e_var("string"), e_var("config"), e_var("encoding"), e_str("tidy::repairString()")])),
                ]),
        )
        .method(
            method("repairFile")
                .static_()
                .param("filename", TypeExpr::Str)
                .param_default("config", t_mixed(), e_null())
                .param_default("encoding", t_mixed(), e_null())
                .param_default("useIncludePath", TypeExpr::Bool, e_bool(false))
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_return(e_call("__elephc_tidy_repair_file", vec![e_var("filename"), e_var("config"), e_var("encoding"), e_var("useIncludePath"), e_str("tidy::repairFile()")])),
                ]),
        )
        .method(
            method("diagnose")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("_ok", e_binop(e_call("elephc_tidy_diagnose", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])]), BinOp::StrictNotEq, e_int(0))),
                    s_if(
                        e_var("_ok"),
                        vec![
                            s_expr(e_call("__elephc_tidy_sync_properties", vec![e_this()])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("_ok")),
                ]),
        )
        .method(
            method("getRelease")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("elephc_tidy_release_date", vec![])),
                ]),
        )
        .method(
            method("getConfig")
                .returns(t_array())
                .body(vec![
                    s_assign("_config", e_array(vec![])),
                    s_assign("_count", e_call("elephc_tidy_option_count", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])])),
                    s_for(Some(s_assign("_index", e_int(0))), Some(e_binop(e_var("_index"), BinOp::Lt, e_var("_count"))), Some(s_expr(e_post_inc("_index"))), vec![
                        s_assign("_name", e_call("elephc_tidy_option_name", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("_index")])),
                        s_assign("_type", e_call("elephc_tidy_option_type", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("_index")])),
                        s_if(
                            e_binop(e_var("_type"), BinOp::StrictEq, e_int(0)),
                            vec![
                                s_array_assign("_config", e_var("_name"), e_call("elephc_tidy_option_value_str", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("_index")])),
                            ],
                            vec![
                            (e_binop(e_var("_type"), BinOp::StrictEq, e_int(1)), vec![
                                s_array_assign("_config", e_var("_name"), e_call("elephc_tidy_option_value_int", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("_index")])),
                            ]),
                        ],
                            Some(vec![
                            s_array_assign("_config", e_var("_name"), e_binop(e_call("elephc_tidy_option_value_bool", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("_index")]), BinOp::StrictNotEq, e_int(0))),
                        ]),
                        ),
                    ]),
                    s_return(e_var("_config")),
                ]),
        )
        .method(
            method("getStatus")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_call("elephc_tidy_status", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])])),
                ]),
        )
        .method(
            method("getHtmlVer")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_tidy_initialized", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("tidy object is not initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_tidy_html_version", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])])),
                ]),
        )
        .method(
            method("getOptDoc")
                .param("option", TypeExpr::Str)
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_return(e_call("__elephc_tidy_get_option_doc", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![]), e_var("option"), e_str("tidy::getOptDoc()"), e_int(1)])),
                ]),
        )
        .method(
            method("isXhtml")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_tidy_initialized", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("tidy object is not initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_call("elephc_tidy_is_xhtml", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("isXml")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_tidy_initialized", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("tidy object is not initialized")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_binop(e_call("elephc_tidy_is_xml", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("root")
                .returns(t_nullable(t_class("tidyNode")))
                .body(vec![
                    s_assign("_handle", e_call("elephc_tidy_root", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])])),
                    s_return(e_ternary(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), e_null(), e_static_call("tidyNode", "__elephc_wrap", vec![e_var("_handle")]))),
                ]),
        )
        .method(
            method("head")
                .returns(t_nullable(t_class("tidyNode")))
                .body(vec![
                    s_assign("_handle", e_call("elephc_tidy_head", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])])),
                    s_return(e_ternary(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), e_null(), e_static_call("tidyNode", "__elephc_wrap", vec![e_var("_handle")]))),
                ]),
        )
        .method(
            method("html")
                .returns(t_nullable(t_class("tidyNode")))
                .body(vec![
                    s_assign("_handle", e_call("elephc_tidy_html", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])])),
                    s_return(e_ternary(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), e_null(), e_static_call("tidyNode", "__elephc_wrap", vec![e_var("_handle")]))),
                ]),
        )
        .method(
            method("body")
                .returns(t_nullable(t_class("tidyNode")))
                .body(vec![
                    s_assign("_handle", e_call("elephc_tidy_body", vec![e_method_call(e_this(), "__elephc_doc_handle", vec![])])),
                    s_return(e_ternary(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), e_null(), e_static_call("tidyNode", "__elephc_wrap", vec![e_var("_handle")]))),
                ]),
        )
        .build()
}

/// `tidyNode` — transcribed from the PHP form.
pub(super) fn decl_class_tidynode() -> Stmt {
    class("tidyNode")
        .final_()
        .readonly_prop("value", TypeExpr::Str)
        .readonly_prop("name", TypeExpr::Str)
        .readonly_prop("type", TypeExpr::Int)
        .readonly_prop("line", TypeExpr::Int)
        .readonly_prop("column", TypeExpr::Int)
        .readonly_prop("proprietary", TypeExpr::Bool)
        .readonly_prop("id", t_nullable(TypeExpr::Int))
        .readonly_prop("attribute", t_nullable(t_array()))
        .readonly_prop("child", t_nullable(t_array()))
        .private_prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .private_static_prop("__elephc_minting", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .private()
                .param_default("handle", TypeExpr::Int, e_int(0))
                .body(vec![
                    s_if(
                        e_not(e_self_static_prop("__elephc_minting")),
                        vec![
                            s_assign("_frames", e_call("debug_backtrace", vec![e_const("DEBUG_BACKTRACE_IGNORE_ARGS"), e_int(2)])),
                            s_assign("_caller", e_null_coalesce(e_index(e_index(e_var("_frames"), e_int(1)), e_str("class")), e_str(""))),
                            s_assign("_scope", e_ternary(e_binop(e_var("_caller"), BinOp::StrictEq, e_str("")), e_str("global scope"), e_binop(e_str("scope "), BinOp::Concat, e_var("_caller")))),
                            s_throw(e_new("Error", vec![e_binop(e_str("Call to private tidyNode::__construct() from "), BinOp::Concat, e_var("_scope"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_handle", e_var("handle")),
                    s_prop_assign(e_this(), "value", e_call("elephc_tidy_node_text", vec![e_var("handle")])),
                    s_prop_assign(e_this(), "name", e_call("elephc_tidy_node_name", vec![e_var("handle")])),
                    s_prop_assign(e_this(), "type", e_call("elephc_tidy_node_type", vec![e_var("handle")])),
                    s_prop_assign(e_this(), "line", e_call("elephc_tidy_node_line", vec![e_var("handle")])),
                    s_prop_assign(e_this(), "column", e_call("elephc_tidy_node_column", vec![e_var("handle")])),
                    s_prop_assign(e_this(), "proprietary", e_binop(e_call("elephc_tidy_node_proprietary", vec![e_var("handle")]), BinOp::StrictNotEq, e_int(0))),
                    s_assign("_type", e_this_prop("type")),
                    s_if(
                        e_binop(e_binop(e_binop(e_binop(e_var("_type"), BinOp::StrictEq, e_const("TIDY_NODETYPE_ROOT")), BinOp::Or, e_binop(e_var("_type"), BinOp::StrictEq, e_const("TIDY_NODETYPE_DOCTYPE"))), BinOp::Or, e_binop(e_var("_type"), BinOp::StrictEq, e_const("TIDY_NODETYPE_TEXT"))), BinOp::Or, e_binop(e_var("_type"), BinOp::StrictEq, e_const("TIDY_NODETYPE_COMMENT"))),
                        vec![
                            s_prop_assign(e_this(), "id", e_null()),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "id", e_call("elephc_tidy_node_id", vec![e_var("handle")])),
                    ]),
                    ),
                    s_assign("_attribute_count", e_call("elephc_tidy_node_attr_count", vec![e_var("handle")])),
                    s_if(
                        e_binop(e_var("_attribute_count"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_attributes", e_array(vec![])),
                            s_for(Some(s_assign("_index", e_int(0))), Some(e_binop(e_var("_index"), BinOp::Lt, e_var("_attribute_count"))), Some(s_expr(e_post_inc("_index"))), vec![
                                s_assign("_name", e_call("elephc_tidy_node_attr_name", vec![e_var("handle"), e_var("_index")])),
                                s_array_assign("_attributes", e_var("_name"), e_call("elephc_tidy_node_attr_value", vec![e_var("handle"), e_var("_index")])),
                            ]),
                            s_prop_assign(e_this(), "attribute", e_var("_attributes")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "attribute", e_null()),
                    ]),
                    ),
                    s_assign("_child_count", e_call("elephc_tidy_node_child_count", vec![e_var("handle")])),
                    s_if(
                        e_binop(e_var("_child_count"), BinOp::Gt, e_int(0)),
                        vec![
                            s_assign("_children", e_array(vec![])),
                            s_for(Some(s_assign("_index", e_int(0))), Some(e_binop(e_var("_index"), BinOp::Lt, e_var("_child_count"))), Some(s_expr(e_post_inc("_index"))), vec![
                                s_array_push("_children", e_self_call("__elephc_wrap", vec![e_call("elephc_tidy_node_child", vec![e_var("handle"), e_var("_index")])])),
                            ]),
                            s_prop_assign(e_this(), "child", e_var("_children")),
                        ],
                        vec![],
                        Some(vec![
                        s_prop_assign(e_this(), "child", e_null()),
                    ]),
                    ),
                ]),
        )
        .method(
            method("__elephc_wrap")
                .static_()
                .param("handle", TypeExpr::Int)
                .returns(t_class("tidyNode"))
                .body(vec![
                    s_self_static_prop_assign("__elephc_minting", e_bool(true)),
                    s_try(vec![
                        s_return(e_new_self(vec![e_var("handle")])),
                    ], vec![
                    ], Some(vec![
                        s_self_static_prop_assign("__elephc_minting", e_bool(false)),
                    ])),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_assign("_handle", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_var("_handle"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
                            s_expr(e_call("elephc_tidy_node_release", vec![e_var("_handle")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__clone")
                .body(vec![
                    s_throw(e_new("Error", vec![e_binop(e_str("Trying to clone an uncloneable object of class "), BinOp::Concat, e_call("get_class", vec![e_this()]))])),
                ]),
        )
        .method(
            method("__toString")
                .returns(TypeExpr::Str)
                .body(vec![
                    s_return(e_call("elephc_tidy_node_text", vec![e_this_prop("__elephc_handle")])),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_array())
                .body(vec![
                    s_return(e_array_assoc(vec![(e_str("value"), e_this_prop("value")), (e_str("name"), e_this_prop("name")), (e_str("type"), e_this_prop("type")), (e_str("line"), e_this_prop("line")), (e_str("column"), e_this_prop("column")), (e_str("proprietary"), e_this_prop("proprietary")), (e_str("id"), e_this_prop("id")), (e_str("attribute"), e_this_prop("attribute")), (e_str("child"), e_this_prop("child"))])),
                ]),
        )
        .method(
            method("__elephc_to_int")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("__elephc_to_float")
                .returns(TypeExpr::Float)
                .body(vec![
                    s_return(e_float(0.0)),
                ]),
        )
        .method(
            method("__elephc_to_number")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_return(e_int(0)),
                ]),
        )
        .method(
            method("__elephc_to_bool")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("hasChildren")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_has_children", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("hasSiblings")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_has_siblings", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("isComment")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_is_comment", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("isHtml")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_is_html", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("isText")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_is_text", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("isJste")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_is_jste", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("isAsp")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_is_asp", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("isPhp")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_binop(e_call("elephc_tidy_node_is_php", vec![e_this_prop("__elephc_handle")]), BinOp::StrictNotEq, e_int(0))),
                ]),
        )
        .method(
            method("getParent")
                .returns(t_nullable(t_class("tidyNode")))
                .body(vec![
                    s_assign("_handle", e_call("elephc_tidy_node_parent", vec![e_this_prop("__elephc_handle")])),
                    s_return(e_ternary(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), e_null(), e_self_call("__elephc_wrap", vec![e_var("_handle")]))),
                ]),
        )
        .method(
            method("getPreviousSibling")
                .returns(t_nullable(t_class("tidyNode")))
                .body(vec![
                    s_assign("_handle", e_call("elephc_tidy_node_previous", vec![e_this_prop("__elephc_handle")])),
                    s_return(e_ternary(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), e_null(), e_self_call("__elephc_wrap", vec![e_var("_handle")]))),
                ]),
        )
        .method(
            method("getNextSibling")
                .returns(t_nullable(t_class("tidyNode")))
                .body(vec![
                    s_assign("_handle", e_call("elephc_tidy_node_next", vec![e_this_prop("__elephc_handle")])),
                    s_return(e_ternary(e_binop(e_var("_handle"), BinOp::StrictEq, e_int(0)), e_null(), e_self_call("__elephc_wrap", vec![e_var("_handle")]))),
                ]),
        )
        .build()
}
