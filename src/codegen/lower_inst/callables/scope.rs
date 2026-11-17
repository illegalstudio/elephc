//! Purpose:
//! Applies PHP creation-scope visibility to callable factory candidates.
//!
//! Called from:
//! - The instance and static descriptor inventories in the parent callable lowerer.
//!
//! Key details:
//! - Protected overrides use their prototype root; private static fallbacks stop at shadows.

use super::{FunctionContext, Visibility};

/// Applies creation-scope access while leaving ordinary callback inventories public.
pub(super) fn factory_method_visible(
    ctx: &FunctionContext<'_>, visibility: Option<&Visibility>, declaring: &str,
    method: &str, is_static: bool, scoped: bool,
) -> bool {
    match visibility {
        Some(Visibility::Public) => true,
        Some(Visibility::Private) if scoped => ctx.lexical_class() == Some(declaring),
        Some(Visibility::Protected) if scoped => {
            let mut root = declaring;
            while let Some(parent) = ctx.module.class_infos.get(root).and_then(|info| info.parent.as_deref()) {
                let Some(info) = ctx.module.class_infos.get(parent) else { break; };
                let visibility = if is_static { info.static_method_visibilities.get(method) }
                    else { info.method_visibilities.get(method) };
                if !matches!(visibility, Some(Visibility::Public | Visibility::Protected)) { break; }
                root = parent;
            }
            ctx.lexical_class().is_some_and(|scope|
                super::super::method_dispatch::class_is_same_or_subclass(ctx, scope, root)
                    || super::super::method_dispatch::class_is_same_or_subclass(ctx, root, scope))
        }
        _ => false,
    }
}

/// Allows a lexical private static method through an unshadowed subclass name.
pub(super) fn inherits_lexical_private_static(
    ctx: &FunctionContext<'_>, class_name: &str, method: &str,
) -> bool {
    let Some(scope) = ctx.lexical_class() else { return false; };
    if class_name == scope { return false; }
    let mut current = class_name;
    loop {
        if current == scope { return true; }
        let Some(info) = ctx.module.class_infos.get(current) else { return false; };
        if info.static_methods.contains_key(method) || info.methods.contains_key(method) { return false; }
        let Some(parent) = info.parent.as_deref() else { return false; };
        current = parent;
    }
}
