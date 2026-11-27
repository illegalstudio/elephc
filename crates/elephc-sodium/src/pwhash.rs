//! Purpose:
//! Argon2id/Argon2i password hashing for PHP's `sodium_crypto_pwhash*` surface.
//!
//! Called from:
//! - `crate::` C ABI wrappers.
//!
//! Key details:
//! - libsodium's `crypto_pwhash` maps `memlimit` to Argon2's KiB `m_cost`,
//!   `opslimit` to `t_cost`, and fixes parallelism at 1.
//! - `*_str` emits the PHC `$argon2id$v=19$m=...,t=...,p=1$...$...` encoding and
//!   verification accepts Argon2i/Argon2id PHC strings via the RustCrypto
//!   password-hash parser.
//! - Range checks live in the PHP wrapper so PHP 8.6's `ValueError` text is
//!   exact; this module returns `None` for unsupported parameters.

use argon2::password_hash::phc::PasswordHash;
use argon2::{
    password_hash::{PasswordHasher, PasswordVerifier},
    Algorithm, Argon2, Params, Version,
};

/// `SODIUM_CRYPTO_PWHASH_ALG_ARGON2I13`.
pub(crate) const ALG_ARGON2I13: i64 = 1;
/// `SODIUM_CRYPTO_PWHASH_ALG_ARGON2ID13`.
pub(crate) const ALG_ARGON2ID13: i64 = 2;
/// `SODIUM_CRYPTO_PWHASH_SALTBYTES`.
pub(crate) const SALTBYTES: usize = 16;
/// `SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_SALTBYTES`.
pub(crate) const SCRYPT_SALTBYTES: usize = 32;
/// `SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_BYTES_MIN`.
pub(crate) const SCRYPT_BYTES_MIN: usize = 16;
/// `SODIUM_CRYPTO_PWHASH_SCRYPTSALSA208SHA256_OPSLIMIT_MIN`.
pub(crate) const SCRYPT_OPSLIMIT_MIN: i64 = 32768;

/// Hashes `password` with Argon2i/Argon2id and the libsodium parameter mapping.
pub(crate) fn hash(
    output_len: usize,
    password: &[u8],
    salt: &[u8],
    opslimit: u64,
    memlimit: u64,
    algorithm: i64,
) -> Option<Vec<u8>> {
    if output_len < 16 || salt.len() != SALTBYTES || opslimit < 1 || memlimit < 8192 {
        return None;
    }
    let algorithm = match algorithm {
        ALG_ARGON2I13 => Algorithm::Argon2i,
        ALG_ARGON2ID13 => Algorithm::Argon2id,
        _ => return None,
    };
    let memory_kib = u32::try_from(memlimit / 1024).ok()?;
    let time = u32::try_from(opslimit).ok()?;
    let params = Params::new(memory_kib, time, 1, Some(output_len)).ok()?;
    let argon2 = Argon2::new(algorithm, Version::V0x13, params);
    let mut output = vec![0u8; output_len];
    argon2.hash_password_into(password, salt, &mut output).ok()?;
    Some(output)
}

/// Chooses libsodium's scrypt `(log_n, r, p)` for the requested limits.
fn scrypt_params(opslimit: u64, memlimit: u64) -> Option<scrypt::Params> {
    let opslimit = opslimit.max(SCRYPT_OPSLIMIT_MIN as u64);
    let r: u64 = 8;
    let (log_n, p) = if opslimit < memlimit / 32 {
        let max_n = opslimit / (r * 4);
        let mut log_n = 1u32;
        while log_n < 63 {
            if (1u64 << log_n) > max_n / 2 {
                break;
            }
            log_n += 1;
        }
        (log_n, 1)
    } else {
        let max_n = memlimit / (r as usize * 128) as u64;
        let mut log_n = 1u32;
        while log_n < 63 {
            if (1u64 << log_n) > max_n / 2 {
                break;
            }
            log_n += 1;
        }
        let max_rp = ((opslimit / 4) / (1u64 << log_n)).min(0x3fff_ffff);
        (log_n, max_rp / r)
    };
    if p == 0 {
        return None;
    }
    scrypt::Params::new(u8::try_from(log_n).ok()?, u32::try_from(r).ok()?, u32::try_from(p).ok()?).ok()
}

/// Raw scrypt-salsa20/poly1305 derivation using libsodium's parameter chooser.
pub(crate) fn scrypt_hash(
    output_len: usize,
    password: &[u8],
    salt: &[u8],
    opslimit: u64,
    memlimit: u64,
) -> Option<Vec<u8>> {
    if output_len < SCRYPT_BYTES_MIN || salt.len() != SCRYPT_SALTBYTES {
        return None;
    }
    let params = scrypt_params(opslimit, memlimit)?;
    let mut output = vec![0u8; output_len];
    scrypt::scrypt(password, salt, &params, &mut output).ok()?;
    Some(output)
}

