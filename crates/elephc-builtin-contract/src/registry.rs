//! Purpose:
//! Validates and indexes the canonical shared builtin contract catalog.
//!
//! Called from:
//! - Compiler and Magician backend registries when joining implementation bindings.
//! - Documentation and parity audits that enumerate the shared PHP surface.
//!
//! Key details:
//! - Duplicate names, duplicate IDs, non-canonical names, hash mismatches, and
//!   unstable source ordering fail before the catalog is exposed.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use crate::{BuiltinContract, BuiltinId};

/// Validated lookup indexes over the static contract data.
struct Registry {
    /// Case-normalized PHP name to contract.
    by_name: HashMap<&'static str, &'static BuiltinContract>,
    /// Stable builtin ID to contract.
    by_id: HashMap<BuiltinId, &'static BuiltinContract>,
}

/// Process-wide shared contract indexes initialized on first lookup.
static REGISTRY: OnceLock<Registry> = OnceLock::new();

/// Returns every canonical contract in stable name order.
pub fn contracts() -> &'static [BuiltinContract] {
    static CONTRACTS: OnceLock<Vec<BuiltinContract>> = OnceLock::new();
    CONTRACTS
        .get_or_init(|| {
            let mut contracts = Vec::with_capacity(
                crate::catalog_data::CONTRACTS.len()
                    + crate::catalog_surfaces::SURFACE_CONTRACTS.len(),
            );
            contracts.extend_from_slice(crate::catalog_data::CONTRACTS);
            contracts.extend_from_slice(crate::catalog_surfaces::SURFACE_CONTRACTS);
            for contract in &mut contracts {
                contract.requirements = crate::requirements::fixed_requirements(contract.id);
            }
            contracts.sort_unstable_by_key(|contract| contract.name);
            contracts
        })
        .as_slice()
}

/// Finds one contract by a PHP-style case-insensitive name.
pub fn lookup(name: &str) -> Option<&'static BuiltinContract> {
    let canonical = name.trim_start_matches('\\').to_ascii_lowercase();
    registry().by_name.get(canonical.as_str()).copied()
}

/// Finds one contract by its stable backend-neutral identity.
pub fn lookup_id(id: BuiltinId) -> Option<&'static BuiltinContract> {
    registry().by_id.get(&id).copied()
}

/// Builds and returns the validated lookup indexes.
fn registry() -> &'static Registry {
    REGISTRY.get_or_init(build_registry)
}

/// Validates the static catalog and constructs its name and ID maps.
fn build_registry() -> Registry {
    let mut by_name = HashMap::with_capacity(contracts().len());
    let mut by_id = HashMap::with_capacity(contracts().len());
    let mut previous_name: Option<&str> = None;

    for contract in contracts() {
        assert!(!contract.name.is_empty(), "builtin contract names cannot be empty");
        assert_eq!(
            contract.name,
            contract.name.to_ascii_lowercase(),
            "builtin contract name must be canonical lowercase: {}",
            contract.name
        );
        assert!(
            !contract.name.starts_with('\\'),
            "builtin contract name must not have a leading namespace separator: {}",
            contract.name
        );
        if let Some(previous_name) = previous_name {
            assert!(
                previous_name < contract.name,
                "builtin contracts must be strictly sorted: {previous_name} before {}",
                contract.name
            );
        }
        previous_name = Some(contract.name);

        let expected_id = BuiltinId::from_canonical_name(contract.name);
        assert_eq!(
            contract.id, expected_id,
            "builtin contract ID does not match canonical name: {}",
            contract.name
        );
        let mut callback_names = HashSet::new();
        for callback_name in contract.callback_parameter_names() {
            assert!(
                callback_names.insert(*callback_name),
                "duplicate callback parameter metadata for {}::${callback_name}",
                contract.name
            );
            assert!(
                contract
                    .params
                    .iter()
                    .any(|parameter| parameter.name == *callback_name),
                "callback parameter metadata references unknown parameter {}::${callback_name}",
                contract.name
            );
        }
        assert!(
            by_name.insert(contract.name, contract).is_none(),
            "duplicate builtin contract name: {}",
            contract.name
        );
        if let Some(existing) = by_id.insert(contract.id, contract) {
            panic!(
                "builtin contract ID collision between {} and {}",
                existing.name, contract.name
            );
        }
    }

    Registry { by_name, by_id }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the shared catalog validates and exposes every compiler/eval surface.
    #[test]
    fn catalog_is_valid_and_complete_for_all_contract_surfaces() {
        assert_eq!(contracts().len(), 567);
        assert_eq!(lookup("STRLEN").map(|contract| contract.name), Some("strlen"));
        assert_eq!(lookup("\\parse_url").map(|contract| contract.name), Some("parse_url"));
    }

    /// Verifies stable IDs resolve to the same contracts as PHP names.
    #[test]
    fn id_and_name_lookup_agree() {
        for contract in contracts() {
            assert!(std::ptr::eq(
                lookup(contract.name).expect("catalog name must resolve"),
                lookup_id(contract.id).expect("catalog ID must resolve")
            ));
        }
    }

    /// Verifies fixed bridge and runtime capabilities are visible on assembled contracts.
    #[test]
    fn assembled_contracts_include_neutral_requirements() {
        assert_eq!(
            lookup("bcadd").expect("bcadd contract").requirements,
            &[crate::BuiltinRequirement::Bridge("elephc_bcmath")]
        );
        assert_eq!(
            lookup("hash").expect("hash contract").requirements,
            &[crate::BuiltinRequirement::Bridge("elephc_crypto")]
        );
        assert_eq!(
            lookup("openssl_encrypt")
                .expect("openssl_encrypt contract")
                .requirements,
            &[crate::BuiltinRequirement::Bridge("elephc_crypto")]
        );
        assert_eq!(
            lookup("preg_match").expect("preg_match contract").requirements,
            &[crate::BuiltinRequirement::RuntimeCapability("pcre2")]
        );
        assert!(lookup("strlen")
            .expect("strlen contract")
            .requirements
            .is_empty());
    }
}
