//! Purpose:
//! Integration tests for PHP's `gz*` stream surface, which elephc serves through the
//! `compress.zlib://` wrapper rather than through a second implementation.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The premise these tests defend is an EQUIVALENCE, not a resemblance: php-src implements
//!   `gzopen` as a stream open on the zlib wrapper, so `gzread` IS `fread`, `gzgets` IS `fgets`,
//!   and so on. Each expectation below was measured on `php -n` 8.5.6, and the same programs were
//!   measured in their `compress.zlib://` spelling to confirm the two agree there too.
//! - The archives are written by elephc itself through `compress.zlib://` in write mode, so no
//!   binary fixture is checked in and each test is self-contained. That write path is measured
//!   against php separately — `gzwrite` and `fwrite` on the wrapper produce the same file.
//! - `gzgets($h, null)` and `gzwrite($h, $d, null)` are pinned because they are the reason the
//!   prelude branches on the length instead of forwarding it, AND because forwarding a `?int`
//!   parameter to `fgets` did not compile at all before: `unsupported EIR backend feature: fgets
//!   length for PHP type TaggedScalar`.
//! - `readgzfile()` writes to the output buffer, so its bytes land on `stdout` while its return
//!   value is the count.

use crate::support::*;

/// Verifies the write-then-read round trip through `gzopen`, `gzwrite` and `gzread`.
///
/// `php -n` 8.5.6 answers `int(8)` for the write and the original bytes for the read.
#[test]
fn test_gzopen_write_then_read_round_trips() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("round.gz", "wb9");
var_dump(gzwrite($h, "one\ntwo\n"));
gzclose($h);
$h = gzopen("round.gz", "rb");
var_dump(gzread($h, 100));
var_dump(gzeof($h));
gzclose($h);
"#,
    );
    assert_eq!(out, "int(8)\nstring(8) \"one\ntwo\n\"\nbool(true)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `gzgets` reads a line at a time and `gzgetc` a byte at a time.
#[test]
fn test_gzgets_and_gzgetc_walk_the_stream() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("lines.gz", "w");
gzwrite($h, "one\ntwo\n");
gzclose($h);
$h = gzopen("lines.gz", "r");
var_dump(gzgets($h));
var_dump(gzgetc($h));
var_dump(gzgets($h));
gzclose($h);
"#,
    );
    assert_eq!(out, "string(4) \"one\n\"\nstring(1) \"t\"\nstring(3) \"wo\n\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies an explicit null length reads a whole line, as php's `?int $length = null` says.
///
/// This is the shape that did not compile: a `?int` parameter forwarded to `fgets` reached codegen
/// as a tagged (value, tag) pair no resolver knew.
#[test]
fn test_gzgets_with_a_null_length_reads_a_whole_line() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("nulls.gz", "w");
gzwrite($h, "hello world\n", null);
gzclose($h);
$h = gzopen("nulls.gz", "r");
var_dump(gzgets($h, null));
gzclose($h);
"#,
    );
    assert_eq!(out, "string(12) \"hello world\n\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a bounded `gzgets` stops at `$length - 1` bytes, exactly as `fgets` does.
#[test]
fn test_gzgets_honours_its_length() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("bound.gz", "w");
gzwrite($h, "abcdef\n");
gzclose($h);
$h = gzopen("bound.gz", "r");
var_dump(gzgets($h, 4));
gzclose($h);
"#,
    );
    assert_eq!(out, "string(3) \"abc\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `gztell`, `gzseek` and `gzrewind` agree about the DECOMPRESSED position.
///
/// The position php reports is the offset in the decoded bytes, not in the file — which is what
/// makes the wrapper spelling the right implementation rather than a convenient one.
#[test]
fn test_gzseek_gztell_and_gzrewind_track_the_decoded_position() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("seek.gz", "w");
gzwrite($h, "0123456789");
gzclose($h);
$h = gzopen("seek.gz", "r");
var_dump(gzread($h, 4));
var_dump(gztell($h));
var_dump(gzseek($h, 8));
var_dump(gzread($h, 2));
var_dump(gzrewind($h));
var_dump(gztell($h));
gzclose($h);
"#,
    );
    assert_eq!(
        out,
        "string(4) \"0123\"\nint(4)\nint(0)\nstring(2) \"89\"\nbool(true)\nint(0)\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `gzfile()` splits the decompressed bytes into lines, keeping the terminators.
#[test]
fn test_gzfile_returns_the_decompressed_lines() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("lines2.gz", "w");
gzwrite($h, "alpha\nbeta\n");
gzclose($h);
var_dump(gzfile("lines2.gz"));
"#,
    );
    assert_eq!(
        out,
        "array(2) {\n  [0]=>\n  string(6) \"alpha\n\"\n  [1]=>\n  string(5) \"beta\n\"\n}\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `readgzfile()` writes the decompressed bytes and answers their count.
#[test]
fn test_readgzfile_writes_the_bytes_and_counts_them() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("cat.gz", "w");
gzwrite($h, "alpha\nbeta\n");
gzclose($h);
var_dump(readgzfile("cat.gz"));
"#,
    );
    assert_eq!(out, "alpha\nbeta\nint(11)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `gzpassthru()` writes the rest of the stream from the current position.
#[test]
fn test_gzpassthru_writes_from_the_current_position() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("pass.gz", "w");
gzwrite($h, "0123456789");
gzclose($h);
$h = gzopen("pass.gz", "r");
gzread($h, 4);
var_dump(gzpassthru($h));
gzclose($h);
"#,
    );
    assert_eq!(out, "456789int(6)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `gzputs()` is `gzwrite()`, which is what php's alias means.
#[test]
fn test_gzputs_is_gzwrite() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("puts.gz", "w");
var_dump(gzputs($h, "abc"));
gzclose($h);
var_dump(file_get_contents("compress.zlib://puts.gz"));
"#,
    );
    assert_eq!(out, "int(3)\nstring(3) \"abc\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A compress:// stream reports the end as soon as a LINE read drains it; a plain file does not.
///
/// MEASURED on `php -n` 8.5.6 with `"one\ntwo\nthree\n"`, `feof()` after each `fgets()`:
///
/// ```text
/// compress.zlib://   false, false, TRUE
/// a plain file       false, false, false
/// ```
///
/// and the sized readers agree with each other on both: `fread`, `fgetc` and
/// `stream_get_contents` never see it early. It is the line reader alone, which is the signature
/// `php://temp` already had — php fills a whole chunk to find a line, and for a stream php
/// filters that fill drives the source one read past its last byte.
///
/// `gzeof()` IS `feof()`, so a `while (!gzeof($h)) { gzgets($h); }` loop ran one extra round
/// answering false before this.
#[test]
fn test_gzeof_turns_true_on_the_line_that_drains_the_stream() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
$h = gzopen("eof.gz", "w");
gzwrite($h, "one
two
three
");
gzclose($h);

$g = gzopen("eof.gz", "r");
$parts = [];
while (($l = gzgets($g)) !== false) { $parts[] = trim($l) . "=" . var_export(gzeof($g), true); }
gzclose($g);

file_put_contents("eof.txt", "one
two
three
");
$p = fopen("eof.txt", "rb");
while (($l = fgets($p)) !== false) { $parts[] = trim($l) . "=" . var_export(feof($p), true); }
fclose($p);
echo implode("|", $parts);
"#,
    );
    assert_eq!(
        out,
        "one=false|two=false|three=true|one=false|two=false|three=false"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a failed `gzopen` answers php's `false` rather than a resource.
///
/// The WARNING php prints alongside is not asserted: this implementation warns in the words of the
/// `fopen` it is built from and at the prelude's own line, a divergence recorded in
/// `crate::gz_prelude`'s module doc. The value is what a program branches on.
#[test]
fn test_gzopen_on_a_missing_file_is_false() {
    let (out, dir) = compile_and_run_in_dir(
        r#"<?php
var_dump(@gzopen("no_such_archive.gz", "r"));
"#,
    );
    assert_eq!(out, "bool(false)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a program that never names a `gz*` function does not carry the prelude.
///
/// Pay-for-use is the reason a fourteen-function surface can be injected at all, so it is pinned
/// rather than assumed: the emitted assembly of a program with no reference names none of them.
#[test]
fn test_a_program_without_gz_calls_carries_no_gz_declarations() {
    let dir = make_cli_test_dir("elephc_gz_prelude_pay_for_use");
    let (user_asm, _runtime_asm, _required_libraries) = compile_source_to_asm_with_options(
        "<?php var_dump(strlen(\"hello\"));\n",
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        !user_asm.contains("gzopen") && !user_asm.contains("gzgets"),
        "the gz prelude leaked into a program that never mentions it"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
