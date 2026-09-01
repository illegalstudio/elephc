//! Purpose:
//! Owns the PHP language profile runtime eval reports, so that a binary compiled
//! `--php-version 8.2` reports `8.2.0` from inside `eval()` exactly as it does
//! natively. Without this, the version surface forks at the eval boundary.
//!
//! Called from:
//! - `crate::ffi::context::__elephc_eval_set_php_version_id` (generated code sets
//!   the profile before every runtime eval dispatch).
//! - `crate::interpreter::constant_eval` (`PHP_VERSION`, `PHP_VERSION_ID`,
//!   `PHP_MINOR_VERSION`).
//! - `crate::interpreter::builtins::network_env` (`phpversion()` and the four
//!   opcache builtins whose directive set is profile-gated).
//!
//! Key details:
//! - Thread-local, mirroring `crate::strict_php_mode`: the profile is a property
//!   of the whole compiled binary, and elephc programs execute the setter and
//!   every eval fragment on one thread (fibers switch stacks, not OS threads),
//!   while parallel `cargo test` threads stay isolated.
//! - The default is the NEWEST profile, not the oldest. Anything linking this
//!   archive without elephc's codegen — every test harness in this crate
//!   included — observes the pre-existing behaviour unchanged.
//! - An id outside the supported set is IGNORED rather than stored. The version
//!   string is a lookup, not a computation, so an unknown id has no spelling;
//!   keeping the default is the only answer that stays a real PHP version.

use std::cell::Cell;

/// The profile eval reports when generated code never sets one.
///
/// KEEP IN SYNC with the default of `--php-version` in the compiler
/// (`crate::web_prelude::PhpVersion::default()`).
const DEFAULT_EVAL_PHP_VERSION_ID: u32 = 80510;

/// The supported profiles, paired with the `PHP_VERSION` string each one reports.
///
/// KEEP IN SYNC with `crate::php_version::PhpVersion::ALL` and `version_string()`
/// in the compiler. The patch component is `0` for every entry by the same rule
/// the compiler applies — elephc targets a language profile, not an upstream
/// patch release.
const EVAL_PHP_PROFILES: &[(u32, &str)] = &[
    (80000, "8.0.0"),
    (80100, "8.1.0"),
    (80200, "8.2.0"),
    (80300, "8.3.0"),
    (80400, "8.4.0"),
    (80510, "8.5.10-dev"),
    (80600, "8.6.0"),
];

thread_local! {
    /// The profile the binary embedding this bridge was compiled for.
    static EVAL_PHP_VERSION_ID: Cell<u32> = const { Cell::new(DEFAULT_EVAL_PHP_VERSION_ID) };
}

/// Selects the PHP profile eval reports on the current thread.
///
/// Ids outside [`EVAL_PHP_PROFILES`] leave the current profile untouched.
pub(crate) fn set_eval_php_version_id(id: u32) {
    if EVAL_PHP_PROFILES.iter().any(|(known, _)| *known == id) {
        EVAL_PHP_VERSION_ID.with(|cell| cell.set(id));
    }
}

/// Returns `PHP_VERSION_ID` for the profile active on the current thread.
pub(crate) fn eval_php_version_id() -> u32 {
    EVAL_PHP_VERSION_ID.with(Cell::get)
}

/// Returns `PHP_VERSION` for the profile active on the current thread.
pub(crate) fn eval_php_version_string() -> &'static str {
    let id = eval_php_version_id();
    EVAL_PHP_PROFILES
        .iter()
        .find(|(known, _)| *known == id)
        .map_or("8.5.10-dev", |(_, spelling)| *spelling)
}

/// Returns `PHP_MINOR_VERSION` for the profile active on the current thread.
///
/// `PHP_MAJOR_VERSION`, `PHP_RELEASE_VERSION` and `PHP_EXTRA_VERSION` have no
/// equivalent here because they are invariant across every supported profile:
/// `8`, `0` and `""` from 8.2 through 8.5.
pub(crate) fn eval_php_minor_version() -> i64 {
    i64::from((eval_php_version_id() / 100) % 100)
}

/// Returns `PHP_RELEASE_VERSION` for the active profile.
pub(crate) fn eval_php_release_version() -> i64 {
    i64::from(eval_php_version_id() % 100)
}

/// Returns `PHP_EXTRA_VERSION` for the active frozen profile.
pub(crate) fn eval_php_extra_version() -> &'static str {
    if eval_php_version_id() == 80510 { "-dev" } else { "" }
}

/// RAII guard restoring the previous profile on drop.
///
/// Test fixtures hold one of these instead of calling [`set_eval_php_version_id`]
/// in pairs, so a panicking assertion cannot leak a profile into later fixtures
/// on the same thread.
#[cfg(test)]
pub(crate) struct EvalPhpProfileGuard {
    previous: u32,
}

#[cfg(test)]
impl Drop for EvalPhpProfileGuard {
    /// Restores the profile captured when the guard was created.
    fn drop(&mut self) {
        set_eval_php_version_id(self.previous);
    }
}

/// Selects `id` as the eval profile and returns a guard restoring the previous one.
#[cfg(test)]
pub(crate) fn scoped_profile(id: u32) -> EvalPhpProfileGuard {
    let previous = eval_php_version_id();
    set_eval_php_version_id(id);
    EvalPhpProfileGuard { previous }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The default profile is the newest one, so linking this archive without
    /// elephc's codegen observes the behaviour that shipped before the setter.
    #[test]
    fn default_profile_is_the_newest_one() {
        assert_eq!(eval_php_version_id(), 80510);
        assert_eq!(eval_php_version_string(), "8.5.10-dev");
        assert_eq!(eval_php_minor_version(), 5);
    }

    /// Every supported profile round-trips through the setter.
    #[test]
    fn every_supported_profile_reports_its_own_spelling() {
        for (id, spelling) in EVAL_PHP_PROFILES {
            let _guard = scoped_profile(*id);
            assert_eq!(eval_php_version_id(), *id);
            assert_eq!(eval_php_version_string(), *spelling);
            assert_eq!(eval_php_minor_version(), i64::from((*id / 100) % 100));
        }
    }

    /// An unsupported id leaves the active profile alone rather than inventing
    /// a version string for it.
    #[test]
    fn an_unsupported_id_is_ignored() {
        let _guard = scoped_profile(80200);
        set_eval_php_version_id(90000);
        assert_eq!(eval_php_version_id(), 80200);
        assert_eq!(eval_php_version_string(), "8.2.0");
    }

    /// The spelling table agrees with the id it is keyed by, so a typo in one
    /// column cannot silently ship.
    #[test]
    fn spellings_agree_with_their_ids() {
        for (id, spelling) in EVAL_PHP_PROFILES {
            assert!(spelling.starts_with(&format!("{}.{}.", id / 10000, (id / 100) % 100)));
        }
    }
}
