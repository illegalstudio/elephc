//! Purpose:
//! Owns the runtime script cache's configuration — the `opcache.*` directives that
//! actually govern caching behaviour, as opposed to the ones elephc merely reports.
//! Defaults come from the shared directive matrix; generated code overrides them with
//! the compile-time `--ini`-effective values before the first include.
//!
//! Called from:
//! - `crate::script_cache::store` for every lookup and fill decision.
//! - `crate::ffi::context::__elephc_eval_configure_opcache()` (generated code).
//!
//! Key details:
//! - `enabled` is the master gate. It mirrors `opcache_cache_enabled`, so a default
//!   CLI binary caches nothing and behaves exactly as it did before this module
//!   existed; `--web` and `--ini opcache.enable_cli=1` turn it on.
//! - Thread-local, mirroring `crate::eval_php_profile`: the configuration is a
//!   property of the whole compiled binary, and elephc programs execute the setter
//!   and every eval fragment on one thread, while parallel `cargo test` threads stay
//!   isolated from one another.
//! - Defaults here must stay reachable without generated code: every harness that
//!   links this archive directly observes the cache DISABLED, which is the
//!   pre-existing behaviour.

use std::cell::RefCell;

/// The `opcache.*` subset that governs what the runtime script cache actually does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ScriptCacheConfig {
    /// The master gate: `opcache.enable && (web || opcache.enable_cli)`.
    pub(crate) enabled: bool,
    /// `opcache.validate_timestamps` — revalidate an entry against the file's mtime.
    pub(crate) validate_timestamps: bool,
    /// `opcache.revalidate_freq` — seconds between two revalidations of one entry.
    pub(crate) revalidate_freq: u64,
    /// `opcache.max_file_size` — refuse larger files; `0` means no limit.
    pub(crate) max_file_size: u64,
    /// `opcache.memory_consumption` — the cache's byte budget.
    pub(crate) memory_consumption: usize,
    /// `opcache.max_accelerated_files` — the cache's entry-count ceiling.
    pub(crate) max_accelerated_files: usize,
    /// `opcache.file_update_protection` — refuse to cache a file this many seconds
    /// young, so a file caught mid-write is never stored. `0` disables the guard.
    pub(crate) file_update_protection: u64,
}

impl ScriptCacheConfig {
    /// Returns the configuration a binary without generated OPcache wiring observes.
    ///
    /// Disabled, so linking this archive without elephc's codegen — every test harness
    /// in this crate included — keeps the pre-cache behaviour of re-reading and
    /// re-parsing each include.
    pub(crate) const fn disabled() -> Self {
        Self {
            enabled: false,
            validate_timestamps: true,
            revalidate_freq: 2,
            max_file_size: 0,
            memory_consumption: 128 * 1024 * 1024,
            max_accelerated_files: 10_000,
            file_update_protection: 2,
        }
    }

    /// Returns whether a file last modified at `mtime` may be admitted now.
    ///
    /// php-src refuses to cache a file younger than `opcache.file_update_protection`
    /// seconds, so a file caught part-written is never stored. The comparison is a
    /// STRICT `<` against the age: VERIFIED on reference PHP 8.5.10 that with the
    /// default `2`, ages 0 and 1 are refused and age 2 is admitted.
    ///
    /// `0` disables the guard (every age is `>= 0`), and an UNKNOWN mtime is admitted —
    /// php-src compares against a zero timestamp there, which can never be in the
    /// protected window either.
    pub(crate) fn admits_age(&self, mtime: Option<i64>, now: i64) -> bool {
        let Some(mtime) = mtime else {
            return true;
        };
        // A file dated in the FUTURE has a negative age and is refused while the clock
        // catches up, which is what php-src's comparison does with the same inputs.
        now.saturating_sub(mtime) >= self.file_update_protection as i64
    }

    /// Returns whether a file of `size` bytes may be admitted under `max_file_size`.
    ///
    /// php-src treats `0` as "no limit" rather than "refuse everything", and compares
    /// with a strict `>`: a file of exactly `max_file_size` bytes is admitted.
    pub(crate) const fn admits_size(&self, size: u64) -> bool {
        self.max_file_size == 0 || size <= self.max_file_size
    }
}

thread_local! {
    /// The configuration the binary embedding this bridge was compiled with.
    static SCRIPT_CACHE_CONFIG: RefCell<ScriptCacheConfig> =
        RefCell::new(ScriptCacheConfig::disabled());
}

/// Installs the compile-time OPcache configuration for the current thread.
pub(crate) fn set_config(config: ScriptCacheConfig) {
    SCRIPT_CACHE_CONFIG.with(|cell| *cell.borrow_mut() = config);
}

