//! Purpose:
//! Codegen tests for the injected `filter_var()` prelude's `FILTER_VALIDATE_IP` surface.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Every expectation was measured against php 8.5: the validator returns the address as
//!   given, or `false`, and the `FILTER_FLAG_IPV4`/`IPV6` bits gate the family.
//! - The IPv6 parser uses a private `-v4-` placeholder internally; an address that spells
//!   that text literally must be refused, and an IPv4-mapped address (`::ffff:192.0.2.1`)
//!   must count as IPv6 despite its dotted tail.

use crate::support::*;

/// Verifies the family flags agree with php for plain, mapped and refused addresses.
#[test]
fn filter_var_ip_family_flags_match_php() {
    let out = compile_and_run(
        r#"<?php
        $cases = [
            ["127.0.0.1", 0],
            ["127.0.0.1", FILTER_FLAG_IPV4],
            ["127.0.0.1", FILTER_FLAG_IPV6],
            ["::1", 0],
            ["::1", FILTER_FLAG_IPV6],
            ["::1", FILTER_FLAG_IPV4],
            ["::ffff:192.0.2.1", 0],
            ["::ffff:192.0.2.1", FILTER_FLAG_IPV6],
            ["::ffff:192.0.2.1", FILTER_FLAG_IPV4],
        ];
        foreach ($cases as [$ip, $flags]) {
            $result = filter_var($ip, FILTER_VALIDATE_IP, $flags);
            echo $result === false ? "F" : $result, "\n";
        }
        "#,
    );
    assert_eq!(
        out,
        "127.0.0.1\n127.0.0.1\nF\n::1\n::1\nF\n::ffff:192.0.2.1\n::ffff:192.0.2.1\nF\n"
    );
}

/// Verifies the parser's private `-v4-` placeholder cannot be spelled by an input address.
#[test]
fn filter_var_rejects_the_internal_ipv4_placeholder() {
    let out = compile_and_run(
        r#"<?php
        var_dump(filter_var("1:2:3:4:5:6:7:8:-v4-", FILTER_VALIDATE_IP));
        var_dump(filter_var("1:2:3:4:5:6:7:8:-v4-", FILTER_VALIDATE_IP, FILTER_FLAG_IPV6));
        var_dump(filter_var("::ffff:192.0.2.1", FILTER_VALIDATE_IP, FILTER_FLAG_IPV6));
        "#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nstring(16) \"::ffff:192.0.2.1\"\n");
}
