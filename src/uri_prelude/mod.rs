//! Purpose:
//! PHP 8.5's `ext/uri` surface, built through the ordinary synthetic AST pipeline: the
//! `Uri\Rfc3986\Uri` and `Uri\WhatWg\Url` classes, the comparison and validation enums,
//! the exception hierarchy, and the `elephc_uri_*` bridge symbols they call.
//!
//! Called from:
//! - `crate::pipeline::compile()` before name resolution, through `inject_if_used`.
//!
//! Key details:
//! - Every declaration is built in Rust. No PHP text is parsed at compile time.
//! - The classes are declared in `SourceMode::Php` so `__debugInfo()` is a run-time call:
//!   internal classes use declared descriptor rows instead, and these objects have none.
//! - Per-object native state lives in a private opaque slot the property walkers skip, so
//!   casts, JSON, and `var_export` see no fields.
//! - Injection is pay-for-use: a program that never names `Uri\` neither declares the
//!   classes nor links the bridge.

mod rfc3986;
mod types;
mod whatwg;

use crate::parser::ast::{CType, Program, Stmt};
use crate::synthetic_class::extern_fn;

/// The prelude group id used by declaration reachability and `--with-uri`.
pub(crate) const PRELUDE_GROUP: &str = "uri";

/// Builds the complete URI declaration set.
pub(crate) fn uri_declarations() -> Program {
    crate::source::with_parse_mode(
        crate::source::SourceProfile::new(crate::source::SourceMode::Php),
        || {
            let mut declarations = bridge_externs();
            declarations.push(types::uri_types_declaration());
            declarations.push(rfc3986::rfc3986_declaration());
            declarations.push(whatwg::whatwg_declaration());
            declarations
        },
    )
}

/// Declares the bridge symbols the prelude calls.
fn bridge_externs() -> Vec<Stmt> {
    vec![
        extern_fn("elephc_uri_parse", "elephc_uri")
            .param("kind", CType::Int)
            .param("input", CType::Str)
            .param("base", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_free", "elephc_uri")
            .param("handle", CType::Int)
            .build(),
        extern_fn("elephc_uri_clone", "elephc_uri")
            .param("handle", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_component", "elephc_uri")
            .param("handle", CType::Int)
            .param("component", CType::Int)
            .param("mode", CType::Int)
            .returns(CType::NullableStr)
            .build(),
        extern_fn("elephc_uri_port", "elephc_uri")
            .param("handle", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_with", "elephc_uri")
            .param("handle", CType::Int)
            .param("component", CType::Int)
            .param("value", CType::Str)
            .param("is_null", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_charge_units", "elephc_uri")
            .param("handle", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_with_port", "elephc_uri")
            .param("handle", CType::Int)
            .param("port", CType::Int)
            .param("is_null", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_to_string", "elephc_uri")
            .param("handle", CType::Int)
            .param("mode", CType::Int)
            .param("exclude_fragment", CType::Int)
            .returns(CType::NullableStr)
            .build(),
        extern_fn("elephc_uri_equals", "elephc_uri")
            .param("left", CType::Int)
            .param("right", CType::Int)
            .param("include_fragment", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_error_message", "elephc_uri")
            .returns(CType::NullableStr)
            .build(),
        extern_fn("elephc_uri_error_count", "elephc_uri")
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_error_context", "elephc_uri")
            .param("index", CType::Int)
            .returns(CType::NullableStr)
            .build(),
        extern_fn("elephc_uri_error_type", "elephc_uri")
            .param("index", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_error_failure", "elephc_uri")
            .param("index", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_uri_clear_errors", "elephc_uri").build(),
    ]
}

/// Injects the URI prelude for an explicit PHP compatibility version.
///
/// `ext/uri` ships with PHP 8.5, so an 8.4-profile build must leave `Uri\` undeclared and let
/// its calls fail as undefined classes.
pub fn inject_if_used_for_version(
    program: Program,
    force: bool,
    php_version: crate::php_version::PhpVersion,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if php_version < crate::php_version::PhpVersion::Php85 {
        return program;
    }
    crate::compiler_stack::with_compiler_stack(|| {
        if !force && !program_uses_uri(&program) {
            return program;
        }
        let mut declarations = uri_declarations();
        inventory.record_program(PRELUDE_GROUP, &declarations);
        declarations.extend(program);
        declarations
    })
}

/// Returns whether `program` names a URI class or probes one through a literal.
pub fn program_uses_uri(program: &[Stmt]) -> bool {
    crate::compiler_stack::with_compiler_stack(|| {
        let usage = crate::prelude_prune::usage::collect(program);
        if usage.classes.iter().any(|name| name.starts_with("uri\\")) {
            return true;
        }
        if usage
            .literals
            .iter()
            .any(|literal| literal.to_ascii_lowercase().contains("uri\\"))
        {
            return true;
        }
        usage.functions.contains("extension_loaded")
            && usage
                .literals
                .iter()
                .any(|literal| literal.eq_ignore_ascii_case("uri"))
    })
}
