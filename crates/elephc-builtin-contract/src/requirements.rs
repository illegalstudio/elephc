//! Purpose:
//! Defines fixed dependency-neutral bridge, runtime-capability, and native-link
//! requirements attached to shared builtin contracts.
//!
//! Called from:
//! - `crate::registry` while assembling the authoritative catalog view.
//!
//! Key details:
//! - Source-dependent requirements remain in AOT semantic resolvers.
//! - These records describe capability ownership without enabling or linking it.

use crate::{BuiltinId, BuiltinRequirement};

const BCMATH: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_bcmath")];
const CRYPTO: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_crypto")];
const PHAR: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_phar")];
const PCNTL: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_pcntl")];
const XML: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_xml")];
const TLS: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_tls")];
const ZLIB: &[BuiltinRequirement] = &[BuiltinRequirement::SystemLibrary("z")];
const MBSTRING: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_mbstring")];
const MBREGEX: &[BuiltinRequirement] = &[
    BuiltinRequirement::Bridge("elephc_mbstring"), BuiltinRequirement::RuntimeCapability("oniguruma"),
];
const ICONV_BRIDGE: &[BuiltinRequirement] = &[
    BuiltinRequirement::Bridge("elephc_iconv"),
    BuiltinRequirement::MacOsLibrary("iconv"),
];
const REGEX: &[BuiltinRequirement] = &[BuiltinRequirement::RuntimeCapability("pcre2")];

/// Returns fixed neutral requirements for one canonical shared contract ID.
pub(crate) fn fixed_requirements(id: BuiltinId) -> &'static [BuiltinRequirement] {
    if matches_name(id, &["mb_ereg_match", "mb_split", "mb_ereg_replace", "mb_eregi_replace",
        "mb_ereg_search_init", "mb_ereg_search", "mb_ereg_search_pos", "mb_ereg_search_regs", "mb_ereg_search_getpos", "mb_ereg_search_getregs", "mb_ereg_search_setpos"
    ]) { return MBREGEX; }
    if crate::catalog_xml::contract_names()
        .any(|name| id == BuiltinId::from_canonical_name(name))
    {
        return XML;
    }
    if matches_name(
        id,
        &[
            "pcntl_alarm",
            "pcntl_async_signals",
            "pcntl_daemon",
            "pcntl_exec",
            "pcntl_errno",
            "pcntl_fork",
            "pcntl_get_last_error",
            "pcntl_getcpu",
            "pcntl_getcpuaffinity",
            "pcntl_getpriority",
            "pcntl_getqos_class",
            "pcntl_setcpuaffinity",
            "pcntl_setns",
            "pcntl_setpriority",
            "pcntl_setqos_class",
            "pcntl_signal",
            "pcntl_signal_dispatch",
            "pcntl_signal_get_handler",
            "pcntl_sigprocmask",
            "pcntl_sigtimedwait",
            "pcntl_sigwaitinfo",
            "pcntl_strerror",
            "pcntl_unshare",
            "pcntl_wait",
            "pcntl_waitid",
            "pcntl_waitpid",
            "pcntl_wexitstatus",
            "pcntl_wifcontinued",
            "pcntl_wifexited",
            "pcntl_wifsignaled",
            "pcntl_wifstopped",
            "pcntl_wstopsig",
            "pcntl_wtermsig",
            "posix_setpgid",
            "posix_setsid",
        ],
    ) {
        return PCNTL;
    }
    if matches_name(
        id,
        &[
            "bcadd",
            "bcceil",
            "bccomp",
            "bcdiv",
            "bcdivmod",
            "bcfloor",
            "bcmod",
            "bcmul",
            "bcpow",
            "bcpowmod",
            "bcround",
            "bcscale",
            "bcsqrt",
            "bcsub",
        ],
    ) {
        return BCMATH;
    }
    if matches_name(
        id,
        &[
            "__elephc_hash_ctx_copy",
            "__elephc_hash_ctx_final",
            "__elephc_hash_ctx_init",
            "__elephc_hash_ctx_update",
            "hash",
            "hash_copy",
            "hash_file",
            "hash_final",
            "hash_hmac",
            "hash_init",
            "hash_update",
            "md5",
            "openssl_cipher_iv_length",
            "openssl_decrypt",
            "openssl_encrypt",
            "openssl_get_cipher_methods",
            "sha1",
        ],
    ) {
        return CRYPTO;
    }
    if matches_name(
        id,
        &[
            "__elephc_phar_bzip2_archive",
            "__elephc_phar_decompress_archive",
            "__elephc_phar_get_file_metadata",
            "__elephc_phar_get_metadata",
            "__elephc_phar_get_signature_hash",
            "__elephc_phar_get_signature_type",
            "__elephc_phar_get_stub",
            "__elephc_phar_gzip_archive",
            "__elephc_phar_list_entries",
            "__elephc_phar_set_compression",
            "__elephc_phar_set_file_metadata",
            "__elephc_phar_set_metadata",
            "__elephc_phar_set_stub",
            "__elephc_phar_set_zip_password",
            "__elephc_phar_sign_hash",
            "__elephc_phar_sign_openssl",
        ],
    ) {
        return PHAR;
    }
    if matches_name(id, &["stream_socket_enable_crypto"]) {
        return TLS;
    }
    if matches_name(
        id,
        &["gzcompress", "gzdeflate", "gzinflate", "gzuncompress"],
    ) {
        return ZLIB;
    }
    if matches_name(id, &["mb_regex_encoding", "mb_regex_set_options", "mb_strlen", "mb_strwidth", "mb_strtoupper", "mb_strtolower", "mb_convert_case", "mb_ucfirst", "mb_lcfirst", "mb_strimwidth",
        "mb_substr", "mb_strcut", "mb_scrub", "mb_decode_mimeheader", "mb_encode_mimeheader", "mb_get_info", "mb_http_input", "mb_trim", "mb_ltrim", "mb_rtrim", "mb_str_pad", "mb_convert_kana", "mb_substr_count", "mb_ord", "mb_chr", "mb_strpos", "mb_stripos", "mb_strrpos", "mb_strripos", "mb_strstr", "mb_stristr", "mb_strrchr", "mb_strrichr", "mb_language", "mb_internal_encoding", "mb_http_output", "mb_encoding_aliases", "mb_str_split", "mb_preferred_mime_name"]) {
        return MBSTRING;
    }
    if matches_name(
        id,
        &[
            "iconv",
            "iconv_get_encoding",
            "iconv_mime_decode",
            "iconv_mime_decode_headers",
            "iconv_mime_encode",
            "iconv_set_encoding",
            "iconv_strlen",
            "iconv_strpos",
            "iconv_strrpos",
            "iconv_substr",
        ],
    ) {
        return ICONV_BRIDGE;
    }
    if matches_name(
        id,
        &[
            "preg_match",
            "preg_match_all",
            "preg_replace",
            "preg_replace_callback",
            "preg_split",
        ],
    ) {
        return REGEX;
    }
    &[]
}

/// Tests one stable ID against a short canonical-name set.
fn matches_name(id: BuiltinId, names: &[&str]) -> bool {
    names
        .iter()
        .any(|name| id == BuiltinId::from_canonical_name(name))
}
