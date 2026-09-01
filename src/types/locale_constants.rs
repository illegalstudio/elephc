//! Purpose:
//! Defines PHP's target-specific `LC_*` locale category constants.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constant types.
//! - `crate::codegen_support::prescan` when materializing target-specific values.
//!
//! Key details:
//! - PHP exposes the host C library's category values, so Darwin and glibc use different
//!   integers for the same constant names.

/// Locale category names accepted by PHP's `setlocale()`.
pub(crate) const LOCALE_CONSTANT_NAMES: &[&str] = &[
    "LC_CTYPE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_COLLATE",
    "LC_MONETARY",
    "LC_ALL",
    "LC_MESSAGES",
];

/// Darwin locale category values exposed by macOS PHP.
pub(crate) const MACOS_LOCALE_INT_CONSTANTS: &[(&str, i64)] = &[
    ("LC_CTYPE", 2),
    ("LC_NUMERIC", 4),
    ("LC_TIME", 5),
    ("LC_COLLATE", 1),
    ("LC_MONETARY", 3),
    ("LC_ALL", 0),
    ("LC_MESSAGES", 6),
];

/// glibc locale category values exposed by Linux PHP.
pub(crate) const LINUX_LOCALE_INT_CONSTANTS: &[(&str, i64)] = &[
    ("LC_CTYPE", 0),
    ("LC_NUMERIC", 1),
    ("LC_TIME", 2),
    ("LC_COLLATE", 3),
    ("LC_MONETARY", 4),
    ("LC_ALL", 6),
    ("LC_MESSAGES", 5),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies each supported target table covers the same PHP locale constant names once.
    #[test]
    fn locale_constant_tables_cover_all_names() {
        for table in [MACOS_LOCALE_INT_CONSTANTS, LINUX_LOCALE_INT_CONSTANTS] {
            let mut names = table.iter().map(|(name, _)| *name).collect::<Vec<_>>();
            names.sort_unstable();
            names.dedup();
            let mut expected = LOCALE_CONSTANT_NAMES.to_vec();
            expected.sort_unstable();
            assert_eq!(names, expected);
        }
    }
}
