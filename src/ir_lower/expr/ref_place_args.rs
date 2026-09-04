//! Purpose:
//! Lowers a mutating builtin call whose by-reference array argument is a *place* other than
//! a plain local — an object property, a static property, or a container element — as an
//! explicit read/mutate/write-back sequence through a hidden temporary.
//!
//! Called from:
//! - `crate::ir_lower::expr::lower_function_call()`, before the builtin fast paths, so the
//!   rewritten call re-enters the ordinary local-variable by-reference lowering.
//!
//! Key details:
//! - Only a receiver the backend can resolve to a slot reaches its COW write-back
//!   (`ReceiverPlace` in `crate::codegen::lower_inst::receiver_place`), and a plain local
//!   variable is the only argument shape that produces one.
//!   A property or element operand is loaded, `acquire`d, handed to the runtime, and released,
//!   so `__rt_array_ensure_unique` separates a private copy that nothing ever stores back.
//!   That is a silent wrong answer: `usort($obj->items, ...)` used to leave `$obj->items`
//!   untouched with no diagnostic.
//! - The rewrite is `$tmp = <place>; f($tmp, ...); <place> = $tmp;`, where `$tmp` is a
//!   synthetic slot with ordinary PHP local ownership (`declare_synthetic_php_local`), so the
//!   store retains even when the place read is a borrowed pointer — a static-property load
//!   carries no reference of its own, and moving it into a hidden temp would let the
//!   write-back's release of the previous occupant free an array another variable still holds.
//! - Because `$tmp` retains, the runtime's ensure-unique sees a shared buffer and separates
//!   before mutating — which is exactly PHP's copy-on-write behavior: an earlier
//!   `$c = $obj->items;` alias stays unsorted, and a `usort` comparator that reads the
//!   property while sorting still sees the pre-sort array.
//! - Only array/hash-typed places are rewritten. Scalar by-reference parameters (`settype`,
//!   `preg_match` `$matches`, `str_replace` `$count`) keep their existing lowering and their
//!   existing diagnostics, because a hidden temp declared with the place's scalar type cannot
//!   represent a builtin that re-types its argument.
//! - Place types are resolved statically (no IR is emitted before the decision), so a shape
//!   this module cannot resolve falls through to the pre-existing lowering unchanged.

use crate::ir_lower::context::{LoweredValue, LoweringContext};
use crate::names::Name;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{FunctionSig, PhpType};

mod key_sort;

use super::{
    call_signature, is_spread_arg, lower_expr, lower_function_call,
    lower_non_local_assignment_write, normalize_value_php_type, source_prefers_extension_builtin,
    static_property_result_type,
};

/// One by-reference argument rewritten into a hidden temporary.
///
/// `place` is the stabilized target expression written back after the call; `temp` is the
/// hidden local holding the mutated array while the builtin runs.
struct RefPlacePlan {
    index: usize,
    place: Expr,
    temp: String,
}

/// Lowers a builtin call whose by-reference array argument is a non-local place.
///
/// Returns `None` — leaving the call to the ordinary lowering — unless the callee is a
/// registry builtin with a by-reference regular parameter and at least one such argument is a
/// statically array-typed property, static property, or container element. On a rewrite the
/// place is read into a hidden temporary, the call is re-lowered against that temporary, and
/// the temporary is written back to the place so the caller's storage observes the mutation.
pub(super) fn lower_builtin_ref_place_call(
    ctx: &mut LoweringContext<'_, '_>,
    name: &Name,
    args: &[Expr],
    expr: &Expr,
) -> Option<LoweredValue> {
    let canonical = name.as_str();
    let prefer_extension = source_prefers_extension_builtin(canonical);
    if !prefer_extension && ctx.extern_functions.contains_key(canonical) {
        // An EXTERN (C ABI) callee takes a raw pointer, not a php reference cell, so the
        // read/call/write-back rewrite has nothing to write back through.
        return None;
    }
    let user_callee = !prefer_extension && ctx.functions.contains_key(canonical);
    let sig = call_signature(ctx, canonical, prefer_extension)?;
    if !sig.ref_params.iter().any(|is_ref| *is_ref) {
        return None;
    }
    if args.iter().any(is_spread_arg) {
        // A spread cannot be split into per-parameter places here; PHP also rejects spreading
        // into a by-reference parameter, and the checker already reports that.
        return None;
    }
    if let Some(result) =
        key_sort::lower_key_sort_ref_place_call(ctx, canonical, &sig, args, expr)
    {
        return Some(result);
    }
    let rewrite_indices: Vec<usize> = args
        .iter()
        .enumerate()
        .filter(|(index, arg)| {
            ref_param_place(&sig, *index, arg)
                .is_some_and(|place| is_rewritable_place(ctx, place, user_callee))
        })
        .map(|(index, _)| index)
        .collect();
    if rewrite_indices.is_empty() {
        return None;
    }
    let mut call_args: Vec<Expr> = args.to_vec();
    let mut plans: Vec<RefPlacePlan> = Vec::with_capacity(rewrite_indices.len());
    for index in rewrite_indices {
        let arg = &args[index];
        let place_arg = ref_param_place(&sig, index, arg)?;
        let place = stabilize_place(ctx, place_arg);
        let read = lower_expr(ctx, &place);
        let value_type = normalize_value_php_type(ctx.builder.value_php_type(read.value));
        let temp = ctx.declare_synthetic_php_local(value_type.clone());
        ctx.store_local(&temp, read, value_type, Some(place_arg.span));
        let variable = Expr::new(ExprKind::Variable(temp.clone()), place_arg.span);
        call_args[index] = match &arg.kind {
            ExprKind::NamedArg { name, .. } => Expr::new(
                ExprKind::NamedArg {
                    name: name.clone(),
                    value: Box::new(variable),
                },
                arg.span,
            ),
            _ => variable,
        };
        plans.push(RefPlacePlan { index, place, temp });
    }
    // Every rewritten argument now names a plain local (directly, or as the value of the named
    // argument it replaced), so the recursive call takes the ordinary by-reference path and
    // this rewrite cannot re-fire.
    debug_assert!(plans.iter().all(|plan| {
        let rewritten = &call_args[plan.index];
        let place = match &rewritten.kind {
            ExprKind::NamedArg { value, .. } => value.as_ref(),
            _ => rewritten,
        };
        matches!(place.kind, ExprKind::Variable(_))
    }));
    let result = lower_function_call(ctx, name, &call_args, expr);
    for plan in plans {
        let value = Expr::new(ExprKind::Variable(plan.temp), plan.place.span);
        lower_non_local_assignment_write(ctx, &plan.place, &value, plan.place.span);
    }
    Some(result)
}

