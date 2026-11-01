//! Purpose:
//! Implements FILTER_VALIDATE_URL's `Uri\WhatWg\Url` parser route with the WHATWG URL parser.
//!
//! Called from:
//! - `crate::elephc_filter_validate_url_whatwg` in `lib.rs`.
//!
//! Key details:
//! - PHP sanitizes the value before parsing, so this receives the already-validated bytes.
//! - The parser must see an absolute URL; scheme/host and query-required checks mirror
//!   `php_filter_validate_url()` after `php_uri_parse_to_struct()`.
//! - WHATWG parsers normalize backslashes in special URLs; the original string is still the
//!   value filter_var returns, because validation never rewrites it.

/// Returns whether one sanitized value is a valid absolute URL under WHATWG parsing.
pub(crate) fn validate(value: &[u8], query_required: bool) -> bool {
    let Ok(text) = std::str::from_utf8(value) else {
        return false;
    };
    let Ok(url) = url::Url::parse(text) else {
        return false;
    };
    let scheme = url.scheme();
    let host = url.host_str();
    if matches!(scheme, "http" | "https") && host.is_none() {
        return false;
    }
    if host.is_none() && !matches!(scheme, "mailto" | "news" | "file") {
        return false;
    }
    if query_required && url.query().is_none() {
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::validate;

    /// Verifies the WHATWG route accepts special-scheme backslashes and rejects bad ports.
    #[test]
    fn whatwg_validation_matches_php_shapes() {
        assert!(validate(b"http://example.com\\path", false));
        assert!(validate(b"http://example.com:65535", false));
        assert!(!validate(b"http://example.com:65536", false));
        assert!(validate(b"http://0x7f.1", false));
        assert!(validate(b"http://user:pass@example.com", false));
        assert!(validate(b"foo://bar", false));
        assert!(!validate(b"foo:bar", false));
        assert!(validate(b"mailto:", false));
        assert!(validate(b"file:///tmp/x", false));
        assert!(!validate(b"http://example.com", true));
        assert!(validate(b"http://example.com?x", true));
    }
}
