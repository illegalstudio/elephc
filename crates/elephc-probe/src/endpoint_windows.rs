//! Purpose:
//! Preserves the authenticated probe-client wire protocol on Windows without a Unix-socket server.
//!
//! Called from:
//! - `elephc monitor` when it connects to a TCP or TLS probe service.
//! - `crate::handshake` through the shared nonce, proof, and encrypted-payload protocol.
//!
//! Key details:
//! - Windows deliberately has no local Unix-domain endpoint or SIGPROF sampler here.
//! - The portable client remains byte-for-byte protocol-compatible with Unix peers.

use std::io::{Read, Write};
use std::time::Duration;

use crate::handshake::{self, KEY_LEN, NONCE_LEN, TAG_LEN};

/// The sampled-profile request mode shared with Unix endpoint clients.
pub const WANT_SAMPLED: u8 = b'S';
/// The exact-profile request mode shared with Unix endpoint clients.
pub const WANT_EXACT: u8 = b'E';
/// The maximum server-side exact wait, retained for client timeout calculation.
pub const EXACT_WAIT: Duration = Duration::from_secs(30);
/// Maximum encrypted profile payload accepted from a peer.
pub const MAX_PROFILE_BYTES: usize = 64 * 1024 * 1024;

/// Wire protocol shared by portable TCP/TLS clients and Unix probe endpoints.
pub mod wire {
    use super::*;

    /// Reads exactly `n` bytes or reports a short or failed transport read.
    pub fn read_exact_vec(stream: &mut impl Read, n: usize) -> std::io::Result<Vec<u8>> {
        let mut bytes = vec![0; n];
        stream.read_exact(&mut bytes)?;
        Ok(bytes)
    }

    /// Completes the mutually authenticated profile fetch without requiring a Unix socket.
    pub fn client_handshake_and_fetch(
        stream: &mut (impl Read + Write),
        key: &[u8; KEY_LEN],
        nonce_c: &[u8; NONCE_LEN],
        want: u8,
    ) -> std::io::Result<String> {
        stream.write_all(nonce_c)?;
        stream.flush()?;
        let nonce_s = read_exact_vec(stream, NONCE_LEN)?;
        let server_tag = read_exact_vec(stream, TAG_LEN)?;
        let expected = handshake::server_tag(key, nonce_c, &nonce_s);
        if !handshake::tags_equal(&server_tag, &expected) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "probe endpoint failed to prove the build key (wrong binary or key)",
            ));
        }
        let client_tag = handshake::client_tag(key, &nonce_s, nonce_c, want);
        stream.write_all(&[want])?;
        stream.write_all(&client_tag)?;
        stream.flush()?;
        let mut len_bytes = [0; 4];
        stream.read_exact(&mut len_bytes)?;
        let len = u32::from_be_bytes(len_bytes) as usize;
        if len > MAX_PROFILE_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "probe profile exceeds the size cap (buggy or hostile server?)",
            ));
        }
        let ciphertext = read_exact_vec(stream, len)?;
        let payload_tag = read_exact_vec(stream, TAG_LEN)?;
        let (k_enc, k_mac) = handshake::session_keys(key, nonce_c, &nonce_s, want);
        let payload = handshake::open(&k_enc, &k_mac, &ciphertext, &payload_tag).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "probe payload failed authentication (tampered, or not this build)",
            )
        })?;
        String::from_utf8(payload)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "non-UTF-8 profile"))
    }
}
