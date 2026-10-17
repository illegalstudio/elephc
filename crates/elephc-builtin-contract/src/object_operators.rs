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

/// Methods implementing an internal class's immutable value operators.
#[derive(Clone, Copy, Debug)]
pub struct ClassOperators {
    /// Public compiler adapters taking left and right operands in PHP evaluation order.
    pub arithmetic: &'static [(ObjectArithmetic, &'static str)],
    /// Static adapter returning one `ObjectOrdering` integer at full precision.
    pub compare: Option<&'static str>,
    /// Instance adapter returning the object's PHP boolean value.
    pub boolean: Option<&'static str>,
}

impl ClassOperators {
    /// Resolves a typed arithmetic operation independently of PHP class spelling.
    pub fn arithmetic_method(&self, operation: ObjectArithmetic) -> Option<&'static str> {
        self.arithmetic.iter().find_map(|(op, method)| (*op == operation).then_some(*method))
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
    compare: Some("__elephc_compare"),
    boolean: Some("__elephc_bool"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic internal class can declare a subset without depending on decimal code.
    #[test]
    fn synthetic_operator_contract_resolves_only_declared_operations() {
        let operators = ClassOperators {
            arithmetic: &[(ObjectArithmetic::Add, "synthetic_sum")],
            compare: None,
            boolean: None,
        };
        assert_eq!(operators.arithmetic_method(ObjectArithmetic::Add), Some("synthetic_sum"));
        assert_eq!(operators.arithmetic_method(ObjectArithmetic::Mul), None);
    }
}
