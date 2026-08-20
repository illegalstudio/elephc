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
const TLS: &[BuiltinRequirement] = &[BuiltinRequirement::Bridge("elephc_tls")];
const ZLIB: &[BuiltinRequirement] = &[BuiltinRequirement::SystemLibrary("z")];
const ICONV_MACOS: &[BuiltinRequirement] = &[BuiltinRequirement::MacOsLibrary("iconv")];
const ICONV_BRIDGE: &[BuiltinRequirement] = &[
    BuiltinRequirement::Bridge("elephc_iconv"),
    BuiltinRequirement::MacOsLibrary("iconv"),
];
const REGEX: &[BuiltinRequirement] = &[BuiltinRequirement::RuntimeCapability("pcre2")];

/// Returns fixed neutral requirements for one canonical shared contract ID.
pub(crate) fn fixed_requirements(id: BuiltinId) -> &'static [BuiltinRequirement] {
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
    if matches_name(id, &["mb_strlen"]) {
        return ICONV_MACOS;
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
