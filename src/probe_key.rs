//! Purpose:
//! Compile-time build-key material for `--probe`: generate the 32-byte key that
//! embeds in the binary and lands in the `.probe-key` sidecar, so the probe
//! endpoint can prove the binary's identity through the shared HMAC handshake.
//!
//! Called from:
//! - `crate::codegen` embeds the key bytes as a data symbol.
//! - `crate::pipeline::backend` writes the sidecar next to the binary.
//!
//! Key details:
//! - A random key per build IS the revocation model: rebuild to rotate.
//! - `ELEPHC_PROBE_KEY` (64 hex chars) overrides generation, for reproducible
//!   builds and deterministic tests.
//! - Possession of the key is the credential; it is never transmitted (the
//!   handshake sends only nonces and HMAC tags). See `elephc_probe::handshake`.

use elephc_probe::handshake::KEY_LEN;

/// Returns the build key for this compilation: the `ELEPHC_PROBE_KEY` hex
/// override when set and valid, otherwise 32 fresh bytes from the OS RNG.
pub(crate) fn build_key() -> [u8; KEY_LEN] {
    if let Ok(hex) = std::env::var("ELEPHC_PROBE_KEY") {
        if let Some(key) = parse_hex_key(hex.trim()) {
            return key;
        }
        eprintln!(
            "warning: ELEPHC_PROBE_KEY is not {} hex characters; generating a random key",
            KEY_LEN * 2
        );
    }
    random_key()
}

/// The public build fingerprint printed at compile and by `--probe-host`.
pub(crate) fn fingerprint(key: &[u8]) -> String {
    elephc_probe::handshake::fingerprint(key)
}

/// Formats the key as lowercase hex for the sidecar file.
pub(crate) fn to_hex(key: &[u8]) -> String {
    key.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Parses a 64-character hex string into a key, or `None` if malformed.
fn parse_hex_key(hex: &str) -> Option<[u8; KEY_LEN]> {
    if hex.len() != KEY_LEN * 2 {
        return None;
    }
    let mut key = [0u8; KEY_LEN];
    for (i, chunk) in hex.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk).ok()?;
        key[i] = u8::from_str_radix(text, 16).ok()?;
    }
    Some(key)
}

/// Reads 32 bytes from the OS entropy source. Falls back to a time-seeded xorshift
/// only if `/dev/urandom` is unavailable — the build key is a possession credential,
/// not a strong secret, so a degraded source still serves its purpose.
fn random_key() -> [u8; KEY_LEN] {
    use std::io::Read as _;
    if let Ok(mut file) = std::fs::File::open("/dev/urandom") {
        let mut key = [0u8; KEY_LEN];
        if file.read_exact(&mut key).is_ok() {
            return key;
        }
    }
    let mut state = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15)
        | 1;
    let mut key = [0u8; KEY_LEN];
    for byte in key.iter_mut() {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = (state >> 24) as u8;
    }
    key
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_override_round_trips() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        std::env::set_var("ELEPHC_PROBE_KEY", hex);
        let key = build_key();
        std::env::remove_var("ELEPHC_PROBE_KEY");
        assert_eq!(to_hex(&key), hex);
        assert_eq!(key[0], 0x00);
        assert_eq!(key[31], 0xff);
    }

    #[test]
    fn random_keys_differ() {
        assert_ne!(random_key(), random_key());
    }

    #[test]
    fn malformed_override_is_rejected() {
        assert!(parse_hex_key("tooshort").is_none());
        assert!(parse_hex_key(&"zz".repeat(32)).is_none());
        assert!(parse_hex_key(&"ab".repeat(32)).is_some());
    }
}
