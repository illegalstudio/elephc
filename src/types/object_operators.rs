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

/// Resolves inherited adapters and nullable selections using the checked class graph.
pub(crate) fn binary_plan_with_classes(
    op: &BinOp, left: &PhpType, right: &PhpType,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> Option<ObjectOperatorPlan> {
    let normalize = |ty: &PhpType| normalize_operator_type(ty, classes);
    binary_plan(op, &normalize(left), &normalize(right))
}

/// Finds a declared native cast on the checked class or its internal ancestor.
pub(crate) fn supports_cast_with_classes(
    ty: &PhpType, cast: elephc_builtin_contract::ObjectCast,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> bool {
    operator_class(&normalize_operator_type(ty, classes))
        .and_then(|class| class.operators).and_then(|operators| operators.cast_method(cast)).is_some()
}

/// Replaces a native subclass by its declaring ancestor without erasing nullable branches.
fn normalize_operator_type(ty: &PhpType, classes: &std::collections::HashMap<String, crate::types::ClassInfo>) -> PhpType {
    match ty {
        PhpType::Object(name) => {
            let mut name = name.as_str();
            for _ in 0..=classes.len() {
                let Some(info) = classes.get(name) else { break; };
                if info.is_internal && operator_class(&PhpType::Object(name.into())).is_some() {
                    return PhpType::Object(name.into());
                }
                let Some(parent) = info.parent.as_deref() else { break; };
                name = parent;
            }
            ty.clone()
        }
        PhpType::Union(members) => PhpType::Union(members.iter().map(|member| normalize_operator_type(member, classes)).collect()),
        _ => ty.clone(),
    }
}

/// Finds one possible native class without treating a nullable value as a definite object.
fn possible_operator_class(ty: &PhpType) -> Option<&'static ClassContract> {
    operator_class(ty).or_else(|| match ty {
        PhpType::Union(members) => members.iter().find_map(possible_operator_class),
        _ => None,
    })
}

/// Finds the first internal class whose contract declares the requested scalar cast.
pub(crate) fn cast_class(ty: &PhpType) -> Option<&'static ClassContract> {
    match ty {
        PhpType::Object(name) => lookup_class(name).filter(|class| {
            class.operators.is_some_and(|operators| {
                operators.casts.is_some() || !operators.native_casts.is_empty()
            })
        }),
        _ => None,
    }
}

/// Resolves explicit casts on a native class or its checked subclass.
pub(crate) fn cast_class_with_classes(
    ty: &PhpType,
    classes: &std::collections::HashMap<String, crate::types::ClassInfo>,
) -> Option<&'static ClassContract> {
    cast_class(&normalize_operator_type(ty, classes))
}

/// Returns one class's declared cast adapter, if any.
pub(crate) fn cast_method(
    class: &'static ClassContract,
    cast: ObjectCast,
) -> Option<&'static str> {
    let operators = class.operators?;
    let scalar = operators.casts.and_then(|casts| match cast {
        ObjectCast::Int => casts.to_int,
        ObjectCast::Float => casts.to_float,
        ObjectCast::Number => casts.to_number,
        ObjectCast::Bool => casts.to_bool,
    });
    scalar.or_else(|| match cast {
        ObjectCast::Int => operators.cast_method(elephc_builtin_contract::ObjectCast::Int),
        ObjectCast::Float => operators.cast_method(elephc_builtin_contract::ObjectCast::Float),
        ObjectCast::Number | ObjectCast::Bool => None,
    })
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
    let class = possible_operator_class(left).or_else(|| possible_operator_class(right))?;
    binary_plan_for_class(class, op, left, right)
}

