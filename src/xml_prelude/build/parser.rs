//! Purpose:
//! The `ext/xml` half of the xml prelude: the `XMLParser` class (bridge handle, handler
//! slots, `xml_set_object()` binding, event dispatch and the `xml_parse_into_struct()`
//! accumulator) plus the `xml_*` procedural wrappers and the three `__elephc_xml_*`
//! helpers the `xml_parse_into_struct` registry builtin composes.
//!
//! Called from:
//! - `crate::xml_prelude::build::xml_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::xml_prelude::fragments`; the module
//!   doc of that file explains the dispatch rules mirrored from php-src `compat.c`.
//! - The struct accumulator writes entries by explicit index and updates them through
//!   read-modify-write sequences, the array-property shapes elephc-PHP compiles today.

use crate::parser::ast::{BinOp, CastType, Stmt, TypeExpr};
use crate::synthetic_class::{class, e_array, e_array_assoc, e_binop, e_bool, e_call, e_cast, e_index, e_int, e_method_call, e_new, e_new_self, e_not, e_null, e_null_coalesce, e_post_inc, e_prop, e_self_static_prop, e_static_call, e_str, e_ternary, e_this, e_this_prop, e_var, function, method, s_array_assign, s_array_push, s_assign, s_break, s_expr, s_for, s_foreach, s_if, s_prop_array_assign, s_prop_assign, s_return, s_return_void, s_self_static_prop_assign, s_throw, s_try, s_while, t_array, t_class, t_mixed, t_nullable};

