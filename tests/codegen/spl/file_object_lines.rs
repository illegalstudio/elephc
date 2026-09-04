//! Purpose:
//! Integration tests for how many lines an `SplFileObject` iterates: php's iteration is driven
//! by the STREAM, so a file that ends with a newline has one more line than `file()` reports.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc backs the iteration with an array built from `file()`, which never produces an empty
//!   last element — a trailing `"\n"` belongs to the line before it. php reaches end of file only
//!   when a read comes back with nothing, so after the last newline one more round answers `''`.
//!   The array-backed model stopped an iteration early for every file that ends in a newline,
//!   which is most of them.
//! - An EMPTY file is the same rule seen from the other side: `file()` reports no lines at all
//!   and php iterates once, answering `''`.
//! - `READ_CSV` does NOT get that trailing record: php's csv iteration has none, so the record
//!   builder drops it. That is done in the builder rather than by ordering the load, because
//!   `setFlags(READ_CSV)` rebuilds the records long after the object was constructed.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies the line count for the six shapes that decide the rule.
#[test]
fn test_the_line_count_follows_phps_stream_driven_iteration() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function show(string $name, string $content): void
{
    file_put_contents($name, $content);
    $o = new SplFileObject($name);
    $seen = [];
    foreach ($o as $k => $line) {
        $seen[] = $k . "=" . var_export($line, true);
    }
    echo count($seen), " [", implode(", ", $seen), "]\n";
    unlink($name);
}
show("e1.txt", "");
show("e2.txt", "\n");
show("e3.txt", "a");
show("e4.txt", "a\n");
show("e5.txt", "a\nb\n");
show("e6.txt", "a\n\n");
"#,
    );
    assert_eq!(
        out,
        "1 [0='']\n\
         2 [0='\n', 1='']\n\
         1 [0='a']\n\
         2 [0='a\n', 1='']\n\
         3 [0='a\n', 1='b\n', 2='']\n\
         3 [0='a\n', 1='\n', 2='']\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `DROP_NEW_LINE` iteration reaches the trailing empty line too.
///
/// This is the auditor program that named the defect: three written lines, four iterations, the
/// last one empty.
#[test]
fn test_drop_new_line_still_yields_the_trailing_empty_line() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("lines.txt", "line1\nline2\nline3\n");
$obj = new SplFileObject("lines.txt");
$obj->setFlags(SplFileObject::DROP_NEW_LINE);
foreach ($obj as $line) {
    echo bin2hex($line), "\n";
}
unlink("lines.txt");
"#,
    );
    assert_eq!(out, "6c696e6531\n6c696e6532\n6c696e6533\n\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `READ_CSV` gains no EXTRA record from that line.
///
/// php's csv iteration answers its own trailing `[null]` for a file ending in a newline — it
/// reads until a read fails, not until the lines run out — and the record builder already
/// produced it. The plain-iteration line added here must not become a SECOND one: a two-row file
/// answers three records, not four. `setFlags(READ_CSV)` rebuilds the records long after
/// construction, which is why the builder — not the loader — is what drops it.
#[test]
fn test_read_csv_has_no_extra_trailing_record() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("rows.csv", "a,b\nc,d\n");
$csv = new SplFileObject("rows.csv");
$csv->setFlags(SplFileObject::READ_CSV);
$rows = [];
foreach ($csv as $row) {
    $rows[] = json_encode($row);
}
echo count($rows), " ", implode(" ", $rows), "\n";
unlink("rows.csv");
"#,
    );
    assert_eq!(out, "3 [\"a\",\"b\"] [\"c\",\"d\"] [null]\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies the trailing element under `SKIP_EMPTY`, which depends on `READ_AHEAD` as well.
///
/// MEASURED over the whole 6 file shapes × 8 flags matrix on `php -n` 8.5.6, because two earlier
/// readings each had half of it and neither varied `READ_AHEAD`:
///
/// ```text
/// no SKIP_EMPTY               the trailing element is ""
/// SKIP_EMPTY, no READ_AHEAD   the trailing element is false
/// SKIP_EMPTY and READ_AHEAD   there is no trailing element  <- this program
/// ```
///
/// The middle line stays in every one of them: without `DROP_NEW_LINE` it is `"\n"`, not empty.
/// The flags are read at iteration time rather than at load time because `setFlags()` may turn
/// them on long after the lines were read — this very program does.
#[test]
fn test_skip_empty_with_read_ahead_drops_the_trailing_element() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("s.txt", "a\n\nb\n");
$it = new SplFileObject("s.txt");
$it->setFlags(SplFileObject::SKIP_EMPTY | SplFileObject::READ_AHEAD);
foreach ($it as $line) {
    var_dump($line);
}
unlink("s.txt");
"#,
    );
    assert_eq!(out, "string(2) \"a\n\"\nstring(1) \"\n\"\nstring(2) \"b\n\"\n");
    let _ = std::fs::remove_dir_all(dir);
}
