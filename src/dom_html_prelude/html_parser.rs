//! Purpose:
//! Package P6 of the DOM migration: `DOMDocument::loadHTML` / `loadHTMLFile` over a libxml2
//! 2.9.13 HTML4 parser written as AST (`Part::Html`).
//!
//! Called from:
//! - `super::extend::PACKAGES`, through `contribute`.
//!
//! Key details:
//! - The native libxml2 bundled with the bridge (2.15.3) does NOT parse HTML the way php 8.5's
//!   2.9.13 does: its HTML tokenizer was rewritten in 2.14 (no `<p>` wrapping of a bare text
//!   fragment, `<![CDATA[` becomes a comment, ...). php -n is the oracle, so the 2.9.13
//!   behaviour is kept as AST (`tables`, `parser`), differential-tested against php.
//! - The parser builds a flat node table and folds it into the tree arrays
//!   `__elephc_dom_make_children` reads (`helpers`); the eager nodes are free-standing, like
//!   every HTML document of the prelude (`__elephc_dom_html`).
//! - Everything here is `Part::Html`, so a program that never loads HTML does not carry it. The
//!   scanner `loadHTML` used to be built on is gone from `generated.rs`.
//! - The parser model was written as PHP, checked against `php -n` over ~100k fuzzed documents,
//!   then converted with `synthetic_class::transcribe` (see the module comments of `parser`).
//! - Known gap: `&` followed by a Latin-1 high byte at the very end of the input (php keeps the
//!   byte, the model drops it).

use crate::parser::ast::*;
use crate::synthetic_class::*;

use super::extend::{Contributions, Part};

mod helpers;
mod parser;
mod tables;

/// `__elephc_dom_parse_html(string $html, int $flags): mixed` — parses and returns the tree arrays.
fn parse_html() -> Stmt {
    function("__elephc_dom_parse_html")
        .param("html", TypeExpr::Str)
        .param("flags", TypeExpr::Int)
        .returns(t_mixed())
        .body(vec![
            s_assign("_parser", e_new("__ElephcHtmlParser", vec![e_var("html"), e_var("flags")])),
            s_expr(e_method_call(e_var("_parser"), "parse", vec![])),
            s_return(e_method_call(e_var("_parser"), "tree", vec![e_int(0)])),
        ])
        .build()
}

/// `DOMDocument::loadHTML(string $source, int $options = 0): bool`.
fn load_html() -> MethodBuilder {
    method("loadHTML")
        .param("source", TypeExpr::Str)
        .param_default("options", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("source"), BinOp::StrictEq, e_str("")),
                vec![s_throw(e_new("ValueError", vec![e_str("DOMDocument::loadHTML(): Argument #1 ($source) must not be empty")]))],
                vec![],
                None,
            ),
            s_prop_assign(e_this(), "__elephc_flags", e_var("options")),
            s_prop_assign(e_this(), "__elephc_dom_html", e_bool(true)),
            s_assign("_tree", e_call("__elephc_dom_parse_html", vec![e_var("source"), e_var("options")])),
            s_prop_assign(e_this(), "__elephc_simplexml", e_null()),
            s_prop_assign(e_this(), "nodeName", e_str("#document")),
            s_prop_assign(e_this(), "ownerDocument", e_null()),
            s_assign("_kids", e_call("__elephc_dom_make_children", vec![e_this(), e_this(), e_index(e_var("_tree"), e_str("children"))])),
            s_prop_assign(e_this(), "childNodes", e_new("DOMNodeList", vec![e_var("_kids")])),
            s_prop_assign(e_this(), "nodeValue", e_null()),
            s_return(e_bool(true)),
        ])
}

/// `DOMDocument::loadHTMLFile(string $filename, int $options = 0): bool`.
fn load_html_file() -> MethodBuilder {
    method("loadHTMLFile")
        .param("filename", TypeExpr::Str)
        .param_default("options", TypeExpr::Int, e_int(0))
        .returns(TypeExpr::Bool)
        .body(vec![
            s_if(
                e_binop(e_var("filename"), BinOp::StrictEq, e_str("")),
                vec![s_throw(e_new("ValueError", vec![e_str("DOMDocument::loadHTMLFile(): Argument #1 ($filename) must not be empty")]))],
                vec![],
                None,
            ),
            s_assign("source", e_error_suppress(e_call("file_get_contents", vec![e_var("filename")]))),
            s_if(
                e_binop(e_binop(e_var("source"), BinOp::StrictEq, e_bool(false)), BinOp::Or, e_binop(e_var("source"), BinOp::StrictEq, e_str(""))),
                vec![s_return(e_bool(false))],
                vec![],
                None,
            ),
            s_return(e_method_call(e_this(), "loadHTML", vec![e_var("source"), e_var("options")])),
        ])
}

/// Registers this package's declarations.
pub(super) fn contribute(contributions: &mut Contributions) {
    for function in helpers::declarations() {
        contributions.add_function(Part::Html, function);
    }
    contributions.add_function(Part::Html, parse_html());
    contributions.add_class(Part::Html, tables::declaration());
    contributions.add_class(Part::Html, parser::declaration());
    contributions.members(
        Part::Html,
        class("DOMDocument").method(load_html()).method(load_html_file()),
    );
}
