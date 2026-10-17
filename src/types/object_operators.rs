//! Purpose:
//! Resolves internal object operators from shared class metadata and operand types.
//!
//! Called from:
//! - Binary/unary type inference and EIR expression lowering.
//!
//! Key details:
//! - Source order is preserved by static adapters taking left then right operands.
//! - Identity operators and unsupported userland objects never enter this contract.

use elephc_builtin_contract::{lookup_class, ClassContract, ObjectArithmetic};
use crate::parser::ast::BinOp;
use crate::types::PhpType;

/// One resolved internal-class adapter and its declared result representation.
pub(crate) struct ObjectOperatorPlan {
    pub class: &'static ClassContract,
    pub method: &'static str,
    pub comparison: bool,
}

/// Finds internal operator metadata for a non-nullable, statically known object.
pub(crate) fn operator_class(ty: &PhpType) -> Option<&'static ClassContract> {
    match ty {
        PhpType::Object(name) => lookup_class(name).filter(|class| class.operators.is_some()),
        _ => None,
    }
}

/// Selects the class handler and checks that the other operand belongs to its domain.
pub(crate) fn binary_plan(op: &BinOp, left: &PhpType, right: &PhpType) -> Option<ObjectOperatorPlan> {
    let class = operator_class(left).or_else(|| operator_class(right))?;
    binary_plan_for_class(class, op, left, right)
}

/// Resolves a reviewed class contract without depending on a particular PHP class name.
fn binary_plan_for_class(class: &'static ClassContract, op: &BinOp, left: &PhpType, right: &PhpType) -> Option<ObjectOperatorPlan> {
    let comparison = matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq | BinOp::Spaceship);
    let accepts = |ty: &PhpType| comparison
        || matches!(ty, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str | PhpType::Mixed | PhpType::TaggedScalar)
        || matches!(ty, PhpType::Object(name) if name.eq_ignore_ascii_case(class.name));
    if !accepts(left) || !accepts(right) { return None; }
    let operators = class.operators?;
    let arithmetic = match op {
        BinOp::Add => Some(ObjectArithmetic::Add),
        BinOp::Sub => Some(ObjectArithmetic::Sub),
        BinOp::Mul => Some(ObjectArithmetic::Mul),
        BinOp::Div => Some(ObjectArithmetic::Div),
        BinOp::Mod => Some(ObjectArithmetic::Mod),
        BinOp::Pow => Some(ObjectArithmetic::Pow),
        _ => None,
    };
    if let Some(operation) = arithmetic {
        return Some(ObjectOperatorPlan { class, method: operators.arithmetic_method(operation)?, comparison: false });
    }
    if matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq | BinOp::Spaceship) {
        return Some(ObjectOperatorPlan { class, method: operators.compare?, comparison: true });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_builtin_contract::{BuiltinId, ClassKind, ClassOperators, ClassRoute, PhpModule};

    static OPERATORS: ClassOperators = ClassOperators {
        arithmetic: &[(ObjectArithmetic::Add, "synthetic_sum")], compare: Some("synthetic_order"), boolean: None,
    };
    static SYNTHETIC: ClassContract = ClassContract {
        id: BuiltinId::from_canonical_name("syntheticvalue"), name: "SyntheticValue",
        kind: ClassKind::Class, module: PhpModule::Elephc, since: None, aot: ClassRoute::Prelude,
        target_support: None, extension: false, internal: true, php_manual: None, operators: Some(&OPERATORS),
    };

    /// A nondecimal internal contract selects its own methods and preserves strict identity.
    #[test]
    fn synthetic_internal_object_operator_plan() {
        let object = PhpType::Object("SyntheticValue".into());
        let sum = binary_plan_for_class(&SYNTHETIC, &BinOp::Add, &PhpType::Int, &object).unwrap();
        assert_eq!(sum.method, "synthetic_sum");
        assert_eq!(sum.result_type(&BinOp::Add), object);
        let comparison = binary_plan_for_class(&SYNTHETIC, &BinOp::Spaceship, &object, &PhpType::Str).unwrap();
        assert_eq!(comparison.method, "synthetic_order");
        assert_eq!(comparison.result_type(&BinOp::Spaceship), PhpType::Int);
        assert!(binary_plan_for_class(&SYNTHETIC, &BinOp::Mul, &object, &PhpType::Int).is_none());
        assert!(binary_plan_for_class(&SYNTHETIC, &BinOp::StrictEq, &object, &object).is_none());
        assert!(binary_plan_for_class(&SYNTHETIC, &BinOp::Add, &object, &PhpType::Object("Other".into())).is_none());
    }
}

impl ObjectOperatorPlan {
    /// Returns the PHP-facing result, distinct from a comparison adapter's signed integer.
    pub(crate) fn result_type(&self, op: &BinOp) -> PhpType {
        if !self.comparison { PhpType::Object(self.class.name.to_string()) }
        else if matches!(op, BinOp::Spaceship) { PhpType::Int }
        else { PhpType::Bool }
    }
}
