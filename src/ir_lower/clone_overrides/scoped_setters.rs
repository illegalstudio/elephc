//! Purpose:
//! Generates the per-`(declaring class, property)` setter helpers a `clone()` applicator calls
//! when the INVOCATION SCOPE, not the clone's own class, owns the private slot being written.
//!
//! Called from:
//! - `super::lower_clone_override_applicators()` before the applicator bodies that call them.
//!
//! Key details:
//! - `ClassInfo::visible_property_index` answers by name, and a child that redeclares a private
//!   property shadows its parent's slot there. Writing `$this->p = $v` inside an applicator typed
//!   with the CLONE's class would therefore always land on the child slot, which is the wrong one
//!   whenever php resolves the name through an ancestor scope.
//! - The helper closes that gap without a second slot-resolution rule: its `$this` is typed with
//!   the ANCESTOR and its lexical class IS that ancestor, so the ordinary property pipeline picks
//!   the ancestor's own slot exactly as a method written inside that ancestor would. Passing the
//!   clone is an upcast, which is always sound here because the arm only exists when the clone's
//!   class is that ancestor or a descendant of it.
//! - The body is one ordinary assignment, so typed weak coercion, set hooks and ownership all
//!   come from the same lowering a hand-written `$this->p = $v` uses.

use std::collections::HashMap;

use crate::ir::Module;
use crate::parser::ast::{Expr, ExprKind, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{CheckResult, FunctionSig, PhpType};

/// Parameter carrying the override value into a scoped setter helper.
pub(super) const VALUE_PARAM: &str = "__elephc_clone_scoped_value";

/// Returns the reserved EIR function name of one scoped setter helper.
///
/// Reserved for the same reason the applicator name is: a user `function _clone_set_4_0()` would
/// otherwise be called in place of the generated helper and the ancestor's private slot would
/// never be written. See `crate::names::internal_generated_function_name`.
pub(super) fn helper_name(class_id: u64, slot: usize) -> String {
    crate::names::internal_generated_function_name("clone_set", &[class_id, slot as u64])
}

/// Lowers the scoped setter helper for one `(declaring class, property)` pair.
///
/// Returns the helper's EIR function name and signature, or `None` when the class no longer
/// declares the property by name, in which case the caller keeps its own receiver.
pub(super) fn lower(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &HashMap<String, FunctionSig>,
    scope_class: &str,
    property: &str,
) -> Option<(String, FunctionSig)> {
    let class_info = module.class_infos.get(scope_class)?;
    let slot = class_info.visible_property_index(property)?;
    let function_name = helper_name(class_info.class_id, slot);
    let body = vec![Stmt::new(
        StmtKind::PropertyAssign {
            object: Box::new(Expr::new(ExprKind::This, Span::dummy())),
            property: property.to_string(),
            value: Expr::new(ExprKind::Variable(VALUE_PARAM.to_string()), Span::dummy()),
        },
        Span::dummy(),
    )];
    let sig = crate::ir_lower::function::lower_clone_override_function(
        &function_name,
        &[
            ("this".to_string(), PhpType::Object(scope_class.to_string())),
            (VALUE_PARAM.to_string(), PhpType::Mixed),
        ],
        Some(scope_class),
        &body,
        module,
        check_result,
        &check_result.functions,
        constants,
        fiber_return_sigs,
    );
    Some((function_name, sig))
}