/// `XMLParser` — transcribed from the PHP form.
pub(super) fn decl_class_xmlparser() -> Stmt {
    class("XMLParser")
        .final_()
        .private_static_prop("__elephc_minting", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_object", t_mixed(), Some(e_null()))
        .prop("__elephc_start_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_end_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_cdata_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_pi_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_default_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_unparsed_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_notation_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_extref_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_start_ns_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_end_ns_handler", t_mixed(), Some(e_null()))
        .prop("__elephc_start_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_end_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_cdata_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_pi_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_default_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_unparsed_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_notation_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_extref_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_start_ns_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_end_ns_method", TypeExpr::Str, Some(e_str("")))
        .prop("__elephc_element_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_cdata_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_pi_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_default_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_unparsed_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_notation_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_extref_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_start_ns_installed", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_skip_tagstart", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_skip_white", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_parsing", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_level", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_collecting", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_collect_index", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_struct_values", t_array(), Some(e_array(vec![])))
        .prop("__elephc_struct_index", t_array(), Some(e_array(vec![])))
        .prop("__elephc_ltags", t_array(), Some(e_array(vec![])))
        .prop("__elephc_lastwasopen", TypeExpr::Bool, Some(e_bool(false)))
        .prop("__elephc_ctag_index", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_struct_next", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_curtag", TypeExpr::Int, Some(e_int(0)))
        .method(
            method("__construct")
                .body(vec![
                    s_if(
                        e_not(e_self_static_prop("__elephc_minting")),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Cannot directly construct XMLParser, use xml_parser_create() or xml_parser_create_ns() instead")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__destruct")
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_var("raw"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
                            s_expr(e_call("elephc_xml_parser_free", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__clone")
                .final_()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
                    s_throw(e_new("Error", vec![e_binop(e_str("Trying to clone an uncloneable object of class "), BinOp::Concat, e_call("get_class", vec![e_this()]))])),
                ]),
        )
        .method(
            method("__debugInfo")
                .returns(t_array())
                .body(vec![
                    s_return(e_array(vec![])),
                ]),
        )
        .method(
            method("__serialize")
                .returns(t_array())
                .body(vec![
                    s_throw(e_new("Exception", vec![e_str("Serialization of 'XMLParser' is not allowed")])),
                ]),
        )
        .method(
            method("__unserialize")
                .param("data", t_array())
                .keep_unread_params()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_throw(e_new("Exception", vec![e_str("Unserialization of 'XMLParser' is not allowed")])),
                ]),
        )
        .method(
            method("__elephc_create")
                .static_()
                .param("function", TypeExpr::Str)
                .param("encoding", t_nullable(TypeExpr::Str))
                .param("namespaces", TypeExpr::Bool)
                .param("separator", TypeExpr::Str)
                .returns(t_class("XMLParser"))
                .body(vec![
                    s_assign("target", e_null_coalesce(e_var("encoding"), e_str(""))),
                    s_if(
                        e_binop(e_binop(e_var("target"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_binop(e_call("elephc_xml_encoding_supported", vec![e_var("target")]), BinOp::StrictEq, e_int(0))),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #1 ($encoding) is not a supported source encoding"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_self_static_prop_assign("__elephc_minting", e_bool(true)),
                    s_assign("parser", e_new_self(vec![])),
                    s_self_static_prop_assign("__elephc_minting", e_bool(false)),
                    s_assign("raw", e_call("elephc_xml_parser_create", vec![e_ternary(e_var("namespaces"), e_int(1), e_int(0)), e_call("substr", vec![e_var("separator"), e_int(0), e_int(1)])])),
                    s_prop_assign(e_var("parser"), "__elephc_handle", e_var("raw")),
                    s_if(
                        e_binop(e_var("target"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_expr(e_call("elephc_xml_parser_set_target_encoding", vec![e_var("raw"), e_var("target")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("parser")),
                ]),
        )
        .method(
            method("__elephc_bind_handler")
                .param("handler", t_mixed())
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Int)
                .param("parameter", TypeExpr::Str)
                .returns(t_array())
                .body(vec![
                    s_if(
                        e_binop(e_binop(e_var("handler"), BinOp::StrictEq, e_null()), BinOp::Or, e_binop(e_var("handler"), BinOp::StrictEq, e_str(""))),
                        vec![
                            s_return(e_array(vec![e_null(), e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_call("is_callable", vec![e_var("handler")]),
                        vec![
                            s_return(e_array(vec![e_var("handler"), e_str("")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_call("is_int", vec![e_var("handler")]), BinOp::Or, e_call("is_float", vec![e_var("handler")])), BinOp::Or, e_call("is_bool", vec![e_var("handler")])),
                        vec![
                            s_assign("handler", e_cast(CastType::String, e_var("handler"))),
                            s_if(
                                e_binop(e_var("handler"), BinOp::StrictEq, e_str("")),
                                vec![
                                    s_return(e_array(vec![e_null(), e_str("")])),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("is_string", vec![e_var("handler")])),
                        vec![
                            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("argument")), BinOp::Concat, e_str(" ($")), BinOp::Concat, e_var("parameter")), BinOp::Concat, e_str(") must be of type callable|string|null"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("object", e_this_prop("__elephc_object")),
                    s_if(
                        e_binop(e_var("object"), BinOp::StrictEq, e_null()),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("argument")), BinOp::Concat, e_str(" ($")), BinOp::Concat, e_var("parameter")), BinOp::Concat, e_str(") an object must be set via xml_set_object() to be able to lookup method"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_call("is_callable", vec![e_array(vec![e_var("object"), e_var("handler")])])),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #")), BinOp::Concat, e_var("argument")), BinOp::Concat, e_str(" ($")), BinOp::Concat, e_var("parameter")), BinOp::Concat, e_str(") method ")), BinOp::Concat, e_call("get_class", vec![e_var("object")])), BinOp::Concat, e_str("::")), BinOp::Concat, e_var("handler")), BinOp::Concat, e_str("() does not exist"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_array(vec![e_array(vec![e_var("object"), e_var("handler")]), e_var("handler")])),
                ]),
        )
        .method(
            method("__elephc_set_object")
                .param("object", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_not(e_call("is_object", vec![e_var("object")])),
                        vec![
                            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("xml_set_object(): Argument #2 ($object) must be of type object, "), BinOp::Concat, e_call("gettype", vec![e_var("object")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("methods", e_array(vec![e_this_prop("__elephc_start_method"), e_this_prop("__elephc_end_method"), e_this_prop("__elephc_cdata_method"), e_this_prop("__elephc_pi_method"), e_this_prop("__elephc_default_method"), e_this_prop("__elephc_unparsed_method"), e_this_prop("__elephc_notation_method"), e_this_prop("__elephc_extref_method"), e_this_prop("__elephc_start_ns_method"), e_this_prop("__elephc_end_ns_method")])),
                    s_assign("setters", e_array(vec![e_str("xml_set_element_handler"), e_str("xml_set_element_handler"), e_str("xml_set_character_data_handler"), e_str("xml_set_processing_instruction_handler"), e_str("xml_set_default_handler"), e_str("xml_set_unparsed_entity_decl_handler"), e_str("xml_set_notation_decl_handler"), e_str("xml_set_external_entity_ref_handler"), e_str("xml_set_start_namespace_decl_handler"), e_str("xml_set_end_namespace_decl_handler")])),
                    s_foreach(e_var("methods"), Some("slot"), "method", vec![
                        s_if(
                            e_binop(e_binop(e_var("method"), BinOp::StrictNotEq, e_str("")), BinOp::And, e_not(e_call("is_callable", vec![e_array(vec![e_var("object"), e_var("method")])]))),
                            vec![
                                s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_str("xml_set_object(): Argument #2 ($object) cannot safely swap to object of class "), BinOp::Concat, e_call("get_class", vec![e_var("object")])), BinOp::Concat, e_str(" as method \"")), BinOp::Concat, e_var("method")), BinOp::Concat, e_str("\" does not exist, which was set via ")), BinOp::Concat, e_index(e_var("setters"), e_var("slot"))), BinOp::Concat, e_str("()"))])),
                            ],
                            vec![],
                            None,
                        ),
                    ]),
                    s_prop_assign(e_this(), "__elephc_object", e_var("object")),
                    s_if(
                        e_binop(e_this_prop("__elephc_start_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_start_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_start_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_end_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_end_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_end_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_cdata_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_cdata_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_cdata_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_pi_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_pi_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_pi_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_default_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_default_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_default_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_unparsed_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_unparsed_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_unparsed_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_notation_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_notation_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_notation_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_extref_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_extref_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_extref_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_start_ns_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_start_ns_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_start_ns_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_end_ns_method"), BinOp::StrictNotEq, e_str("")),
                        vec![
                            s_prop_assign(e_this(), "__elephc_end_ns_handler", e_array(vec![e_var("object"), e_this_prop("__elephc_end_ns_method")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_element_handler")
                .param("start_handler", t_mixed())
                .param("end_handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("start", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("start_handler"), e_str("xml_set_element_handler"), e_int(2), e_str("start_handler")])),
                    s_assign("end", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("end_handler"), e_str("xml_set_element_handler"), e_int(3), e_str("end_handler")])),
                    s_prop_assign(e_this(), "__elephc_start_handler", e_index(e_var("start"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_start_method", e_cast(CastType::String, e_index(e_var("start"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_end_handler", e_index(e_var("end"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_end_method", e_cast(CastType::String, e_index(e_var("end"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_element_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_character_data_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_character_data_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_cdata_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_cdata_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_cdata_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_processing_instruction_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_processing_instruction_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_pi_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_pi_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_pi_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_default_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_default_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_default_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_default_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_default_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_unparsed_entity_decl_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_unparsed_entity_decl_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_unparsed_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_unparsed_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_unparsed_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_notation_decl_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_notation_decl_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_notation_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_notation_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_notation_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_external_entity_ref_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_external_entity_ref_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_extref_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_extref_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_extref_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_start_namespace_decl_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_start_namespace_decl_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_start_ns_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_start_ns_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_prop_assign(e_this(), "__elephc_start_ns_installed", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_end_namespace_decl_handler")
                .param("handler", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("bound", e_method_call(e_this(), "__elephc_bind_handler", vec![e_var("handler"), e_str("xml_set_end_namespace_decl_handler"), e_int(2), e_str("handler")])),
                    s_prop_assign(e_this(), "__elephc_end_ns_handler", e_index(e_var("bound"), e_int(0))),
                    s_prop_assign(e_this(), "__elephc_end_ns_method", e_cast(CastType::String, e_index(e_var("bound"), e_int(1)))),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_set_option")
                .param("option", TypeExpr::Int)
                .param("value", t_mixed())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_expr(e_call("elephc_xml_parser_set_option", vec![e_var("raw"), e_int(1), e_ternary(e_var("value"), e_int(1), e_int(0))])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_prop_assign(e_this(), "__elephc_skip_white", e_cast(CastType::Bool, e_var("value"))),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(5)),
                        vec![
                            s_if(
                                e_this_prop("__elephc_parsing"),
                                vec![
                                    s_throw(e_new("Error", vec![e_str("Cannot change option XML_OPTION_PARSE_HUGE while parsing")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_call("elephc_xml_parser_set_option", vec![e_var("raw"), e_int(5), e_ternary(e_var("value"), e_int(1), e_int(0))])),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_assign("offset", e_cast(CastType::Int, e_var("value"))),
                            s_if(
                                e_binop(e_binop(e_var("offset"), BinOp::Lt, e_int(0)), BinOp::Or, e_binop(e_var("offset"), BinOp::Gt, e_int(2147483647))),
                                vec![
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "__elephc_skip_tagstart", e_var("offset")),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_assign("name", e_cast(CastType::String, e_var("value"))),
                            s_if(
                                e_binop(e_call("elephc_xml_parser_set_target_encoding", vec![e_var("raw"), e_var("name")]), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_throw(e_new("ValueError", vec![e_str("xml_parser_set_option(): Argument #3 ($value) is not a supported target encoding")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("ValueError", vec![e_str("xml_parser_set_option(): Argument #2 ($option) must be a XML_OPTION_* constant")])),
                ]),
        )
        .method(
            method("__elephc_get_option")
                .param("option", TypeExpr::Int)
                .returns(t_mixed())
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_return(e_binop(e_call("elephc_xml_parser_get_option", vec![e_var("raw"), e_int(1)]), BinOp::StrictEq, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(3)),
                        vec![
                            s_return(e_this_prop("__elephc_skip_tagstart")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(4)),
                        vec![
                            s_return(e_this_prop("__elephc_skip_white")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(5)),
                        vec![
                            s_return(e_binop(e_call("elephc_xml_parser_get_option", vec![e_var("raw"), e_int(5)]), BinOp::StrictEq, e_int(1))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("option"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_return(e_call("elephc_xml_parser_target_encoding", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_throw(e_new("ValueError", vec![e_str("xml_parser_get_option(): Argument #2 ($option) must be a XML_OPTION_* constant")])),
                ]),
        )
        .method(
            method("__elephc_error_code")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_parser_error_code", vec![e_var("raw")])),
                ]),
        )
        .method(
            method("__elephc_line")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_parser_line", vec![e_var("raw")])),
                ]),
        )
        .method(
            method("__elephc_column")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_parser_column", vec![e_var("raw")])),
                ]),
        )
        .method(
            method("__elephc_byte_index")
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_return(e_call("elephc_xml_parser_byte_index", vec![e_var("raw")])),
                ]),
        )
        .method(
            method("__elephc_parse")
                .param("data", TypeExpr::Str)
                .param("is_final", TypeExpr::Bool)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_this_prop("__elephc_parsing"),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Parser must not be called recursively")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_expr(e_call("elephc_xml_parser_feed", vec![e_var("raw"), e_var("data"), e_call("strlen", vec![e_var("data")]), e_ternary(e_var("is_final"), e_int(1), e_int(0))])),
                    s_prop_assign(e_this(), "__elephc_parsing", e_bool(true)),
                    s_assign("completed", e_bool(false)),
                    s_try(vec![
                        s_expr(e_method_call(e_this(), "__elephc_drain", vec![])),
                        s_assign("completed", e_bool(true)),
                    ], vec![
                    ], Some(vec![
                        s_prop_assign(e_this(), "__elephc_parsing", e_bool(false)),
                        s_if(
                            e_not(e_var("completed")),
                            vec![
                                s_expr(e_method_call(e_this(), "__elephc_discard", vec![])),
                            ],
                            vec![],
                            None,
                        ),
                    ])),
                    s_return(e_call("elephc_xml_parser_well_formed", vec![e_var("raw")])),
                ]),
        )
        .method(
            method("__elephc_discard")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_while(e_binop(e_call("elephc_xml_parser_next", vec![e_var("raw")]), BinOp::Gt, e_int(0)), vec![]),
                ]),
        )
        .method(
            method("__elephc_parse_into_struct")
                .param("data", TypeExpr::Str)
                .param("with_index", TypeExpr::Bool)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_if(
                        e_this_prop("__elephc_parsing"),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Parser must not be called recursively")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_collecting", e_bool(true)),
                    s_prop_assign(e_this(), "__elephc_collect_index", e_var("with_index")),
                    s_prop_assign(e_this(), "__elephc_struct_values", e_array(vec![])),
                    s_prop_assign(e_this(), "__elephc_struct_index", e_array(vec![])),
                    s_prop_assign(e_this(), "__elephc_ltags", e_array(vec![])),
                    s_prop_assign(e_this(), "__elephc_level", e_int(0)),
                    s_prop_assign(e_this(), "__elephc_lastwasopen", e_bool(false)),
                    s_prop_assign(e_this(), "__elephc_ctag_index", e_int(0)),
                    s_prop_assign(e_this(), "__elephc_struct_next", e_int(0)),
                    s_prop_assign(e_this(), "__elephc_element_installed", e_bool(true)),
                    s_prop_assign(e_this(), "__elephc_cdata_installed", e_bool(true)),
                    s_try(vec![
                        s_assign("status", e_method_call(e_this(), "__elephc_parse", vec![e_var("data"), e_bool(true)])),
                    ], vec![
                    ], Some(vec![
                        s_prop_assign(e_this(), "__elephc_collecting", e_bool(false)),
                    ])),
                    s_return(e_var("status")),
                ]),
        )
        .method(
            method("__elephc_struct_values")
                .returns(t_mixed())
                .body(vec![
                    s_assign("values", e_call("array_values", vec![e_this_prop("__elephc_struct_values")])),
                    s_prop_assign(e_this(), "__elephc_struct_values", e_array(vec![])),
                    s_return(e_var("values")),
                ]),
        )
        .method(
            method("__elephc_struct_index")
                .returns(t_mixed())
                .body(vec![
                    s_assign("index", e_this_prop("__elephc_struct_index")),
                    s_prop_assign(e_this(), "__elephc_struct_index", e_array(vec![])),
                    s_return(e_var("index")),
                ]),
        )
        .method(
            method("__elephc_strip")
                .private()
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("offset", e_this_prop("__elephc_skip_tagstart")),
                    s_if(
                        e_binop(e_var("offset"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_return(e_call("substr", vec![e_var("name"), e_int(0)])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("offset"), BinOp::GtEq, e_call("strlen", vec![e_var("name")])),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("substr", vec![e_var("name"), e_var("offset")])),
                ]),
        )
        .method(
            method("__elephc_add_to_info")
                .private()
                .param("tag", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_this_prop("__elephc_collect_index"),
                        vec![
                            s_assign("positions", e_null_coalesce(e_index(e_this_prop("__elephc_struct_index"), e_var("tag")), e_array(vec![]))),
                            s_array_push("positions", e_this_prop("__elephc_curtag")),
                            s_prop_array_assign(e_this(), "__elephc_struct_index", e_var("tag"), e_var("positions")),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_curtag", e_binop(e_this_prop("__elephc_curtag"), BinOp::Add, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_append_entry")
                .private()
                .param("entry", t_array())
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("next", e_this_prop("__elephc_struct_next")),
                    s_prop_array_assign(e_this(), "__elephc_struct_values", e_var("next"), e_var("entry")),
                    s_prop_assign(e_this(), "__elephc_struct_next", e_binop(e_var("next"), BinOp::Add, e_int(1))),
                    s_return(e_var("next")),
                ]),
        )
        .method(
            method("__elephc_drain")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_while(e_bool(true), vec![
                        s_assign("kind", e_call("elephc_xml_parser_next", vec![e_var("raw")])),
                        s_if(
                            e_binop(e_var("kind"), BinOp::LtEq, e_int(0)),
                            vec![
                                s_return_void(),
                            ],
                            vec![],
                            None,
                        ),
                        s_if(
                            e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)),
                            vec![
                                s_expr(e_method_call(e_this(), "__elephc_on_start", vec![e_var("raw")])),
                            ],
                            vec![
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)), vec![
                                s_expr(e_method_call(e_this(), "__elephc_on_end", vec![e_var("raw")])),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(3)), vec![
                                s_expr(e_method_call(e_this(), "__elephc_on_characters", vec![e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])])),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(4)), vec![
                                s_expr(e_method_call(e_this(), "__elephc_on_pi", vec![e_var("raw")])),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(5)), vec![
                                s_if(
                                    e_this_prop("__elephc_default_installed"),
                                    vec![
                                        s_expr(e_method_call(e_this(), "__elephc_call_default", vec![e_binop(e_binop(e_str("<!--"), BinOp::Concat, e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])), BinOp::Concat, e_str("-->"))])),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(6)), vec![
                                s_if(
                                    e_not(e_method_call(e_this(), "__elephc_on_entity_ref", vec![e_var("raw")])),
                                    vec![
                                        s_return_void(),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(7)), vec![
                                s_expr(e_method_call(e_this(), "__elephc_on_notation_decl", vec![e_var("raw")])),
                            ]),
                            (e_binop(e_var("kind"), BinOp::StrictEq, e_int(8)), vec![
                                s_expr(e_method_call(e_this(), "__elephc_on_unparsed_entity_decl", vec![e_var("raw")])),
                            ]),
                        ],
                            None,
                        ),
                    ]),
                ]),
        )
        .method(
            method("__elephc_call_default")
                .private()
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handler", e_this_prop("__elephc_default_handler")),
                    s_if(
                        e_binop(e_var("handler"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("data")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_on_start")
                .private()
                .param("raw", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("ns_count", e_call("elephc_xml_parser_event_int", vec![e_var("raw"), e_int(1)])),
                    s_if(
                        e_binop(e_binop(e_var("ns_count"), BinOp::Gt, e_int(0)), BinOp::And, e_this_prop("__elephc_start_ns_installed")),
                        vec![
                            s_assign("handler", e_this_prop("__elephc_start_ns_handler")),
                            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("ns_count"))), Some(s_expr(e_post_inc("i"))), vec![
                                s_if(
                                    e_binop(e_var("handler"), BinOp::StrictNotEq, e_null()),
                                    vec![
                                        s_if(
                                            e_binop(e_call("elephc_xml_parser_event_ns_has_prefix", vec![e_var("raw"), e_var("i")]), BinOp::StrictEq, e_int(1)),
                                            vec![
                                                s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_call("elephc_xml_parser_event_ns_prefix", vec![e_var("raw"), e_var("i")]), e_call("elephc_xml_parser_event_ns_uri", vec![e_var("raw"), e_var("i")])])),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_bool(false), e_call("elephc_xml_parser_event_ns_uri", vec![e_var("raw"), e_var("i")])])),
                                        ]),
                                        ),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("__elephc_element_installed")),
                        vec![
                            s_if(
                                e_this_prop("__elephc_default_installed"),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_call_default", vec![e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)])])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_level", e_binop(e_this_prop("__elephc_level"), BinOp::Add, e_int(1))),
                    s_assign("name", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])),
                    s_assign("stripped", e_method_call(e_this(), "__elephc_strip", vec![e_var("name")])),
                    s_assign("attributes", e_array(vec![])),
                    s_assign("count", e_call("elephc_xml_parser_event_int", vec![e_var("raw"), e_int(0)])),
                    s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("count"))), Some(s_expr(e_post_inc("i"))), vec![
                        s_array_assign("attributes", e_call("elephc_xml_parser_event_attr_name", vec![e_var("raw"), e_var("i")]), e_call("elephc_xml_parser_event_attr_value", vec![e_var("raw"), e_var("i")])),
                    ]),
                    s_assign("handler", e_this_prop("__elephc_start_handler")),
                    s_if(
                        e_binop(e_var("handler"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_assign("attribute_map", e_var("attributes")),
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("stripped"), e_var("attribute_map")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_this_prop("__elephc_collecting"),
                        vec![
                            s_assign("level", e_this_prop("__elephc_level")),
                            s_if(
                                e_binop(e_var("level"), BinOp::LtEq, e_int(255)),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_add_to_info", vec![e_var("stripped")])),
                                    s_assign("tag", e_array_assoc(vec![(e_str("tag"), e_var("stripped")), (e_str("type"), e_str("open")), (e_str("level"), e_var("level"))])),
                                    s_prop_array_assign(e_this(), "__elephc_ltags", e_binop(e_var("level"), BinOp::Sub, e_int(1)), e_var("name")),
                                    s_prop_assign(e_this(), "__elephc_lastwasopen", e_bool(true)),
                                    s_if(
                                        e_binop(e_var("count"), BinOp::Gt, e_int(0)),
                                        vec![
                                            s_array_assign("tag", e_str("attributes"), e_var("attributes")),
                                        ],
                                        vec![],
                                        None,
                                    ),
                                    s_prop_assign(e_this(), "__elephc_ctag_index", e_method_call(e_this(), "__elephc_append_entry", vec![e_var("tag")])),
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
            method("__elephc_on_end")
                .private()
                .param("raw", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_this_prop("__elephc_element_installed")),
                        vec![
                            s_if(
                                e_this_prop("__elephc_default_installed"),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_call_default", vec![e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)])])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("name", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])),
                    s_assign("stripped", e_method_call(e_this(), "__elephc_strip", vec![e_var("name")])),
                    s_assign("handler", e_this_prop("__elephc_end_handler")),
                    s_if(
                        e_binop(e_var("handler"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("stripped")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_this_prop("__elephc_collecting"),
                        vec![
                            s_if(
                                e_this_prop("__elephc_lastwasopen"),
                                vec![
                                    s_assign("index", e_this_prop("__elephc_ctag_index")),
                                    s_assign("entry", e_index(e_this_prop("__elephc_struct_values"), e_var("index"))),
                                    s_array_assign("entry", e_str("type"), e_str("complete")),
                                    s_prop_array_assign(e_this(), "__elephc_struct_values", e_var("index"), e_var("entry")),
                                ],
                                vec![],
                                Some(vec![
                                s_expr(e_method_call(e_this(), "__elephc_add_to_info", vec![e_var("stripped")])),
                                s_expr(e_method_call(e_this(), "__elephc_append_entry", vec![e_array_assoc(vec![(e_str("tag"), e_var("stripped")), (e_str("type"), e_str("close")), (e_str("level"), e_this_prop("__elephc_level"))])])),
                            ]),
                            ),
                            s_prop_assign(e_this(), "__elephc_lastwasopen", e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_level", e_binop(e_this_prop("__elephc_level"), BinOp::Sub, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_on_characters")
                .private()
                .param("data", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_this_prop("__elephc_cdata_installed")),
                        vec![
                            s_if(
                                e_this_prop("__elephc_default_installed"),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_call_default", vec![e_var("data")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handler", e_this_prop("__elephc_cdata_handler")),
                    s_if(
                        e_binop(e_var("handler"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("data")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_not(e_this_prop("__elephc_collecting")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("doprint", e_bool(false)),
                    s_if(
                        e_this_prop("__elephc_skip_white"),
                        vec![
                            s_assign("length", e_call("strlen", vec![e_var("data")])),
                            s_for(Some(s_assign("i", e_int(0))), Some(e_binop(e_var("i"), BinOp::Lt, e_var("length"))), Some(s_expr(e_post_inc("i"))), vec![
                                s_assign("byte", e_index(e_var("data"), e_var("i"))),
                                s_if(
                                    e_binop(e_binop(e_binop(e_var("byte"), BinOp::StrictNotEq, e_str(" ")), BinOp::And, e_binop(e_var("byte"), BinOp::StrictNotEq, e_str("\t"))), BinOp::And, e_binop(e_var("byte"), BinOp::StrictNotEq, e_str("\n"))),
                                    vec![
                                        s_assign("doprint", e_bool(true)),
                                        s_break(1),
                                    ],
                                    vec![],
                                    None,
                                ),
                            ]),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("keep", e_binop(e_var("doprint"), BinOp::Or, e_not(e_this_prop("__elephc_skip_white")))),
                    s_if(
                        e_this_prop("__elephc_lastwasopen"),
                        vec![
                            s_assign("index", e_this_prop("__elephc_ctag_index")),
                            s_assign("entry", e_index(e_this_prop("__elephc_struct_values"), e_var("index"))),
                            s_if(
                                e_call("isset", vec![e_index(e_var("entry"), e_str("value"))]),
                                vec![
                                    s_array_assign("entry", e_str("value"), e_binop(e_index(e_var("entry"), e_str("value")), BinOp::Concat, e_var("data"))),
                                    s_prop_array_assign(e_this(), "__elephc_struct_values", e_var("index"), e_var("entry")),
                                ],
                                vec![
                                (e_var("keep"), vec![
                                    s_array_assign("entry", e_str("value"), e_var("data")),
                                    s_prop_array_assign(e_this(), "__elephc_struct_values", e_var("index"), e_var("entry")),
                                ]),
                            ],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("last", e_binop(e_this_prop("__elephc_struct_next"), BinOp::Sub, e_int(1))),
                    s_if(
                        e_binop(e_var("last"), BinOp::GtEq, e_int(0)),
                        vec![
                            s_assign("previous", e_index(e_this_prop("__elephc_struct_values"), e_var("last"))),
                            s_if(
                                e_binop(e_index(e_var("previous"), e_str("type")), BinOp::StrictEq, e_str("cdata")),
                                vec![
                                    s_array_assign("previous", e_str("value"), e_binop(e_index(e_var("previous"), e_str("value")), BinOp::Concat, e_var("data"))),
                                    s_prop_array_assign(e_this(), "__elephc_struct_values", e_var("last"), e_var("previous")),
                                    s_return_void(),
                                ],
                                vec![],
                                None,
                            ),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("level", e_this_prop("__elephc_level")),
                    s_if(
                        e_binop(e_binop(e_binop(e_var("level"), BinOp::LtEq, e_int(255)), BinOp::And, e_binop(e_var("level"), BinOp::Gt, e_int(0))), BinOp::And, e_var("keep")),
                        vec![
                            s_assign("stripped", e_method_call(e_this(), "__elephc_strip", vec![e_index(e_this_prop("__elephc_ltags"), e_binop(e_var("level"), BinOp::Sub, e_int(1)))])),
                            s_expr(e_method_call(e_this(), "__elephc_add_to_info", vec![e_var("stripped")])),
                            s_expr(e_method_call(e_this(), "__elephc_append_entry", vec![e_array_assoc(vec![(e_str("tag"), e_var("stripped")), (e_str("value"), e_var("data")), (e_str("type"), e_str("cdata")), (e_str("level"), e_var("level"))])])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_on_pi")
                .private()
                .param("raw", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("target", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])),
                    s_assign("has_data", e_binop(e_binop(e_call("elephc_xml_parser_event_int", vec![e_var("raw"), e_int(3)]), BinOp::BitAnd, e_int(1)), BinOp::StrictEq, e_int(1))),
                    s_if(
                        e_not(e_this_prop("__elephc_pi_installed")),
                        vec![
                            s_if(
                                e_this_prop("__elephc_default_installed"),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_call_default", vec![e_binop(e_binop(e_binop(e_binop(e_str("<?"), BinOp::Concat, e_var("target")), BinOp::Concat, e_str(" ")), BinOp::Concat, e_ternary(e_var("has_data"), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)]), e_str("(null)"))), BinOp::Concat, e_str("?>"))])),
                                ],
                                vec![],
                                None,
                            ),
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handler", e_this_prop("__elephc_pi_handler")),
                    s_if(
                        e_binop(e_var("handler"), BinOp::StrictNotEq, e_null()),
                        vec![
                            s_if(
                                e_var("has_data"),
                                vec![
                                    s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("target"), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)])])),
                                ],
                                vec![],
                                Some(vec![
                                s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("target"), e_bool(false)])),
                            ]),
                            ),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_on_entity_ref")
                .private()
                .param("raw", TypeExpr::Int)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("kind", e_call("elephc_xml_parser_event_int", vec![e_var("raw"), e_int(2)])),
                    s_assign("name", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])),
                    s_if(
                        e_binop(e_var("kind"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_if(
                                e_not(e_this_prop("__elephc_extref_installed")),
                                vec![
                                    s_return(e_bool(true)),
                                ],
                                vec![],
                                None,
                            ),
                            s_assign("continue", e_int(0)),
                            s_assign("handler", e_this_prop("__elephc_extref_handler")),
                            s_if(
                                e_binop(e_var("handler"), BinOp::StrictNotEq, e_null()),
                                vec![
                                    s_assign("flags", e_call("elephc_xml_parser_event_int", vec![e_var("raw"), e_int(3)])),
                                    s_assign("system_id", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)])),
                                    s_assign("returned", e_bool(false)),
                                    s_try(vec![
                                        s_if(
                                            e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::StrictEq, e_int(2)),
                                            vec![
                                                s_assign("result", e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_str(""), e_var("system_id"), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(2)])])),
                                            ],
                                            vec![],
                                            Some(vec![
                                            s_assign("result", e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_str(""), e_var("system_id"), e_bool(false)])),
                                        ]),
                                        ),
                                        s_assign("returned", e_bool(true)),
                                        s_assign("continue", e_cast(CastType::Int, e_var("result"))),
                                    ], vec![
                                    ], Some(vec![
                                        s_if(
                                            e_not(e_var("returned")),
                                            vec![
                                                s_expr(e_call("elephc_xml_parser_stop", vec![e_var("raw"), e_int(21)])),
                                            ],
                                            vec![],
                                            None,
                                        ),
                                    ])),
                                ],
                                vec![],
                                None,
                            ),
                            s_if(
                                e_binop(e_var("continue"), BinOp::StrictEq, e_int(0)),
                                vec![
                                    s_expr(e_call("elephc_xml_parser_stop", vec![e_var("raw"), e_int(21)])),
                                    s_return(e_bool(false)),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("predefined", e_binop(e_var("kind"), BinOp::StrictEq, e_int(0))),
                    s_assign("expandable", e_binop(e_binop(e_var("kind"), BinOp::StrictEq, e_int(0)), BinOp::Or, e_binop(e_var("kind"), BinOp::StrictEq, e_int(1)))),
                    s_if(
                        e_binop(e_this_prop("__elephc_default_installed"), BinOp::And, e_not(e_binop(e_var("predefined"), BinOp::And, e_this_prop("__elephc_cdata_installed")))),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_call_default", vec![e_binop(e_binop(e_str("&"), BinOp::Concat, e_var("name")), BinOp::Concat, e_str(";"))])),
                        ],
                        vec![
                        (e_binop(e_this_prop("__elephc_cdata_installed"), BinOp::And, e_var("expandable")), vec![
                            s_expr(e_method_call(e_this(), "__elephc_on_characters", vec![e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)])])),
                        ]),
                    ],
                        None,
                    ),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("__elephc_on_notation_decl")
                .private()
                .param("raw", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_this_prop("__elephc_notation_installed")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handler", e_this_prop("__elephc_notation_handler")),
                    s_if(
                        e_binop(e_var("handler"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("flags", e_call("elephc_xml_parser_event_int", vec![e_var("raw"), e_int(3)])),
                    s_assign("name", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])),
                    s_assign("has_system", e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(4)), BinOp::StrictEq, e_int(4))),
                    s_assign("has_public", e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::StrictEq, e_int(2))),
                    s_if(
                        e_binop(e_var("has_system"), BinOp::And, e_var("has_public")),
                        vec![
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_bool(false), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)]), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(2)])])),
                        ],
                        vec![
                        (e_var("has_system"), vec![
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_bool(false), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)]), e_bool(false)])),
                        ]),
                        (e_var("has_public"), vec![
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_bool(false), e_bool(false), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(2)])])),
                        ]),
                    ],
                        Some(vec![
                        s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_bool(false), e_bool(false), e_bool(false)])),
                    ]),
                    ),
                ]),
        )
        .method(
            method("__elephc_on_unparsed_entity_decl")
                .private()
                .param("raw", TypeExpr::Int)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_not(e_this_prop("__elephc_unparsed_installed")),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("handler", e_this_prop("__elephc_unparsed_handler")),
                    s_if(
                        e_binop(e_var("handler"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return_void(),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("flags", e_call("elephc_xml_parser_event_int", vec![e_var("raw"), e_int(3)])),
                    s_assign("name", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(0)])),
                    s_assign("system_id", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(1)])),
                    s_assign("notation", e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(3)])),
                    s_if(
                        e_binop(e_binop(e_var("flags"), BinOp::BitAnd, e_int(2)), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_bool(false), e_var("system_id"), e_call("elephc_xml_parser_event_string", vec![e_var("raw"), e_int(2)]), e_var("notation")])),
                        ],
                        vec![],
                        Some(vec![
                        s_expr(e_call("call_user_func", vec![e_var("handler"), e_this(), e_var("name"), e_bool(false), e_var("system_id"), e_bool(false), e_var("notation")])),
                    ]),
                    ),
                ]),
        )
        .build()
}

