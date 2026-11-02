//! Purpose:
//! Finds the static methods that only a DESCENDANT of the current class declares, for a
//! `static::method()` call made from that class.
//!
//! Called from:
//! - `crate::types::checker::inference::objects::methods` (type inference of the call).
//! - `crate::ir_lower::expr::static_method_calls` (lowering of the call).
//!
//! Key details:
//! - PHP resolves `static::m()` against the late-static class when the call runs. A method the
//!   lexical class never declared (a protected `getFacadeAccessor()` that only each subclass
//!   defines) is therefore reachable, and visible: a protected method is accessible from any
//!   scope related to its declaring class in either direction.
//! - Both passes must agree on the target list, so it is computed in one place. The list is sorted
//!   so the emitted dispatch ladder is deterministic.
//! - No stub is synthesized in the parent: that would change visibility, `method_exists()` and the
//!   `__callStatic` fallback. The call site dispatches on the late-static class name instead.

use crate::fast_hash::FastMap;
use crate::parser::ast::Visibility;
use crate::types::ClassInfo;

/// Returns the classes below `current` that make `method_key` callable as `static::method()`.
///
/// Empty when `current` itself already has the method (static or instance), because the ordinary
/// static-vtable dispatch covers that case. Private methods are skipped: from the parent's scope
/// PHP treats them as inaccessible and falls back to `__callStatic`.
pub(crate) fn child_only_static_targets(
    classes: &FastMap<String, ClassInfo>,
    current: &str,
    method_key: &str,
) -> Vec<String> {
    let Some(current_info) = classes.get(current) else {
        return Vec::new();
    };
    if current_info.static_methods.contains_key(method_key)
        || current_info.methods.contains_key(method_key)
    {
        return Vec::new();
    }
    let mut targets = classes
        .iter()
        .filter(|(name, info)| {
            name.as_str() != current
                && info.static_methods.contains_key(method_key)
                && !matches!(
                    info.static_method_visibilities.get(method_key),
                    Some(Visibility::Private)
                )
                && descends_from(classes, name, current)
        })
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    targets.sort();
    targets
}

/// Returns whether `class_name` has `ancestor` somewhere on its parent chain.
fn descends_from(classes: &FastMap<String, ClassInfo>, class_name: &str, ancestor: &str) -> bool {
    let mut cursor = classes.get(class_name).and_then(|info| info.parent.as_deref());
    // The depth cap keeps a malformed inheritance cycle from spinning.
    for _ in 0..256 {
        let Some(parent) = cursor else {
            return false;
        };
        if parent == ancestor {
            return true;
        }
        cursor = classes.get(parent).and_then(|info| info.parent.as_deref());
    }
    false
}

/// Returns whether a string LITERAL can never bind a numeric parameter of a late-bound
/// constructor, so the descendant is not a `new static` candidate at this site.
///
/// `Limit::perSecond()` writes `new static('', $maxAttempts, $decaySeconds)` while its descendant
/// `GlobalLimit` narrows the constructor to `(int $maxAttempts, int $decaySeconds)`. php raises a
/// `TypeError` there, but only for a program that really calls `GlobalLimit::perSecond()`; the
/// checker used to refuse the whole program instead. A literal with no digit at all is not a
/// numeric string in any form php accepts, so the verdict is certain; anything that might be
/// numeric (`'5'`, `' 12'`, `'1e3'`) keeps binding and is judged at run time.
pub(crate) fn literal_string_cannot_bind_numeric(
    param: &crate::types::PhpType,
    arg: &crate::parser::ast::Expr,
) -> bool {
    use crate::types::PhpType;
    let crate::parser::ast::ExprKind::StringLiteral(text) = &arg.kind else {
        return false;
    };
    matches!(param.codegen_repr(), PhpType::Int | PhpType::Float)
        && !text.bytes().any(|byte| byte.is_ascii_digit())
}

/// Rewrites `static::method(...)` made from an INSTANCE method to `$this->method(...)` when
/// `method` is an instance method of the current class.
///
/// php forwards the calling `$this` through `static::`, `self::` and `parent::` calls to an
/// instance method; with `static::` the target is resolved on the late-static class, which for
/// an instance method is the class of `$this` -- an ordinary virtual call. Eloquent's
/// `Model::isPrunable()` writes `... || static::isMassPrunable()` for exactly that. The
/// rewritten call carries the original span, so the checker and lowering (which both call this)
/// key their per-site metadata identically.
///
/// Returns `None` for any other receiver, from a static method, or when the class also has a
/// static method of that name (the static-method dispatch keeps that case).
pub(crate) fn static_receiver_instance_method_call(
    classes: &FastMap<String, ClassInfo>,
    current_class: Option<&str>,
    in_static_method: bool,
    receiver: &crate::parser::ast::StaticReceiver,
    method: &str,
    args: &[crate::parser::ast::Expr],
    span: crate::span::Span,
) -> Option<crate::parser::ast::Expr> {
    use crate::parser::ast::{Expr, ExprKind, StaticReceiver};
    if in_static_method || !matches!(receiver, StaticReceiver::Static) {
        return None;
    }
    let info = classes.get(current_class?)?;
    let method_key = crate::names::php_symbol_key(method);
    if !info.methods.contains_key(&method_key) || info.static_methods.contains_key(&method_key) {
        return None;
    }
    Some(Expr::new(
        ExprKind::MethodCall {
            object: Box::new(Expr::new(ExprKind::This, span)),
            method: method.to_string(),
            args: args.to_vec(),
        },
        span,
    ))
}
