//! Purpose:
//! Emits AOT results for `class_exists`, `interface_exists`, `trait_exists`, and `enum_exists`.
//! Evaluates arguments for side effects, then lowers literal lookups to an integer bool.
//!
//! Called from:
//! - `crate::codegen::builtins::types::emit()`
//!
//! Key details:
//! - The autoload pass has already resolved literal autoload demands before codegen.
//! - Non-literal arguments are checker errors; codegen falls back to `false` defensively.

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

/// Emits code for `class_exists`, `interface_exists`, `trait_exists`, or `enum_exists`.
///
/// Evaluates all arguments for side effects, then resolves literal string class names
/// against the folded symbol tables. Non-literal arguments are checker errors, but
/// codegen defensively returns `false` if encountered.
///
/// Returns `PhpType::Bool` in `abi::int_result_reg()`.
pub fn emit(
    name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment(&format!("{}()", name));
    // Always evaluate every argument for side-effects (the user may have
    // passed an expression with observable behavior).
    for arg in args {
        emit_expr(arg, emitter, ctx, data);
    }
    let value = literal_lookup_result(name, args, ctx).unwrap_or(0);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), value);
    Some(PhpType::Bool)
}

/// Resolves a literal class name argument to a folded symbol table lookup result.
///
/// Extracts the string literal from the first argument, normalizes it with a leading
/// backslash trim, and checks the appropriate symbol table based on `name`.
/// Returns `Some(1)` if the class/interface/enum/trait exists, `Some(0)` if not,
/// or `None` if the first argument is not a string literal.
fn literal_lookup_result(name: &str, args: &[Expr], ctx: &Context) -> Option<i64> {
    let first = args.first()?;
    let ExprKind::StringLiteral(class) = &first.kind else {
        return None;
    };
    let cleaned = class.trim_start_matches('\\');
    let present = match name {
        "class_exists" => contains_folded(
            ctx.classes
                .keys()
                .filter(|name| !is_internal_synthetic_class_name(name)),
            cleaned,
        ),
        "interface_exists" => contains_folded(ctx.interfaces.keys(), cleaned),
        "enum_exists" => contains_folded(ctx.enums.keys(), cleaned),
        "trait_exists" => contains_folded(ctx.traits.iter(), cleaned),
        _ => return None,
    };
    Some(if present { 1 } else { 0 })
}

/// Checks whether `needle` (a PHP-style symbol name) exists in `names` using PHP symbol key comparison.
///
/// PHP symbol names are case-insensitive; this normalizes both the needle and each
/// name via `php_symbol_key` before comparing.
fn contains_folded<'a>(
    mut names: impl Iterator<Item = &'a String>,
    needle: &str,
) -> bool {
    let needle_key = php_symbol_key(needle);
    names.any(|name| php_symbol_key(name) == needle_key)
}

/// Returns true when internal synthetic class name.
fn is_internal_synthetic_class_name(name: &str) -> bool {
    php_symbol_key(name).starts_with("__elephc")
}
