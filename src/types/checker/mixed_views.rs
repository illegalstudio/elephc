//! Purpose:
//! Separates nullsafe non-execution from the successful selection's declared value contract.
//!
//! Called from:
//! - Mixed member and dimension receiver proof validation.
//!
//! Key details:
//! - Reuses recorded receiver views and read-only declaration queries, without re-inferring operands.
//! - A genuinely nullable result remains nullable and still requires an explicit guard.

use super::Checker;
use crate::{parser::ast::{Expr, ExprKind}, types::PhpType};

impl Checker {
    /// Removes only null introduced by upstream non-execution when the selected value excludes null.
    pub(crate) fn mixed_selection_receiver_view(&self, receiver: &Expr, observed: &PhpType) -> PhpType {
        if !Self::reference_receiver_uses_nullsafe(receiver) { return observed.clone(); }
        if self.successful_selection_type(receiver).as_ref()
            .is_some_and(crate::semantics::nullability::non_null_contract) {
            self.strip_void_from_union(observed)
        } else { observed.clone() }
    }

    /// Reads one successful selection contract from the checker's existing receiver metadata.
    fn successful_selection_type(&self, selection: &Expr) -> Option<PhpType> {
        if let ExprKind::ArrayAccess { array, .. } = &selection.kind {
            return match self.successful_selection_type(array)? {
                PhpType::Array(element) | PhpType::AssocArray { value: element, .. } => Some(*element),
                _ => None,
            };
        }
        let receiver = self.member_receiver_types.get(
            &(self.current_loop_storage_scope.clone(), selection.span))?.as_ref()?;
        let class = match receiver {
            PhpType::Object(name) => name.clone(),
            PhpType::Union(_) => self.union_single_object_class(receiver)?,
            _ => return None,
        };
        match &selection.kind {
            ExprKind::PropertyAccess { property, .. } | ExprKind::NullsafePropertyAccess { property, .. } =>
                self.infer_property_on_class_type(&class, property, selection).ok(),
            ExprKind::DynamicPropertyAccess { property, .. } | ExprKind::NullsafeDynamicPropertyAccess { property, .. } => {
                let ExprKind::StringLiteral(property) = &property.kind else { return None; };
                self.infer_property_on_class_type(&class, property, selection).ok()
            }
            ExprKind::MethodCall { method, .. } | ExprKind::NullsafeMethodCall { method, .. } => {
                let key = crate::names::php_symbol_key(method);
                self.classes.get(&class).and_then(|info| info.methods.get(&key))
                    .or_else(|| self.interfaces.get(&class).and_then(|info| info.methods.get(&key)))
                    .map(|signature| signature.return_type.clone())
            }
            _ => None,
        }
    }
}
