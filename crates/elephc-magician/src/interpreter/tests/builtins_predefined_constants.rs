//! Purpose:
//! Interpreter tests for the `ENT_*`, `EXTR_*`, `SORT_*`, `LC_*`, `M_*`, and `SEEK_*` predefined
//! constant families, plus the `htmlspecialchars(..., ENT_QUOTES)` consequence of `ENT_QUOTES`
//! being undefined before this file was written.
//!
//! Called from:
//! - `cargo test -p elephc-magician interpreter::tests::builtins_predefined_constants`.
//!
//! Key details:
//! - Every expected string or value is `php -n` 8.5.6's own output on the same fragment.
//! - `LC_*` numbering is platform-specific: macOS/BSD libc and glibc assign different integers to
//!   the SAME category names. Each assertion accepts either this build host's own family (macOS)
//!   or glibc's, mirroring the existing `PHP_OS`-style portable check elsewhere in this test
//!   suite, so the test stays meaningful on either CI host instead of hard-coding one platform.

use super::super::*;
use super::support::*;

/// Runs one fragment and returns what it echoed.
fn out(fragment: &[u8]) -> String {
    let program = parse_fragment(fragment).expect("parse predefined-constant fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values)
        .expect("execute predefined-constant fragment");
    values.output.clone()
}

/// Verifies the full `ENT_*` family `htmlspecialchars()`/`htmlentities()` flags define, with
/// php's own integer values.
///
/// `php -n` 8.5.6 prints `3:2:0:0:48:32:16:8:4:128` for
/// `ENT_QUOTES:ENT_COMPAT:ENT_NOQUOTES:ENT_HTML401:ENT_HTML5:ENT_XHTML:ENT_XML1:ENT_SUBSTITUTE:ENT_IGNORE:ENT_DISALLOWED`.
#[test]
fn defines_the_full_ent_family_with_phps_values() {
    assert_eq!(
        out(br#"echo ENT_QUOTES, ":", ENT_COMPAT, ":", ENT_NOQUOTES, ":", ENT_HTML401, ":",
ENT_HTML5, ":", ENT_XHTML, ":", ENT_XML1, ":", ENT_SUBSTITUTE, ":", ENT_IGNORE, ":", ENT_DISALLOWED;"#),
        "3:2:0:0:48:32:16:8:4:128",
    );
}

/// Verifies the full `EXTR_*` family defines, with php's own integer values -- including the
/// mode `extract()` itself never receives as a literal, `EXTR_OVERWRITE` (`0`, the default).
///
/// `php -n` 8.5.6 prints `0:1:2:3:4:5:6:256`.
#[test]
fn defines_the_full_extr_family_with_phps_values() {
    assert_eq!(
        out(br#"echo EXTR_OVERWRITE, ":", EXTR_SKIP, ":", EXTR_PREFIX_SAME, ":", EXTR_PREFIX_ALL, ":",
EXTR_PREFIX_INVALID, ":", EXTR_PREFIX_IF_EXISTS, ":", EXTR_IF_EXISTS, ":", EXTR_REFS;"#),
        "0:1:2:3:4:5:6:256",
    );
}

/// Verifies the full `SORT_*` family defines, with php's own integer values.
///
/// `php -n` 8.5.6 prints `0:1:2:3:4:5:6:8`.
#[test]
fn defines_the_full_sort_family_with_phps_values() {
    assert_eq!(
        out(br#"echo SORT_REGULAR, ":", SORT_NUMERIC, ":", SORT_STRING, ":", SORT_DESC, ":",
SORT_ASC, ":", SORT_LOCALE_STRING, ":", SORT_NATURAL, ":", SORT_FLAG_CASE;"#),
        "0:1:2:3:4:5:6:8",
    );
}

/// Verifies the full `SEEK_*` family defines, with php's own (platform-independent, POSIX)
/// integer values.
///
/// `php -n` 8.5.6 prints `0:1:2`.
#[test]
fn defines_the_full_seek_family_with_phps_values() {
    assert_eq!(
        out(br#"echo SEEK_SET, ":", SEEK_CUR, ":", SEEK_END;"#),
        "0:1:2",
    );
}

/// Verifies the full `LC_*` family defines, with the target C library's own category numbering.
/// macOS/BSD libc numbers `LC_ALL:LC_COLLATE:LC_CTYPE:LC_MONETARY:LC_NUMERIC:LC_TIME:LC_MESSAGES`
/// as `0:1:2:3:4:5:6`; glibc (Linux) numbers the SAME names `6:3:0:4:1:2:5`. `php -n` 8.5.6 on
/// this macOS build host prints `0:1:2:3:4:5:6`.
#[test]
fn defines_the_full_lc_family_with_the_hosts_own_numbering() {
    let printed = out(br#"echo LC_ALL, ":", LC_COLLATE, ":", LC_CTYPE, ":", LC_MONETARY, ":",
LC_NUMERIC, ":", LC_TIME, ":", LC_MESSAGES;"#);
    assert!(
        printed == "0:1:2:3:4:5:6" || printed == "6:3:0:4:1:2:5",
        "unexpected LC_* numbering: {printed}",
    );
}

/// Verifies the full `M_*` maths family defines, with php's own float values -- each rounded to
/// 10 decimal places (the same `round($x, N)` shape the task's own `$R/x3.php` probe uses for
/// `M_PI`) rather than echoed raw: elephc's `echo` on a float prints the full round-trip decimal
/// where `php -n`'s default `precision=14` ini truncates it, a pre-existing, general
/// float-to-string formatting difference this task does not touch. Rounding both to the same
/// fixed number of decimal places sidesteps that difference and verifies the actual constant
/// VALUE instead.
///
/// `php -n` 8.5.6 prints these exact strings.
#[test]
fn defines_the_full_m_family_with_phps_float_values() {
    assert_eq!(
        out(br#"echo round(M_PI, 10), ":", round(M_E, 10), ":", round(M_LOG2E, 10), ":",
round(M_LOG10E, 10), ":", round(M_LN2, 10), ":", round(M_LN10, 10), ":", round(M_PI_2, 10), ":",
round(M_PI_4, 10), ":", round(M_1_PI, 10), ":", round(M_2_PI, 10), ":", round(M_SQRTPI, 10), ":",
round(M_2_SQRTPI, 10), ":", round(M_SQRT2, 10), ":", round(M_SQRT3, 10), ":",
round(M_SQRT1_2, 10), ":", round(M_LNPI, 10), ":", round(M_EULER, 10);"#),
        "3.1415926536:2.7182818285:1.4426950409:0.4342944819:0.6931471806:2.302585093:\
1.5707963268:0.7853981634:0.3183098862:0.6366197724:1.7724538509:1.1283791671:\
1.4142135624:1.7320508076:0.7071067812:1.1447298858:0.5772156649",
    );
}

/// Verifies `defined()` sees every constant in every family added by this change, matching the
/// `$R/v9.php` probe that measured all thirteen as missing before this file was written.
///
/// `php -n` 8.5.6 prints `total=37 missing=0`.
#[test]
fn every_previously_missing_constant_is_now_defined() {
    assert_eq!(
        out(br#"$names = [
    "ENT_QUOTES", "ENT_COMPAT", "ENT_HTML5", "ENT_SUBSTITUTE", "ENT_NOQUOTES",
    "EXTR_SKIP", "EXTR_OVERWRITE",
    "JSON_THROW_ON_ERROR", "JSON_PRETTY_PRINT", "JSON_UNESCAPED_SLASHES", "JSON_UNESCAPED_UNICODE",
    "ARRAY_FILTER_USE_BOTH", "ARRAY_FILTER_USE_KEY",
    "SORT_REGULAR", "SORT_STRING", "SORT_NUMERIC", "COUNT_RECURSIVE",
    "PREG_PATTERN_ORDER", "PREG_SET_ORDER", "PREG_OFFSET_CAPTURE", "PREG_UNMATCHED_AS_NULL",
    "E_ALL", "E_ERROR", "E_WARNING", "E_NOTICE", "E_USER_DEPRECATED", "E_STRICT",
    "PHP_EOL", "PHP_INT_MAX", "PHP_VERSION", "PHP_OS_FAMILY", "DIRECTORY_SEPARATOR",
    "STR_PAD_LEFT", "LC_ALL", "M_PI", "SEEK_SET", "FILTER_VALIDATE_INT",
];
$missing = [];
foreach ($names as $n) {
    if (!defined($n)) {
        $missing[] = $n;
    }
}
echo "total=", count($names), " missing=", count($missing);"#),
        "total=37 missing=0",
    );
}

/// Verifies the consequence named by the task: `htmlspecialchars()` with an explicit
/// `ENT_QUOTES` no longer fatals on an undefined constant, and its escaped output matches php.
///
/// `php -n` 8.5.6 prints `ent:a&#039;b&lt;c&gt;`.
#[test]
fn htmlspecialchars_with_ent_quotes_no_longer_fatals() {
    assert_eq!(
        out(br#"echo 'ent:', htmlspecialchars("a'b<c>", ENT_QUOTES);"#),
        "ent:a&#039;b&lt;c&gt;",
    );
}