/// Returns the argument expression bound to a by-reference parameter, or `None`.
///
/// A positional argument binds to the parameter at the same index; a named argument
/// (`sort(array: $obj->items)`) binds to the parameter its name selects, so both call forms
/// reach the same rewrite. Variadic tail positions are excluded because only the visible
/// regular parameters carry the registry's by-reference markers.
fn ref_param_place<'a>(sig: &FunctionSig, index: usize, arg: &'a Expr) -> Option<&'a Expr> {
    let regular_param_count = crate::types::call_args::regular_param_count(sig);
    let (param_index, place) = match &arg.kind {
        ExprKind::NamedArg { name, value } => (
            sig.params.iter().position(|(param, _)| param == name)?,
            value.as_ref(),
        ),
        _ => (index, arg),
    };
    if param_index >= regular_param_count {
        return None;
    }
    if !sig.ref_params.get(param_index).copied().unwrap_or(false) {
        return None;
    }
    Some(place)
}

/// Returns whether a by-reference argument is a non-local place this rewrite can carry.
///
/// Plain locals are excluded because the existing lowering already writes the separated array
/// back to their frame slot.
///
/// A SCALAR place is carried only for a USER callee. A builtin may RE-TYPE its by-reference
/// argument — `settype($obj->prop, "string")` is the whole point of that builtin — and a hidden
/// temporary declared with the place's current scalar type cannot represent that. A user
/// function's parameter carries a declared type instead, so the temporary and the write-back
/// agree by construction: `function bump(int &$n)` writes an int back into an int property.
fn is_rewritable_place(ctx: &LoweringContext<'_, '_>, arg: &Expr, user_callee: bool) -> bool {
    if !is_candidate_place_shape(arg) {
        return false;
    }
    static_place_type(ctx, arg).is_some_and(|php_type| {
        matches!(
            php_type.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. }
        ) || (user_callee
            && matches!(
                php_type.codegen_repr(),
                PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str
            ))
    })
}

/// Returns whether an argument has one of the place shapes this rewrite can read and write.
fn is_candidate_place_shape(arg: &Expr) -> bool {
    matches!(
        arg.kind,
        ExprKind::PropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::ArrayAccess { .. }
    )
}

