//! Purpose:
//! The elephc mark, inlined as a `data:` URI for self-contained generated pages.
//!
//! Called from:
//! - `crate::call_graph` when rendering the call-graph and distributed-trace pages.
//!
//! Key details:
//! - Generated pages must open with **no network access**, so the mark cannot be
//!   linked — it has to travel inside the file. The asset is therefore a
//!   header-sized copy (46×48, ~3 KB) rather than `assets/logo-mark.png` (28 KB),
//!   which would add ~37 KB of base64 to every page. `--live` rewrites its page
//!   once per window, so page weight is paid repeatedly, not once.
//! - The PNG stays a real file in `assets/` rather than a base64 literal pasted
//!   into Rust: it can be viewed, replaced, and diffed as an image.
//! - Encoded once per process and reused, since `--live` re-renders continuously.

use std::sync::OnceLock;

/// The mark, at the size the page headers display it.
const MARK_PNG: &[u8] = include_bytes!("../assets/logo-mark-48.png");

/// `data:image/png;base64,…` for the elephc mark, encoded on first use.
pub(crate) fn mark_data_uri() -> &'static str {
    static URI: OnceLock<String> = OnceLock::new();
    URI.get_or_init(|| format!("data:image/png;base64,{}", base64(MARK_PNG)))
}

/// Standard base64, padded. Small enough to keep local rather than take a
/// dependency, and the only caller encodes ~3 KB once.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        // The tail is padded, not truncated: a decoder needs the group length.
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Checked against the vectors in RFC 4648 §10, including every tail length,
    /// because a hand-rolled encoder that is wrong only on the last group produces
    /// an image that silently fails to decode in the browser.
    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // Bytes above 0x7f must not be mangled by a char-vs-byte mistake.
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
    }

    /// The asset must actually be a PNG, and small enough that inlining it into
    /// every generated page — and every `--live` rewrite — stays negligible.
    #[test]
    fn the_inlined_mark_is_a_small_png() {
        assert_eq!(&MARK_PNG[..8], b"\x89PNG\r\n\x1a\n", "asset is not a PNG");
        assert!(
            MARK_PNG.len() < 8 * 1024,
            "the header mark grew to {} bytes; it is inlined into every page",
            MARK_PNG.len()
        );
        let uri = mark_data_uri();
        assert!(uri.starts_with("data:image/png;base64,"));
        assert!(!uri.contains('\n'), "a data URI must not carry newlines");
    }
}
