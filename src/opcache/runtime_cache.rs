//! Purpose:
//! Derives the runtime script cache's configuration — the `opcache.*` subset that
//! actually governs caching, as opposed to the ones elephc only reports — from the
//! shared directive matrix plus the compile-time `--ini` overrides.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::eval` (emits the values into the
//!   `__elephc_eval_configure_opcache` bridge call).
//!
//! Key details:
//! - This is the compiler half of the channel. The runtime cannot derive these
//!   values: `--ini` is a compile-time flag with no runtime counterpart, so the
//!   effective directive set exists only here.
//! - `enabled` is the same predicate `opcache_reset()` and friends are baked with,
//!   read through `opcache_cache_enabled_with_overrides`, so the cache cannot end up
//!   active in a binary whose own `opcache_get_status()` reports it disabled.
//! - Values are the NORMALIZED ones (`opcache.memory_consumption` in bytes, not the
//!   raw `"128"` that `ini_get()` reports), because they are consumed as quantities.

use super::directives::{effective_opcache_directives, DirectiveValue};
use super::state::opcache_cache_enabled_with_overrides;

/// The directive values the runtime script cache needs, resolved for one compilation.
///
/// NOT `Copy`: the two path directives are owned strings, because they are emitted into
/// the binary's read-only data rather than passed as immediates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeCacheConfig {
    /// `opcache.enable && (web || opcache.enable_cli)` — the master gate.
    pub enabled: bool,
    /// `opcache.validate_timestamps`.
    pub validate_timestamps: bool,
    /// `opcache.revalidate_freq`, in seconds.
    pub revalidate_freq: u64,
    /// `opcache.max_file_size`, in bytes; `0` means no limit.
    pub max_file_size: u64,
    /// `opcache.memory_consumption`, in bytes.
    pub memory_consumption: u64,
    /// `opcache.max_accelerated_files`, as an entry count.
    pub max_accelerated_files: u64,
    /// `opcache.file_cache`. EMPTY means unset — php-src's C `NULL` default.
    pub file_cache: String,
    /// `opcache.file_cache_read_only`. An 8.5-only directive; `false` on older targets,
    /// which is what the absent-directive lookup already yields.
    pub file_cache_read_only: bool,
    /// `opcache.log_verbosity_level`, the gate on the accelerator diagnostic channel.
    pub log_verbosity_level: i64,
    /// `opcache.error_log`. Empty, or the literal `stderr`, means stderr.
    pub error_log: String,
    /// `opcache.file_update_protection`, in seconds; `0` disables the guard.
    pub file_update_protection: u64,
    /// `opcache.blacklist_filename` — a `glob()` naming the files that list the paths to
    /// run but never cache. Empty means unset.
    pub blacklist_filename: String,
}

