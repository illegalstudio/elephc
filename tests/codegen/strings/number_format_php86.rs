//! Purpose:
//! Runs PHP 8.6's `number_format()` decimal-range validation fixtures.
//!
//! Called from:
//! - `cargo test --test codegen_tests number_format_php86`.
//!
//! Key details:
//! - PHP 8.6 rejects a `$decimals` outside the platform `int` range with a `ValueError`.
//!   PHP 8.5 silently clamps it to the nearest representable precision instead.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// Compiles and runs one fixture with the PHP 8.5 profile.
fn run85(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php85)
}

/// Verifies PHP 8.6 throws php-src's `ValueError` for an out-of-range precision while
/// 8.5 keeps formatting.
#[test]
fn test_number_format_decimals_range_php86() {
    // The result is consumed so the pure call is not dropped as dead code.
    let overflow = "<?php try { echo number_format(1234.5, 3000000000); } catch (ValueError $e) { echo $e->getMessage(); }";
    assert_eq!(
        run86(overflow),
        "number_format(): Argument #2 ($decimals) must be between -2147483648 and 2147483647"
    );

    let underflow = "<?php try { echo number_format(1234.5, -3000000000); } catch (ValueError $e) { echo $e->getMessage(); }";
    assert_eq!(
        run86(underflow),
        "number_format(): Argument #2 ($decimals) must be between -2147483648 and 2147483647"
    );

    let out85 = run85(overflow);
    assert!(
        out85.starts_with("1,234.5") && out85.len() > "1,234.5".len(),
        "PHP 8.5 clamps an out-of-range precision instead of throwing: {out85:?}"
    );
}

/// Verifies the platform-range boundaries stay accepted on 8.6 and keep their formatting.
#[test]
fn test_number_format_decimals_bounds_php86() {
    let source = "<?php
echo number_format(1234.5, 0), '|';
echo number_format(1234.5, 2), '|';
echo number_format(1234.5, -2), '|';
echo number_format(1234.5, 2147483647) !== '' ? 'high' : 'empty', '|';
echo number_format(1234.5, -2147483648) !== '' ? 'low' : 'empty';";
    let out = run86(source);
    assert!(out.starts_with("1,234|1,234.50|1,200|"), "{out}");
    assert!(out.ends_with("high|low"), "{out}");
}