/// `xml_parser_create` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_parser_create() -> Stmt {
    function("xml_parser_create")
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .returns(t_class("XMLParser"))
        .body(vec![
            s_return(e_static_call("XMLParser", "__elephc_create", vec![e_str("xml_parser_create"), e_var("encoding"), e_bool(false), e_str(":")])),
        ])
        .build()
}

/// `xml_parser_create_ns` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_parser_create_ns() -> Stmt {
    function("xml_parser_create_ns")
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .param_default("separator", TypeExpr::Str, e_str(":"))
        .returns(t_class("XMLParser"))
        .body(vec![
            s_return(e_static_call("XMLParser", "__elephc_create", vec![e_str("xml_parser_create_ns"), e_var("encoding"), e_bool(true), e_var("separator")])),
        ])
        .build()
}

/// `xml_set_object` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_set_object() -> Stmt {
    function("xml_set_object")
        .param("parser", t_class("XMLParser"))
        .param("object", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_object", vec![e_var("object")])),
        ])
        .build()
}

/// `__elephc_xml_set_element_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_element_handler() -> Stmt {
    function("__elephc_xml_set_element_handler")
        .param("parser", t_class("XMLParser"))
        .param("start_handler", t_mixed())
        .param("end_handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_element_handler", vec![e_var("start_handler"), e_var("end_handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_character_data_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_character_data_handler() -> Stmt {
    function("__elephc_xml_set_character_data_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_character_data_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_processing_instruction_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_processing_instruction_handler() -> Stmt {
    function("__elephc_xml_set_processing_instruction_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_processing_instruction_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_default_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_default_handler() -> Stmt {
    function("__elephc_xml_set_default_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_default_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_unparsed_entity_decl_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_unparsed_entity_decl_handler() -> Stmt {
    function("__elephc_xml_set_unparsed_entity_decl_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_unparsed_entity_decl_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_notation_decl_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_notation_decl_handler() -> Stmt {
    function("__elephc_xml_set_notation_decl_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_notation_decl_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_external_entity_ref_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_external_entity_ref_handler() -> Stmt {
    function("__elephc_xml_set_external_entity_ref_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_external_entity_ref_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_start_namespace_decl_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_start_namespace_decl_handler() -> Stmt {
    function("__elephc_xml_set_start_namespace_decl_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_start_namespace_decl_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `__elephc_xml_set_end_namespace_decl_handler` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_set_end_namespace_decl_handler() -> Stmt {
    function("__elephc_xml_set_end_namespace_decl_handler")
        .param("parser", t_class("XMLParser"))
        .param("handler", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_end_namespace_decl_handler", vec![e_var("handler")])),
        ])
        .build()
}

/// `xml_parse` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_parse() -> Stmt {
    function("xml_parse")
        .param("parser", t_class("XMLParser"))
        .param("data", TypeExpr::Str)
        .param_default("is_final", TypeExpr::Bool, e_bool(false))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_parse", vec![e_var("data"), e_var("is_final")])),
        ])
        .build()
}

/// `__elephc_xml_parse_into_struct` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_parse_into_struct() -> Stmt {
    function("__elephc_xml_parse_into_struct")
        .param("parser", t_class("XMLParser"))
        .param("data", TypeExpr::Str)
        .param("with_index", TypeExpr::Bool)
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_parse_into_struct", vec![e_var("data"), e_var("with_index")])),
        ])
        .build()
}

/// `__elephc_xml_struct_values` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_struct_values() -> Stmt {
    function("__elephc_xml_struct_values")
        .param("parser", t_class("XMLParser"))
        .returns(t_mixed())
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_struct_values", vec![])),
        ])
        .build()
}

