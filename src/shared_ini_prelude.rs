//! Purpose:
//! Builds shared Core/mbstring INI routing used by both CLI and web compatibility wrappers.
//!
//! Called from:
//! - The opcache, web, and version prelude declaration builders.
//!
//! Key details:
//! - Directive names derive from neutral catalogs; public declaration guards remain with each prelude.
//! - Extension filters use exact lowercase names, and combined enumeration sorts all keys.

use crate::{parser::ast::{BinOp, CastType, Expr, Stmt, TypeExpr}, synthetic_class::*};
use elephc_builtin_contract::mbstring_abi::ini::{catalog, core, INI_GET, INI_SET, INI_RESTORE, INI_GET_ALL};

/// Routes a known shared directive before the existing session/opcache fallback.
pub(crate) fn directive(operation: u32) -> Stmt {
    let condition = core::DIRECTIVES.iter().chain(catalog::DIRECTIVES.iter())
        .map(|entry| e_binop(e_var("option"), BinOp::StrictEq, e_str(entry.name)))
        .reduce(|left, right| e_binop(left, BinOp::Or, right)).expect("shared INI catalog");
    let value = if operation == INI_SET { e_cast(CastType::String, e_var("value")) } else { e_str("") };
    let call = e_call("__elephc_shared_ini", vec![e_int(i64::from(operation)), e_var("option"), value, e_bool(false)]);
    let body = match operation {
        INI_GET | INI_SET => vec![s_return(call)],
        INI_RESTORE => vec![s_expr(call), s_return_void()],
        _ => unreachable!("single-directive INI operation"),
    };
    s_if(condition, body, vec![], None)
}

/// Handles mbstring's explicit extension filter before legacy unknown-module diagnostics.
pub(crate) fn all_prefix() -> Vec<Stmt> {
    ["mbstring"].into_iter().map(|module| s_if(
        e_binop(e_var("extension"), BinOp::StrictEq, e_str(module)),
        vec![s_return(e_cast(CastType::Array, all(module, e_var("details"))))], vec![], None,
    )).collect()
}

/// Builds the same extension filtering and sorted enumeration around CLI or session-aware legacy rows.
pub(crate) fn all_declaration(web: bool) -> Stmt {
    let mut unsupported = e_binop(e_binop(e_var("extension"), BinOp::StrictNotEq, e_null()), BinOp::And,
        e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("zend opcache")));
    unsupported = e_binop(unsupported, BinOp::And, e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("core")));
    if web { unsupported = e_binop(unsupported, BinOp::And,
        e_binop(e_var("extension"), BinOp::StrictNotEq, e_str("session"))); }
    let legacy = |details| e_call(match (web, details) {
        (true, true) => "__elephc_ini_all_details", (true, false) => "__elephc_ini_all_plain",
        (false, true) => "__elephc_opcache_ini_all_details", (false, false) => "__elephc_opcache_ini_all_plain",
    }, if web { vec![e_var("extension")] } else { vec![] });
    let mut body = all_prefix();
    let message = e_binop(e_binop(e_str("ini_get_all(): Extension \""), BinOp::Concat,
        e_var("extension")), BinOp::Concat, e_str("\" cannot be found"));
    let warning = if web { e_call("trigger_error", vec![message, e_const("E_WARNING")]) }
        else { e_call("fwrite", vec![e_const("STDERR"), e_binop(e_binop(e_str("Warning: "), BinOp::Concat, message),
            BinOp::Concat, e_str("\n"))]) };
    body.push(s_if(unsupported, vec![
        s_if(e_call("__elephc_ini_module_known", vec![e_var("extension")]), vec![s_return(e_array(vec![]))], vec![], None),
        s_expr(warning),
        s_return(e_bool(false)),
    ], vec![], None));
    body.push(s_if(e_var("details"), all_result(legacy(true), true), vec![], None));
    body.extend(all_result(legacy(false), false));
    function("ini_get_all")
        .param_default("extension", t_nullable(TypeExpr::Str), e_null())
        .param_default("details", TypeExpr::Bool, e_bool(true))
        .returns(t_union(vec![t_array(), TypeExpr::False]))
        .body(body).build()
}

/// Adds shared rows for null/Core enumeration and sorts the final combined result.
pub(crate) fn all_result(legacy: Expr, details: bool) -> Vec<Stmt> {
    let suffix = if details { "details" } else { "plain" };
    let result = format!("__elephc_shared_all_{suffix}");
    let mut body = vec![s_assign(&result, e_array(vec![]))];
    for (index, source) in [all("core", e_bool(details)), all("mbstring", e_bool(details)), legacy.clone()].into_iter().enumerate() {
        let key = format!("__elephc_shared_key_{suffix}_{index}");
        let value = format!("__elephc_shared_value_{suffix}_{index}");
        body.push(s_foreach(source, Some(&key), &value, vec![
            s_array_assign(&result, e_cast(CastType::String, e_var(&key)), e_var(&value)),
        ]));
    }
    body.push(s_expr(e_call("ksort", vec![e_var(&result)])));
    body.push(s_return(e_var(&result)));
    // PHP's Core module number is zero, so its explicit name also disables per-module filtering.
    vec![s_if(e_binop(e_binop(e_var("extension"), BinOp::StrictEq, e_null()), BinOp::Or,
        e_binop(e_var("extension"), BinOp::StrictEq, e_str("core"))), body, vec![], None), s_return(legacy)]
}

/// Forms one typed internal invocation without redeclaring metadata or exposing bridge operations publicly.
fn all(module: &str, details: Expr) -> Expr {
    e_call("__elephc_shared_ini", vec![e_int(i64::from(INI_GET_ALL)), e_str(module), e_str(""), details])
}