/// Resolves a reviewed class contract without depending on a particular PHP class name.
fn binary_plan_for_class(class: &'static ClassContract, op: &BinOp, left: &PhpType, right: &PhpType) -> Option<ObjectOperatorPlan> {
    let comparison = matches!(op, BinOp::Eq | BinOp::NotEq | BinOp::Lt | BinOp::LtEq | BinOp::Gt | BinOp::GtEq | BinOp::Spaceship);
    let operators = class.operators?;
    // Classes that own foreign-operand diagnostics still use PHP's null ordering.
    if operators.accepts_any_operands && comparison && (matches!(left, PhpType::Void | PhpType::Never)
        || matches!(right, PhpType::Void | PhpType::Never))
    {
        return None;
    }
    let accepts = |ty: &PhpType| operators.accepts_any_operands || comparison
        || matches!(ty, PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::False | PhpType::Str | PhpType::Mixed | PhpType::TaggedScalar)
        || (*ty == PhpType::Void && operators.arithmetic_result == elephc_builtin_contract::ObjectArithmeticResult::Number)
        || matches!(ty, PhpType::Object(name) if name.eq_ignore_ascii_case(class.name)
            || (operators.arithmetic_result == elephc_builtin_contract::ObjectArithmeticResult::Number
                && operator_class(ty).and_then(|other| other.operators)
                .is_some_and(|other| std::ptr::eq(other, operators))));
    let accepted = |ty: &PhpType| match ty {
        PhpType::Union(members) if comparison
            || operators.arithmetic_result == elephc_builtin_contract::ObjectArithmeticResult::Number => members.iter().all(accepts),
        PhpType::Union(_) => false,
        _ => accepts(ty),
    };
    if !accepted(left) || !accepted(right) { return None; }
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
        dimensions: &[],
        properties: &[],
        native_clone: None,
        json: None,
        native_casts: &[],
        arithmetic: &[(ObjectArithmetic::Add, "synthetic_sum")], compare: Some("synthetic_order"), boolean: None,
        arithmetic_result: elephc_builtin_contract::ObjectArithmeticResult::Object,
        bitwise: &[(ObjectBitwise::And, "synthetic_and")],
        shift: &[(ObjectShift::Left, "synthetic_shl")],
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

    /// Possible object-result operands cannot promise an object when the scalar branch runs.
    #[test]
    fn union_arithmetic_preserves_native_result_representation() {
        let possible_xml = PhpType::Union(vec![PhpType::Object("SimpleXMLElement".into()), PhpType::Void]);
        for op in [BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Mod, BinOp::Pow] {
            for name in ["BcMath\\Number", "GMP"] {
                let possible_number = PhpType::Union(vec![PhpType::Object(name.into()), PhpType::Int]);
                assert!(binary_plan(&op, &possible_number, &PhpType::Int).is_none());
                assert!(binary_plan(&op, &PhpType::Int, &possible_number).is_none());
                let comparison = binary_plan(&BinOp::Lt, &possible_number, &PhpType::Int).unwrap();
                assert_eq!(comparison.result_type(&BinOp::Lt), PhpType::Bool);
            }
            assert!(binary_plan(&op, &possible_xml, &PhpType::Int).unwrap().result_type(&op).is_int_float_union());
        }
    }

    /// Native classes sharing one operator contract accept each other's values and unions.
    #[test]
    fn simplexml_lineages_share_numeric_operator_plans() {
        let element = PhpType::Object("SimpleXMLElement".into());
        let iterator = PhpType::Object("SimpleXMLIterator".into());
        let either = PhpType::Union(vec![element.clone(), iterator.clone()]);
        for op in [BinOp::Add, BinOp::Sub, BinOp::Mul, BinOp::Div, BinOp::Mod, BinOp::Pow] {
            for (left, right) in [(&element, &iterator), (&iterator, &element), (&either, &PhpType::Int)] {
                assert!(binary_plan(&op, left, right).unwrap().result_type(&op).is_int_float_union());
            }
        }
    }

    /// Explicit casts resolve both native families without adding implicit GMP integer casts.
    #[test]
    fn simplexml_and_gmp_scalar_cast_plans_coexist() {
        for (name, int_method, float_method) in [
            ("SimpleXMLElement", "__elephc_int", "__elephc_float"),
            ("SimpleXMLIterator", "__elephc_int", "__elephc_float"),
            ("GMP", "__elephc_to_int", "__elephc_to_float"),
        ] {
            let class = cast_class(&PhpType::Object(name.into())).unwrap();
            assert_eq!(cast_method(class, ObjectCast::Int), Some(int_method));
            assert_eq!(cast_method(class, ObjectCast::Float), Some(float_method));
        }
        let gmp = lookup_class("GMP").unwrap().operators.unwrap();
        assert!(gmp.cast_method(elephc_builtin_contract::ObjectCast::Int).is_none());
    }
}

impl ObjectOperatorPlan {
    /// Returns the PHP-facing result, distinct from a comparison adapter's signed integer.
    pub(crate) fn result_type(&self, op: &BinOp) -> PhpType {
        if !self.comparison {
            if self.class.operators.is_some_and(|ops| ops.arithmetic_result == elephc_builtin_contract::ObjectArithmeticResult::Number) {
                PhpType::Union(vec![PhpType::Int, PhpType::Float])
            } else { PhpType::Object(self.class.name.to_string()) }
        }
        else if matches!(op, BinOp::Spaceship) { PhpType::Int }
        else { PhpType::Bool }
    }
}
