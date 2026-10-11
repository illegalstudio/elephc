//! Purpose:
//! Owns PHP registration availability for builtin contracts across target platforms.
//!
//! Called from:
//! - Compiler, documentation, and Magician registry consumers before exposing a contract.
//!
//! Key details:
//! - PHP surface availability is independent from backend implementation support.
//! - Exceptions are keyed by stable `BuiltinId`, never backend-local name lists.

use crate::BuiltinId;

/// Dependency-neutral PHP platform families used by contract consumers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhpTarget {
    MacOs,
    Ios,
    Linux,
    Windows,
}

/// Platforms on which php-src registers a builtin contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PhpTargetAvailability {
    All,
    Unix,
    WindowsOnly,
}

impl PhpTargetAvailability {
    /// Returns whether php-src exposes the contract on `target`.
    pub const fn includes(self, target: PhpTarget) -> bool {
        match self {
            Self::All => true,
            Self::Unix => !matches!(target, PhpTarget::Windows),
            Self::WindowsOnly => matches!(target, PhpTarget::Windows),
        }
    }
}

const LCHGRP: u64 = BuiltinId::from_canonical_name("lchgrp").as_u64();
const LCHOWN: u64 = BuiltinId::from_canonical_name("lchown").as_u64();
const SAPI_WINDOWS_VT100_SUPPORT: u64 =
    BuiltinId::from_canonical_name("sapi_windows_vt100_support").as_u64();
const SAPI_WINDOWS_CP_SET: u64 =
    BuiltinId::from_canonical_name("sapi_windows_cp_set").as_u64();
const SAPI_WINDOWS_CP_GET: u64 =
    BuiltinId::from_canonical_name("sapi_windows_cp_get").as_u64();
const SAPI_WINDOWS_CP_CONV: u64 =
    BuiltinId::from_canonical_name("sapi_windows_cp_conv").as_u64();
const SAPI_WINDOWS_CP_IS_UTF8: u64 =
    BuiltinId::from_canonical_name("sapi_windows_cp_is_utf8").as_u64();
const SAPI_WINDOWS_SET_CTRL_HANDLER: u64 =
    BuiltinId::from_canonical_name("sapi_windows_set_ctrl_handler").as_u64();
const SAPI_WINDOWS_GENERATE_CTRL_EVENT: u64 =
    BuiltinId::from_canonical_name("sapi_windows_generate_ctrl_event").as_u64();

/// Returns the authoritative PHP registration policy for a builtin identity.
pub const fn php_target_availability(id: BuiltinId) -> PhpTargetAvailability {
    match id.as_u64() {
        LCHGRP | LCHOWN => PhpTargetAvailability::Unix,
        SAPI_WINDOWS_VT100_SUPPORT
        | SAPI_WINDOWS_CP_SET
        | SAPI_WINDOWS_CP_GET
        | SAPI_WINDOWS_CP_CONV
        | SAPI_WINDOWS_CP_IS_UTF8
        | SAPI_WINDOWS_SET_CTRL_HANDLER
        | SAPI_WINDOWS_GENERATE_CTRL_EVENT => PhpTargetAvailability::WindowsOnly,
        _ => PhpTargetAvailability::All,
    }
}

/// Returns whether one builtin identity is registered by php-src on `target`.
pub const fn php_builtin_is_available(id: BuiltinId, target: PhpTarget) -> bool {
    php_target_availability(id).includes(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_ownership_contracts_follow_have_lchown() {
        for name in ["lchgrp", "lchown"] {
            let id = BuiltinId::from_canonical_name(name);
            assert!(php_builtin_is_available(id, PhpTarget::MacOs));
            assert!(php_builtin_is_available(id, PhpTarget::Ios));
            assert!(php_builtin_is_available(id, PhpTarget::Linux));
            assert!(!php_builtin_is_available(id, PhpTarget::Windows));
        }
        for name in ["chgrp", "chown", "strlen"] {
            let id = BuiltinId::from_canonical_name(name);
            assert_eq!(php_target_availability(id), PhpTargetAvailability::All);
        }
    }

    #[test]
    fn windows_sapi_contracts_are_windows_only() {
        for name in [
            "sapi_windows_vt100_support",
            "sapi_windows_cp_set",
            "sapi_windows_cp_get",
            "sapi_windows_cp_conv",
            "sapi_windows_cp_is_utf8",
            "sapi_windows_set_ctrl_handler",
            "sapi_windows_generate_ctrl_event",
        ] {
            let id = BuiltinId::from_canonical_name(name);
            assert_eq!(
                php_target_availability(id),
                PhpTargetAvailability::WindowsOnly,
                "{name}",
            );
            assert!(php_builtin_is_available(id, PhpTarget::Windows), "{name}");
            assert!(!php_builtin_is_available(id, PhpTarget::MacOs), "{name}");
            assert!(!php_builtin_is_available(id, PhpTarget::Ios), "{name}");
            assert!(!php_builtin_is_available(id, PhpTarget::Linux), "{name}");
        }
    }
}