/// Resolves the runtime cache configuration for a compile target and SAPI.
pub fn runtime_cache_config(
    version_id: u32,
    is_web_sapi: bool,
    overrides: &[(String, String)],
) -> RuntimeCacheConfig {
    let directives = effective_opcache_directives(version_id, overrides);
    let boolean = |key: &str| {
        directives
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| matches!(value, DirectiveValue::Bool(true)))
            .unwrap_or(false)
    };
    // A negative or missing count is clamped to 0 rather than wrapping: the byte-verified
    // table never produces one, and 0 is the reading that refuses rather than admits.
    let count = |key: &str| {
        directives
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| match value {
                DirectiveValue::Int(raw) => u64::try_from(*raw).ok(),
                _ => None,
            })
            .unwrap_or(0)
    };
    // A missing string directive reads as empty, which is exactly how both of these
    // spell "unset" — so an 8.2 target, where some of them do not exist, needs no
    // per-version branch here.
    let text = |key: &str| {
        directives
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| match value {
                DirectiveValue::Str(raw) => Some((*raw).to_string()),
                _ => None,
            })
            .unwrap_or_default()
    };
    // `opcache.log_verbosity_level` is signed in php-src and its gate is a `<=`, so a
    // negative value silences even the fatals' LINE (never their exit). Preserving the
    // sign rather than clamping to 0 is what reproduces that.
    let signed = |key: &str| {
        directives
            .iter()
            .find(|(name, _)| *name == key)
            .and_then(|(_, value)| match value {
                DirectiveValue::Int(raw) => Some(*raw),
                _ => None,
            })
            .unwrap_or(0)
    };
    RuntimeCacheConfig {
        enabled: opcache_cache_enabled_with_overrides(version_id, is_web_sapi, overrides),
        validate_timestamps: boolean("opcache.validate_timestamps"),
        revalidate_freq: count("opcache.revalidate_freq"),
        max_file_size: count("opcache.max_file_size"),
        memory_consumption: count("opcache.memory_consumption"),
        max_accelerated_files: count("opcache.max_accelerated_files"),
        file_cache: text("opcache.file_cache"),
        file_cache_read_only: boolean("opcache.file_cache_read_only"),
        log_verbosity_level: signed("opcache.log_verbosity_level"),
        error_log: text("opcache.error_log"),
        file_update_protection: count("opcache.file_update_protection"),
        blacklist_filename: text("opcache.blacklist_filename"),
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the runtime cache configuration against the directive defaults and the
    //! `--ini` overrides that move it.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The CLI/web split of `enabled` is the gate that keeps a default CLI binary
    //!   on the pre-cache code path, so it is asserted directly rather than inferred.

    use super::*;

    /// The PHP profile the defaults below are pinned against.
    const PHP_85: u32 = 80500;

    /// Verifies a default CLI binary resolves the cache disabled.
    #[test]
    fn a_default_cli_binary_leaves_the_cache_disabled() {
        assert!(!runtime_cache_config(PHP_85, false, &[]).enabled);
    }

    /// Verifies a `--web` binary resolves the cache enabled.
    #[test]
    fn a_web_binary_enables_the_cache() {
        assert!(runtime_cache_config(PHP_85, true, &[]).enabled);
    }

    /// Verifies `--ini opcache.enable_cli=1` turns a CLI binary's cache on.
    #[test]
    fn enable_cli_turns_a_cli_binary_on() {
        let overrides = [("opcache.enable_cli".to_string(), "1".to_string())];

        assert!(runtime_cache_config(PHP_85, false, &overrides).enabled);
    }

    /// Verifies `--ini opcache.enable=0` turns a `--web` binary's cache off.
    #[test]
    fn the_master_switch_turns_a_web_binary_off() {
        let overrides = [("opcache.enable".to_string(), "0".to_string())];

        assert!(!runtime_cache_config(PHP_85, true, &overrides).enabled);
    }

    /// Verifies the normalized defaults reach the runtime, not the raw INI spellings.
    ///
    /// `opcache.memory_consumption` in particular must arrive as the 128 MiB BYTE count,
    /// since the cache spends it as a byte budget; `ini_get()` reports `"128"`.
    #[test]
    fn defaults_are_the_normalized_quantities() {
        let config = runtime_cache_config(PHP_85, true, &[]);

        assert!(config.validate_timestamps);
        assert_eq!(config.revalidate_freq, 2);
        assert_eq!(config.max_file_size, 0);
        assert_eq!(config.memory_consumption, 134_217_728);
        assert_eq!(config.max_accelerated_files, 10_000);
    }

    /// Verifies the file-cache and diagnostic directives default to php-src's own values.
    ///
    /// An empty `opcache.file_cache` is what makes a default binary skip the startup
    /// validation entirely, and verbosity `1` is php-src's default — the level at which a
    /// FATAL prints and a WARNING does not.
    #[test]
    fn file_cache_and_log_directives_carry_the_reference_defaults() {
        let config = runtime_cache_config(PHP_85, true, &[]);

        assert_eq!(config.file_cache, "");
        assert!(!config.file_cache_read_only);
        assert_eq!(config.log_verbosity_level, 1);
        assert_eq!(config.error_log, "");
    }

    /// Verifies `--ini` moves the directives that drive the validation and the log channel.
    #[test]
    fn an_ini_override_moves_the_file_cache_directives() {
        let overrides = [
            ("opcache.file_cache".to_string(), "/var/cache/oc".to_string()),
            ("opcache.file_cache_read_only".to_string(), "1".to_string()),
            ("opcache.log_verbosity_level".to_string(), "3".to_string()),
            ("opcache.error_log".to_string(), "/tmp/accel.log".to_string()),
        ];
        let config = runtime_cache_config(PHP_85, true, &overrides);

        assert_eq!(config.file_cache, "/var/cache/oc");
        assert!(config.file_cache_read_only);
        assert_eq!(config.log_verbosity_level, 3);
        assert_eq!(config.error_log, "/tmp/accel.log");
    }

    /// Verifies an 8.2 target reports `file_cache_read_only` as `false` rather than failing.
    ///
    /// The directive is registered only by 8.5, so the lookup finds nothing — and "nothing"
    /// must read as `false`, which is the branch that never raises the read-only fatal.
    #[test]
    fn an_older_target_has_no_file_cache_read_only() {
        let overrides = [("opcache.file_cache_read_only".to_string(), "1".to_string())];

        assert!(!runtime_cache_config(80200, true, &overrides).file_cache_read_only);
    }

    /// Verifies a `--ini` override moves the value the cache actually spends.
    #[test]
    fn an_ini_override_moves_the_runtime_value() {
        let overrides = [
            ("opcache.revalidate_freq".to_string(), "60".to_string()),
            ("opcache.max_file_size".to_string(), "4096".to_string()),
            ("opcache.validate_timestamps".to_string(), "0".to_string()),
        ];
        let config = runtime_cache_config(PHP_85, true, &overrides);

        assert_eq!(config.revalidate_freq, 60);
        assert_eq!(config.max_file_size, 4096);
        assert!(!config.validate_timestamps);
    }
}
