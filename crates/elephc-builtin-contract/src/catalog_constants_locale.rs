//! Purpose:
//! Canonical contracts for the locale constants declared by the injected locale prelude.
//!
//! Called from:
//! - `crate::symbol_registry` when assembling the complete shared constant catalog.
//!
//! Key details:
//! - Every value is `TargetDependent`: Apple's BSD libc and glibc number the `LC_*`
//!   categories and `CODESET` differently, and `elephc::locale_prelude` injects the
//!   selected target's numbers.
//! - The `Prelude` route keeps these names out of the unconditional predefined set.

use crate::{BuiltinId, ConstType, ConstValue, ConstantContract, ConstantRoute, PhpModule};

/// Builds one locale constant contract.
macro_rules! constant {
    ($name:literal) => {
        ConstantContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            module: PhpModule::Standard,
            since: None,
            value: ConstValue::TargetDependent(ConstType::Int),
            route: ConstantRoute::Prelude,
            extension: false,
            internal: false,
        }
    };
}

/// The locale constants, one per libc category plus the portable codeset item.
pub(crate) static CONSTANTS: &[ConstantContract] = &[
    constant!("CODESET"),
    constant!("LC_ALL"),
    constant!("LC_COLLATE"),
    constant!("LC_CTYPE"),
    constant!("LC_MESSAGES"),
    constant!("LC_MONETARY"),
    constant!("LC_NUMERIC"),
    constant!("LC_TIME"),
];
