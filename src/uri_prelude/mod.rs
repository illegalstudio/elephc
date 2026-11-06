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

use crate::names::php_symbol_key;
use crate::parser::ast::{BinOp, CType, Expr, Program, Stmt, TypeExpr};
use crate::synthetic_class::{
    e_binop, e_call, e_instance_of, e_new_fq, e_not, e_null, e_str, e_var, extern_fn, s_if,
    s_throw, t_class, t_nullable,
};

/// The prelude group id used by declaration reachability and `--with-uri`.
pub(crate) const PRELUDE_GROUP: &str = "uri";

/// Builds the complete URI declaration set.
pub(crate) fn uri_declarations() -> Program {
    crate::source::with_parse_mode(
        crate::source::SourceProfile::new(crate::source::SourceMode::Php),
        || {
            let mut declarations = bridge_externs();
            declarations.push(types::uri_type_name_declaration());
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

/// Builds php-src's `TypeError` message tail: `<prefix><type> given`.
pub(super) fn type_error_message(prefix: &str, value: &str) -> Expr {
    e_binop(
        e_str(prefix),
        BinOp::Concat,
        e_binop(
            e_call("\\__elephc_uri_type_name", vec![e_var(value)]),
            BinOp::Concat,
            e_str(" given"),
        ),
    )
}

/// Rejects a runtime argument that is not an instance of the declared URI class.
///
/// `allow_null` matches a nullable parameter; a null value is legal there, while `equals()`
/// takes a non-nullable URI and must reject it.
pub(super) fn s_reject_wrong_uri_instance(
    value: &str,
    class: &str,
    prefix: &str,
    allow_null: bool,
) -> Stmt {
    let wrong_type = e_not(e_instance_of(e_var(value), class));
    let condition = if allow_null {
        e_binop(
            e_binop(e_var(value), BinOp::StrictNotEq, e_null()),
            BinOp::And,
            wrong_type,
        )
    } else {
        wrong_type
    };
    s_if(
        condition,
        vec![s_throw(e_new_fq(
            "TypeError",
            vec![type_error_message(prefix, value)],
        ))],
        vec![],
        None,
    )
}

/// Restores php-src's reflected parameter types for methods whose internal contract stays
/// wider (`mixed`) so nullable and runtime-checked URI values remain callable.
///
/// The public methods keep the prelude's runtime guards and exact `TypeError` wording; this
/// only repairs `ReflectionParameter::getType()` and the callable-signature metadata.
pub fn apply_reflection_signatures(check: &mut crate::types::CheckResult) {
    let rfc_class = t_class("\\Uri\\Rfc3986\\Uri");
    let whatwg_class = t_class("\\Uri\\WhatWg\\Url");
    patch_signature(
        check,
        "Uri\\Rfc3986\\Uri",
        "parse",
        true,
        1,
        t_nullable(rfc_class.clone()),
    );
    patch_signature(
        check,
        "Uri\\Rfc3986\\Uri",
        "__construct",
        false,
        1,
        t_nullable(rfc_class.clone()),
    );
    patch_signature(check, "Uri\\Rfc3986\\Uri", "equals", false, 0, rfc_class);
    patch_signature(
        check,
        "Uri\\WhatWg\\Url",
        "parse",
        true,
        1,
        t_nullable(whatwg_class.clone()),
    );
    patch_signature(
        check,
        "Uri\\WhatWg\\Url",
        "__construct",
        false,
        1,
        t_nullable(whatwg_class.clone()),
    );
    patch_signature(check, "Uri\\WhatWg\\Url", "equals", false, 0, whatwg_class);
}

/// Replaces one reflected parameter type on a static or instance method.
fn patch_signature(
    check: &mut crate::types::CheckResult,
    class: &str,
    method: &str,
    is_static: bool,
    index: usize,
    ty: TypeExpr,
) {
    let Some(info) = check.classes.get_mut(class) else {
        return;
    };
    let signatures = if is_static {
        &mut info.static_methods
    } else {
        &mut info.methods
    };
    let Some(signature) = signatures.get_mut(&php_symbol_key(method)) else {
        return;
    };
    if let Some(slot) = signature.param_type_exprs.get_mut(index) {
        *slot = Some(ty);
    }
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
