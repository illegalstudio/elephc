//! Purpose:
//! Identifies abstract setter contracts retained by concrete readonly storage.
//!
//! Called from:
//! - EIR property-write ownership lowering and target-aware property dispatch.
//!
//! Key details:
//! - A concrete accessor supersedes the retained contract.
//! - Storage and its inherited setter have distinct declaration and lifetime semantics.

use super::{ClassInfo, PropertyHookContract};

impl ClassInfo {
    /// Returns a no-op abstract setter still inherited by this property's readonly storage.
    pub fn retained_abstract_setter(&self, property: &str) -> Option<&PropertyHookContract> {
        if !self.readonly_properties.contains(property)
            || self.abstract_properties.contains(property)
            || self.methods.contains_key(&crate::names::php_symbol_key(
                &crate::names::property_hook_set_method(property),
            ))
        {
            return None;
        }
        self.abstract_property_hooks.get(property)
            .filter(|contract| contract.set_type.is_some())
    }
}
