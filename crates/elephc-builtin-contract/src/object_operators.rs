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

/// PHP result domain produced by an internal arithmetic adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectArithmeticResult { Object, Number }

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

/// Native conversion exposed by an internal object class.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectCast { Int, Float, Array }

/// Native property operation that takes precedence over PHP slots and magic methods.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectProperty { Get, Set, Isset, Unset, Probe, Empty, WriteFetch }

/// Native dimension operations, independent of userland ArrayAccess method overrides.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ObjectDimension { Get, Set, Isset, Unset, Append }

/// Methods implementing an internal class's native operators and clone protocol.
#[derive(Clone, Copy, Debug)]
pub struct ClassOperators {
    /// Native property adapters, independent of userland magic-method overrides.
    pub properties: &'static [(ObjectProperty, &'static str)],
    /// Native dimension adapters, including a distinct absent-offset append entry.
    pub dimensions: &'static [(ObjectDimension, &'static str)],
    /// Native clone handler after a shallow copy; replaces the userland clone callback.
    pub native_clone: Option<&'static str>,
    /// Native JSON projection without a PHP-visible JsonSerializable interface.
    pub json: Option<&'static str>,
    /// Instance adapters implementing PHP casts.
    pub casts: &'static [(ObjectCast, &'static str)],
    /// Public compiler adapters taking left and right operands in PHP evaluation order.
    pub arithmetic: &'static [(ObjectArithmetic, &'static str)],
    /// Whether arithmetic produces another class instance or a PHP integer/float.
    pub arithmetic_result: ObjectArithmeticResult,
    /// Static adapter returning one `ObjectOrdering` integer at full precision.
    pub compare: Option<&'static str>,
    /// Instance adapter returning the object's PHP boolean value.
    pub boolean: Option<&'static str>,
}

impl ClassOperators {
    /// Resolves a native dimension operation without selecting a userland protocol method.
    pub fn dimension_method(&self, operation: ObjectDimension) -> Option<&'static str> {
        self.dimensions.iter().find_map(|(op, method)| (*op == operation).then_some(*method))
    }

    /// Resolves a native property operation without selecting a user magic method.
    pub fn property_method(&self, operation: ObjectProperty) -> Option<&'static str> {
        self.properties.iter().find_map(|(op, method)| (*op == operation).then_some(*method))
    }
    /// Returns the reviewed instance adapter for a native conversion.
    pub fn cast_method(&self, cast: ObjectCast) -> Option<&'static str> {
        self.casts.iter().find_map(|(candidate, method)| (*candidate == cast).then_some(*method))
    }

    /// Resolves a typed arithmetic operation independently of PHP class spelling.
    pub fn arithmetic_method(&self, operation: ObjectArithmetic) -> Option<&'static str> {
        self.arithmetic.iter().find_map(|(op, method)| (*op == operation).then_some(*method))
    }
}

/// Number adapters compile through ordinary EIR calls with fresh immutable results.
pub const BCMATH_NUMBER_OPERATORS: ClassOperators = ClassOperators {
    properties: &[],
    dimensions: &[],
    native_clone: None,
    json: None,
    casts: &[],
    arithmetic: &[
        (ObjectArithmetic::Add, "__elephc_add"),
        (ObjectArithmetic::Sub, "__elephc_sub"),
        (ObjectArithmetic::Mul, "__elephc_mul"),
        (ObjectArithmetic::Div, "__elephc_div"),
        (ObjectArithmetic::Mod, "__elephc_mod"),
        (ObjectArithmetic::Pow, "__elephc_pow"),
    ],
    arithmetic_result: ObjectArithmeticResult::Object,
    compare: Some("__elephc_compare"),
    boolean: Some("__elephc_bool"),
};

/// SimpleXML uses native document truthiness rather than ordinary object truthiness.
pub const SIMPLEXML_OPERATORS: ClassOperators = ClassOperators {
    dimensions: &[(ObjectDimension::Get, "__elephc_dimension_get"),
        (ObjectDimension::Set, "__elephc_dimension_set"),
        (ObjectDimension::Isset, "__elephc_dimension_isset"),
        (ObjectDimension::Unset, "__elephc_dimension_unset"),
        (ObjectDimension::Append, "__elephc_append")],
    properties: &[(ObjectProperty::Get, "__elephc_property_get"),
        (ObjectProperty::Set, "__elephc_property_set"),
        (ObjectProperty::Isset, "__elephc_property_isset"),
        (ObjectProperty::Empty, "__elephc_property_empty"),
        (ObjectProperty::WriteFetch, "__elephc_property_write_fetch"),
        (ObjectProperty::Unset, "__elephc_property_unset"),
        (ObjectProperty::Probe, "__elephc_property_probe")],
    native_clone: Some("__elephc_clone_initialize"),
    json: Some("__elephc_json"),
    casts: &[(ObjectCast::Int, "__elephc_int"), (ObjectCast::Float, "__elephc_float"), (ObjectCast::Array, "__elephc_array")],
    arithmetic: &[(ObjectArithmetic::Add, "__elephc_add"),
        (ObjectArithmetic::Sub, "__elephc_sub"), (ObjectArithmetic::Mul, "__elephc_mul"),
        (ObjectArithmetic::Div, "__elephc_div"), (ObjectArithmetic::Mod, "__elephc_mod"),
        (ObjectArithmetic::Pow, "__elephc_pow")],
    arithmetic_result: ObjectArithmeticResult::Number,
    compare: Some("__elephc_compare"), boolean: Some("__elephc_bool"),
};

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic internal class can declare a subset without depending on decimal code.
    #[test]
    fn synthetic_operator_contract_resolves_only_declared_operations() {
        let operators = ClassOperators {
            properties: &[],
            dimensions: &[],
            native_clone: None,
            json: None,
            casts: &[],
            arithmetic: &[(ObjectArithmetic::Add, "synthetic_sum")],
            arithmetic_result: ObjectArithmeticResult::Object,
            compare: None,
            boolean: None,
        };
        assert_eq!(operators.arithmetic_method(ObjectArithmetic::Add), Some("synthetic_sum"));
        assert_eq!(operators.arithmetic_method(ObjectArithmetic::Mul), None);
        assert_eq!(operators.dimension_method(ObjectDimension::Get), None);
        assert_eq!(SIMPLEXML_OPERATORS.dimension_method(ObjectDimension::Append), Some("__elephc_append"));
        assert_eq!(SIMPLEXML_OPERATORS.dimension_method(ObjectDimension::Set), Some("__elephc_dimension_set"));
    }
}