/// Resolves the static PHP type of a place expression without emitting any IR.
///
/// Only the shapes this module can read and write back are resolved — locals, `$this`,
/// declared instance properties, declared static properties, and elements of those. Anything
/// else returns `None`, which keeps the call on its pre-existing lowering path.
fn static_place_type(ctx: &LoweringContext<'_, '_>, expr: &Expr) -> Option<PhpType> {
    match &expr.kind {
        ExprKind::Variable(name) => {
            if ctx.has_local_slot(name) {
                Some(ctx.local_type(name))
            } else {
                None
            }
        }
        ExprKind::This => {
            if ctx.has_local_slot("this") {
                Some(ctx.local_type("this"))
            } else {
                None
            }
        }
        ExprKind::PropertyAccess { object, property } => {
            let class_name = place_object_class_name(ctx, object)?;
            let class_info = ctx.classes.get(class_name.as_str())?;
            let (_, (_, property_ty)) = class_info.visible_property(property)?;
            Some(normalize_value_php_type(property_ty.clone()))
        }
        ExprKind::StaticPropertyAccess { receiver, property } => Some(
            static_property_result_type(ctx, receiver, property, expr),
        ),
        ExprKind::ArrayAccess { array, .. } => {
            match static_place_type(ctx, array)?.codegen_repr() {
                PhpType::Array(elem_ty) => Some(normalize_value_php_type(*elem_ty)),
                PhpType::AssocArray { value, .. } => Some(normalize_value_php_type(*value)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Resolves the class a property receiver refers to, for property-type lookup.
///
/// Returns `None` for a receiver whose static type is not a single known class — `Mixed`,
/// a union, or an unresolved local — so the caller leaves the argument on its existing path.
fn place_object_class_name(ctx: &LoweringContext<'_, '_>, object: &Expr) -> Option<String> {
    match static_place_type(ctx, object)?.codegen_repr() {
        PhpType::Object(class_name) => Some(class_name.trim_start_matches('\\').to_string()),
        _ => None,
    }
}

/// Rebuilds a place expression so it can be evaluated twice — once to read, once to write.
///
/// Container indexes are the only sub-expression that may carry side effects, so a non-trivial
/// index is evaluated once into a synthetic local and both evaluations read that local. The
/// rest of the receiver chain is composed exclusively of the shapes `static_place_type`
/// resolves, which are side-effect-free local, property, and element reads.
///
/// Infallible by construction: the caller only reaches this for an argument
/// `static_place_type` already resolved, and that resolver matches exactly the variants below.
/// An unmatched shape is returned unchanged, which is the conservative identity — it cannot be
/// reached without emitting IR for a place this module then refuses to write back.
fn stabilize_place(ctx: &mut LoweringContext<'_, '_>, place: &Expr) -> Expr {
    match &place.kind {
        ExprKind::PropertyAccess { object, property } => {
            let object = stabilize_place(ctx, object);
            Expr::new(
                ExprKind::PropertyAccess {
                    object: Box::new(object),
                    property: property.clone(),
                },
                place.span,
            )
        }
        ExprKind::ArrayAccess { array, index } => {
            let array = stabilize_place(ctx, array);
            let index = stabilize_index(ctx, index);
            Expr::new(
                ExprKind::ArrayAccess {
                    array: Box::new(array),
                    index: Box::new(index),
                },
                place.span,
            )
        }
        _ => place.clone(),
    }
}

/// Evaluates a container index once when re-evaluating it could repeat a side effect.
///
/// Literals and already-stored locals are re-read directly; anything else is lowered into a
/// synthetic local whose variable reference replaces the original index expression, so
/// `sort($m[next_index()])` calls `next_index()` exactly once like PHP.
fn stabilize_index(ctx: &mut LoweringContext<'_, '_>, index: &Expr) -> Expr {
    if matches!(
        index.kind,
        ExprKind::Variable(_)
            | ExprKind::This
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::StringLiteral(_)
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
    ) {
        return index.clone();
    }
    let value = lower_expr(ctx, index);
    let value_type = normalize_value_php_type(ctx.builder.value_php_type(value.value));
    let temp = ctx.declare_synthetic_php_local(value_type.clone());
    ctx.store_local(&temp, value, value_type, Some(index.span));
    Expr::new(ExprKind::Variable(temp), index.span)
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit coverage for the by-reference place rewrite's argument classification.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - These assertions are pure predicates over AST shapes; the end-to-end behavior is
    //!   covered by `tests/codegen/arrays/` and `tests/codegen/objects/property_access/`.

    use super::*;
    use crate::parser::ast::StaticReceiver;
    use crate::span::Span;

    /// A plain local argument is never treated as a rewritable place: the backend's
    /// local-slot write-back already stores the separated array back for it.
    #[test]
    fn plain_local_is_not_a_candidate_place_shape() {
        let local = Expr::new(ExprKind::Variable("a".to_string()), Span::dummy());
        assert!(!is_candidate_place_shape(&local));
    }

    /// Property, static-property, and element arguments are the shapes the rewrite considers.
    #[test]
    fn property_and_element_shapes_are_candidate_places() {
        let span = Span::dummy();
        let object = Expr::new(ExprKind::Variable("o".to_string()), span);
        let property = Expr::new(
            ExprKind::PropertyAccess {
                object: Box::new(object.clone()),
                property: "items".to_string(),
            },
            span,
        );
        let static_property = Expr::new(
            ExprKind::StaticPropertyAccess {
                receiver: StaticReceiver::Self_,
                property: "items".to_string(),
            },
            span,
        );
        let element = Expr::new(
            ExprKind::ArrayAccess {
                array: Box::new(object),
                index: Box::new(Expr::new(ExprKind::IntLiteral(0), span)),
            },
            span,
        );
        assert!(is_candidate_place_shape(&property));
        assert!(is_candidate_place_shape(&static_property));
        assert!(is_candidate_place_shape(&element));
    }
}
