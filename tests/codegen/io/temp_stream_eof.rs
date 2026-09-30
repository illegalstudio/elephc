//! Purpose:
//! Integration tests for WHEN a stream reports end of file, which is not the same question for
//! every kind of stream.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - `php://temp` reports EOF one read EARLIER than every other stream, and only for a LINE read.
//!   MEASURED on `php -n` 8.5.6 over `"a\nbb\n"`: `fgets()` returning the final `"bb\n"` leaves
//!   `feof()` TRUE on `php://temp` and FALSE on `php://memory` and on a plain file. `fread()` and
//!   `fgetc()` never differ, on any of the three.
//! - The reason is php-src's plumbing rather than a rule about temporary files: `php://temp`
//!   WRAPS an inner memory stream and copies that stream's `eof` after every read, and a line read
//!   asks for a whole chunk — driving the inner stream one read past its last byte. A sized
//!   `fread()` stops as soon as it has what was asked for and never makes that extra read.
//! - So it is not a size effect either: a first line of 9001 bytes, well past the chunk, answers
//!   exactly the same way.

use crate::support::*;

/// Verifies the three stream kinds agree, and disagree, exactly where php says they do.
#[test]
fn only_a_temp_stream_reports_eof_when_a_line_read_drains_it() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
function mk(string $kind, string $body) {
    $h = $kind === "plain" ? fopen("k.txt", "w+") : fopen("php://$kind", "r+");
    fwrite($h, $body);
    rewind($h);
    return $h;
}
foreach (["memory", "temp", "plain"] as $k) {
    $h = mk($k, "a\nbb\n");
    fgets($h);
    echo $k, " line1=", var_export(feof($h), true);
    fgets($h);
    echo " line2=", var_export(feof($h), true);

    $r = mk($k, "abcde");
    fread($r, 3);
    echo " read3=", var_export(feof($r), true);
    fread($r, 2);
    echo " read5=", var_export(feof($r), true);

    $c = mk($k, "ab");
    fgetc($c);
    fgetc($c);
    echo " char2=", var_export(feof($c), true), "\n";
}
"#,
    );
    assert_eq!(
        out,
        "memory line1=false line2=false read3=false read5=false char2=false\n\
         temp line1=false line2=true read3=false read5=false char2=false\n\
         plain line1=false line2=false read3=false read5=false char2=false\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies `stream_get_line()` follows `fgets()`, and that a seek takes the answer back.
#[test]
fn a_delimited_read_follows_fgets_and_a_seek_undoes_it() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
foreach (["memory", "temp"] as $k) {
    $h = fopen("php://$k", "r+");
    fwrite($h, "a\nbb\n");
    rewind($h);
    stream_get_line($h, 100, "\n");
    stream_get_line($h, 100, "\n");
    echo $k, " afterLast=", var_export(feof($h), true);
    fseek($h, 0);
    echo " afterSeek=", var_export(feof($h), true), "\n";
}
"#,
    );
    assert_eq!(out, "memory afterLast=false afterSeek=false\n\
                     temp afterLast=true afterSeek=false\n");
    let _ = std::fs::remove_dir_all(dir);
}

/// Verifies a line longer than the chunk does not change the answer.
///
/// The extra read is what sets EOF, not the buffer running out — so a stream whose first line
/// needs several fills still reports it only on the line that reaches the end.
#[test]
fn a_line_past_the_chunk_size_answers_the_same_way() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
foreach (["memory", "temp"] as $k) {
    $h = fopen("php://$k", "r+");
    fwrite($h, str_repeat("x", 9000) . "\n" . "tail\n");
    rewind($h);
    $one = fgets($h);
    echo $k, " len=", strlen($one), " after1=", var_export(feof($h), true);
    fgets($h);
    echo " after2=", var_export(feof($h), true), "\n";
}
"#,
    );
    assert_eq!(out, "memory len=9001 after1=false after2=false\n\
                     temp len=9001 after1=false after2=true\n");
    let _ = std::fs::remove_dir_all(dir);
}
