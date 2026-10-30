//! Purpose:
//! Declares typed operator handlers supplied by internal PHP value classes.
//!
//! Called from:
//! - Compiler inference/EIR lowering and Magician evaluated-operator dispatch.
//!
//! Key details:
//! - These are internal class contracts, never userland operator magic methods.
//! - Strict equality is intentionally absent and retains object-identity semantics.

/// Arithmetic operation supported by an internal class handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectArithmetic {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Pow,
}

/// Bitwise operation supported by an internal class handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectBitwise {
    And,
    Or,
    Xor,
}

/// Shift operation supported by an internal class handler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectShift {
    Left,
    Right,
}

/// Normalized ordering returned by internal comparison adapters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i64)]
pub enum ObjectOrdering {
    Less = -1,
    Equal = 0,
    Greater = 1,
    /// PHP relational operators are false; spaceship reports one in either direction.
    Unordered = 2,
}

/// Instance adapters implementing an internal class's scalar casts.
#[derive(Clone, Copy, Debug)]
pub struct ClassCasts {
    /// Adapter returning the value a PHP `(int)` cast produces.
    pub to_int: Option<&'static str>,
    /// Adapter returning the value a PHP `(float)` cast produces.
    pub to_float: Option<&'static str>,
    /// Adapter returning PHP's `_IS_NUMBER` value (int when it fits, float otherwise).
    pub to_number: Option<&'static str>,
    /// Adapter returning the value a PHP `(bool)` cast produces.
    pub to_bool: Option<&'static str>,
}

/// Methods implementing an internal class's immutable value operators.
#[derive(Clone, Copy, Debug)]
pub struct ClassOperators {
    /// Public compiler adapters taking left and right operands in PHP evaluation order.
    pub arithmetic: &'static [(ObjectArithmetic, &'static str)],
    /// Public compiler adapters for `&`, `|`, and `^`.
    pub bitwise: &'static [(ObjectBitwise, &'static str)],
    /// Public compiler adapters for `<<` and `>>`.
    pub shift: &'static [(ObjectShift, &'static str)],
    /// Static adapter returning one `ObjectOrdering` integer at full precision.
    pub compare: Option<&'static str>,
    /// Instance adapter returning the object's PHP boolean value.
    pub boolean: Option<&'static str>,
    /// Instance adapters for scalar casts.
    pub casts: Option<&'static ClassCasts>,
    /// Whether every operand type must reach the adapter so it can raise PHP's own error.
    ///
    /// PHP's internal GMP handlers accept `GMP|string|int` after weak coercion but
    /// deliberately run for arrays, foreign objects, resources and null too, so the
    /// handler owns the exact `TypeError`. Such a class opts out of the compiler's
    /// operand-family filter; ordinary internal classes keep it.
    pub accepts_any_operands: bool,
}

impl ClassOperators {
    /// Resolves a typed arithmetic operation independently of PHP class spelling.
    pub fn arithmetic_method(&self, operation: ObjectArithmetic) -> Option<&'static str> {
        self.arithmetic.iter().find_map(|(op, method)| (*op == operation).then_some(*method))
    }

    /// Resolves a typed bitwise operation independently of PHP class spelling.
    pub fn bitwise_method(&self, operation: ObjectBitwise) -> Option<&'static str> {
        self.bitwise.iter().find_map(|(op, method)| (*op == operation).then_some(*method))
    }

    /// Resolves a typed shift operation independently of PHP class spelling.
    pub fn shift_method(&self, operation: ObjectShift) -> Option<&'static str> {
        self.shift.iter().find_map(|(op, method)| (*op == operation).then_some(*method))
    }
}

