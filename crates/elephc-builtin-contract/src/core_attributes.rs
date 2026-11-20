//! Purpose:
//! Shares PHP Core attribute target masks and target diagnostics between native and interpreted
//! validation.
//!
//! Called from:
//! - Compiler declaration checks, reflection factories, and Magician attribute construction.
//!
//! Key details:
//! - PHP 8.5 uses seven target bits, including global constants, followed by repeatability.
//! - Masks follow the selected profile: `Override` and `Deprecated` gained targets in 8.5, and a
//!   profile older than the attribute's class contract treats the name as a user attribute.
//! - Namespaced user attributes never match a global predefined attribute.

use crate::PhpVersion;

/// Targets class-like declarations.
pub const CORE_ATTRIBUTE_TARGET_CLASS: i64 = 1;
/// Targets functions.
pub const CORE_ATTRIBUTE_TARGET_FUNCTION: i64 = 2;
/// Targets methods.
pub const CORE_ATTRIBUTE_TARGET_METHOD: i64 = 4;
/// Targets properties.
pub const CORE_ATTRIBUTE_TARGET_PROPERTY: i64 = 8;
/// Targets class constants and enum cases.
pub const CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT: i64 = 16;
/// Targets function and method parameters.
pub const CORE_ATTRIBUTE_TARGET_PARAMETER: i64 = 32;
/// Targets global constants.
pub const CORE_ATTRIBUTE_TARGET_CONSTANT: i64 = 64;
/// Combines every PHP 8.5 attribute declaration target, including global constants.
pub const CORE_ATTRIBUTE_TARGET_ALL: i64 = 127;
/// Marks an attribute class as accepting repeated instances on one declaration.
pub const CORE_ATTRIBUTE_IS_REPEATABLE: i64 = 128;
/// Defines the PHP-visible Attribute class constants shared by prelude and validation consumers.
pub const CORE_ATTRIBUTE_CONSTANTS: &[(&str, i64)] = &[
    ("TARGET_CLASS", CORE_ATTRIBUTE_TARGET_CLASS), ("TARGET_FUNCTION", CORE_ATTRIBUTE_TARGET_FUNCTION),
    ("TARGET_METHOD", CORE_ATTRIBUTE_TARGET_METHOD), ("TARGET_PROPERTY", CORE_ATTRIBUTE_TARGET_PROPERTY),
    ("TARGET_CLASS_CONSTANT", CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT),
    ("TARGET_PARAMETER", CORE_ATTRIBUTE_TARGET_PARAMETER), ("TARGET_CONSTANT", CORE_ATTRIBUTE_TARGET_CONSTANT),
    ("TARGET_ALL", CORE_ATTRIBUTE_TARGET_ALL), ("IS_REPEATABLE", CORE_ATTRIBUTE_IS_REPEATABLE),
];
/// Pairs each target bit with PHP's diagnostic spelling, in PHP's listing order.
pub const CORE_ATTRIBUTE_TARGET_NAMES: &[(i64, &str)] = &[
    (CORE_ATTRIBUTE_TARGET_CLASS, "class"), (CORE_ATTRIBUTE_TARGET_FUNCTION, "function"),
    (CORE_ATTRIBUTE_TARGET_METHOD, "method"), (CORE_ATTRIBUTE_TARGET_PROPERTY, "property"),
    (CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT, "class constant"), (CORE_ATTRIBUTE_TARGET_PARAMETER, "parameter"),
    (CORE_ATTRIBUTE_TARGET_CONSTANT, "constant"),
];