/// `__elephc_xml_struct_index` — transcribed from the PHP form.
pub(super) fn decl_fn_elephc_xml_struct_index() -> Stmt {
    function("__elephc_xml_struct_index")
        .param("parser", t_class("XMLParser"))
        .returns(t_mixed())
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_struct_index", vec![])),
        ])
        .build()
}

/// `xml_get_error_code` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_get_error_code() -> Stmt {
    function("xml_get_error_code")
        .param("parser", t_class("XMLParser"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_error_code", vec![])),
        ])
        .build()
}

/// `xml_error_string` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_error_string() -> Stmt {
    function("xml_error_string")
        .param("error_code", TypeExpr::Int)
        .returns(t_nullable(TypeExpr::Str))
        .body(vec![
            s_return(e_call("elephc_xml_error_string", vec![e_var("error_code")])),
        ])
        .build()
}

/// `xml_get_current_line_number` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_get_current_line_number() -> Stmt {
    function("xml_get_current_line_number")
        .param("parser", t_class("XMLParser"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_line", vec![])),
        ])
        .build()
}

/// `xml_get_current_column_number` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_get_current_column_number() -> Stmt {
    function("xml_get_current_column_number")
        .param("parser", t_class("XMLParser"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_column", vec![])),
        ])
        .build()
}

/// `xml_get_current_byte_index` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_get_current_byte_index() -> Stmt {
    function("xml_get_current_byte_index")
        .param("parser", t_class("XMLParser"))
        .returns(TypeExpr::Int)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_byte_index", vec![])),
        ])
        .build()
}

/// `xml_parser_free` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_parser_free() -> Stmt {
    function("xml_parser_free")
        .param("parser", t_class("XMLParser"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_prop(e_var("parser"), "__elephc_parsing"),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_return(e_bool(true)),
        ])
        .build()
}

/// `xml_parser_set_option` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_parser_set_option() -> Stmt {
    function("xml_parser_set_option")
        .param("parser", t_class("XMLParser"))
        .param("option", TypeExpr::Int)
        .param("value", t_mixed())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_set_option", vec![e_var("option"), e_var("value")])),
        ])
        .build()
}

/// `xml_parser_get_option` — transcribed from the PHP form.
pub(super) fn decl_fn_xml_parser_get_option() -> Stmt {
    function("xml_parser_get_option")
        .param("parser", t_class("XMLParser"))
        .param("option", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_return(e_method_call(e_var("parser"), "__elephc_get_option", vec![e_var("option")])),
        ])
        .build()
}