thread_local! {
    /// Per-directive `ini_set()` overrides, indexed by the ids below.
    ///
    /// KEPT SEPARATE FROM THE CONFIGURATION ON PURPOSE, and this is load-bearing rather
    /// than tidy: generated code installs the compiled configuration when the eval context
    /// is first created, which happens at the program's FIRST eval — and an `ini_set()`
    /// before that point would otherwise be silently clobbered by an install that runs
    /// later. Holding overrides beside the configuration and applying them on read makes
    /// the order irrelevant, and matches PHP, where `ini_set()` outranks the ini file.
    static DIRECTIVE_OVERRIDES: RefCell<[Option<u64>; DIRECTIVE_COUNT]> =
        const { RefCell::new([None; DIRECTIVE_COUNT]) };
}

/// Returns the configuration active on the current thread, with `ini_set()` applied.
pub(crate) fn config() -> ScriptCacheConfig {
    let mut config = SCRIPT_CACHE_CONFIG.with(|cell| cell.borrow().clone());
    DIRECTIVE_OVERRIDES.with(|cell| {
        let overrides = cell.borrow();
        if let Some(value) = overrides[DIRECTIVE_REVALIDATE_FREQ as usize] {
            config.revalidate_freq = value;
        }
        if let Some(value) = overrides[DIRECTIVE_VALIDATE_TIMESTAMPS as usize] {
            config.validate_timestamps = value != 0;
        }
        if let Some(value) = overrides[DIRECTIVE_FILE_UPDATE_PROTECTION as usize] {
            config.file_update_protection = value;
        }
    });
    config
}

/// `opcache.revalidate_freq`, as [`swap_directive`] addresses it.
pub const DIRECTIVE_REVALIDATE_FREQ: u64 = 0;
/// `opcache.validate_timestamps`, as [`swap_directive`] addresses it.
pub const DIRECTIVE_VALIDATE_TIMESTAMPS: u64 = 1;
/// `opcache.file_update_protection`, as [`swap_directive`] addresses it.
pub const DIRECTIVE_FILE_UPDATE_PROTECTION: u64 = 2;

/// The value [`swap_directive`] answers for an id it does not know.
pub const DIRECTIVE_UNKNOWN: u64 = u64::MAX;

/// How many ids [`swap_directive`] knows; the override table's width.
const DIRECTIVE_COUNT: usize = 3;

/// Installs one directive's value on the live configuration, returning the previous one.
///
/// This is the mutable half of the channel, and it exists because the three directives
/// it addresses are `PHP_INI_ALL` in php-src: `ini_set()` genuinely moves them there, and
/// the runtime cache reads all three on every lookup, so a change takes effect on the
/// very next include rather than needing a restart.
///
/// THE IDS ARE A WIRE CONTRACT shared with generated code, not an internal detail — they
/// are matched by number on the other side of the C ABI, so their values may never be
/// reordered, only appended to.
///
/// Answers [`DIRECTIVE_UNKNOWN`] for an id this build does not know, which is what lets
/// generated code from a newer compiler fail soft against an older archive instead of
/// silently writing the wrong field.
pub fn swap_directive(id: u64, value: u64, as_override: bool) -> u64 {
    if id as usize >= DIRECTIVE_COUNT {
        return DIRECTIVE_UNKNOWN;
    }
    // The PREVIOUS value is the one a reader would have seen, so it comes from the
    // override-applied view rather than from the raw compiled configuration.
    let effective = config();
    let previous = match id {
        DIRECTIVE_REVALIDATE_FREQ => effective.revalidate_freq,
        DIRECTIVE_VALIDATE_TIMESTAMPS => u64::from(effective.validate_timestamps),
        DIRECTIVE_FILE_UPDATE_PROTECTION => effective.file_update_protection,
        _ => return DIRECTIVE_UNKNOWN,
    };
    if as_override {
        DIRECTIVE_OVERRIDES.with(|cell| cell.borrow_mut()[id as usize] = Some(value));
        return previous;
    }
    // The COMPILED install, emitted once while the eval context is built. It writes the
    // base configuration and deliberately leaves the override table alone, so an
    // `ini_set()` that ran before the program's first eval still wins afterwards.
    SCRIPT_CACHE_CONFIG.with(|cell| {
        let mut config = cell.borrow_mut();
        match id {
            DIRECTIVE_REVALIDATE_FREQ => config.revalidate_freq = value,
            DIRECTIVE_VALIDATE_TIMESTAMPS => config.validate_timestamps = value != 0,
            DIRECTIVE_FILE_UPDATE_PROTECTION => config.file_update_protection = value,
            _ => {}
        }
    });
    previous
}

