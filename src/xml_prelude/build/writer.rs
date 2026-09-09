//! Purpose:
//! The `ext/xmlwriter` half of the xml prelude: the `XMLWriter` class over the bridge
//! writer (memory and URI/stream output modes, php-src's name validation errors and
//! `outputMemory()`/`flush()` return shapes) plus the `xmlwriter_*` procedural wrappers.
//!
//! Called from:
//! - `crate::xml_prelude::build::xml_declarations`.
//!
//! Key details:
//! - TRANSCRIBED from the PHP form kept in `crate::xml_prelude::fragments`.
//! - Every public method forwards to an `__elephc_*` twin carrying the calling function's
//!   display name and PHP's quirky argument label, so `ValueError` messages match php-src
//!   for both the method and the procedural spelling.

use crate::parser::ast::{BinOp, Stmt, TypeExpr};
use crate::synthetic_class::{class, e_array, e_binop, e_bool, e_call, e_const, e_int, e_method_call, e_new, e_new_static, e_not, e_null, e_null_coalesce, e_self_call, e_str, e_ternary, e_this, e_this_prop, e_var, function, method, s_assign, s_expr, s_if, s_prop_assign, s_return, s_throw, t_array, t_class, t_mixed, t_nullable, t_union};

/// `XMLWriter` — transcribed from the PHP form.
pub(super) fn decl_class_xmlwriter() -> Stmt {
    class("XMLWriter")
        .prop("__elephc_handle", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_stream", t_mixed(), Some(e_null()))
        .prop("__elephc_std_fd", TypeExpr::Int, Some(e_int(0)))
        .prop("__elephc_uri_mode", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__destruct")
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_var("raw"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_if(
                                e_this_prop("__elephc_uri_mode"),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_flush_to_stream", vec![e_var("raw")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_prop_assign(e_this(), "__elephc_handle", e_int(0)),
                            s_expr(e_call("elephc_xml_writer_free", vec![e_var("raw")])),
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
                    s_prop_assign(e_this(), "__elephc_uri_mode", e_bool(false)),
                    s_prop_assign(e_this(), "__elephc_std_fd", e_int(0)),
                    s_prop_assign(e_this(), "__elephc_stream", e_null()),
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
            method("__elephc_reset")
                .private()
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_var("raw"), BinOp::StrictNotEq, e_int(0)),
                        vec![
                            s_if(
                                e_this_prop("__elephc_uri_mode"),
                                vec![
                                    s_expr(e_method_call(e_this(), "__elephc_flush_to_stream", vec![e_var("raw")])),
                                ],
                                vec![],
                                None,
                            ),
                            s_expr(e_call("elephc_xml_writer_free", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_prop_assign(e_this(), "__elephc_handle", e_call("elephc_xml_writer_create", vec![])),
                    s_prop_assign(e_this(), "__elephc_stream", e_null()),
                    s_prop_assign(e_this(), "__elephc_std_fd", e_int(0)),
                    s_prop_assign(e_this(), "__elephc_uri_mode", e_bool(false)),
                ]),
        )
        .method(
            method("__elephc_output")
                .private()
                .param("raw", TypeExpr::Int)
                .param("take", TypeExpr::Bool)
                .returns(TypeExpr::Str)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_xml_writer_output_has_nul", vec![e_var("raw")]), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_assign("decoded", e_call("hex2bin", vec![e_call("elephc_xml_writer_output_hex", vec![e_var("raw"), e_ternary(e_var("take"), e_int(1), e_int(0))])])),
                            s_if(
                                e_call("is_string", vec![e_var("decoded")]),
                                vec![
                                    s_return(e_var("decoded")),
                                ],
                                vec![],
                                None,
                            ),
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_var("take"),
                        vec![
                            s_return(e_call("elephc_xml_writer_take_output", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("elephc_xml_writer_output", vec![e_var("raw")])),
                ]),
        )
        .method(
            method("__elephc_flush_to_stream")
                .private()
                .param("raw", TypeExpr::Int)
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("pending", e_method_call(e_this(), "__elephc_output", vec![e_var("raw"), e_bool(true)])),
                    s_if(
                        e_binop(e_var("pending"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_std_fd"), BinOp::StrictEq, e_int(1)),
                        vec![
                            s_assign("written", e_call("fwrite", vec![e_const("STDOUT"), e_var("pending")])),
                            s_return(e_ternary(e_binop(e_var("written"), BinOp::StrictEq, e_bool(false)), e_int(0), e_var("written"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_this_prop("__elephc_std_fd"), BinOp::StrictEq, e_int(2)),
                        vec![
                            s_assign("written", e_call("fwrite", vec![e_const("STDERR"), e_var("pending")])),
                            s_return(e_ternary(e_binop(e_var("written"), BinOp::StrictEq, e_bool(false)), e_int(0), e_var("written"))),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("stream", e_this_prop("__elephc_stream")),
                    s_if(
                        e_binop(e_var("stream"), BinOp::StrictEq, e_null()),
                        vec![
                            s_return(e_int(0)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("written", e_call("fwrite", vec![e_var("stream"), e_var("pending")])),
                    s_return(e_ternary(e_binop(e_var("written"), BinOp::StrictEq, e_bool(false)), e_int(0), e_var("written"))),
                ]),
        )
        .method(
            method("__elephc_require")
                .private()
                .returns(TypeExpr::Int)
                .body(vec![
                    s_assign("raw", e_this_prop("__elephc_handle")),
                    s_if(
                        e_binop(e_var("raw"), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("Error", vec![e_str("Invalid or uninitialized XMLWriter object")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("raw")),
                ]),
        )
        .method(
            method("__elephc_check_name")
                .private()
                .static_()
                .param("name", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .param("subject", TypeExpr::Str)
                .returns(TypeExpr::Void)
                .body(vec![
                    s_if(
                        e_binop(e_call("elephc_xml_writer_valid_name", vec![e_var("name")]), BinOp::StrictEq, e_int(0)),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument ")), BinOp::Concat, e_var("argument")), BinOp::Concat, e_str(" must be a valid ")), BinOp::Concat, e_var("subject")), BinOp::Concat, e_str(", \"")), BinOp::Concat, e_var("name")), BinOp::Concat, e_str("\" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("__elephc_open_uri")
                .param("uri", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_if(
                        e_binop(e_var("uri"), BinOp::StrictEq, e_str("")),
                        vec![
                            s_throw(e_new("ValueError", vec![e_binop(e_var("function"), BinOp::Concat, e_str("(): Argument #1 ($uri) must not be empty"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_binop(e_var("uri"), BinOp::StrictEq, e_str("php://output")), BinOp::Or, e_binop(e_var("uri"), BinOp::StrictEq, e_str("php://stdout"))),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_reset", vec![])),
                            s_prop_assign(e_this(), "__elephc_std_fd", e_int(1)),
                            s_prop_assign(e_this(), "__elephc_uri_mode", e_bool(true)),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_if(
                        e_binop(e_var("uri"), BinOp::StrictEq, e_str("php://stderr")),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_reset", vec![])),
                            s_prop_assign(e_this(), "__elephc_std_fd", e_int(2)),
                            s_prop_assign(e_this(), "__elephc_uri_mode", e_bool(true)),
                            s_return(e_bool(true)),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("stream", e_call("fopen", vec![e_var("uri"), e_str("wb")])),
                    s_if(
                        e_binop(e_var("stream"), BinOp::StrictEq, e_bool(false)),
                        vec![
                            s_return(e_bool(false)),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_method_call(e_this(), "__elephc_reset", vec![])),
                    s_prop_assign(e_this(), "__elephc_stream", e_var("stream")),
                    s_prop_assign(e_this(), "__elephc_uri_mode", e_bool(true)),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("openUri")
                .param("uri", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_open_uri", vec![e_var("uri"), e_str("XMLWriter::openUri")])),
                ]),
        )
        .method(
            method("toUri")
                .static_()
                .param("uri", TypeExpr::Str)
                .returns(t_class("static"))
                .body(vec![
                    s_assign("writer", e_new_static(vec![])),
                    s_if(
                        e_not(e_method_call(e_var("writer"), "__elephc_open_uri", vec![e_var("uri"), e_str("XMLWriter::toUri")])),
                        vec![
                            s_throw(e_new("ValueError", vec![e_str("XMLWriter::toUri(): Argument #1 ($uri) must resolve to a valid file path")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("writer")),
                ]),
        )
        .method(
            method("openMemory")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_expr(e_method_call(e_this(), "__elephc_reset", vec![])),
                    s_return(e_bool(true)),
                ]),
        )
        .method(
            method("toMemory")
                .static_()
                .returns(t_class("static"))
                .body(vec![
                    s_assign("writer", e_new_static(vec![])),
                    s_expr(e_method_call(e_var("writer"), "openMemory", vec![])),
                    s_return(e_var("writer")),
                ]),
        )
        .method(
            method("toStream")
                .static_()
                .param("stream", t_mixed())
                .returns(t_class("static"))
                .body(vec![
                    s_if(
                        e_not(e_call("is_resource", vec![e_var("stream")])),
                        vec![
                            s_throw(e_new("TypeError", vec![e_binop(e_binop(e_str("XMLWriter::toStream(): Argument #1 ($stream) must be of type resource, "), BinOp::Concat, e_call("gettype", vec![e_var("stream")])), BinOp::Concat, e_str(" given"))])),
                        ],
                        vec![],
                        None,
                    ),
                    s_assign("writer", e_new_static(vec![])),
                    s_expr(e_method_call(e_var("writer"), "__elephc_reset", vec![])),
                    s_prop_assign(e_var("writer"), "__elephc_stream", e_var("stream")),
                    s_prop_assign(e_var("writer"), "__elephc_uri_mode", e_bool(true)),
                    s_return(e_var("writer")),
                ]),
        )
        .method(
            method("setIndent")
                .param("enable", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_set_indent", vec![e_var("raw"), e_ternary(e_var("enable"), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("setIndentString")
                .param("indentation", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_set_indent_string", vec![e_var("raw"), e_var("indentation")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startComment")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_comment", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("endComment")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_comment", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_start_attribute")
                .param("name", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("attribute name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_attribute", vec![e_var("raw"), e_var("name")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startAttribute")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_attribute", vec![e_var("name"), e_str("XMLWriter::startAttribute"), e_str("#2")])),
                ]),
        )
        .method(
            method("endAttribute")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_attribute", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_write_attribute")
                .param("name", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("attribute name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_attribute", vec![e_var("raw"), e_var("name"), e_var("value")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeAttribute")
                .param("name", TypeExpr::Str)
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_attribute", vec![e_var("name"), e_var("value"), e_str("XMLWriter::writeAttribute"), e_str("#2 ($value)")])),
                ]),
        )
        .method(
            method("__elephc_start_attribute_ns")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("attribute name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_attribute_ns", vec![e_var("raw"), e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("prefix"), e_str("")), e_var("name"), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("namespace"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startAttributeNs")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_attribute_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_str("XMLWriter::startAttributeNs"), e_str("#3 ($namespace)")])),
                ]),
        )
        .method(
            method("__elephc_write_attribute_ns")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("value", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("attribute name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_attribute_ns", vec![e_var("raw"), e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("prefix"), e_str("")), e_var("name"), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("namespace"), e_str("")), e_var("value")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeAttributeNs")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("value", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_attribute_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_var("value"), e_str("XMLWriter::writeAttributeNs"), e_str("#3 ($namespace)")])),
                ]),
        )
        .method(
            method("__elephc_start_element")
                .param("name", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_element", vec![e_var("raw"), e_var("name")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startElement")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_element", vec![e_var("name"), e_str("XMLWriter::startElement"), e_str("#2")])),
                ]),
        )
        .method(
            method("endElement")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_element", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("fullEndElement")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_full_end_element", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_start_element_ns")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_element_ns", vec![e_var("raw"), e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("prefix"), e_str("")), e_var("name"), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("namespace"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startElementNs")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_element_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_str("XMLWriter::startElementNs"), e_str("#3 ($namespace)")])),
                ]),
        )
        .method(
            method("__elephc_write_element")
                .param("name", TypeExpr::Str)
                .param("content", t_nullable(TypeExpr::Str))
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_element", vec![e_var("raw"), e_var("name"), e_ternary(e_binop(e_var("content"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("content"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeElement")
                .param("name", TypeExpr::Str)
                .param_default("content", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_element", vec![e_var("name"), e_var("content"), e_str("XMLWriter::writeElement"), e_str("#2 ($content)")])),
                ]),
        )
        .method(
            method("__elephc_write_element_ns")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .param("content", t_nullable(TypeExpr::Str))
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_element_ns", vec![e_var("raw"), e_ternary(e_binop(e_var("prefix"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("prefix"), e_str("")), e_var("name"), e_ternary(e_binop(e_var("namespace"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("namespace"), e_str("")), e_ternary(e_binop(e_var("content"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("content"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeElementNs")
                .param("prefix", t_nullable(TypeExpr::Str))
                .param("name", TypeExpr::Str)
                .param("namespace", t_nullable(TypeExpr::Str))
                .param_default("content", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_element_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_var("content"), e_str("XMLWriter::writeElementNs"), e_str("#3 ($namespace)")])),
                ]),
        )
        .method(
            method("__elephc_start_pi")
                .param("target", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("target"), e_var("function"), e_var("argument"), e_str("PI target")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_pi", vec![e_var("raw"), e_var("target")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startPi")
                .param("target", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_pi", vec![e_var("target"), e_str("XMLWriter::startPi"), e_str("#2")])),
                ]),
        )
        .method(
            method("endPi")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_pi", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_write_pi")
                .param("target", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("target"), e_var("function"), e_var("argument"), e_str("PI target")])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_pi", vec![e_var("raw"), e_var("target"), e_var("content")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writePi")
                .param("target", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_pi", vec![e_var("target"), e_var("content"), e_str("XMLWriter::writePi"), e_str("#2 ($content)")])),
                ]),
        )
        .method(
            method("startCdata")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_cdata", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("endCdata")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_cdata", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeCdata")
                .param("content", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_cdata", vec![e_var("raw"), e_var("content")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("text")
                .param("content", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_text", vec![e_var("raw"), e_var("content")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeRaw")
                .param("content", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_raw", vec![e_var("raw"), e_var("content")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startDocument")
                .param_default("version", t_nullable(TypeExpr::Str), e_str("1.0"))
                .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
                .param_default("standalone", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_document", vec![e_var("raw"), e_ternary(e_binop(e_var("version"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("version"), e_str("")), e_ternary(e_binop(e_var("encoding"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("encoding"), e_str("")), e_ternary(e_binop(e_var("standalone"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("standalone"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("endDocument")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_assign("result", e_binop(e_call("elephc_xml_writer_end_document", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                    s_if(
                        e_this_prop("__elephc_uri_mode"),
                        vec![
                            s_expr(e_method_call(e_this(), "__elephc_flush_to_stream", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_var("result")),
                ]),
        )
        .method(
            method("writeComment")
                .param("content", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_comment", vec![e_var("raw"), e_var("content")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startDtd")
                .param("qualifiedName", TypeExpr::Str)
                .param_default("publicId", t_nullable(TypeExpr::Str), e_null())
                .param_default("systemId", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_dtd", vec![e_var("raw"), e_var("qualifiedName"), e_ternary(e_binop(e_var("publicId"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("publicId"), e_str("")), e_ternary(e_binop(e_var("systemId"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("systemId"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("endDtd")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_dtd", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeDtd")
                .param("name", TypeExpr::Str)
                .param_default("publicId", t_nullable(TypeExpr::Str), e_null())
                .param_default("systemId", t_nullable(TypeExpr::Str), e_null())
                .param_default("content", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_dtd", vec![e_var("raw"), e_var("name"), e_ternary(e_binop(e_var("publicId"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("publicId"), e_str("")), e_ternary(e_binop(e_var("systemId"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("systemId"), e_str("")), e_ternary(e_binop(e_var("content"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), e_null_coalesce(e_var("content"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_start_dtd_element")
                .param("qualifiedName", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("qualifiedName"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_dtd_element", vec![e_var("raw"), e_var("qualifiedName")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startDtdElement")
                .param("qualifiedName", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_dtd_element", vec![e_var("qualifiedName"), e_str("XMLWriter::startDtdElement"), e_str("#2")])),
                ]),
        )
        .method(
            method("endDtdElement")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_dtd_element", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_write_dtd_element")
                .param("name", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_dtd_element", vec![e_var("raw"), e_var("name"), e_var("content")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeDtdElement")
                .param("name", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_dtd_element", vec![e_var("name"), e_var("content"), e_str("XMLWriter::writeDtdElement"), e_str("#2 ($content)")])),
                ]),
        )
        .method(
            method("__elephc_start_dtd_attlist")
                .param("name", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_dtd_attlist", vec![e_var("raw"), e_var("name")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startDtdAttlist")
                .param("name", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_dtd_attlist", vec![e_var("name"), e_str("XMLWriter::startDtdAttlist"), e_str("#2")])),
                ]),
        )
        .method(
            method("endDtdAttlist")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_dtd_attlist", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_write_dtd_attlist")
                .param("name", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_write_dtd_attlist", vec![e_var("raw"), e_var("name"), e_var("content")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeDtdAttlist")
                .param("name", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_dtd_attlist", vec![e_var("name"), e_var("content"), e_str("XMLWriter::writeDtdAttlist"), e_str("#2 ($content)")])),
                ]),
        )
        .method(
            method("__elephc_start_dtd_entity")
                .param("name", TypeExpr::Str)
                .param("isParam", TypeExpr::Bool)
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("attribute name")])),
                    s_return(e_binop(e_call("elephc_xml_writer_start_dtd_entity", vec![e_var("raw"), e_var("name"), e_ternary(e_var("isParam"), e_int(1), e_int(0))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("startDtdEntity")
                .param("name", TypeExpr::Str)
                .param("isParam", TypeExpr::Bool)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_start_dtd_entity", vec![e_var("name"), e_var("isParam"), e_str("XMLWriter::startDtdEntity"), e_str("#2 ($isParam)")])),
                ]),
        )
        .method(
            method("endDtdEntity")
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_return(e_binop(e_call("elephc_xml_writer_end_dtd_entity", vec![e_var("raw")]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("__elephc_write_dtd_entity")
                .param("name", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .param("isParam", TypeExpr::Bool)
                .param("publicId", t_nullable(TypeExpr::Str))
                .param("systemId", t_nullable(TypeExpr::Str))
                .param("notationData", t_nullable(TypeExpr::Str))
                .param("function", TypeExpr::Str)
                .param("argument", TypeExpr::Str)
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_expr(e_self_call("__elephc_check_name", vec![e_var("name"), e_var("function"), e_var("argument"), e_str("element name")])),
                    s_assign("flags", e_binop(e_binop(e_ternary(e_binop(e_var("publicId"), BinOp::StrictEq, e_null()), e_int(0), e_int(1)), BinOp::BitOr, e_ternary(e_binop(e_var("systemId"), BinOp::StrictEq, e_null()), e_int(0), e_int(2))), BinOp::BitOr, e_ternary(e_binop(e_var("notationData"), BinOp::StrictEq, e_null()), e_int(0), e_int(4)))),
                    s_return(e_binop(e_call("elephc_xml_writer_write_dtd_entity", vec![e_var("raw"), e_var("name"), e_var("content"), e_ternary(e_var("isParam"), e_int(1), e_int(0)), e_var("flags"), e_null_coalesce(e_var("publicId"), e_str("")), e_null_coalesce(e_var("systemId"), e_str("")), e_null_coalesce(e_var("notationData"), e_str(""))]), BinOp::StrictEq, e_int(1))),
                ]),
        )
        .method(
            method("writeDtdEntity")
                .param("name", TypeExpr::Str)
                .param("content", TypeExpr::Str)
                .param_default("isParam", TypeExpr::Bool, e_bool(false))
                .param_default("publicId", t_nullable(TypeExpr::Str), e_null())
                .param_default("systemId", t_nullable(TypeExpr::Str), e_null())
                .param_default("notationData", t_nullable(TypeExpr::Str), e_null())
                .returns(TypeExpr::Bool)
                .body(vec![
                    s_return(e_method_call(e_this(), "__elephc_write_dtd_entity", vec![e_var("name"), e_var("content"), e_var("isParam"), e_var("publicId"), e_var("systemId"), e_var("notationData"), e_str("XMLWriter::writeDtdEntity"), e_str("#2 ($content)")])),
                ]),
        )
        .method(
            method("outputMemory")
                .param_default("flush", TypeExpr::Bool, e_bool(true))
                .returns(TypeExpr::Str)
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_if(
                        e_this_prop("__elephc_uri_mode"),
                        vec![
                            s_return(e_str("")),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "__elephc_output", vec![e_var("raw"), e_var("flush")])),
                ]),
        )
        .method(
            method("flush")
                .param_default("empty", TypeExpr::Bool, e_bool(true))
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::Int]))
                .body(vec![
                    s_assign("raw", e_method_call(e_this(), "__elephc_require", vec![])),
                    s_if(
                        e_this_prop("__elephc_uri_mode"),
                        vec![
                            s_return(e_method_call(e_this(), "__elephc_flush_to_stream", vec![e_var("raw")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_method_call(e_this(), "__elephc_output", vec![e_var("raw"), e_var("empty")])),
                ]),
        )
        .build()
}

/// `xmlwriter_open_uri` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_open_uri() -> Stmt {
    function("xmlwriter_open_uri")
        .param("uri", TypeExpr::Str)
        .returns(t_class("XMLWriter"))
        .body(vec![
            s_assign("writer", e_new("XMLWriter", vec![])),
            s_if(
                e_not(e_method_call(e_var("writer"), "__elephc_open_uri", vec![e_var("uri"), e_str("xmlwriter_open_uri")])),
                vec![
                    s_throw(e_new("ValueError", vec![e_str("xmlwriter_open_uri(): Argument #1 ($uri) must resolve to a valid file path")])),
                ],
                vec![],
                None,
            ),
            s_return(e_var("writer")),
        ])
        .build()
}

/// `xmlwriter_open_memory` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_open_memory() -> Stmt {
    function("xmlwriter_open_memory")
        .returns(t_class("XMLWriter"))
        .body(vec![
            s_assign("writer", e_new("XMLWriter", vec![])),
            s_expr(e_method_call(e_var("writer"), "openMemory", vec![])),
            s_return(e_var("writer")),
        ])
        .build()
}

/// `xmlwriter_set_indent` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_set_indent() -> Stmt {
    function("xmlwriter_set_indent")
        .param("writer", t_class("XMLWriter"))
        .param("enable", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "setIndent", vec![e_var("enable")])),
        ])
        .build()
}

/// `xmlwriter_set_indent_string` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_set_indent_string() -> Stmt {
    function("xmlwriter_set_indent_string")
        .param("writer", t_class("XMLWriter"))
        .param("indentation", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "setIndentString", vec![e_var("indentation")])),
        ])
        .build()
}

