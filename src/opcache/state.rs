//! Purpose:
//! Derives Zend OPcache's *cache-enabled* state — the boolean that governs whether
//! `opcache_reset()` (and the wider cache API) actually operates — from the shared
//! directive matrix in `crate::opcache::directives`. The OPcache *extension* is always
//! present; the *cache* is enabled only per-SAPI: under a web/FPM SAPI it follows
//! `opcache.enable`, under CLI it follows `opcache.enable_cli`.
//!
//! Called from:
//! - `crate::opcache_prelude` (native, bakes `opcache_reset(): bool` with the
//!   compile-time enabled constant selected by the compiler `web`/SAPI flag).
//! - `crates/elephc-magician` builtins (eval, via the same `#[path]` include that
//!   shares `directives.rs`), so the two surfaces never drift.
//!
//! Key details:
//! - This module is intentionally dependency-free (no `crate::` references): it is
//!   compiled into both the `elephc` and `elephc-magician` crates through a shared
//!   file include, so it must not name types from either crate. It re-uses
//!   `opcache_directives` from its sibling `directives` module, which is included the
//!   same way, so the enabled state stays correct if a directive default ever flips.
//! - Elephc's SAPI is a compile-time fact: the compiler's `web` flag is true for
//!   `--web` (alias `--with-web`) and false for a default CLI binary. Because directive values are
//!   the compiled-in defaults, the enabled state is a compile-time constant:
//!   default CLI binary → `opcache.enable_cli` default (`false`); `--web` binary →
//!   `opcache.enable` default (`true`). This matches reference PHP, where a bare
//!   `php script.php` run (`enable_cli=0`) reports the cache disabled.

use super::directives::{effective_opcache_directives, DirectiveValue};

/// Returns whether the OPcache *cache* is enabled for the given compile target and SAPI:
///
/// > `opcache.enable && (is_web_sapi || opcache.enable_cli)`
///
/// BOTH directives are consulted on CLI — `opcache.enable` is not a web-only switch. php-src
/// copies it into `ZCG(enabled)` and every cache-API guard tests that before anything else, while
/// `opcache.enable_cli` is the EXTRA condition the CLI SAPI adds; neither alone is sufficient
/// there. VERIFIED on reference PHP 8.5.6 with a script asserting
/// `opcache_get_status() === false`, `opcache_reset() === false`,
/// `opcache_invalidate(__FILE__) === false` and `opcache_is_script_cached(__FILE__) === false`:
///
/// | `-d opcache.enable` | `-d opcache.enable_cli` | all four `=== false`? |
/// |---------------------|-------------------------|-----------------------|
/// | `0`                 | `1`                     | YES — cache disabled  |
/// | `1`                 | `0`                     | YES — cache disabled  |
/// | `1`                 | `1`                     | no — cache enabled    |
///
/// An earlier revision read exactly ONE key (`enable_cli` on CLI, `enable` on web) and so let
/// `--ini opcache.enable=0 --ini opcache.enable_cli=1` compile a binary that reported the status
/// array and `true` from `opcache_reset()` while its own `opcache_get_configuration()` reported
/// `opcache.enable => false` — the binary contradicting itself.
///
/// The values are read from the shared directive table for `version_id` so the state stays correct
/// if a default ever changes. A missing or non-boolean directive (which the byte-verified table
/// never produces) is treated as disabled — the safe default that matches an unconfigured cache.
///
/// This is the no-override form (the eval interpreter has no `--ini`); the native compiler
/// calls [`opcache_cache_enabled_with_overrides`] so compile-time `--ini` overrides are honored.
/// `allow(dead_code)`: in `elephc` every caller now routes through the override form, so the
/// no-override wrapper is dead here; it stays live in the `elephc-magician` `#[path]` includes,
/// which call it directly (eval has no `--ini`).
#[allow(dead_code)]
pub fn opcache_cache_enabled(version_id: u32, is_web_sapi: bool) -> bool {
    opcache_cache_enabled_with_overrides(version_id, is_web_sapi, &[])
}

