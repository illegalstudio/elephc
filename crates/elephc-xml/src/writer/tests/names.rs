//! Purpose:
//! Tests for `is_valid_name`, the `xmlValidateName` port PHP's `XMLWriter` argument
//! checks rely on, against the names the probe corpus accepted and rejected.
//!
//! Called from:
//! - `cargo test -p elephc-xml --lib writer` through Rust's test harness.
//!
//! Key details:
//! - Rejections come from PHP `ValueError`s (`must be a valid element name, "..." given`).

use crate::writer::is_valid_name;

/// Names PHP accepted.
#[test]
fn accepts_valid_names() {
    for name in [
        "élé", "aé", "€", "a.b", "_a", "a:b", "a-b", "ok", "a1", "html", "p:a", "_",
        "a.b-c_d:e", "xml", ":a",
    ] {
        assert!(is_valid_name(name.as_bytes()), "{name}");
    }
}

/// Names PHP rejected with a `ValueError`.
#[test]
fn rejects_invalid_names() {
    for name in [&b""[..], b" ", b"a b", b"1a", b".a", b"-a", b"a\x7f", b"\xff", b" a", b"a ", b"a\n", b"\xc0\x80a", b"a\xed\xa0\x80"] {
        assert!(!is_valid_name(name), "{name:?}");
    }
}