/// `xmlwriter_start_comment` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_comment() -> Stmt {
    function("xmlwriter_start_comment")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "startComment", vec![])),
        ])
        .build()
}

/// `xmlwriter_end_comment` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_comment() -> Stmt {
    function("xmlwriter_end_comment")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endComment", vec![])),
        ])
        .build()
}

/// `xmlwriter_start_attribute` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_attribute() -> Stmt {
    function("xmlwriter_start_attribute")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_attribute", vec![e_var("name"), e_str("xmlwriter_start_attribute"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_end_attribute` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_attribute() -> Stmt {
    function("xmlwriter_end_attribute")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endAttribute", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_attribute` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_attribute() -> Stmt {
    function("xmlwriter_write_attribute")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .param("value", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_attribute", vec![e_var("name"), e_var("value"), e_str("xmlwriter_write_attribute"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_start_attribute_ns` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_attribute_ns() -> Stmt {
    function("xmlwriter_start_attribute_ns")
        .param("writer", t_class("XMLWriter"))
        .param("prefix", t_nullable(TypeExpr::Str))
        .param("name", TypeExpr::Str)
        .param("namespace", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_attribute_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_str("xmlwriter_start_attribute_ns"), e_str("#3 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_write_attribute_ns` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_attribute_ns() -> Stmt {
    function("xmlwriter_write_attribute_ns")
        .param("writer", t_class("XMLWriter"))
        .param("prefix", t_nullable(TypeExpr::Str))
        .param("name", TypeExpr::Str)
        .param("namespace", t_nullable(TypeExpr::Str))
        .param("value", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_attribute_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_var("value"), e_str("xmlwriter_write_attribute_ns"), e_str("#3 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_start_element` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_element() -> Stmt {
    function("xmlwriter_start_element")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_element", vec![e_var("name"), e_str("xmlwriter_start_element"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_end_element` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_element() -> Stmt {
    function("xmlwriter_end_element")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endElement", vec![])),
        ])
        .build()
}

/// `xmlwriter_full_end_element` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_full_end_element() -> Stmt {
    function("xmlwriter_full_end_element")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "fullEndElement", vec![])),
        ])
        .build()
}