/// Same as [`opcache_cache_enabled`] but honoring compile-time `--ini` overrides, so
/// `--ini opcache.enable_cli=1` reports the cache enabled on a CLI binary and
/// `--ini opcache.enable=0` reports it disabled under `--web`. Reads the effective directive
/// table (`effective_opcache_directives`) rather than the raw defaults; passing an empty
/// override slice is byte-identical to the no-override form.
/// `allow(dead_code)`: dead in the `elephc-magician` `#[path]` includes (eval has no `--ini`),
/// live in `elephc`.
#[allow(dead_code)]
pub fn opcache_cache_enabled_with_overrides(
    version_id: u32,
    is_web_sapi: bool,
    overrides: &[(String, String)],
) -> bool {
    let directives = effective_opcache_directives(version_id, overrides);
    let flag = |key: &str| {
        directives
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| matches!(value, DirectiveValue::Bool(true)))
            .unwrap_or(false)
    };
    // `opcache.enable` is the MASTER switch: php-src copies it into `ZCG(enabled)` and every
    // cache-API guard tests that first, whatever the SAPI. `opcache.enable_cli` is an ADDITIONAL
    // requirement that applies only under the CLI SAPI, so the two are ANDed there.
    flag("opcache.enable") && (is_web_sapi || flag("opcache.enable_cli"))
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the compile-time enabled-state derivation: a web/FPM SAPI enables the cache
    //! (`opcache.enable` default true) and CLI disables it (`opcache.enable_cli` default
    //! false), for every maintained PHP version.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness (both crates that include this file).

    use super::*;

    /// The four maintained PHP language-version ids exercised by the matrix.
    const VERSIONS: [u32; 4] = [80200, 80300, 80400, 80500];

    /// Under a web/FPM SAPI the cache is enabled for every maintained version.
    #[test]
    fn web_sapi_enables_cache_for_all_versions() {
        for version in VERSIONS {
            assert!(
                opcache_cache_enabled(version, true),
                "web SAPI must enable the cache for {version}"
            );
        }
    }

    /// Under CLI the cache is disabled for every maintained version.
    #[test]
    fn cli_sapi_disables_cache_for_all_versions() {
        for version in VERSIONS {
            assert!(
                !opcache_cache_enabled(version, false),
                "CLI SAPI must disable the cache for {version}"
            );
        }
    }

    /// `--ini opcache.enable_cli=1` enables the cache on a CLI binary (`opcache.enable` defaults
    /// to true, so the AND is satisfied); `--ini opcache.enable=0` disables it under `--web`. An
    /// empty override slice matches the default derivation.
    #[test]
    fn overrides_flip_enabled_state() {
        let enable_cli = vec![("opcache.enable_cli".to_string(), "1".to_string())];
        let disable = vec![("opcache.enable".to_string(), "0".to_string())];
        for version in VERSIONS {
            assert!(
                opcache_cache_enabled_with_overrides(version, false, &enable_cli),
                "enable_cli=1 must enable the CLI cache for {version}"
            );
            assert!(
                !opcache_cache_enabled_with_overrides(version, true, &disable),
                "enable=0 must disable the web cache for {version}"
            );
            // Empty overrides are byte-identical to the no-override form.
            assert_eq!(
                opcache_cache_enabled_with_overrides(version, false, &[]),
                opcache_cache_enabled(version, false)
            );
        }
    }

    /// The CLI predicate is the CONJUNCTION of `opcache.enable` and `opcache.enable_cli`, so
    /// turning the master switch off disables the cache even with `enable_cli=1`. Reference table
    /// (PHP 8.5.6, `opcache_get_status()`/`opcache_reset()`/`opcache_invalidate()`/
    /// `opcache_is_script_cached()` all `=== false` when disabled):
    ///
    /// | `opcache.enable` | `opcache.enable_cli` | CLI      | web/FPM  |
    /// |------------------|----------------------|----------|----------|
    /// | `0`              | `1`                  | disabled | disabled |
    /// | `1`              | `0`                  | disabled | enabled  |
    /// | `1`              | `1`                  | enabled  | enabled  |
    /// | `0`              | `0`                  | disabled | disabled |
    ///
    /// The web column follows `opcache.enable` alone because `opcache.enable_cli` is tested only
    /// under the CLI SAPI in php-src.
    #[test]
    fn cli_state_ands_enable_with_enable_cli() {
        let ini = |enable: &str, enable_cli: &str| {
            vec![
                ("opcache.enable".to_string(), enable.to_string()),
                ("opcache.enable_cli".to_string(), enable_cli.to_string()),
            ]
        };
        for version in VERSIONS {
            for (enable, enable_cli, cli, web) in [
                ("0", "1", false, false),
                ("1", "0", false, true),
                ("1", "1", true, true),
                ("0", "0", false, false),
            ] {
                let overrides = ini(enable, enable_cli);
                assert_eq!(
                    opcache_cache_enabled_with_overrides(version, false, &overrides),
                    cli,
                    "CLI enable={enable} enable_cli={enable_cli} for {version}"
                );
                assert_eq!(
                    opcache_cache_enabled_with_overrides(version, true, &overrides),
                    web,
                    "web enable={enable} enable_cli={enable_cli} for {version}"
                );
            }
        }
    }
}
