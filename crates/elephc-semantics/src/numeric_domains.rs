//! Purpose:
//! Defines numeric operator domains independently of compiler and runtime value storage.
//!
//! Called from:
//! - Source/checker validation, EIR guards and eval operator adapters.
//!
//! Key details:
//! - Arithmetic and coercion restrictions overlap without enabling each other's profile bit.
//! - Native integer/float arithmetic has an explicit numeric result contract.

use crate::{conversions::ValueKind, SemanticProfile, StrictFeature};

/// Operator families sharing the same operand-domain contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericOperator { Arithmetic, Division, Modulo, Power, Bitwise, Shift, Unary, BitNot, Update }

/// Concrete PHP kinds accepted without PHP scalar coercion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NumericDomain { Number, Integer, IntegerOrString }

impl NumericDomain {
    /// Tests one concrete scalar kind without interpreting its contents or changing its value.
    pub fn allows(self, kind: ValueKind) -> bool {
        match self {
            Self::Number => matches!(kind, ValueKind::Integer | ValueKind::Float),
            Self::Integer => kind == ValueKind::Integer,
            Self::IntegerOrString => matches!(kind, ValueKind::Integer | ValueKind::String),
        }
    }

    /// Supplies the same readable domain description to source and runtime diagnostics.
    pub const fn description(self) -> &'static str {
        match self { Self::Number => "int or float", Self::Integer => "int", Self::IntegerOrString => "int or string" }
    }
}

/// One selected guarantee owns a domain guard without changing the immutable profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NumericDomainRule { pub feature: StrictFeature, pub domain: NumericDomain }

impl NumericDomainRule {
    /// Formats one authoritative kind failure for compiler and eval adapters.
    pub fn message(self) -> String { format!("--{} requires {} operands", self.feature.flag(), self.domain.description()) }
}

/// Selects the strict numeric domain while preserving ordinary PHP operations when neither applies.
pub fn operand_rule(profile: SemanticProfile, operator: NumericOperator) -> Option<NumericDomainRule> {
    let feature = if profile.contains(StrictFeature::Arithmetic) { StrictFeature::Arithmetic }
        else if profile.contains(StrictFeature::Coercions) { StrictFeature::Coercions }
        else { return None; };
    let domain = match operator {
        NumericOperator::Modulo | NumericOperator::Bitwise | NumericOperator::Shift => NumericDomain::Integer,
        NumericOperator::BitNot if feature == StrictFeature::Coercions => NumericDomain::IntegerOrString,
        NumericOperator::BitNot => NumericDomain::Integer,
        _ => NumericDomain::Number,
    };
    Some(NumericDomainRule { feature, domain })
}

/// Rejects only the integer-power value domain, leaving explicit floating power available.
pub fn rejects_negative_integer_power(profile: SemanticProfile, left: ValueKind, right: ValueKind, exponent: i64) -> bool {
    profile.contains(StrictFeature::Arithmetic) && left == ValueKind::Integer
        && right == ValueKind::Integer && exponent < 0
}

/// Shared failure text for an integer operation whose result would require floating power.
pub const NEGATIVE_INTEGER_POWER_MESSAGE: &str = "--strict-arithmetic: integer power requires a nonnegative exponent; use a float operand";

/// Integer bit operations preserve explicit bit patterns rather than performing checked arithmetic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IntegerBitOp { And, Or, Xor, ShiftLeft, ShiftRight }

/// Shares PHP's signed 64-bit shift bounds and deliberately wrapping bit operations.
pub fn integer_bitwise(op: IntegerBitOp, left: i64, right: i64) -> Result<i64, &'static str> {
    Ok(match op {
        IntegerBitOp::And => left & right, IntegerBitOp::Or => left | right, IntegerBitOp::Xor => left ^ right,
        IntegerBitOp::ShiftLeft | IntegerBitOp::ShiftRight if right < 0 => return Err("Bit shift by negative number"),
        IntegerBitOp::ShiftLeft if right >= 64 => 0,
        IntegerBitOp::ShiftRight if right >= 64 => if left < 0 { -1 } else { 0 },
        IntegerBitOp::ShiftLeft => left.wrapping_shl(right as u32),
        IntegerBitOp::ShiftRight => left >> right as u32,
    })
}