/// `xmlwriter_start_element_ns` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_element_ns() -> Stmt {
    function("xmlwriter_start_element_ns")
        .param("writer", t_class("XMLWriter"))
        .param("prefix", t_nullable(TypeExpr::Str))
        .param("name", TypeExpr::Str)
        .param("namespace", t_nullable(TypeExpr::Str))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_element_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_str("xmlwriter_start_element_ns"), e_str("#3 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_write_element` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_element() -> Stmt {
    function("xmlwriter_write_element")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .param_default("content", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_element", vec![e_var("name"), e_var("content"), e_str("xmlwriter_write_element"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_write_element_ns` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_element_ns() -> Stmt {
    function("xmlwriter_write_element_ns")
        .param("writer", t_class("XMLWriter"))
        .param("prefix", t_nullable(TypeExpr::Str))
        .param("name", TypeExpr::Str)
        .param("namespace", t_nullable(TypeExpr::Str))
        .param_default("content", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_element_ns", vec![e_var("prefix"), e_var("name"), e_var("namespace"), e_var("content"), e_str("xmlwriter_write_element_ns"), e_str("#3 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_start_pi` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_pi() -> Stmt {
    function("xmlwriter_start_pi")
        .param("writer", t_class("XMLWriter"))
        .param("target", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_pi", vec![e_var("target"), e_str("xmlwriter_start_pi"), e_str("#2 ($target)")])),
        ])
        .build()
}

/// `xmlwriter_end_pi` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_pi() -> Stmt {
    function("xmlwriter_end_pi")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endPi", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_pi` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_pi() -> Stmt {
    function("xmlwriter_write_pi")
        .param("writer", t_class("XMLWriter"))
        .param("target", TypeExpr::Str)
        .param("content", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_pi", vec![e_var("target"), e_var("content"), e_str("xmlwriter_write_pi"), e_str("#2 ($target)")])),
        ])
        .build()
}

