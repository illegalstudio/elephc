//! Purpose:
//! Compile-time build-key material for `--probe`: generate the 32-byte key that
//! embeds in the binary and lands in the `.key` file beside it, so the probe
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
///
/// `Err` when no entropy source can be read. There is no weaker key: see
/// `random_key`.
pub(crate) fn build_key() -> Result<[u8; KEY_LEN], String> {
    if let Ok(hex) = std::env::var("ELEPHC_PROBE_KEY") {
        if let Some(key) = parse_hex_key(hex.trim()) {
            return Ok(key);
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

/// Reads 32 bytes from the OS entropy source, or fails.
///
/// This used to fall back to a xorshift seeded from the current nanosecond,
/// justified as "a possession credential, not a strong secret". That rationale
/// is backwards: a possession credential whose value can be *derived* is not a
/// credential at all. The seed is the build time, which is in CI logs and in
/// artifact timestamps, and the handshake hands any unauthenticated client a
/// nonce and an HMAC over it — a free offline verifier for guessed keys, with no
/// rate limit and no need to connect twice. Anyone who knew the build second
/// could recover the key and authenticate to the endpoint without ever holding
/// the binary.
///
/// So: no weaker key. A build that cannot read entropy fails, loudly, rather
/// than shipping a credential that only looks like one.
fn random_key() -> Result<[u8; KEY_LEN], String> {
    use std::io::Read as _;
    let mut file = std::fs::File::open("/dev/urandom")
        .map_err(|error| format!("cannot open /dev/urandom to generate a build key: {error}"))?;
    let mut key = [0u8; KEY_LEN];
    file.read_exact(&mut key)
        .map_err(|error| format!("cannot read a build key from /dev/urandom: {error}"))?;
    Ok(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The override is read through the parser, not through the environment.
    ///
    /// This used to `set_var`/`remove_var` around a `build_key()` call. Tests run
    /// in parallel by default, and the variable is process-global: the moment any
    /// other test in this binary reads a build key, the two race and one of them
    /// fails for reasons that have nothing to do with what it tests. Exercising
    /// the parser directly asserts the same property with nothing shared.
    #[test]
    fn hex_override_round_trips() {
        let hex = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        let key = parse_hex_key(hex).expect("a well-formed override must parse");
        assert_eq!(to_hex(&key), hex);
        assert_eq!(key[0], 0x00);
        assert_eq!(key[31], 0xff);
    }

    /// Two builds must not share a key — that is the whole revocation model.
    #[test]
    fn random_keys_differ() {
        let (a, b) = (
            random_key().expect("entropy"),
            random_key().expect("entropy"),
        );
        assert_ne!(a, b);
        // And neither is the all-zero key a failed read would leave behind.
        assert_ne!(a, [0u8; KEY_LEN]);
    }

    #[test]
    fn malformed_override_is_rejected() {
        assert!(parse_hex_key("tooshort").is_none());
        assert!(parse_hex_key(&"zz".repeat(32)).is_none());
        assert!(parse_hex_key(&"ab".repeat(32)).is_some());
    }
}
