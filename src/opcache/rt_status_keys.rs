//! Purpose:
//! The key space the native `opcache_get_status()` body uses to pull one runtime
//! script-cache figure at a time across the eval-bridge boundary, and the field space
//! it uses to pull one cached script's numbers.
//!
//! Called from:
//! - `crate::opcache_prelude::build` (bakes the key constants into the generated PHP).
//! - `crates/elephc-magician` (shares this file verbatim via a `#[path]` include, so the
//!   two sides of the ABI cannot drift).
//!
//! Key details:
//! - This module is intentionally dependency-free (no `crate::` references): it is
//!   compiled into both the `elephc` and `elephc-magician` crates through a shared file
//!   include, so it must not name types from either crate.
//! - The values are an ABI. Renumbering one silently changes what a previously compiled
//!   binary asks for, so append rather than reorder.
//! - Every figure is an INTEGER on purpose. Booleans arrive as `0`/`1` and
//!   `opcache_hit_rate` is computed in PHP from `HITS` and `MISSES`, which keeps the
//!   bridge to one scalar shape and avoids a float ABI for one field.

/// Live cache hits: includes served from the runtime script cache.
pub const RT_STAT_HITS: i64 = 0;
/// Cache misses: includes that had to read and segment the file.
pub const RT_STAT_MISSES: i64 = 1;
/// Number of scripts currently held by the runtime cache.
pub const RT_STAT_SCRIPT_COUNT: i64 = 2;
/// Bytes the runtime cache's entries are charged against its budget.
pub const RT_STAT_USED_MEMORY: i64 = 3;
/// `1` when the budget or the entry ceiling has refused an entry, else `0`.
pub const RT_STAT_CACHE_FULL: i64 = 4;
/// Number of `opcache_reset()` calls that actually scheduled a restart.
pub const RT_STAT_MANUAL_RESTARTS: i64 = 5;
/// Unix timestamp of the last scheduled restart, or `0` if there has been none.
pub const RT_STAT_LAST_RESTART_TIME: i64 = 6;
/// `1` once a restart has been scheduled in this process, else `0`.
pub const RT_STAT_RESTART_PENDING: i64 = 7;
/// `opcache.blacklist_filename` refusals — scripts run but deliberately not cached.
pub const RT_STAT_BLACKLIST_MISSES: i64 = 8;
/// How many blacklist patterns are loaded, for the `opcache_get_configuration()` listing.
pub const RT_STAT_BLACKLIST_COUNT: i64 = 9;

/// One cached script's hit count.
pub const RT_SCRIPT_HITS: i64 = 0;
/// One cached script's accounted byte footprint.
pub const RT_SCRIPT_MEMORY: i64 = 1;
/// One cached script's last-used Unix timestamp.
pub const RT_SCRIPT_LAST_USED: i64 = 2;
/// One cached script's source mtime, or `0` when a forced invalidate discarded it.
pub const RT_SCRIPT_TIMESTAMP: i64 = 3;

/// Returns the figure for `key` from a status tuple, or `0` for an unknown key.
///
/// An unknown key answers `0` rather than panicking: the key travels across an ABI from
/// a binary that may have been compiled against a different revision of this file, and
/// `0` is the same answer an empty cache gives.
// `allow(dead_code)`: in `elephc` only the key CONSTANTS are used — the compiler bakes the
// numbers into the generated PHP and never answers them. The selector is live in the
// `elephc-magician` `#[path]` include, which is the side that reads the cache.
#[allow(dead_code)]
pub fn rt_stat_value(
    key: i64,
    hits: u64,
    misses: u64,
    script_count: usize,
    used_memory: usize,
    cache_full: bool,
    manual_restarts: u64,
    last_restart_time: i64,
    restart_pending: bool,
    blacklist_misses: u64,
    blacklist_count: usize,
) -> i64 {
    match key {
        RT_STAT_HITS => hits as i64,
        RT_STAT_MISSES => misses as i64,
        RT_STAT_SCRIPT_COUNT => script_count as i64,
        RT_STAT_USED_MEMORY => used_memory as i64,
        RT_STAT_CACHE_FULL => i64::from(cache_full),
        RT_STAT_MANUAL_RESTARTS => manual_restarts as i64,
        RT_STAT_LAST_RESTART_TIME => last_restart_time,
        RT_STAT_RESTART_PENDING => i64::from(restart_pending),
        RT_STAT_BLACKLIST_MISSES => blacklist_misses as i64,
        RT_STAT_BLACKLIST_COUNT => blacklist_count as i64,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Pins the key space as an ABI: the numbering, and the unknown-key answer.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness (both crates that include this file).
    //!
    //! Key details:
    //! - The numbering assertions look tautological but are the point: they fail loudly
    //!   if someone reorders the constants, which would silently repurpose the requests a
    //!   previously compiled binary makes.

    use super::*;

    /// Pins the status key numbering, which is an ABI a compiled binary bakes in.
    #[test]
    fn status_keys_keep_their_wire_numbers() {
        assert_eq!(
            [
                RT_STAT_HITS,
                RT_STAT_MISSES,
                RT_STAT_SCRIPT_COUNT,
                RT_STAT_USED_MEMORY,
                RT_STAT_CACHE_FULL,
                RT_STAT_MANUAL_RESTARTS,
                RT_STAT_LAST_RESTART_TIME,
                RT_STAT_RESTART_PENDING,
                RT_STAT_BLACKLIST_MISSES,
                RT_STAT_BLACKLIST_COUNT,
            ],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
        );
    }

    /// Pins the per-script field numbering, for the same reason.
    #[test]
    fn script_fields_keep_their_wire_numbers() {
        assert_eq!(
            [
                RT_SCRIPT_HITS,
                RT_SCRIPT_MEMORY,
                RT_SCRIPT_LAST_USED,
                RT_SCRIPT_TIMESTAMP,
            ],
            [0, 1, 2, 3]
        );
    }

    /// Verifies each key selects its own figure.
    #[test]
    fn every_key_selects_its_own_figure() {
        let value = |key| rt_stat_value(key, 11, 22, 33, 44, true, 55, 66, true, 77, 88);

        assert_eq!(value(RT_STAT_HITS), 11);
        assert_eq!(value(RT_STAT_MISSES), 22);
        assert_eq!(value(RT_STAT_SCRIPT_COUNT), 33);
        assert_eq!(value(RT_STAT_USED_MEMORY), 44);
        assert_eq!(value(RT_STAT_CACHE_FULL), 1);
        assert_eq!(value(RT_STAT_MANUAL_RESTARTS), 55);
        assert_eq!(value(RT_STAT_LAST_RESTART_TIME), 66);
        assert_eq!(value(RT_STAT_RESTART_PENDING), 1);
        assert_eq!(value(RT_STAT_BLACKLIST_MISSES), 77);
        assert_eq!(value(RT_STAT_BLACKLIST_COUNT), 88);
    }

    /// Verifies booleans arrive as `0` when false, not as an absent field.
    #[test]
    fn false_booleans_arrive_as_zero() {
        let value = |key| rt_stat_value(key, 0, 0, 0, 0, false, 0, 0, false, 0, 0);

        assert_eq!(value(RT_STAT_CACHE_FULL), 0);
        assert_eq!(value(RT_STAT_RESTART_PENDING), 0);
    }

    /// Verifies an unknown key answers `0` rather than panicking across the ABI.
    #[test]
    fn an_unknown_key_answers_zero() {
        assert_eq!(rt_stat_value(9999, 11, 22, 33, 44, true, 55, 66, true, 77, 88), 0);
    }
}