/// `xmlwriter_start_cdata` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_cdata() -> Stmt {
    function("xmlwriter_start_cdata")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "startCdata", vec![])),
        ])
        .build()
}

/// `xmlwriter_end_cdata` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_cdata() -> Stmt {
    function("xmlwriter_end_cdata")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endCdata", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_cdata` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_cdata() -> Stmt {
    function("xmlwriter_write_cdata")
        .param("writer", t_class("XMLWriter"))
        .param("content", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "writeCdata", vec![e_var("content")])),
        ])
        .build()
}

/// `xmlwriter_text` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_text() -> Stmt {
    function("xmlwriter_text")
        .param("writer", t_class("XMLWriter"))
        .param("content", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "text", vec![e_var("content")])),
        ])
        .build()
}

/// `xmlwriter_write_raw` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_raw() -> Stmt {
    function("xmlwriter_write_raw")
        .param("writer", t_class("XMLWriter"))
        .param("content", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "writeRaw", vec![e_var("content")])),
        ])
        .build()
}

/// `xmlwriter_start_document` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_document() -> Stmt {
    function("xmlwriter_start_document")
        .param("writer", t_class("XMLWriter"))
        .param_default("version", t_nullable(TypeExpr::Str), e_str("1.0"))
        .param_default("encoding", t_nullable(TypeExpr::Str), e_null())
        .param_default("standalone", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "startDocument", vec![e_var("version"), e_var("encoding"), e_var("standalone")])),
        ])
        .build()
}