/// Number adapters compile through ordinary EIR calls with fresh immutable results.
pub const BCMATH_NUMBER_OPERATORS: ClassOperators = ClassOperators {
    arithmetic: &[
        (ObjectArithmetic::Add, "__elephc_add"),
        (ObjectArithmetic::Sub, "__elephc_sub"),
        (ObjectArithmetic::Mul, "__elephc_mul"),
        (ObjectArithmetic::Div, "__elephc_div"),
        (ObjectArithmetic::Mod, "__elephc_mod"),
        (ObjectArithmetic::Pow, "__elephc_pow"),
    ],
    bitwise: &[],
    shift: &[],
    compare: Some("__elephc_compare"),
    boolean: Some("__elephc_bool"),
    casts: None,
    accepts_any_operands: false,
};

/// GMP's PHP 8.5 object handlers: arithmetic, bitwise, shifts, ordering and casts.
pub const GMP_CASTS: ClassCasts = ClassCasts {
    to_int: Some("__elephc_to_int"),
    to_float: Some("__elephc_to_float"),
    to_number: Some("__elephc_to_number"),
    to_bool: Some("__elephc_to_bool"),
};

/// GMP adapters run for every operand family so php-src's own diagnostics are preserved.
pub const GMP_OPERATORS: ClassOperators = ClassOperators {
    arithmetic: &[
        (ObjectArithmetic::Add, "__elephc_add"),
        (ObjectArithmetic::Sub, "__elephc_sub"),
        (ObjectArithmetic::Mul, "__elephc_mul"),
        (ObjectArithmetic::Div, "__elephc_div"),
        (ObjectArithmetic::Mod, "__elephc_mod"),
        (ObjectArithmetic::Pow, "__elephc_pow"),
    ],
    bitwise: &[
        (ObjectBitwise::And, "__elephc_bitand"),
        (ObjectBitwise::Or, "__elephc_bitor"),
        (ObjectBitwise::Xor, "__elephc_bitxor"),
    ],
    shift: &[
        (ObjectShift::Left, "__elephc_shl"),
        (ObjectShift::Right, "__elephc_shr"),
    ],
    compare: Some("__elephc_compare"),
    boolean: Some("__elephc_bool"),
    casts: Some(&GMP_CASTS),
    accepts_any_operands: true,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic internal class can declare a subset without depending on decimal code.
    #[test]
    fn synthetic_operator_contract_resolves_only_declared_operations() {
        let operators = ClassOperators {
            arithmetic: &[(ObjectArithmetic::Add, "synthetic_sum")],
            bitwise: &[(ObjectBitwise::And, "synthetic_and")],
            shift: &[(ObjectShift::Left, "synthetic_shl")],
            compare: None,
            boolean: None,
            casts: None,
            accepts_any_operands: false,
        };
        assert_eq!(operators.arithmetic_method(ObjectArithmetic::Add), Some("synthetic_sum"));
        assert_eq!(operators.arithmetic_method(ObjectArithmetic::Mul), None);
        assert_eq!(operators.bitwise_method(ObjectBitwise::And), Some("synthetic_and"));
        assert_eq!(operators.shift_method(ObjectShift::Left), Some("synthetic_shl"));
    }

    /// GMP declares every typed handler group the compiler dispatches and opts into
    /// runtime-owned operand diagnostics.
    #[test]
    fn gmp_operator_contract_is_complete() {
        assert!(GMP_OPERATORS.accepts_any_operands);
        for operation in [ObjectArithmetic::Add, ObjectArithmetic::Sub, ObjectArithmetic::Mul,
            ObjectArithmetic::Div, ObjectArithmetic::Mod, ObjectArithmetic::Pow] {
            assert!(GMP_OPERATORS.arithmetic_method(operation).is_some());
        }
        for operation in [ObjectBitwise::And, ObjectBitwise::Or, ObjectBitwise::Xor] {
            assert!(GMP_OPERATORS.bitwise_method(operation).is_some());
        }
        for operation in [ObjectShift::Left, ObjectShift::Right] {
            assert!(GMP_OPERATORS.shift_method(operation).is_some());
        }
        let casts = GMP_OPERATORS.casts.expect("GMP casts");
        assert!(casts.to_int.is_some() && casts.to_float.is_some()
            && casts.to_number.is_some() && casts.to_bool.is_some());
    }
}
