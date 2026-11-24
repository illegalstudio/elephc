//! Purpose:
//! End-to-end tests for PHP 8.6's GMP additions: `gmp_powm_sec`, `gmp_prevprime`, and the
//! range/NUL diagnostics that changed in the same release.
//!
//! Called from:
//! - `cargo test --test codegen_tests gmp_php86` through the codegen test module.
//!
//! Key details:
//! - Exceptions are caught in the fixture and printed with their class, so stdout is the
//!   assertion surface and the diagnostic text stays readable.
//! - Expected strings are php-src 8.6's; `ULONG_MAX` is 18446744073709551615 on every
//!   supported 64-bit target.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Runs one fixture under the PHP 8.6 profile, where these functions and diagnostics exist.
fn run(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// `gmp_powm_sec()` computes the same residue as `gmp_powm()` and validates its operands.
#[test]
fn test_gmp_powm_sec_php86() {
    let out = run(r#"<?php
echo gmp_strval(gmp_powm_sec(4, 13, 497)), "\n";
foreach ([['exponent-zero', 4, 0, 497], ['even-modulus', 4, 13, 498], ['zero-modulus', 4, 13, 0]] as [$label, $base, $exp, $mod]) {
    try { gmp_powm_sec($base, $exp, $mod); }
    catch (Throwable $error) { echo $label, ':', get_class($error), ':', $error->getMessage(), "\n"; }
}
"#);
    assert_eq!(
        out,
        "445\n\
exponent-zero:ValueError:gmp_powm_sec(): Argument #2 ($exponent) must be greater than 0\n\
even-modulus:ValueError:gmp_powm_sec(): Argument #3 ($modulus) must be odd\n\
zero-modulus:DivisionByZeroError:gmp_powm_sec(): Argument #3 ($modulus) Modulo by zero\n"
    );
}

/// `gmp_prevprime()` returns the previous prime and reports its certainty through the
/// by-reference output parameter.
#[test]
fn test_gmp_prevprime_php86() {
    let out = run(r#"<?php
echo gmp_strval(gmp_prevprime(100)), "\n";
$flag = false;
echo gmp_strval(gmp_prevprime(100, $flag)), '|', $flag ? '1' : '0', "\n";
$named = false;
echo gmp_strval(gmp_prevprime(3, $named)), '|', $named ? '1' : '0', "\n";
try { gmp_prevprime(2); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), "\n"; }
$untouched = true;
try { gmp_prevprime(1, $untouched); }
catch (Throwable $error) { echo get_class($error), ':', $error->getMessage(), '|', $untouched ? '1' : '0', "\n"; }
"#);
    assert_eq!(
        out,
        "97\n97|1\n2|1\nValueError:gmp_prevprime(): Argument #1 ($num) must be greater than 2\nValueError:gmp_prevprime(): Argument #1 ($num) must be greater than 2|1\n"
    );
}

/// The 8.6 range diagnostics name the unsigned-long bound for `gmp_fact`, `gmp_pow`,
/// `gmp_binomial`, `gmp_root` and `gmp_rootrem`.
#[test]
fn test_gmp_unsigned_long_range_messages_php86() {
    let out = run(r#"<?php
$cases = [
    'fact-large' => fn() => gmp_fact('18446744073709551616'),
    'fact-negative' => fn() => gmp_fact(-1),
    'pow-negative' => fn() => gmp_pow(2, -1),
    'binomial-negative' => fn() => gmp_binomial(5, -1),
    'root-zero' => fn() => gmp_root(16, 0),
    'rootrem-zero' => fn() => gmp_rootrem(16, 0),
];
foreach ($cases as $label => $case) {
    try { $case(); }
    catch (Throwable $error) { echo $label, ':', get_class($error), ':', $error->getMessage(), "\n"; }
}
"#);
    assert_eq!(
        out,
        "fact-large:ValueError:gmp_fact(): Argument #1 ($num) must be between 0 and 18446744073709551615\n\
fact-negative:ValueError:gmp_fact(): Argument #1 ($num) must be between 0 and 18446744073709551615\n\
pow-negative:ValueError:gmp_pow(): Argument #2 ($exponent) must be between 0 and 18446744073709551615\n\
binomial-negative:ValueError:gmp_binomial(): Argument #2 ($k) must be between 0 and 18446744073709551615\n\
root-zero:ValueError:gmp_root(): Argument #2 ($nth) must be between 1 and 18446744073709551615\n\
rootrem-zero:ValueError:gmp_rootrem(): Argument #2 ($nth) must be between 1 and 18446744073709551615\n"
    );
}

/// NUL bytes in an integer string are rejected instead of truncating the value, and the
/// `gmp_powm()` division-by-zero message names its modulus argument.
#[test]
fn test_gmp_nul_and_powm_message_php86() {
    let out = run(r#"<?php
try { gmp_init("12\0" . "3"); }
catch (Throwable $error) { echo 'init:', get_class($error), ':', $error->getMessage(), "\n"; }
try { gmp_add("12\0", 1); }
catch (Throwable $error) { echo 'add:', get_class($error), ':', $error->getMessage(), "\n"; }
try { gmp_powm(4, 13, 0); }
catch (Throwable $error) { echo 'powm:', get_class($error), ':', $error->getMessage(), "\n"; }
"#);
    assert_eq!(
        out,
        "init:ValueError:gmp_init(): Argument #1 ($num) is not an integer string\n\
add:ValueError:gmp_add(): Argument #1 ($num1) is not an integer string\n\
powm:DivisionByZeroError:gmp_powm(): Argument #3 ($modulus) Modulo by zero\n"
    );
}

/// The GMP shift and exponentiation operators reject out-of-range right operands.
#[test]
fn test_gmp_operator_range_messages_php86() {
    let out = run(r#"<?php
try { echo gmp_strval(gmp_init(2) ** -1); }
catch (Throwable $error) { echo 'pow:', get_class($error), ':', $error->getMessage(), "\n"; }
try { echo gmp_strval(gmp_init(1) << -1); }
catch (Throwable $error) { echo 'shift:', get_class($error), ':', $error->getMessage(), "\n"; }
"#);
    assert_eq!(
        out,
        "pow:ValueError:Exponent must be between 0 and 18446744073709551615\n\
shift:ValueError:Shift must be between 0 and 18446744073709551615\n"
    );
}
