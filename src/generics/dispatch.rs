//! Purpose:
//! Shares the generic-instantiation stability proof used by method dispatch and inlining.
//!
//! Called from:
//! - EIR method-target selection and method assembly lowering.
//!
//! Key details:
//! - Generic variance changes only object arguments; scalar arguments cannot widen in place.
//! - Object arguments remain conservative even when their declaration is invariant.

use crate::parser::ast::TypeExpr;

/// Returns whether generic variance cannot change this receiver's instantiated class.
/// Ordinary classes also qualify; inheritance and final-method rules are checked separately.
pub(crate) fn has_fixed_instantiation(class: &str) -> bool {
    if !class.contains('<') { return true; }
    let Some(TypeExpr::GenericClass { args, .. }) = super::instantiated_type(class) else {
        return false;
    };
    !args.is_empty() && args.iter().all(|arg| {
        crate::types::width_type_from_type_expr(arg).is_some()
            || !matches!(arg, TypeExpr::Named(_) | TypeExpr::GenericClass { .. })
    })
}

#[cfg(test)]
mod tests {
    use super::has_fixed_instantiation;

    /// Scalar slots cannot select a variance sibling, including mixed and container arguments.
    #[test]
    fn checked_managed_buffer_scalar_instantiations_are_fixed() {
        for name in ["Owner", "Owner<int>", "Owner<float>", "Owner<int,string>", "Owner<array<int>>", "Owner<u8>", "Owner<u32>", "Owner<f32>", "Owner<f64>"] {
            assert!(has_fixed_instantiation(name), "{name}");
        }
        for name in ["Owner<Animal>", "Owner<int,Animal>", "Owner<Owner<int>>", "Owner<\\u8>", "Owner<\\f32>", "Owner<", "Owner<>"] {
            assert!(!has_fixed_instantiation(name), "{name}");
        }
    }
}