/// Returns target and repeatability flags for a global attribute `php_version` predefines.
pub fn core_attribute_flags(name: &str, php_version: PhpVersion) -> Option<i64> {
    let flags = match name.trim_start_matches('\\').to_ascii_lowercase().as_str() {
        "attribute" | "allowdynamicproperties" => CORE_ATTRIBUTE_TARGET_CLASS,
        "returntypewillchange" => CORE_ATTRIBUTE_TARGET_METHOD,
        "sensitiveparameter" => CORE_ATTRIBUTE_TARGET_PARAMETER,
        "override" if php_version < PhpVersion::Php85 => CORE_ATTRIBUTE_TARGET_METHOD,
        "override" => CORE_ATTRIBUTE_TARGET_METHOD | CORE_ATTRIBUTE_TARGET_PROPERTY,
        "deprecated" if php_version < PhpVersion::Php85 => {
            CORE_ATTRIBUTE_TARGET_FUNCTION | CORE_ATTRIBUTE_TARGET_METHOD | CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT
        }
        "deprecated" => {
            CORE_ATTRIBUTE_TARGET_CLASS
                | CORE_ATTRIBUTE_TARGET_FUNCTION
                | CORE_ATTRIBUTE_TARGET_METHOD
                | CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT
                | CORE_ATTRIBUTE_TARGET_CONSTANT
        }
        "nodiscard" => CORE_ATTRIBUTE_TARGET_FUNCTION | CORE_ATTRIBUTE_TARGET_METHOD,
        "delayedtargetvalidation" => CORE_ATTRIBUTE_TARGET_ALL,
        _ => return None,
    };
    crate::lookup_class(name)
        .is_some_and(|class| class.since.is_none_or(|since| php_version >= since))
        .then_some(flags)
}

/// Formats PHP's error for an attribute used on `target` outside its allowed `flags`.
pub fn core_attribute_target_error(name: &str, target: i64, flags: i64) -> String {
    format!(
        "Attribute \"{name}\" cannot target {} (allowed targets: {})",
        target_names(target),
        target_names(flags),
    )
}

/// Lists the target bits set in `flags`, ignoring repeatability.
fn target_names(flags: i64) -> String {
    CORE_ATTRIBUTE_TARGET_NAMES
        .iter()
        .filter(|(target, _)| flags & target != 0)
        .map(|(_, name)| *name)
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Masks match php-src's `Zend/zend_attributes.stub.php` on each release branch.
    #[test]
    fn masks_follow_each_php_branch() {
        let cases = [
            ("Attribute", PhpVersion::Php80, Some(1)),
            ("ReturnTypeWillChange", PhpVersion::Php80, None),
            ("ReturnTypeWillChange", PhpVersion::Php81, Some(4)),
            ("AllowDynamicProperties", PhpVersion::Php81, None),
            ("AllowDynamicProperties", PhpVersion::Php82, Some(1)),
            ("SensitiveParameter", PhpVersion::Php81, None),
            ("SensitiveParameter", PhpVersion::Php82, Some(32)),
            ("Override", PhpVersion::Php82, None),
            ("Override", PhpVersion::Php83, Some(4)),
            ("Override", PhpVersion::Php84, Some(4)),
            ("Override", PhpVersion::Php85, Some(12)),
            ("Override", PhpVersion::Php86, Some(12)),
            ("Deprecated", PhpVersion::Php83, None),
            ("Deprecated", PhpVersion::Php84, Some(22)),
            ("Deprecated", PhpVersion::Php85, Some(87)),
            ("NoDiscard", PhpVersion::Php84, None),
            ("NoDiscard", PhpVersion::Php85, Some(6)),
            ("DelayedTargetValidation", PhpVersion::Php84, None),
            ("DelayedTargetValidation", PhpVersion::Php85, Some(127)),
        ];
        for (name, version, expected) in cases {
            assert_eq!(core_attribute_flags(name, version), expected, "{name} on {version}");
        }
    }

    /// Lookups keep PHP's case-insensitive global names and leave user attributes alone.
    #[test]
    fn names_are_global_and_case_insensitive() {
        assert_eq!(core_attribute_flags("\\OVERRIDE", PhpVersion::Php85), Some(12));
        assert_eq!(core_attribute_flags("App\\Override", PhpVersion::Php85), None);
    }

    /// Target lists use PHP's order regardless of bit combination, and skip repeatability.
    #[test]
    fn target_errors_use_php_wording_and_order() {
        assert_eq!(
            core_attribute_target_error("Deprecated", CORE_ATTRIBUTE_TARGET_PROPERTY, 87 | CORE_ATTRIBUTE_IS_REPEATABLE),
            "Attribute \"Deprecated\" cannot target property (allowed targets: class, function, method, class constant, constant)",
        );
        assert_eq!(
            CORE_ATTRIBUTE_TARGET_NAMES.iter().fold(0, |mask, (target, _)| mask | target),
            CORE_ATTRIBUTE_TARGET_ALL,
        );
    }
}
