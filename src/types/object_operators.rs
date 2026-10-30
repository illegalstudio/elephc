//! Purpose:
//! Resolves internal object operators and scalar casts from shared class metadata.
//!
//! Called from:
//! - Binary/unary type inference and EIR expression lowering.
//! - Explicit scalar-cast lowering for internal value classes.
//!
//! Key details:
//! - Source order is preserved by static adapters taking left then right operands.
//! - Identity operators and unsupported userland objects never enter this contract.
//! - Classes that declare `accepts_any_operands` own their PHP diagnostics at runtime,
//!   including foreign objects, arrays, resources and null arithmetic.

use elephc_builtin_contract::{
    lookup_class, ClassContract, ObjectArithmetic, ObjectBitwise, ObjectShift,
};
use crate::parser::ast::BinOp;
use crate::types::PhpType;

/// One resolved internal-class adapter and its declared result representation.
#[derive(Clone, Copy)]
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

/// Finds the first internal class whose contract declares the requested scalar cast.
pub(crate) fn cast_class(ty: &PhpType) -> Option<&'static ClassContract> {
    match ty {
        PhpType::Object(name) => lookup_class(name).filter(|class| {
            class.operators.and_then(|operators| operators.casts).is_some()
        }),
        _ => None,
    }
}

/// Returns one class's declared cast adapter, if any.
pub(crate) fn cast_method(
    class: &'static ClassContract,
    cast: ObjectCast,
) -> Option<&'static str> {
    let casts = class.operators?.casts?;
    match cast {
        ObjectCast::Int => casts.to_int,
        ObjectCast::Float => casts.to_float,
        ObjectCast::Number => casts.to_number,
        ObjectCast::Bool => casts.to_bool,
    }
}

/// One scalar cast an internal value class can provide.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum ObjectCast {
    Int,
    Float,
    /// PHP's `_IS_NUMBER` conversion: int when it fits, float otherwise.
    Number,
    /// The explicit `(bool)` adapter, retained for numeric-context consumers.
    Bool,
}

/// Returns whether an internal-operator class accepts a standard object-versus-null
/// ordering comparison, which PHP resolves without calling the class handler.
pub(crate) fn null_comparison_allowed(left: &PhpType, right: &PhpType) -> bool {
    let class = operator_class(left).or_else(|| operator_class(right));
    let Some(class) = class else { return false; };
    let Some(operators) = class.operators else { return false; };
    operators.accepts_any_operands
        && (matches!(left, PhpType::Void | PhpType::Never)
            || matches!(right, PhpType::Void | PhpType::Never))
}

/// Selects the class handler and checks that the other operand belongs to its domain.
pub(crate) fn binary_plan(op: &BinOp, left: &PhpType, right: &PhpType) -> Option<ObjectOperatorPlan> {
    let class = operator_class(left).or_else(|| operator_class(right))?;
    binary_plan_for_class(class, op, left, right)
}

/// Resolves a reviewed class contract without depending on a particular PHP class name.
fn binary_plan_for_class(
    class: &'static ClassContract,
    op: &BinOp,
    left: &PhpType,
    right: &PhpType,
) -> Option<ObjectOperatorPlan> {
    let operators = class.operators?;
    let comparison = matches!(
        op,
        BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq | BinOp::Spaceship
    );
    // PHP never invokes an object's comparison handler against null: the engine's
    // standard object-versus-null ordering is the observable result.
    if comparison && (matches!(left, PhpType::Void | PhpType::Never)
        || matches!(right, PhpType::Void | PhpType::Never))
    {
        return None;
    }
    let accepts = |ty: &PhpType| {
        if operators.accepts_any_operands {
            return true;
        }
        comparison
            || matches!(ty, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False
                | PhpType::Str | PhpType::Mixed | PhpType::TaggedScalar)
            || matches!(ty, PhpType::Object(name) if name.eq_ignore_ascii_case(class.name))
    };
    if !accepts(left) || !accepts(right) {
        return None;
    }
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
        return Some(ObjectOperatorPlan {
            class,
            method: operators.arithmetic_method(operation)?,
            comparison: false,
        });
    }
    let bitwise = match op {
        BinOp::BitAnd => Some(ObjectBitwise::And),
        BinOp::BitOr => Some(ObjectBitwise::Or),
        BinOp::BitXor => Some(ObjectBitwise::Xor),
        _ => None,
    };
    if let Some(operation) = bitwise {
        return Some(ObjectOperatorPlan {
            class,
            method: operators.bitwise_method(operation)?,
            comparison: false,
        });
    }
    let shift = match op {
        BinOp::ShiftLeft => Some(ObjectShift::Left),
        BinOp::ShiftRight => Some(ObjectShift::Right),
        _ => None,
    };
    if let Some(operation) = shift {
        return Some(ObjectOperatorPlan {
            class,
            method: operators.shift_method(operation)?,
            comparison: false,
        });
    }
    if comparison {
        return Some(ObjectOperatorPlan {
            class,
            method: operators.compare?,
            comparison: true,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use elephc_builtin_contract::{
        BuiltinId, ClassCasts, ClassKind, ClassOperators, ClassRoute, PhpModule,
    };

    static CASTS: ClassCasts = ClassCasts {
        to_int: Some("synthetic_int"),
        to_float: Some("synthetic_float"),
        to_number: Some("synthetic_number"),
        to_bool: Some("synthetic_bool"),
    };
    static OPERATORS: ClassOperators = ClassOperators {
        arithmetic: &[(ObjectArithmetic::Add, "synthetic_sum")],
        bitwise: &[(ObjectBitwise::And, "synthetic_and")],
        shift: &[(ObjectShift::Left, "synthetic_shl")],
        compare: Some("synthetic_order"),
        boolean: None,
        casts: Some(&CASTS),
        accepts_any_operands: true,
    };
    static SYNTHETIC: ClassContract = ClassContract {
        cloneable: true,
        id: BuiltinId::from_canonical_name("syntheticvalue"),
        name: "SyntheticValue",
        kind: ClassKind::Class,
        module: PhpModule::Elephc,
        since: None,
        aot: ClassRoute::Prelude,
        target_support: None,
        extension: false,
        internal: true,
        hidden_properties: &[],
        php_visible_methods: None,
        php_manual: None,
        operators: Some(&OPERATORS),
        dynamic_properties_deprecated: false,
        dump_dynamic_properties_first: false,
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
        assert!(binary_plan_for_class(&SYNTHETIC, &BinOp::Add, &object, &PhpType::Object("Other".into())).is_some());
        assert!(binary_plan_for_class(&SYNTHETIC, &BinOp::Eq, &object, &PhpType::Void).is_none());
        assert_eq!(binary_plan_for_class(&SYNTHETIC, &BinOp::BitAnd, &object, &PhpType::Int).unwrap().method, "synthetic_and");
        assert_eq!(binary_plan_for_class(&SYNTHETIC, &BinOp::ShiftLeft, &object, &PhpType::Int).unwrap().method, "synthetic_shl");
        assert_eq!(cast_method(&SYNTHETIC, ObjectCast::Int), Some("synthetic_int"));
        assert_eq!(cast_method(&SYNTHETIC, ObjectCast::Number), Some("synthetic_number"));
    }
}

impl ObjectOperatorPlan {
    /// Returns the PHP-facing result, distinct from a comparison adapter's signed integer.
    pub(crate) fn result_type(&self, op: &BinOp) -> PhpType {
        if !self.comparison {
            PhpType::Object(self.class.name.to_string())
        } else if matches!(op, BinOp::Spaceship) {
            PhpType::Int
        } else {
            PhpType::Bool
        }
    }
}