/// `xmlwriter_end_document` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_document() -> Stmt {
    function("xmlwriter_end_document")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endDocument", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_comment` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_comment() -> Stmt {
    function("xmlwriter_write_comment")
        .param("writer", t_class("XMLWriter"))
        .param("content", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "writeComment", vec![e_var("content")])),
        ])
        .build()
}

/// `xmlwriter_start_dtd` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_dtd() -> Stmt {
    function("xmlwriter_start_dtd")
        .param("writer", t_class("XMLWriter"))
        .param("qualifiedName", TypeExpr::Str)
        .param_default("publicId", t_nullable(TypeExpr::Str), e_null())
        .param_default("systemId", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "startDtd", vec![e_var("qualifiedName"), e_var("publicId"), e_var("systemId")])),
        ])
        .build()
}

/// `xmlwriter_end_dtd` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_dtd() -> Stmt {
    function("xmlwriter_end_dtd")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endDtd", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_dtd` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_dtd() -> Stmt {
    function("xmlwriter_write_dtd")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .param_default("publicId", t_nullable(TypeExpr::Str), e_null())
        .param_default("systemId", t_nullable(TypeExpr::Str), e_null())
        .param_default("content", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "writeDtd", vec![e_var("name"), e_var("publicId"), e_var("systemId"), e_var("content")])),
        ])
        .build()
}

