//! Purpose:
//! Unit tests pinning the bridge against values independently captured from PHP 8.5.10.
//!
//! Called from:
//! - `cargo test -p elephc-filter` through Rust's test harness.
//!
//! Key details:
//! - The cases mirror php-src's ext/filter PHPT edge cases: overflow, leading zeroes, URI
//!   status ranges, separator options, binary strings and strip-tag state transitions.

use super::sanitize;
use super::validate::{boolean, domain, float, int, ip, mac};
use super::Outcome;

/// FILTER_FLAG_ALLOW_OCTAL | FILTER_FLAG_ALLOW_HEX.
const PREFIXES: i64 = 0x0001 | 0x0002;

/// FILTER_FLAG_ALLOW_THOUSAND.
const THOUSAND: i64 = 0x2000;
/// FILTER_FLAG_ALLOW_FRACTION.
const FRACTION: i64 = 0x1000;
/// FILTER_FLAG_ALLOW_SCIENTIFIC.
const SCIENTIFIC: i64 = 0x4000;
/// FILTER_FLAG_IPV4.
const IPV4: i64 = 0x0010_0000;
/// FILTER_FLAG_IPV6.
const IPV6: i64 = 0x0020_0000;
/// FILTER_FLAG_NO_RES_RANGE.
const NO_RES: i64 = 0x0040_0000;
/// FILTER_FLAG_NO_PRIV_RANGE.
const NO_PRIV: i64 = 0x0080_0000;
/// FILTER_FLAG_HOSTNAME.
const HOSTNAME: i64 = 0x0010_0000;

/// Verifies decimal integer validation and its exact overflow boundary.
#[test]
fn int_matches_php() {
    assert_eq!(int(b" 42 ", 0), Some(42));
    assert_eq!(int(b"+42", 0), Some(42));
    assert_eq!(int(b"-0", 0), Some(0));
    assert_eq!(int(b"012", 0), None);
    assert_eq!(int(b"0x1A", 0), None);
    assert_eq!(int(b"", 0), None);
    assert_eq!(int(b"9223372036854775807", 0), Some(i64::MAX));
    assert_eq!(int(b"9223372036854775808", 0), None);
    assert_eq!(int(b"-9223372036854775808", 0), Some(i64::MIN));
    assert_eq!(int(b"-9223372036854775809", 0), None);
}

/// Verifies the explicit octal and hexadecimal prefixes and overflow rejection.
#[test]
fn int_prefixes_match_php() {
    assert_eq!(int(b"0x1A", PREFIXES), Some(26));
    assert_eq!(int(b"0Xff", PREFIXES), Some(255));
    assert_eq!(int(b"0o17", PREFIXES), Some(15));
    assert_eq!(int(b"017", PREFIXES), Some(15));
    assert_eq!(int(b"0x", PREFIXES), None);
    assert_eq!(int(b"0xg", PREFIXES), None);
    assert_eq!(int(b"0o", PREFIXES), None);
    assert_eq!(int(b"0og", PREFIXES), None);
    assert_eq!(int(b"0xFFFFFFFFFFFFFFFF", PREFIXES), Some(-1));
}

/// Verifies the boolean truth table, including surrounding whitespace and mixed case.
#[test]
fn bool_matches_php() {
    assert_eq!(boolean(b"on"), Some(true));
    assert_eq!(boolean(b"OFF"), Some(false));
    assert_eq!(boolean(b" yes "), Some(true));
    assert_eq!(boolean(b""), Some(false));
    assert_eq!(boolean(b"0"), Some(false));
    assert_eq!(boolean(b"1"), Some(true));
    assert_eq!(boolean(b"NONE"), None);
}

/// Verifies float separators, exponent handling, underflow and overflow.
#[test]
fn float_matches_php() {
    assert_eq!(float(b"1.234   ", 0, None, None), Some(1.234));
    assert_eq!(float(b"7E-3", 0, None, None), Some(0.007));
    assert_eq!(float(b"1,234.5", THOUSAND, None, None), Some(1234.5));
    assert_eq!(float(b"1.234", 0, Some(b','), None), None);
    assert_eq!(float(b"1,234", 0, Some(b','), None), Some(1.234));
    assert_eq!(float(b"0e5", 0, None, None), None);
    assert_eq!(float(b"0e0", 0, None, None), Some(0.0));
    assert_eq!(float(b"1e-9999", 0, None, None), None);
    assert_eq!(float(b"1e9999", 0, None, None), None);
    assert_eq!(float(b"1.", 0, None, None), Some(1.0));
    assert_eq!(float(b"+.5", 0, None, None), Some(0.5));
    assert_eq!(float(b".", 0, None, None), None);
    assert_eq!(float(b"123.4", FRACTION, None, None), Some(123.4));
    assert_eq!(float(b"123.4E2", FRACTION | SCIENTIFIC, None, None), Some(12340.0));
}

/// Verifies domain and hostname label rules, including trailing dots and hyphens.
#[test]
fn domain_matches_php() {
    assert!(domain(b"example.com", 0));
    assert!(domain(b"a.", 0));
    assert!(domain(b"", 0));
    assert!(!domain(b"a..b", 0));
    assert!(!domain(b".a", 0));
    assert!(!domain(b"-a", HOSTNAME));
    assert!(!domain(b"a-", HOSTNAME));
    assert!(domain(b"a-.", HOSTNAME));
    assert!(domain(b"cont-ains.h-yph-en-s.com", HOSTNAME));
    assert!(!domain(&[b'a'; 64], HOSTNAME));
}

