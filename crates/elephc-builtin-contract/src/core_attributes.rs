//! Purpose:
//! Shares PHP Core attribute target masks between native and interpreted validation.
//!
//! Called from:
//! - Compiler declaration checks, reflection factories, and Magician attribute construction.
//!
//! Key details:
//! - PHP 8.5 uses seven target bits, including global constants, followed by repeatability.
//! - Namespaced user attributes never match a global predefined attribute.

/// Combines every PHP 8.5 attribute declaration target, including global constants.
pub const CORE_ATTRIBUTE_TARGET_ALL: i64 = 127;
/// Marks an attribute class as accepting repeated instances on one declaration.
pub const CORE_ATTRIBUTE_IS_REPEATABLE: i64 = 128;
/// Defines the PHP-visible Attribute class constants shared by prelude and validation consumers.
pub const CORE_ATTRIBUTE_CONSTANTS: &[(&str, i64)] = &[
    ("TARGET_CLASS", 1), ("TARGET_FUNCTION", 2), ("TARGET_METHOD", 4),
    ("TARGET_PROPERTY", 8), ("TARGET_CLASS_CONSTANT", 16), ("TARGET_PARAMETER", 32),
    ("TARGET_CONSTANT", 64), ("TARGET_ALL", CORE_ATTRIBUTE_TARGET_ALL),
    ("IS_REPEATABLE", CORE_ATTRIBUTE_IS_REPEATABLE),
];

/// Returns predefined target and repeatability flags for a canonical global attribute name.
pub fn core_attribute_flags(name: &str) -> Option<i64> {
    Some(match name.trim_start_matches('\\').to_ascii_lowercase().as_str() {
        "attribute" | "allowdynamicproperties" => 1,
        "returntypewillchange" => 4,
        "sensitiveparameter" => 32,
        "override" => 12,
        "deprecated" => 87,
        "nodiscard" => 6,
        "delayedtargetvalidation" => CORE_ATTRIBUTE_TARGET_ALL,
        _ => return None,
    })
}
