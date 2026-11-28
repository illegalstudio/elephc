//! Purpose:
//! Describes restricted logical inputs whose public PHP signatures use mixed storage.
//!
//! Called from:
//! - Compiler builtin semantic selectors and Magician source-proof validation.
//!
//! Key details:
//! - Stable builtin identities select reviewed operation domains, not backend emitters.
//! - These roles supplement signatures without changing PHP reflection or conversion rules.

use crate::{BuiltinContract, BuiltinId};

/// A consumer capability, interpreted using each backend's existing nominal and scalar types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedInputDomain {
    Array,
    Resource,
    String,
    Countable,
}

impl BuiltinContract {
    /// Selects argument roles for the reached overload without consulting runtime value tags.
    pub fn mixed_input_domains(&self, argument_count: usize) -> Vec<(usize, MixedInputDomain)> {
        use MixedInputDomain as D;
        let is = |name| self.id == BuiltinId::from_canonical_name(name);
        if is("get_resource_id") || is("get_resource_type") { return vec![(0,D::Resource)]; }
        if is("vprintf") || is("vsprintf") { return vec![(1,D::Array)]; }
        if is("count") || is("sizeof") { return vec![(0,D::Countable)]; }
        if is("implode") || is("join") {
            return match argument_count { 0=>Vec::new(),1=>vec![(0,D::Array)],
                _=>vec![(0,D::String),(1,D::Array)] };
        }
        if argument_count == 1 && (is("min") || is("max")) { return vec![(0,D::Array)]; }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Aliases and overload arity share canonical reviewed argument capabilities.
    #[test]
    fn strict_mixed_builtin_domains_keep_overloads_and_aliases() {
        use MixedInputDomain as D;
        for name in ["implode","join"] {
            let contract=crate::lookup(name).unwrap();
            assert_eq!(contract.mixed_input_domains(1),vec![(0,D::Array)]);
            assert_eq!(contract.mixed_input_domains(2),vec![(0,D::String),(1,D::Array)]);
        }
        for name in ["count","sizeof"] {
            assert_eq!(crate::lookup(name).unwrap().mixed_input_domains(1),vec![(0,D::Countable)]);
        }
        for name in ["min","max"] {
            let contract=crate::lookup(name).unwrap();
            assert_eq!(contract.mixed_input_domains(1),vec![(0,D::Array)]);
            assert!(contract.mixed_input_domains(2).is_empty());
        }
        assert!(crate::lookup("gettype").unwrap().mixed_input_domains(1).is_empty());
    }
}