/// Verifies IPv4 parsing, leading-zero rejection and RFC 6890 range flags.
#[test]
fn ipv4_matches_php() {
    assert!(ip(b"1.1.1.1", IPV4));
    assert!(!ip(b"1.1.1.1.1", 0));
    assert!(!ip(b"01.1.1.1", 0));
    assert!(!ip(b"256.1.1.1", 0));
    assert!(!ip(b"10.0.0.0", NO_PRIV));
    assert!(!ip(b"127.0.0.1", NO_RES));
    assert!(!ip(b"255.255.255.255", NO_RES));
    assert!(ip(b"100.64.0.0", NO_RES));
    assert!(ip(b"192.0.34.166", NO_PRIV));
    assert!(!ip(b"::1", IPV4));
}

/// Verifies IPv6 parsing, compression and range flags against php-src's cases.
#[test]
fn ipv6_matches_php() {
    assert!(ip(b"::1", IPV6));
    assert!(!ip(b"::1", IPV6 | NO_RES));
    assert!(ip(b"fe00::0", 0));
    assert!(!ip(b"::123456", 0));
    assert!(!ip(b"::1::b", 0));
    assert!(!ip(b"fffffffffffffffffffffffffffffffffffff::", IPV6));
    assert!(ip(b"2001:db8:3333:4444:5555:6666:1.2.3.4", IPV6));
    assert!(ip(b"::ffff:1.2.3.4", 0));
}

/// Verifies all three MAC address layouts and the separator option.
#[test]
fn mac_matches_php() {
    assert!(mac(b"00:11:22:33:44:55", false, b""));
    assert!(mac(b"00-11-22-33-44-55", false, b""));
    assert!(mac(b"0011.2233.4455", false, b""));
    assert!(!mac(b"00:11:22:33:44:55", true, b"-"));
    assert!(mac(b"00-11-22-33-44-55", true, b"-"));
    assert!(!mac(b"00:11:22:33:44:gg", false, b""));
}

/// Verifies the sanitizer's tag stripping and entity encoding.
#[test]
fn sanitize_string_matches_php() {
    assert_eq!(
        sanitize::apply(0x0201, 0x0080, b"<>!@#$%^&*()'\""),
        Outcome::Str(b"!@#$%^&*()'\"".to_vec())
    );
    assert_eq!(
        sanitize::apply(0x0201, 0x0040, b"<>!@#$%^&*()'\""),
        Outcome::Str(b"!@#$%^&#38;*()&#39;&#34;".to_vec())
    );
    assert_eq!(sanitize::apply(0x0201, 0, b"<>"), Outcome::Str(Vec::new()));
}

/// Verifies encoded, special-character, email, URL and numeric sanitizers.
#[test]
fn sanitize_maps_match_php() {
    assert_eq!(
        sanitize::apply(0x0202, 0, b"\"<br>blah</ph>"),
        Outcome::Str(b"%22%3Cbr%3Eblah%3C%2Fph%3E".to_vec())
    );
    assert_eq!(
        sanitize::apply(0x0202, 0, b"\xff"),
        Outcome::Str(b"%FF".to_vec())
    );
    assert_eq!(
        sanitize::apply(0x0203, 0, b"<>'\"&"),
        Outcome::Str(b"&#60;&#62;&#39;&#34;&#38;".to_vec())
    );
    assert_eq!(
        sanitize::apply(0x0205, 0, b"white space here \\ \\ \" some more"),
        Outcome::Str(b"whitespaceheresomemore".to_vec())
    );
    assert_eq!(
        sanitize::apply(0x0206, 0, b"http://example.com/?a=b"),
        Outcome::Str(b"http://example.com/?a=b".to_vec())
    );
    assert_eq!(
        sanitize::apply(0x0207, 0, b"asd123.2asd"),
        Outcome::Str(b"1232".to_vec())
    );
    assert_eq!(
        sanitize::apply(0x0208, FRACTION | SCIENTIFIC, b"123,4e"),
        Outcome::Str(b"1234e".to_vec())
    );
}

/// Verifies unsafe_raw's flag-driven encoding and empty-string nulling.
#[test]
fn unsafe_raw_matches_php() {
    assert_eq!(
        sanitize::apply(0x0204, 0x0040, b"a&b"),
        Outcome::Str(b"a&#38;b".to_vec())
    );
    assert_eq!(sanitize::apply(0x0204, 0x0100, b""), Outcome::Null);
    assert_eq!(sanitize::apply(0x0204, 0, b"\0"), Outcome::Str(vec![0]));
}

/// Verifies the deprecated string sanitizer's empty-string-null flag.
#[test]
fn string_empty_null_matches_php() {
    assert_eq!(sanitize::apply(0x0201, 0x0100, b""), Outcome::Null);
    assert_eq!(sanitize::apply(0x0201, 0, b""), Outcome::Str(Vec::new()));
}

/// Verifies the octal/hex overflow boundary just past u64.
#[test]
fn prefix_overflow_boundary() {
    assert_eq!(int(b"02777777777777777777777", 0x0001), None);
    assert_eq!(int(b"010000000000000000000000", 0x0001), None);
    assert_eq!(int(b"0x10000000000000000", 0x0002), None);
    assert_eq!(int(b"01777777777777777777777", 0x0001), Some(-1));
}
