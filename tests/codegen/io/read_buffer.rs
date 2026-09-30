//! Purpose:
//! Integration tests for php's per-stream READ BUFFER on a regular file: a read takes a whole
//! chunk from the descriptor and serves the caller out of it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - elephc read exactly what each call asked for, so `fgetc()` was one `read(2)` per byte —
//!   MEASURED at 499 ms for 900 000 bytes where php takes 14 ms — and
//!   `stream_get_meta_data()['unread_bytes']` was a hardcoded zero where php reports what its
//!   buffer still holds.
//! - The buffer is what makes those numbers php's, and it changes four other answers that must
//!   stay php's: `ftell()` reports the CONSUMED position, not where the descriptor stopped; a
//!   seek discards the buffer; `feof()` is judged against what the CALLER asked for, not against
//!   the chunk the fill asked for; and a read that outruns the buffer is topped up from the
//!   descriptor rather than answering short.
//! - Restricted to REGULAR files: reading ahead on a socket or a pipe changes when a read blocks
//!   and what `stream_select()` sees, which is a separate question.
//! - Every expectation was measured on `php -n` 8.5.6.

use crate::support::*;

/// Verifies the position, EOF and content answers a buffered read must not change.
///
/// One program covers them together on purpose: the buffer is a single mechanism, and a fix that
/// gets `ftell()` right while breaking `feof()` would pass two narrower tests.
#[test]
fn test_a_buffered_read_keeps_phps_positions_and_eof() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("pos.txt", "0123456789abcdefghijklmnopqrstuvwxyz");
$h = fopen("pos.txt", "rb");
echo "start ", ftell($h), " ", var_export(feof($h), true), "\n";
echo "read5 ", fread($h, 5), " tell ", ftell($h), "\n";
echo "read3 ", fread($h, 3), " tell ", ftell($h), "\n";
echo "getc ", fgetc($h), " tell ", ftell($h), "\n";
fseek($h, 30);
echo "seek30 tell ", ftell($h), " next ", fread($h, 3), " tell ", ftell($h), "\n";
rewind($h);
echo "rewind tell ", ftell($h), " next ", fread($h, 2), "\n";
fseek($h, 0, SEEK_END);
echo "end tell ", ftell($h), " eof ", var_export(feof($h), true), "\n";
echo "readAtEnd ", var_export(fread($h, 5), true), " eof ", var_export(feof($h), true), "\n";
fclose($h);
unlink("pos.txt");
"#,
    );
    assert_eq!(
        out,
        "start 0 false\n\
         read5 01234 tell 5\n\
         read3 567 tell 8\n\
         getc 8 tell 9\n\
         seek30 tell 30 next uvw tell 33\n\
         rewind tell 0 next 01\n\
         end tell 36 eof false\n\
         readAtEnd '' eof true\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies a read that outruns the buffer is topped up from the descriptor.
///
/// This is the failure the buffer introduces if the drain simply returns what it holds: with
/// 8190 bytes of an 8192-byte chunk consumed, `fread($h, 100)` answered TWO bytes. php answers
/// 100, because one read is served from its buffer AND the source behind it.
#[test]
fn test_a_read_spanning_the_buffer_boundary_is_topped_up() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("cross.txt", str_repeat("A", 8190) . str_repeat("B", 4000));
$h = fopen("cross.txt", "rb");
$first = fread($h, 8190);
echo "first ", strlen($first), " ", $first[0], "\n";
$second = fread($h, 100);
echo "second ", strlen($second), " ", $second[0], substr($second, -1), "\n";
echo "tell ", ftell($h), " eof ", var_export(feof($h), true), "\n";
$rest = fread($h, 99999);
echo "rest ", strlen($rest), " eof ", var_export(feof($h), true), "\n";
fclose($h);
unlink("cross.txt");
"#,
    );
    assert_eq!(
        out,
        "first 8190 A\nsecond 100 BB\ntell 8290 eof false\nrest 3900 eof true\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `unread_bytes` reports what the buffer still holds.
///
/// php reads a whole chunk and reports the remainder: on a 192-byte file, `fread($h, 5)` leaves
/// 187. This was a hardcoded zero with the comment "elephc keeps no read buffer".
#[test]
fn test_unread_bytes_reports_the_buffer_remainder() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("meta.txt", str_repeat("z", 192));
$h = fopen("meta.txt", "rb");
$m = stream_get_meta_data($h);
echo $m["mode"], " ", $m["unread_bytes"], "\n";
fread($h, 5);
echo stream_get_meta_data($h)["unread_bytes"], "\n";
fseek($h, 0);
echo "after seek ", stream_get_meta_data($h)["unread_bytes"], "\n";
fclose($h);
unlink("meta.txt");
"#,
    );
    assert_eq!(out, "rb 0\n187\nafter seek 0\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `feof()` is judged against the CALLER's request, not the chunk the fill asked for.
///
/// A fill asks for 8192 and almost always gets less, so judging EOF by the fill reported end of
/// file on a stream with plenty left. php: reading a 3-byte stream with `fread($h, 3)` leaves
/// `feof()` FALSE, and reading a 1-byte stream with `fread($h, 2)` leaves it TRUE.
#[test]
fn test_eof_follows_the_callers_request_not_the_fill() {
    let out = compile_and_run(
        r#"<?php
$exact = fopen("php://memory", "w+");
fwrite($exact, "abc");
rewind($exact);
echo fread($exact, 3), "|", var_export(feof($exact), true), "\n";
echo var_export(fread($exact, 1), true), "|", var_export(feof($exact), true), "\n";
fclose($exact);

$short = fopen("php://memory", "w+");
fwrite($short, "x");
rewind($short);
echo fread($short, 2), "|", var_export(feof($short), true), "\n";
fclose($short);
"#,
    );
    assert_eq!(out, "abc|false\n''|true\nx|true\n");
}

/// Verifies a seek inside `stream_get_contents()` discards the buffer.
///
/// Its offset argument seeks without going through `fseek()`, so it needs the discard of its
/// own: without it, `stream_get_contents($h, 2)` followed by `stream_get_contents($h, -1, 4)`
/// answered the buffered remainder AND the bytes at offset 4 — `"cdefef"` where php says `"ef"`.
#[test]
fn test_stream_get_contents_offset_discards_the_buffer() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("x.txt", "abcdef");
$h = fopen("x.txt", "r");
var_dump(stream_get_contents($h, 2));
var_dump(stream_get_contents($h, -1, 4));
fclose($h);
unlink("x.txt");
"#,
    );
    assert_eq!(out, "string(2) \"ab\"\nstring(2) \"ef\"\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `fpassthru()` writes what the buffer holds before touching the descriptor.
///
/// The descriptor has already moved past the buffered bytes, so reading straight from it SKIPPED
/// them: `fread($h, 4); fpassthru($h);` lost the rest of the first chunk.
#[test]
fn test_fpassthru_writes_the_buffered_bytes_first() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("pass.txt", "abcdefghij");
$h = fopen("pass.txt", "rb");
fread($h, 4);
$n = fpassthru($h);
echo "|", $n, "\n";
fclose($h);
unlink("pass.txt");
"#,
    );
    assert_eq!(out, "efghij|6\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies a byte-at-a-time read of a large file stays a fraction of its unbuffered cost.
///
/// The assertion is deliberately loose — a wall-clock budget on shared CI hardware is a flake
/// waiting to happen — but 900 000 syscalls cannot fit in it: the unbuffered path measured
/// 499 ms against php's 14 ms, and the buffered one measured 92 ms.
#[test]
fn test_byte_at_a_time_reading_is_not_one_syscall_per_byte() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
file_put_contents("big.txt", str_repeat("abcdefgh\n", 100000));
$h = fopen("big.txt", "rb");
$t = microtime(true);
$n = 0;
while (($c = fgetc($h)) !== false) {
    $n++;
}
$elapsed = microtime(true) - $t;
fclose($h);
unlink("big.txt");
echo $n, "|", $elapsed < 0.4 ? "fast" : ("SLOW " . round($elapsed, 3)), "\n";
"#,
    );
    assert_eq!(out, "900000|fast\n");
    let _ = std::fs::remove_dir_all(dir);
}