/// `xmlwriter_start_dtd_element` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_dtd_element() -> Stmt {
    function("xmlwriter_start_dtd_element")
        .param("writer", t_class("XMLWriter"))
        .param("qualifiedName", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_dtd_element", vec![e_var("qualifiedName"), e_str("xmlwriter_start_dtd_element"), e_str("#2 ($qualifiedName)")])),
        ])
        .build()
}

/// `xmlwriter_end_dtd_element` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_dtd_element() -> Stmt {
    function("xmlwriter_end_dtd_element")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endDtdElement", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_dtd_element` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_dtd_element() -> Stmt {
    function("xmlwriter_write_dtd_element")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .param("content", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_dtd_element", vec![e_var("name"), e_var("content"), e_str("xmlwriter_write_dtd_element"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_start_dtd_attlist` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_dtd_attlist() -> Stmt {
    function("xmlwriter_start_dtd_attlist")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_dtd_attlist", vec![e_var("name"), e_str("xmlwriter_start_dtd_attlist"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_end_dtd_attlist` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_dtd_attlist() -> Stmt {
    function("xmlwriter_end_dtd_attlist")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endDtdAttlist", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_dtd_attlist` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_dtd_attlist() -> Stmt {
    function("xmlwriter_write_dtd_attlist")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .param("content", TypeExpr::Str)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_dtd_attlist", vec![e_var("name"), e_var("content"), e_str("xmlwriter_write_dtd_attlist"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_start_dtd_entity` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_start_dtd_entity() -> Stmt {
    function("xmlwriter_start_dtd_entity")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .param("isParam", TypeExpr::Bool)
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_start_dtd_entity", vec![e_var("name"), e_var("isParam"), e_str("xmlwriter_start_dtd_entity"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_end_dtd_entity` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_end_dtd_entity() -> Stmt {
    function("xmlwriter_end_dtd_entity")
        .param("writer", t_class("XMLWriter"))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "endDtdEntity", vec![])),
        ])
        .build()
}