/// Builds a PHC Argon2i/Argon2id string for a 32-byte hash.
pub(crate) fn hash_str(
    password: &[u8],
    salt: &[u8],
    opslimit: u64,
    memlimit: u64,
    algorithm: i64,
) -> Option<String> {
    if salt.len() != SALTBYTES || opslimit < 1 || memlimit < 8192 {
        return None;
    }
    let algorithm = match algorithm {
        ALG_ARGON2I13 => Algorithm::Argon2i,
        ALG_ARGON2ID13 => Algorithm::Argon2id,
        _ => return None,
    };
    let memory_kib = u32::try_from(memlimit / 1024).ok()?;
    let time = u32::try_from(opslimit).ok()?;
    let params = Params::new(memory_kib, time, 1, Some(32)).ok()?;
    let argon2 = Argon2::new(algorithm, Version::V0x13, params);
    let hash = argon2.hash_password_with_salt(password, salt).ok()?;
    Some(hash.to_string())
}

/// Verifies a PHC Argon2 string.
pub(crate) fn verify_str(password: &[u8], hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    let algorithm = match parsed.algorithm.as_str() {
        "argon2i" => Algorithm::Argon2i,
        "argon2id" => Algorithm::Argon2id,
        _ => return false,
    };
    let Ok(params) = Params::try_from(&parsed.params) else {
        return false;
    };
    Argon2::new(algorithm, Version::V0x13, params)
        .verify_password(password, &parsed)
        .is_ok()
}

/// Builds a `$7$` scrypt string using libsodium's parameter chooser.
pub(crate) fn scrypt_hash_str(
    password: &[u8],
    salt: &[u8],
    opslimit: u64,
    memlimit: u64,
) -> Option<String> {
    if salt.len() != SCRYPT_SALTBYTES {
        return None;
    }
    let params = scrypt_params(opslimit, memlimit)?;
    let hasher = scrypt::Scrypt::new_with_params(params);
    let hash: scrypt::mcf::PasswordHash = hasher.hash_password_with_salt(password, salt).ok()?;
    Some(hash.to_string())
}

/// Verifies a `$7$` scrypt string.
pub(crate) fn scrypt_verify_str(password: &[u8], hash: &str) -> bool {
    let Ok(parsed) = scrypt::mcf::PasswordHashRef::new(hash) else {
        return false;
    };
    scrypt::Scrypt::new()
        .verify_password(password, parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Decodes a lowercase hex string into bytes.
    fn hex(value: &str) -> Vec<u8> {
        let mut out = Vec::with_capacity(value.len() / 2);
        let digits = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            _ => panic!("invalid hex"),
        };
        for pair in value.as_bytes().chunks_exact(2) {
            out.push(digits(pair[0]) << 4 | digits(pair[1]));
        }
        out
    }

    /// Raw scrypt matches PHP/libsodium for a fixed salt.
    #[test]
    fn scrypt_matches_php_vector() {
        let salt = [b'S'; SCRYPT_SALTBYTES];
        let output = scrypt_hash(32, b"password", &salt, 524_288, 16_777_216).unwrap();
        assert_eq!(
            output,
            hex("7dda42b29bdce385ff353534b97cead6504a5f9fc9216d30d6e4635867e07e2c")
        );
    }

    /// `$7$` scrypt strings round-trip through the MCF parser.
    #[test]
    fn scrypt_str_round_trips() {
        let salt = [b'S'; SCRYPT_SALTBYTES];
        let hash = scrypt_hash_str(b"password", &salt, 524_288, 16_777_216).unwrap();
        assert!(hash.starts_with("$7$"));
        assert!(scrypt_verify_str(b"password", &hash));
        assert!(!scrypt_verify_str(b"wrong", &hash));
    }

    /// Argon2i PHC strings round-trip through verification.
    #[test]
    fn argon2i_str_round_trips() {
        let salt = [3u8; SALTBYTES];
        let hash = hash_str(b"password", &salt, 2, 8192, ALG_ARGON2I13).unwrap();
        assert!(hash.starts_with("$argon2i$"));
        assert!(verify_str(b"password", &hash));
        assert!(!verify_str(b"wrong", &hash));
    }

    /// libsodium's first Argon2id raw-hash vector.
    #[test]
    fn argon2id_matches_libsodium_vector() {
        let password = hex(concat!(
            "a347ae92bce9f80f6f595a4480fc9c2fe7e7d7148d371e9487d75f5c23008ffae0",
            "65577a928febd9b1973a5a95073acdbeb6a030cfc0d79caa2dc5cd011cef02c08d",
            "a232d76d52dfbca38ca8dcbd665b17d1665f7cf5fe59772ec909733b24de97d6f5",
            "8d220b20c60d7c07ec1fd93c52c31020300c6c1facd77937a597c7a6"
        ));
        let salt = hex("5541fbc995d5c197ba290346d2c559de");
        let output = hash(155, &password, &salt, 5, 7_256_678, ALG_ARGON2ID13).unwrap();
        let expected = hex(concat!(
            "18acec5d6507739f203d1f5d9f1d862f7c2cdac4f19d2bdff64487e60d969e3c",
            "ed615337b9eec6ac4461c6ca07f0939741e57c24d0005c7ea171a0ee1e734824",
            "9d135b38f222e4dad7b9a033ed83f5ca27277393e316582033c74affe2566a2b",
            "ea47f91f0fd9fe49ece7e1f79f3ad6e9b23e0277c8ecc4b313225748dd2a80f5",
            "679534a0700e246a79a49b3f74eb89ec6205fe1eeb941c73b1fcf1"
        ));
        assert_eq!(output, expected);
    }
}
