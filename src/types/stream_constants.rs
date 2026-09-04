//! Purpose:
//! Defines PHP stream / stream-adjacent constants exposed as integer constants.
//! Single source of truth for `STREAM_*`, `PSFS_*`, `FILE_*`, and `GLOB_*` values.
//!
//! Called from:
//! - `crate::types::checker::driver::init` when registering predefined constants.
//! - `crate::codegen::prescan` when materializing constant literal values.
//! - `crate::name_resolver::names` when recognizing builtin constant names.
//!
//! Key details:
//! - Values must match PHP 8.x exactly (`php -r 'echo CONST;'`) for parity.
//! - Only target-INVARIANT constants live here. `LOCK_*` and `FNM_*` are
//!   registered elsewhere (and `FNM_*` is target-sensitive). `STREAM_PF_INET6`
//!   is target-divergent (AF_INET6: 30 on macOS, 10 on Linux) and is registered
//!   target-sensitively when the socket layer lands.
//! - The configured wrapper/transport/filter slices are shared by lowering and
//!   Gate 0 compliance export so advertised capabilities cannot drift silently.

/// The wrappers `stream_get_wrappers()` advertises, in php-src's own
/// registration order (measured on php 8.5.6).
///
/// `zip` is last, exactly where php-src registers it. It is advertised only
/// because `fopen("zip://archive.zip#entry")` and `file_get_contents()` now
/// really read the entry through the elephc-phar bridge — advertising a scheme
/// `fopen()` cannot open would move the lie rather than remove it.
pub(crate) const STREAM_WRAPPERS: &[&str] = &[
    "https",
    "ftps",
    "compress.zlib",
    "compress.bzip2",
    "php",
    "file",
    "glob",
    "data",
    "http",
    "ftp",
    "phar",
    "zip",
];

/// The transports `stream_get_transports()` advertises, in php-src's own order.
///
/// `sslv2` and `sslv3` are deliberately absent: PHP 8.5.6 does not list them, the
/// protocols are dead, and advertising a transport tells a caller it may open one.
pub(crate) const STREAM_TRANSPORTS: &[&str] = &[
    "tcp",
    "udp",
    "unix",
    "udg",
    "ssl",
    "tls",
    "tlsv1.0",
    "tlsv1.1",
    "tlsv1.2",
    "tlsv1.3",
];

/// The filters `stream_get_filters()` advertises, in php-src's own order
/// (measured on php 8.5.6).
///
/// php publishes filter FAMILIES, not concrete names: `zlib.*` stands for
/// `zlib.deflate`/`zlib.inflate`, `bzip2.*` for the bzip2 pair, and `convert.*`
/// for the base64 and quoted-printable pairs. `string.strip_tags` is absent
/// because php removed that filter in 8.0.
pub(crate) const STREAM_FILTERS: &[&str] = &[
    "zlib.*",
    "bzip2.*",
    "convert.iconv.*",
    "string.rot13",
    "string.toupper",
    "string.tolower",
    "convert.*",
    "consumed",
    "dechunk",
];

// `STREAM_SERVER_BIND` is declared beside these but named only in prose here, so it is not
// re-exported: the compiler reads it through the constant table like any other php constant.
pub(crate) use elephc_builtin_contract::php_constants::{
    STREAM_SERVER_DEFAULT_FLAGS, STREAM_SERVER_LISTEN,
};

pub(crate) use elephc_builtin_contract::php_constants::STREAM_INT_CONSTANTS;

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the stream constant invariant for stream filter all is three.
    #[test]
    fn stream_filter_all_is_three() {
        let entry = STREAM_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "STREAM_FILTER_ALL")
            .expect("STREAM_FILTER_ALL defined");
        assert_eq!(entry.1, 3);
    }

    /// Verifies the three `STREAM_CLIENT_*` bits carry php-src's values.
    ///
    /// This test used to read `stream_client_connect_is_one` and assert `1`,
    /// pinning a permutation: php orders the bits PERSISTENT, ASYNC_CONNECT,
    /// CONNECT, so `STREAM_CLIENT_CONNECT` is 4. Measured on php 8.5.6 —
    /// `var_dump(STREAM_CLIENT_CONNECT, STREAM_CLIENT_PERSISTENT,
    /// STREAM_CLIENT_ASYNC_CONNECT)` = `int(4) int(1) int(2)` — and confirmed
    /// by the frozen oracle in `tests/php_oracle/manifests/streams`.
    #[test]
    fn stream_client_flags_match_php_src_order() {
        for (name, expected) in [
            ("STREAM_CLIENT_PERSISTENT", 1),
            ("STREAM_CLIENT_ASYNC_CONNECT", 2),
            ("STREAM_CLIENT_CONNECT", 4),
        ] {
            let entry = STREAM_INT_CONSTANTS
                .iter()
                .find(|(entry_name, _)| *entry_name == name)
                .unwrap_or_else(|| panic!("{name} defined"));
            assert_eq!(entry.1, expected, "{name}");
        }
    }

    /// Verifies php-src's internal-only names stay out of the PHP constant table.
    ///
    /// `php -n -r 'var_dump(defined("STREAM_FROM_START"));'` and the other four
    /// all report `false`; declaring them let a program name a constant php does
    /// not have and still compile.
    #[test]
    fn does_not_declare_php_src_internal_constants() {
        for name in [
            "STREAM_FROM_START",
            "STREAM_FROM_CUR",
            "STREAM_FROM_END",
            "STREAM_META_MODIFIED",
            "STREAM_OPTION_CHUNK_SIZE",
        ] {
            assert!(
                !STREAM_INT_CONSTANTS.iter().any(|(entry, _)| *entry == name),
                "{name} is not a PHP constant",
            );
        }
    }

    /// Verifies the published surfaces stay in php-src's own order.
    ///
    /// Measured on php 8.5.6 with `var_export(stream_get_wrappers())` and
    /// `var_export(stream_get_filters())`; php's twelfth and last wrapper is
    /// `zip`, which elephc now reads through the elephc-phar bridge.
    #[test]
    fn published_surfaces_follow_php_order() {
        assert_eq!(
            STREAM_WRAPPERS,
            [
                "https",
                "ftps",
                "compress.zlib",
                "compress.bzip2",
                "php",
                "file",
                "glob",
                "data",
                "http",
                "ftp",
                "phar",
                "zip",
            ]
        );
        assert_eq!(
            STREAM_FILTERS,
            [
                "zlib.*",
                "bzip2.*",
                "convert.iconv.*",
                "string.rot13",
                "string.toupper",
                "string.tolower",
                "convert.*",
                "consumed",
                "dechunk",
            ]
        );
    }

    /// Verifies the stream constant invariant for no duplicate constant names.
    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = STREAM_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate stream constant name");
    }

    /// Verifies the stream constant invariant for does not redeclare lock or fnmatch constants.
    #[test]
    fn does_not_redeclare_lock_or_fnmatch_constants() {
        // LOCK_* and FNM_* are registered elsewhere — keep them out of this table.
        for (name, _) in STREAM_INT_CONSTANTS {
            assert!(
                !name.starts_with("LOCK_") && !name.starts_with("FNM_"),
                "{name} must not be registered here",
            );
        }
    }
}
