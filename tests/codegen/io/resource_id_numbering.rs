//! Purpose:
//! Integration tests for the php-visible resource id a stream carries: `php://temp` consumes
//! TWO of them, so the numbering has a hole after every one.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - php's `php://temp` is a temporary-file stream WRAPPING a memory stream, and php registers
//!   both — so the id after one is two higher, not one. Measured on `php -n` 8.5.6: three
//!   `php://memory` opens number `5, 6, 7`, while memory-temp-memory numbers `5, 6, 8`.
//! - The temp stream keeps the FIRST of its two ids, which is why the second is burnt AFTER the
//!   stream has taken its own rather than allocated inside the open.
//! - Ids are never REUSED on either side: closing a stream leaves its number spent.
//! - The literal and run-time spellings of the same URL must agree, so both are pinned here.

use crate::support::*;

/// Verifies the numbering of a memory-temp-memory sequence, in both spellings.
#[test]
fn test_php_temp_consumes_two_resource_ids() {
    let out = compile_and_run(
        r#"<?php
function id($r): string { return (string) (int) $r; }
$a = fopen("php://memory", "r");
$b = fopen("php://temp", "r");
$c = fopen("php://memory", "r");
echo id($a), " ", id($b), " ", id($c), "\n";
$dyn = "php://" . "temp";
$d = fopen($dyn, "r");
$e = fopen("php://memory", "r");
echo id($d), " ", id($e), "\n";
$m = fopen("php://temp/maxmemory:64", "r");
$f = fopen("php://memory", "r");
echo id($m), " ", id($f), "\n";
"#,
    );
    assert_eq!(out, "5 6 8\n9 11\n12 14\n");
}

/// Verifies `php://memory` and `tmpfile()` still consume exactly one id each.
///
/// The control: only `php://temp` wraps a second stream, and a closed id is never handed out
/// again on either side.
#[test]
fn test_the_other_streams_consume_one_id_each() {
    let out = compile_and_run(
        r#"<?php
function id($r): string { return (string) (int) $r; }
$a = fopen("php://memory", "r");
$b = fopen("php://memory", "r");
$c = fopen("php://memory", "r");
echo id($a), " ", id($b), " ", id($c), "\n";
fclose($b);
$d = fopen("php://memory", "r");
$t = tmpfile();
$e = fopen("php://memory", "r");
echo id($d), " ", id($t), " ", id($e), "\n";
"#,
    );
    assert_eq!(out, "5 6 7\n8 9 10\n");
}
