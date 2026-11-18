//! Purpose:
//! Joins the classes of indexed array literal items into one object element class, so the
//! literal's EIR storage agrees with the element type the checker infers for it.
//!
//! Called from:
//! - `crate::ir_lower::expr::indexed_array_literals::array_literal_type_for_ir()`.
//! - `crate::ir_lower::function` when it types a closure's returned literal.
//! - `crate::ir_lower::stmt::array_write_storage` for the nominal upcast of an element write.
//!
//! Key details:
//! - Every object element is one counted object pointer whatever its class, so two items of
//!   related classes share raw object slots typed as their common ancestor.
//! - The join mirrors `Checker::common_object_type` (`types::checker::type_compat`), which is
//!   what `Checker::merge_array_element_type` applies to the same literal. When the two layers
//!   disagree, a declared `array<Base>` slot is bound to boxed `mixed` cells that its readers
//!   take for object pointers.
//! - Only a pair of object types joins here. Every other pair keeps the representation
//!   widening of `merge_ir_indexed_element_type`.

use super::*;

/// Returns true when `class_name` is `ancestor_name`, extends it, or implements it.
///
/// This is the nominal relation under which an object slot typed as the ancestor can hold the
/// class without changing its view. The non-nominal `object` type (an empty class name) is
/// excluded on both sides.
pub(crate) fn object_class_is_a(
    ctx: &LoweringContext<'_, '_>,
    class_name: &str,
    ancestor_name: &str,
) -> bool {
    !class_name.is_empty()
        && !ancestor_name.is_empty()
        && (class_extends_class(ctx, class_name, ancestor_name)
            || ctx.type_is_or_implements_interface(class_name, ancestor_name))
}

/// Returns the nearest class or interface that both `left` and `right` can be viewed as.
///
/// One side wins when the other is an instance of it (a subclass, or an implementation of
/// that interface). Otherwise the nearest class both extend is the join, and two classes with
/// no shared class ancestor have none, even when they implement the same interface.
fn common_object_class(ctx: &LoweringContext<'_, '_>, left: &str, right: &str) -> Option<String> {
    if object_class_is_a(ctx, right, left) {
        return Some(left.to_string());
    }
    if object_class_is_a(ctx, left, right) {
        return Some(right.to_string());
    }
    let parent_of = |name: &str| {
        ctx.classes
            .get(name)
            .and_then(|info| info.parent.as_deref())
            .map(|parent| parent.trim_start_matches('\\').to_string())
    };
    let mut left_ancestors = std::collections::HashSet::new();
    let mut current = parent_of(left);
    while let Some(name) = current {
        current = parent_of(&name);
        if !left_ancestors.insert(name) {
            break;
        }
    }
    let mut seen = std::collections::HashSet::new();
    let mut current = parent_of(right);
    while let Some(name) = current {
        if left_ancestors.contains(&name) {
            return Some(name);
        }
        current = parent_of(&name);
        if !seen.insert(name) {
            break;
        }
    }
    None
}

/// Merges the storage element types of two items of one indexed array literal.
///
/// Two object items of related classes keep object storage typed as their common ancestor,
/// which is the element the checker infers for the literal. Every other pair follows
/// `merge_ir_indexed_element_type`, so unrelated classes still widen to boxed `mixed` slots.
pub(crate) fn merge_ir_literal_element_type(
    ctx: &LoweringContext<'_, '_>,
    left: PhpType,
    right: PhpType,
) -> PhpType {
    if let (PhpType::Object(left_class), PhpType::Object(right_class)) = (&left, &right) {
        if left_class != right_class {
            if let Some(common) = common_object_class(ctx, left_class, right_class) {
                return PhpType::Object(common);
            }
        }
    }
    merge_ir_indexed_element_type(left, right)
}
