//! Codegen for `is_a` and `is_subclass_of`.
//!
//! When the second argument is a literal class/interface name and the
//! first argument's static type is `Object(...)`, the result folds at
//! compile time:
//!
//!   `is_a($obj, "Foo")`            — same class, parent chain, or
//!                                    implemented interface
//!   `is_subclass_of($obj, "Foo")`  — same as above but excluding the
//!                                    case where the static type IS Foo
//!
//! For non-literal target arguments or non-Object first arguments, the
//! result is `false`. Both arguments are still evaluated for side
//! effects.

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
    emitter.comment(&format!("{}() — AOT static-type check", name));

    // Eval first arg, capture static type, eval rest for side effects.
    let arg_ty = emit_expr(&args[0], emitter, ctx, data);
    for arg in args.iter().skip(1) {
        emit_expr(arg, emitter, ctx, data);
    }

    let exclude_self = name == "is_subclass_of";
    let result = static_relation_holds(&arg_ty, &args[1], ctx, exclude_self);

    abi::emit_load_int_immediate(
        emitter,
        abi::int_result_reg(emitter),
        if result { 1 } else { 0 },
    );
    Some(PhpType::Bool)
}

fn static_relation_holds(
    arg_ty: &PhpType,
    target_arg: &Expr,
    ctx: &Context,
    exclude_self: bool,
) -> bool {
    let PhpType::Object(obj_class) = arg_ty else {
        return false;
    };
    let ExprKind::StringLiteral(target) = &target_arg.kind else {
        return false;
    };
    let obj_class = obj_class.trim_start_matches('\\');
    let target = target.trim_start_matches('\\');

    if !exclude_self && obj_class == target {
        return true;
    }

    // Walk the parent chain.
    let mut current = obj_class.to_string();
    while let Some(info) = ctx.classes.get(&current) {
        if let Some(parent) = &info.parent {
            let parent_clean = parent.trim_start_matches('\\');
            if parent_clean == target {
                return true;
            }
            current = parent.clone();
        } else {
            break;
        }
    }

    // Walk implemented (and transitively-inherited) interfaces.
    if let Some(info) = ctx.classes.get(obj_class) {
        for iface in &info.interfaces {
            if iface.trim_start_matches('\\') == target {
                return true;
            }
        }
    }

    false
}
