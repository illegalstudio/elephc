//! Purpose:
//! Declares every `SODIUM_*` constant exposed by php-src 8.5 with libsodium 1.0.22.
//!
//! Called from:
//! - `crate::sodium_prelude::declarations`.
//!
//! Key details:
//! - Values are the pinned libsodium 1.0.22 ABI values, so they match the PHP 8.5.10 oracle.
//! - `SODIUM_LIBRARY_VERSION` is the linked library version string; major/minor use
//!   libsodium's own `SODIUM_LIBRARY_VERSION_*` numbering (26/4 for 1.0.22).

use crate::parser::ast::Stmt;
use crate::synthetic_class::{e_int, e_str, s_const};

/// Builds every `SODIUM_*` constant declaration.
pub(super) fn declarations() -> Vec<Stmt> {
    vec![
        s_const("SODIUM_BASE64_VARIANT_ORIGINAL", e_int(1)),
        s_const("SODIUM_BASE64_VARIANT_ORIGINAL_NO_PADDING", e_int(3)),
        s_const("SODIUM_BASE64_VARIANT_URLSAFE", e_int(5)),
        s_const("SODIUM_BASE64_VARIANT_URLSAFE_NO_PADDING", e_int(7)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS128L_ABYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS128L_KEYBYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS128L_NPUBBYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS128L_NSECBYTES", e_int(0)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS256_ABYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS256_NPUBBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_AEGIS256_NSECBYTES", e_int(0)),
        s_const("SODIUM_CRYPTO_AEAD_AES256GCM_ABYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_AEAD_AES256GCM_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_AES256GCM_NPUBBYTES", e_int(12)),
        s_const("SODIUM_CRYPTO_AEAD_AES256GCM_NSECBYTES", e_int(0)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_ABYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_ABYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NPUBBYTES", e_int(12)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_IETF_NSECBYTES", e_int(0)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NPUBBYTES", e_int(8)),
        s_const("SODIUM_CRYPTO_AEAD_CHACHA20POLY1305_NSECBYTES", e_int(0)),
        s_const("SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_ABYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NPUBBYTES", e_int(24)),
        s_const("SODIUM_CRYPTO_AEAD_XCHACHA20POLY1305_IETF_NSECBYTES", e_int(0)),
        s_const("SODIUM_CRYPTO_AUTH_BYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_AUTH_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_BOX_KEYPAIRBYTES", e_int(64)),
        s_const("SODIUM_CRYPTO_BOX_MACBYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_BOX_NONCEBYTES", e_int(24)),
        s_const("SODIUM_CRYPTO_BOX_PUBLICKEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_BOX_SEALBYTES", e_int(48)),
        s_const("SODIUM_CRYPTO_BOX_SECRETKEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_BOX_SEEDBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_CORE_RISTRETTO255_BYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_CORE_RISTRETTO255_HASHBYTES", e_int(64)),
        s_const("SODIUM_CRYPTO_CORE_RISTRETTO255_NONREDUCEDSCALARBYTES", e_int(64)),
        s_const("SODIUM_CRYPTO_CORE_RISTRETTO255_SCALARBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_GENERICHASH_BYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_GENERICHASH_BYTES_MAX", e_int(64)),
        s_const("SODIUM_CRYPTO_GENERICHASH_BYTES_MIN", e_int(16)),
        s_const("SODIUM_CRYPTO_GENERICHASH_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MAX", e_int(64)),
        s_const("SODIUM_CRYPTO_GENERICHASH_KEYBYTES_MIN", e_int(16)),
        s_const("SODIUM_CRYPTO_KDF_BYTES_MAX", e_int(64)),
        s_const("SODIUM_CRYPTO_KDF_BYTES_MIN", e_int(16)),
        s_const("SODIUM_CRYPTO_KDF_CONTEXTBYTES", e_int(8)),
        s_const("SODIUM_CRYPTO_KDF_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_KX_KEYPAIRBYTES", e_int(64)),
        s_const("SODIUM_CRYPTO_KX_PUBLICKEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_KX_SECRETKEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_KX_SEEDBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_KX_SESSIONKEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_PWHASH_ALG_ARGON2I13", e_int(1)),
        s_const("SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13", e_int(2)),
        s_const("SODIUM_CRYPTO_PWHASH_ALG_DEFAULT", e_int(2)),
        s_const("SODIUM_CRYPTO_PWHASH_MEMLIMIT_INTERACTIVE", e_int(67108864)),
        s_const("SODIUM_CRYPTO_PWHASH_MEMLIMIT_MODERATE", e_int(268435456)),
        s_const("SODIUM_CRYPTO_PWHASH_MEMLIMIT_SENSITIVE", e_int(1073741824)),
        s_const("SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE", e_int(2)),
        s_const("SODIUM_CRYPTO_PWHASH_OPSLIMIT_MODERATE", e_int(3)),
        s_const("SODIUM_CRYPTO_PWHASH_OPSLIMIT_SENSITIVE", e_int(4)),
        s_const("SODIUM_CRYPTO_PWHASH_SALTBYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_INTERACTIVE", e_int(16777216)),
        s_const("SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_MEMLIMIT_SENSITIVE", e_int(1073741824)),
        s_const("SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_INTERACTIVE", e_int(524288)),
        s_const("SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_SENSITIVE", e_int(33554432)),
        s_const("SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_STRPREFIX", e_str("$7$")),
        s_const("SODIUM_CRYPTO_PWHASH_STRPREFIX", e_str("$argon2id$")),
        s_const("SODIUM_CRYPTO_SCALARMULT_BYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_BYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_SCALARMULT_RISTRETTO255_SCALARBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_SCALARMULT_SCALARBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_SECRETBOX_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_SECRETBOX_MACBYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_SECRETBOX_NONCEBYTES", e_int(24)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_ABYTES", e_int(17)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_HEADERBYTES", e_int(24)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_MESSAGEBYTES_MAX", e_int(274877906816)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_FINAL", e_int(3)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_MESSAGE", e_int(0)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_PUSH", e_int(1)),
        s_const("SODIUM_CRYPTO_SECRETSTREAM_XCHACHA20POLY1305_TAG_REKEY", e_int(2)),
        s_const("SODIUM_CRYPTO_SHORTHASH_BYTES", e_int(8)),
        s_const("SODIUM_CRYPTO_SHORTHASH_KEYBYTES", e_int(16)),
        s_const("SODIUM_CRYPTO_SIGN_BYTES", e_int(64)),
        s_const("SODIUM_CRYPTO_SIGN_KEYPAIRBYTES", e_int(96)),
        s_const("SODIUM_CRYPTO_SIGN_PUBLICKEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_SIGN_SECRETKEYBYTES", e_int(64)),
        s_const("SODIUM_CRYPTO_SIGN_SEEDBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_STREAM_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_STREAM_NONCEBYTES", e_int(24)),
        s_const("SODIUM_CRYPTO_STREAM_XCHACHA20_KEYBYTES", e_int(32)),
        s_const("SODIUM_CRYPTO_STREAM_XCHACHA20_NONCEBYTES", e_int(24)),
        s_const("SODIUM_LIBRARY_MAJOR_VERSION", e_int(26)),
        s_const("SODIUM_LIBRARY_MINOR_VERSION", e_int(4)),
        s_const("SODIUM_LIBRARY_VERSION", e_str("1.0.22")),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the list covers the oracle surface including AEGIS and the version triple.
    #[test]
    fn exposes_the_php_85_surface() {
        let declarations = declarations();
        assert_eq!(declarations.len(), 102);
        let names: Vec<&str> = declarations.iter().filter_map(|statement| {
            match &statement.kind { crate::parser::ast::StmtKind::ConstDecl { name, .. } => Some(name.as_str()), _ => None }
        }).collect();
        for expected in ["SODIUM_LIBRARY_VERSION", "SODIUM_CRYPTO_PWHASH_OPSLIMIT_INTERACTIVE", "SODIUM_CRYPTO_AEAD_AEGIS256_KEYBYTES"] {
            assert!(names.contains(&expected), "missing {expected}");
        }
    }
}
