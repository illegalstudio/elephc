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

pub(crate) const STREAM_INT_CONSTANTS: &[(&str, i64)] = &[
    // Client / server connection flags.
    ("STREAM_CLIENT_PERSISTENT", 1),
    ("STREAM_CLIENT_ASYNC_CONNECT", 2),
    ("STREAM_CLIENT_CONNECT", 4),
    ("STREAM_SERVER_BIND", 4),
    ("STREAM_SERVER_LISTEN", 8),
    // Shutdown directions for stream_socket_shutdown().
    ("STREAM_SHUT_RD", 0),
    ("STREAM_SHUT_WR", 1),
    ("STREAM_SHUT_RDWR", 2),
    // Out-of-band / peek flags for stream_socket_recvfrom().
    ("STREAM_OOB", 1),
    ("STREAM_PEEK", 2),
    // Stream filter chain direction.
    ("STREAM_FILTER_READ", 1),
    ("STREAM_FILTER_WRITE", 2),
    ("STREAM_FILTER_ALL", 3),
    // TLS crypto methods (client side).
    ("STREAM_CRYPTO_METHOD_SSLv2_CLIENT", 3),
    ("STREAM_CRYPTO_METHOD_SSLv3_CLIENT", 5),
    ("STREAM_CRYPTO_METHOD_SSLv23_CLIENT", 57),
    ("STREAM_CRYPTO_METHOD_TLS_CLIENT", 121),
    ("STREAM_CRYPTO_METHOD_TLSv1_0_CLIENT", 9),
    ("STREAM_CRYPTO_METHOD_TLSv1_1_CLIENT", 17),
    ("STREAM_CRYPTO_METHOD_TLSv1_2_CLIENT", 33),
    ("STREAM_CRYPTO_METHOD_TLSv1_3_CLIENT", 65),
    ("STREAM_CRYPTO_METHOD_ANY_CLIENT", 127),
    // TLS crypto methods (server side).
    ("STREAM_CRYPTO_METHOD_SSLv2_SERVER", 2),
    ("STREAM_CRYPTO_METHOD_SSLv3_SERVER", 4),
    ("STREAM_CRYPTO_METHOD_SSLv23_SERVER", 120),
    ("STREAM_CRYPTO_METHOD_TLS_SERVER", 120),
    ("STREAM_CRYPTO_METHOD_TLSv1_0_SERVER", 8),
    ("STREAM_CRYPTO_METHOD_TLSv1_1_SERVER", 16),
    ("STREAM_CRYPTO_METHOD_TLSv1_2_SERVER", 32),
    ("STREAM_CRYPTO_METHOD_TLSv1_3_SERVER", 64),
    ("STREAM_CRYPTO_METHOD_ANY_SERVER", 126),
    // TLS crypto protocol aliases.
    ("STREAM_CRYPTO_PROTO_SSLv3", 4),
    ("STREAM_CRYPTO_PROTO_TLSv1_0", 8),
    ("STREAM_CRYPTO_PROTO_TLSv1_1", 16),
    ("STREAM_CRYPTO_PROTO_TLSv1_2", 32),
    ("STREAM_CRYPTO_PROTO_TLSv1_3", 64),
    // Socket-pair domain / type / protocol (target-invariant values only;
    // STREAM_PF_INET6 is target-divergent and registered with the socket layer).
    ("STREAM_PF_INET", 2),
    ("STREAM_PF_UNIX", 1),
    ("STREAM_SOCK_STREAM", 1),
    ("STREAM_SOCK_DGRAM", 2),
    ("STREAM_SOCK_RAW", 3),
    ("STREAM_SOCK_RDM", 4),
    ("STREAM_SOCK_SEQPACKET", 5),
    ("STREAM_IPPROTO_IP", 0),
    ("STREAM_IPPROTO_TCP", 6),
    ("STREAM_IPPROTO_UDP", 17),
    ("STREAM_IPPROTO_ICMP", 1),
    ("STREAM_IPPROTO_RAW", 255),
    // Notification codes / severities for stream context notifiers.
    ("STREAM_NOTIFY_RESOLVE", 1),
    ("STREAM_NOTIFY_CONNECT", 2),
    ("STREAM_NOTIFY_AUTH_REQUIRED", 3),
    ("STREAM_NOTIFY_MIME_TYPE_IS", 4),
    ("STREAM_NOTIFY_FILE_SIZE_IS", 5),
    ("STREAM_NOTIFY_REDIRECTED", 6),
    ("STREAM_NOTIFY_PROGRESS", 7),
    ("STREAM_NOTIFY_COMPLETED", 8),
    ("STREAM_NOTIFY_FAILURE", 9),
    ("STREAM_NOTIFY_AUTH_RESULT", 10),
    ("STREAM_NOTIFY_SEVERITY_INFO", 0),
    ("STREAM_NOTIFY_SEVERITY_WARN", 1),
    ("STREAM_NOTIFY_SEVERITY_ERR", 2),
    // Wrapper / cast / meta / option / buffer / URL-stat flags.
    ("STREAM_IS_URL", 1),
    ("STREAM_USE_PATH", 1),
    ("STREAM_REPORT_ERRORS", 8),
    ("STREAM_CAST_FOR_SELECT", 3),
    ("STREAM_CAST_AS_STREAM", 0),
    ("STREAM_META_TOUCH", 1),
    ("STREAM_META_OWNER_NAME", 2),
    ("STREAM_META_OWNER", 3),
    ("STREAM_META_GROUP_NAME", 4),
    ("STREAM_META_GROUP", 5),
    ("STREAM_META_ACCESS", 6),
    ("STREAM_MKDIR_RECURSIVE", 1),
    ("STREAM_OPTION_BLOCKING", 1),
    ("STREAM_OPTION_READ_BUFFER", 2),
    ("STREAM_OPTION_WRITE_BUFFER", 3),
    ("STREAM_OPTION_READ_TIMEOUT", 4),
    ("STREAM_BUFFER_NONE", 0),
    ("STREAM_BUFFER_LINE", 1),
    ("STREAM_BUFFER_FULL", 2),
    ("STREAM_URL_STAT_LINK", 1),
    ("STREAM_URL_STAT_QUIET", 2),
    ("STREAM_MUST_SEEK", 16),
    ("STREAM_IGNORE_URL", 2),
    // User stream-filter return values / flags.
    ("PSFS_ERR_FATAL", 0),
    ("PSFS_FEED_ME", 1),
    ("PSFS_PASS_ON", 2),
    ("PSFS_FLAG_NORMAL", 0),
    ("PSFS_FLAG_FLUSH_INC", 1),
    ("PSFS_FLAG_FLUSH_CLOSE", 2),
    // file() / file_put_contents() flags.
    ("FILE_USE_INCLUDE_PATH", 1),
    ("FILE_IGNORE_NEW_LINES", 2),
    ("FILE_SKIP_EMPTY_LINES", 4),
    ("FILE_APPEND", 8),
    ("FILE_NO_DEFAULT_CONTEXT", 16),
    // glob() flags (POSIX-portable values).
    ("GLOB_ERR", 4),
    ("GLOB_MARK", 8),
    ("GLOB_NOCHECK", 16),
    ("GLOB_NOSORT", 32),
    ("GLOB_BRACE", 128),
    ("GLOB_NOESCAPE", 4096),
    ("GLOB_ONLYDIR", 1073741824),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_filter_all_is_three() {
        let entry = STREAM_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "STREAM_FILTER_ALL")
            .expect("STREAM_FILTER_ALL defined");
        assert_eq!(entry.1, 3);
    }

    #[test]
    fn stream_client_connect_is_four() {
        let entry = STREAM_INT_CONSTANTS
            .iter()
            .find(|(name, _)| *name == "STREAM_CLIENT_CONNECT")
            .expect("STREAM_CLIENT_CONNECT defined");
        assert_eq!(entry.1, 4);
    }

    #[test]
    fn no_duplicate_constant_names() {
        let mut names: Vec<&str> = STREAM_INT_CONSTANTS.iter().map(|(n, _)| *n).collect();
        names.sort_unstable();
        let len_before = names.len();
        names.dedup();
        assert_eq!(names.len(), len_before, "duplicate stream constant name");
    }

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
