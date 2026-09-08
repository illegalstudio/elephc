//! Purpose:
//! Maps declared instance-property PHP types to safe physical runtime representations.
//! Keeps Closure descriptors and generic object values out of ordinary object-layout slots.
//!
//! Called from:
//! - Property EIR lowering, object code generation, and user runtime-metadata emission.
//!
//! Key details:
//! - Declared `Closure` uses a callable descriptor and declared `object` uses a boxed Mixed cell.
//! - Named class and interface properties remain ordinary object-pointer slots.

use super::PhpType;

/// Returns the physical representation for a declared instance property.
///
/// A Closure descriptor has its own header and capture-aware destructor, so it cannot inhabit an
/// `Object` slot. Generic `object` accepts either an ordinary object or a Closure, which requires
/// a boxed Mixed cell to preserve the runtime tag across loads, GC, and destruction.
pub(crate) fn property_runtime_storage_type(declared: &PhpType) -> PhpType {
    match declared.codegen_repr() {
        PhpType::Object(class_name)
            if class_name.trim_start_matches('\\').eq_ignore_ascii_case("Closure") =>
        {
            PhpType::Callable
        }
        PhpType::Object(class_name) if class_name.trim_start_matches('\\').is_empty() => {
            PhpType::Mixed
        }
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::property_runtime_storage_type;
    use crate::types::PhpType;

    /// Verifies nullable scalar properties preserve their inline payload-and-tag representation.
    #[test]
    fn tagged_scalar_properties_use_inline_storage() {
        assert_eq!(
            property_runtime_storage_type(&PhpType::TaggedScalar),
            PhpType::TaggedScalar
        );
    }
}