/// `xmlwriter_write_dtd_entity` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_write_dtd_entity() -> Stmt {
    function("xmlwriter_write_dtd_entity")
        .param("writer", t_class("XMLWriter"))
        .param("name", TypeExpr::Str)
        .param("content", TypeExpr::Str)
        .param_default("isParam", TypeExpr::Bool, e_bool(false))
        .param_default("publicId", t_nullable(TypeExpr::Str), e_null())
        .param_default("systemId", t_nullable(TypeExpr::Str), e_null())
        .param_default("notationData", t_nullable(TypeExpr::Str), e_null())
        .returns(TypeExpr::Bool)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "__elephc_write_dtd_entity", vec![e_var("name"), e_var("content"), e_var("isParam"), e_var("publicId"), e_var("systemId"), e_var("notationData"), e_str("xmlwriter_write_dtd_entity"), e_str("#2 ($name)")])),
        ])
        .build()
}

/// `xmlwriter_output_memory` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_output_memory() -> Stmt {
    function("xmlwriter_output_memory")
        .param("writer", t_class("XMLWriter"))
        .param_default("flush", TypeExpr::Bool, e_bool(true))
        .returns(TypeExpr::Str)
        .body(vec![
            s_return(e_method_call(e_var("writer"), "outputMemory", vec![e_var("flush")])),
        ])
        .build()
}

/// `xmlwriter_flush` — transcribed from the PHP form.
pub(super) fn decl_fn_xmlwriter_flush() -> Stmt {
    function("xmlwriter_flush")
        .param("writer", t_class("XMLWriter"))
        .param_default("empty", TypeExpr::Bool, e_bool(true))
        .returns(t_union(vec![TypeExpr::Str, TypeExpr::Int]))
        .body(vec![
            s_return(e_method_call(e_var("writer"), "flush", vec![e_var("empty")])),
        ])
        .build()
}
