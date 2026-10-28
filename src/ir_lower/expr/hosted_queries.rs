//! Purpose:
//! Answers `property_exists()` and `get_class_vars()` for a hosted PHP extension class from the
//! engine: the extension declares that class's properties on its engine class, which the
//! compiler's class metadata (the wrapper's) does not hold.
//!
//! Called from:
//! - `crate::ir_lower::expr::function_calls`, before the builtin call is lowered.
//!
//! Key details:
//! - Only a target the compiler knows to be hosted is routed: a string literal naming a class
//!   the extension registers, or a value whose static type is a hosted class (or a subclass of
//!   one). Anything else keeps the builtin's own lowering.
//! - `get_class_vars()` depends on the calling scope, which is the lexical class here: its chain
//!   of class names goes to the engine as a literal, as the magic accessors publish it.

use super::*;

/// Lowers `property_exists()` or `get_class_vars()` on a hosted class through the prelude's
/// engine queries; `None` for any other call.
pub(super) fn lower_hosted_class_query(
    ctx: &mut LoweringContext<'_, '_>,
    name: &str,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let key = php_symbol_key(name.trim_start_matches('\\'));
    let arity = match key.as_str() {
        "property_exists" => 2,
        "get_class_vars" => 1,
        _ => return None,
    };
    if args.len() != arity
        || !ctx.classes.contains_key(crate::php_ext::prelude::OWNER_CLASS)
        || crate::types::call_args::has_named_args(args)
        || args.iter().any(is_spread_arg)
    {
        return None;
    }
    let hosted = |class: &str| {
        ctx.classes.get(class).is_some_and(|info| {
            info.interfaces.iter().any(|interface| interface.trim_start_matches('\\') == crate::php_ext::prelude::OBJECT_INTERFACE)
        })
    };
    let wrapper = |class: &str| {
        hosted(class) && ctx.classes.get(class).and_then(|info| info.parent.as_deref()).is_none_or(|parent| !hosted(parent))
    };
    let target_is_hosted = match &args[0].kind {
        // A class name: only the extension's own classes are engine classes.
        ExprKind::StringLiteral(class) => {
            let class = class.trim_start_matches('\\');
            ctx.classes.keys().find(|candidate| candidate.eq_ignore_ascii_case(class)).is_some_and(|class| wrapper(class))
        }
        _ if key == "property_exists" => isset_object_expr_type(ctx, &args[0])
            .as_ref()
            .and_then(|ty| singular_object_class(ty).map(|(class, _)| class.to_string()))
            .and_then(|class| normalized_class_name(&class))
            .is_some_and(|class| hosted(&class)),
        _ => false,
    };
    if !target_is_hosted {
        return None;
    }
    let call = if key == "property_exists" {
        crate::synthetic_class::e_call(&format!("\\{}", crate::php_ext::prelude::PROPERTY_EXISTS_FUNCTION), args.to_vec())
    } else {
        let mut chain = Vec::new();
        let mut current = ctx.current_class.clone();
        while let Some(class) = current {
            if chain.contains(&class) {
                break;
            }
            current = ctx.classes.get(&class).and_then(|info| info.parent.clone());
            chain.push(class);
        }
        crate::synthetic_class::e_call(
            &format!("\\{}", crate::php_ext::prelude::CLASS_VARS_FUNCTION),
            vec![args[0].clone(), crate::synthetic_class::e_str(&chain.join(","))],
        )
    };
    Some(lower_expr(ctx, &Expr::new(call.kind, expr.span)))
}
