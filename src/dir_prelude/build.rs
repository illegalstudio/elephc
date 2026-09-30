//! Purpose:
//! Builds php's `Directory` class and `dir()` function as AST, replacing the PHP source this module
//! used to carry as a raw string and reparse on every compile that touched it.
//!
//! Called from:
//! - `crate::dir_prelude::inject_if_used`, after include resolution and before name resolution.
//!
//! Key details:
//! - TRANSCRIBED, not rewritten: every declaration here was generated from the parse of the PHP it
//!   replaces (`synthetic_class::transcribe`), and `build_oracle_tests` in the parent module
//!   compares the built AST against that parse node by node on every test run.
//! - Built nodes carry SYNTHETIC spans, so a warning raised inside `dir()` names the caller's line
//!   the way php's internal function does, instead of a line of this prelude's former PHP text.

use crate::parser::ast::{BinOp, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    class, e_binop, e_bool, e_call, e_new, e_new_fq, e_not, e_null, e_static_prop, e_str,
    e_this_prop, e_var, function, internal_declarations, method, s_assign, s_expr, s_if,
    s_prop_assign, s_return, s_static_prop_assign, s_throw, t_class, t_mixed, t_union,
};

/// `Directory` — transcribed from the PHP form.
fn decl_class_directory() -> Stmt {
    class("Directory")
        .final_()
        .prop("path", TypeExpr::Str, Some(e_str("")))
        .prop("handle", t_mixed(), Some(e_null()))
        .static_prop("__elephc_opening", TypeExpr::Bool, Some(e_bool(false)))
        .method(
            method("__construct")
                .body(vec![
                    s_if(
                        e_not(e_static_prop("Directory", "__elephc_opening")),
                        vec![
                            s_throw(e_new_fq("Error", vec![e_str("Cannot directly construct Directory, use dir() instead")])),
                        ],
                        vec![],
                        None,
                    ),
                ]),
        )
        .method(
            method("read")
                .returns(t_union(vec![TypeExpr::Str, TypeExpr::False]))
                .body(vec![
                    s_assign("handle", e_this_prop("handle")),
                    s_if(
                        e_not(e_call("is_resource", vec![e_var("handle")])),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_str("Directory::read(): cannot use Directory resource after it has been closed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_return(e_call("readdir", vec![e_var("handle")])),
                ]),
        )
        .method(
            method("rewind")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handle", e_this_prop("handle")),
                    s_if(
                        e_not(e_call("is_resource", vec![e_var("handle")])),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_str("Directory::rewind(): cannot use Directory resource after it has been closed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("rewinddir", vec![e_var("handle")])),
                ]),
        )
        .method(
            method("close")
                .returns(TypeExpr::Void)
                .body(vec![
                    s_assign("handle", e_this_prop("handle")),
                    s_if(
                        e_not(e_call("is_resource", vec![e_var("handle")])),
                        vec![
                            s_throw(e_new_fq("TypeError", vec![e_str("Directory::close(): cannot use Directory resource after it has been closed")])),
                        ],
                        vec![],
                        None,
                    ),
                    s_expr(e_call("closedir", vec![e_var("handle")])),
                ]),
        )
        .build()
}

/// `dir` — transcribed from the PHP form.
fn decl_fn_dir() -> Stmt {
    function("dir")
        .param("directory", TypeExpr::Str)
        .param_default("context", t_mixed(), e_null())
        .returns(t_union(vec![t_class("Directory"), TypeExpr::False]))
        .body(vec![
            s_assign("handle", e_call("opendir", vec![e_var("directory")])),
            s_if(
                e_binop(e_var("handle"), BinOp::StrictEq, e_bool(false)),
                vec![
                    s_return(e_bool(false)),
                ],
                vec![],
                None,
            ),
            s_static_prop_assign("Directory", "__elephc_opening", e_bool(true)),
            s_assign("entry", e_new("Directory", vec![])),
            s_static_prop_assign("Directory", "__elephc_opening", e_bool(false)),
            s_prop_assign(e_var("entry"), "path", e_var("directory")),
            s_prop_assign(e_var("entry"), "handle", e_var("handle")),
            s_return(e_var("entry")),
        ])
        .build()
}

/// Builds the whole surface, one declaration per helper above.
pub(crate) fn dir_declarations() -> Program {
    internal_declarations(|| {
        vec![
            decl_class_directory(),
            decl_fn_dir(),
        ]
    })
}
