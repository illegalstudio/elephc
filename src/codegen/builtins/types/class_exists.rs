//! Codegen for `class_exists` / `interface_exists` / `trait_exists` /
//! `enum_exists`.
//!
//! In the AOT model, the autoload pass has already resolved every class
//! the program either references directly or names through
//! `class_exists("Literal", true)`. The corresponding entry is therefore
//! present in `ctx.classes` (or `interfaces` / `enums`) by the time we
//! reach this codegen.
//!
//! Decision matrix at compile time:
//!   * literal class name + present in the relevant ctx map → emit `1`
//!   * literal class name + absent → emit `0`
//!   * non-literal argument → emit `1` (the rest of the program would
//!     have failed earlier if the class didn't compile in)

use crate::codegen::abi;
use crate::codegen::context::Context;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::expr::emit_expr;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::PhpType;

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
    let value = literal_lookup_result(name, args, ctx).unwrap_or(1);
    abi::emit_load_int_immediate(emitter, abi::int_result_reg(emitter), value);
    Some(PhpType::Bool)
}

fn literal_lookup_result(name: &str, args: &[Expr], ctx: &Context) -> Option<i64> {
    let first = args.first()?;
    let ExprKind::StringLiteral(class) = &first.kind else {
        return None;
    };
    let cleaned = class.trim_start_matches('\\');
    let present = match name {
        "class_exists" => ctx.classes.contains_key(cleaned),
        "interface_exists" => ctx.interfaces.contains_key(cleaned),
        "enum_exists" => ctx.enums.contains_key(cleaned),
        // The compiler doesn't keep a separate trait registry on Context
        // — traits are flattened away. Always-present is the conservative
        // answer for AOT.
        "trait_exists" => true,
        _ => return None,
    };
    Some(if present { 1 } else { 0 })
}