/// Drops every `ini_set()` override, restoring the compiled configuration.
///
/// Exists for tests, which share one thread and would otherwise leak an override from one
/// case into the next.
#[cfg(test)]
pub(crate) fn clear_directive_overrides() {
    DIRECTIVE_OVERRIDES.with(|cell| *cell.borrow_mut() = [None; DIRECTIVE_COUNT]);
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the configuration defaults and the `max_file_size` admission rule.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The disabled default is load-bearing: it is what keeps a CLI binary and every
    //!   direct consumer of this archive on the pre-cache code path.

    use super::*;

    /// Verifies the no-codegen default leaves the cache off.
    #[test]
    fn default_configuration_disables_the_cache() {
        assert!(!ScriptCacheConfig::disabled().enabled);
    }

    /// Verifies `max_file_size = 0` admits any size, matching php-src's "no limit".
    #[test]
    fn zero_max_file_size_admits_every_file() {
        let config = ScriptCacheConfig::disabled();

        assert!(config.admits_size(0));
        assert!(config.admits_size(u64::MAX));
    }

    /// Verifies a non-zero `max_file_size` refuses only strictly larger files.
    #[test]
    fn max_file_size_admits_the_boundary_and_refuses_beyond_it() {
        let config = ScriptCacheConfig {
            max_file_size: 1024,
            ..ScriptCacheConfig::disabled()
        };

        assert!(config.admits_size(1023));
        assert!(config.admits_size(1024));
        assert!(!config.admits_size(1025));
    }

    /// Verifies the `file_update_protection` boundary, which is a STRICT `<` on the age.
    ///
    /// VERIFIED on reference PHP 8.5.10 with the default `2`: a file aged 0s and 1s is
    /// refused, and one aged exactly 2s is admitted. Getting this off by one would either
    /// cache a file php-src protects or refuse one it caches.
    #[test]
    fn file_update_protection_refuses_only_younger_files() {
        let config = ScriptCacheConfig {
            file_update_protection: 2,
            ..ScriptCacheConfig::disabled()
        };
        let now = 1_000_000;

        assert!(!config.admits_age(Some(now), now), "age 0 must be refused");
        assert!(!config.admits_age(Some(now - 1), now), "age 1 must be refused");
        assert!(config.admits_age(Some(now - 2), now), "age 2 must be admitted");
        assert!(config.admits_age(Some(now - 60), now));
    }

    /// Verifies `0` disables the guard, matching php-src treating it as "no protection".
    #[test]
    fn zero_file_update_protection_admits_every_age() {
        let config = ScriptCacheConfig::disabled();
        let now = 1_000_000;

        assert_eq!(config.file_update_protection, 2, "the DEFAULT is php-src's 2");
        let off = ScriptCacheConfig {
            file_update_protection: 0,
            ..ScriptCacheConfig::disabled()
        };
        assert!(off.admits_age(Some(now), now));
    }

    /// Verifies an unknown mtime is admitted rather than refused.
    ///
    /// php-src compares against a zero timestamp, which can never fall inside the
    /// protected window, so "no timestamp" admits there too.
    #[test]
    fn an_unknown_mtime_is_admitted() {
        assert!(ScriptCacheConfig::disabled().admits_age(None, 1_000_000));
    }

    /// Verifies the directive setter round-trips each id and answers the previous value.
    ///
    /// The returned previous value is what `ini_set()` reports, so it is part of the
    /// contract rather than a convenience.
    #[test]
    fn swap_directive_returns_the_previous_value() {
        set_config(ScriptCacheConfig::disabled());

        assert_eq!(swap_directive(DIRECTIVE_REVALIDATE_FREQ, 30, true), 2);
        assert_eq!(config().revalidate_freq, 30);
        assert_eq!(swap_directive(DIRECTIVE_VALIDATE_TIMESTAMPS, 0, true), 1);
        assert!(!config().validate_timestamps);
        assert_eq!(swap_directive(DIRECTIVE_FILE_UPDATE_PROTECTION, 9, true), 2);
        assert_eq!(config().file_update_protection, 9);

        clear_directive_overrides();
        set_config(ScriptCacheConfig::disabled());
    }

    /// Verifies an unknown id writes nothing and reports it, so a newer compiler's
    /// generated call fails soft against an older archive.
    #[test]
    fn an_unknown_directive_id_writes_nothing() {
        set_config(ScriptCacheConfig::disabled());

        assert_eq!(swap_directive(9_999, 1, true), DIRECTIVE_UNKNOWN);
        assert_eq!(config(), ScriptCacheConfig::disabled());
    }

    /// Verifies the thread-local setter round-trips and stays isolated per thread.
    #[test]
    fn configuration_round_trips_on_the_installing_thread() {
        let installed = ScriptCacheConfig {
            enabled: true,
            revalidate_freq: 7,
            ..ScriptCacheConfig::disabled()
        };
        set_config(installed.clone());

        assert_eq!(config(), installed);
        assert!(std::thread::spawn(|| !config().enabled)
            .join()
            .expect("probe thread should not panic"));
    }
}
