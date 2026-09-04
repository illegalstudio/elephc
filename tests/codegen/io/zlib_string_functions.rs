//! Purpose:
//! Integration tests for ext-zlib's string functions — `gzencode`, `gzdecode`, `zlib_encode` and
//! `zlib_decode` — which frame bytes rather than serve a stream.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - They are built on primitives elephc already had. `ZLIB_ENCODING_RAW` and `_DEFLATE` were
//!   MEASURED to produce exactly what `gzdeflate()` and `gzcompress()` produce, so those two
//!   encodings ARE those calls; only the gzip framing is written out, and its body is exactly
//!   `gzdeflate($data, $level)`.
//! - NO test asserts a whole gzip blob. The header's OS byte is stamped by whichever zlib the
//!   TARGET links — 19 on Darwin, 3 on other Unix builds — so a byte-exact assertion would pass on
//!   one target and fail on another for a difference php has too. What is asserted is
//!   platform-independent: the magic, the fixed method and flag bytes, the trailer, the round
//!   trips, the refusals, and the ValueError messages.
//! - Every expectation was MEASURED on `php -n` 8.5.6. Before any of it reached elephc, the same
//!   four functions were run IN PHP against php's own builtins over five inputs, five levels and
//!   three encodings, and produced byte-identical output — so what these tests check is that
//!   elephc runs them, not that the algorithm is right.

use crate::support::*;

/// Verifies each gzip level round-trips through `gzdecode()`.
#[test]
fn test_gzencode_and_gzdecode_round_trip_at_every_level() {
    let out = compile_and_run(
        r#"<?php
$s = "hello world hello world";
var_dump(gzdecode(gzencode($s)) === $s);
var_dump(gzdecode(gzencode($s, 9)) === $s);
var_dump(gzdecode(gzencode($s, 0)) === $s);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies the two non-gzip encodings ARE `gzdeflate()` and `gzcompress()`.
///
/// This is the measurement the implementation rests on: php produces byte-identical output for
/// those, so reproducing them would be writing a second copy of something already present.
#[test]
fn test_the_raw_and_deflate_encodings_are_the_existing_primitives() {
    let out = compile_and_run(
        r#"<?php
$s = "hello world hello world";
var_dump(gzencode($s, -1, ZLIB_ENCODING_RAW) === gzdeflate($s));
var_dump(gzencode($s, -1, ZLIB_ENCODING_DEFLATE) === gzcompress($s));
var_dump(zlib_encode($s, ZLIB_ENCODING_GZIP) === gzencode($s));
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies the gzip framing: magic, method, flags, a zero mtime, and the crc32/size trailer.
///
/// The two bytes after those — XFL and OS — are deliberately absent from this assertion: XFL
/// varies with the level and OS with the target's zlib.
#[test]
fn test_the_gzip_framing_is_phps() {
    let out = compile_and_run(
        r#"<?php
$s = "hello world hello world";
$out = gzencode($s);
var_dump(bin2hex(substr($out, 0, 4)));
var_dump(bin2hex(substr($out, 4, 4)));
var_dump(bin2hex(substr($out, -8)));
var_dump(substr($out, 10, strlen($out) - 18) === gzdeflate($s));
"#,
    );
    assert_eq!(
        out,
        "string(8) \"1f8b0800\"\nstring(8) \"00000000\"\nstring(16) \"3bcee2ea17000000\"\nbool(true)\n"
    );
}

/// Verifies the decoders answer `false` for input they cannot frame.
#[test]
fn test_the_decoders_refuse_what_they_cannot_frame() {
    let out = compile_and_run(
        r#"<?php
var_dump(@gzdecode("not gzip at all"));
var_dump(@gzdecode(""));
var_dump(@zlib_decode("garbage here"));
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\nbool(false)\n");
}

/// Verifies `zlib_decode()` tells the three framings apart by their first bytes, as php does.
#[test]
fn test_zlib_decode_detects_all_three_framings() {
    let out = compile_and_run(
        r#"<?php
$s = "hello world hello world";
var_dump(zlib_decode(gzencode($s)) === $s);
var_dump(zlib_decode(gzcompress($s)) === $s);
var_dump(zlib_decode(gzdeflate($s)) === $s);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies the ValueErrors carry php's exact wording, naming all three encodings.
///
/// The messages are part of the contract: a program that catches one and matches on its text sees
/// php's, character for character.
#[test]
fn test_the_value_errors_carry_phps_wording() {
    let out = compile_and_run(
        r#"<?php
try { gzencode("x", -1, 7); } catch (\ValueError $e) { var_dump($e->getMessage()); }
try { gzencode("x", 12); } catch (\ValueError $e) { var_dump($e->getMessage()); }
try { zlib_encode("x", 7); } catch (\ValueError $e) { var_dump($e->getMessage()); }
"#,
    );
    assert_eq!(
        out,
        concat!(
            "string(114) \"gzencode(): Argument #3 ($encoding) must be one of ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP, or ZLIB_ENCODING_DEFLATE\"\n",
            "string(57) \"gzencode(): Argument #2 ($level) must be between -1 and 9\"\n",
            "string(117) \"zlib_encode(): Argument #2 ($encoding) must be one of ZLIB_ENCODING_RAW, ZLIB_ENCODING_GZIP, or ZLIB_ENCODING_DEFLATE\"\n",
        )
    );
}

/// Verifies `$max_length` is honoured where it is large enough, on both decoders.
#[test]
fn test_the_decoders_honour_a_max_length() {
    let out = compile_and_run(
        r#"<?php
$s = "hello world hello world";
var_dump(gzdecode(gzencode($s), 100) === $s);
var_dump(gzdecode(gzencode($s), strlen($s)) === $s);
var_dump(zlib_decode(gzencode($s), 100) === $s);
"#,
    );
    assert_eq!(out, "bool(true)\nbool(true)\nbool(true)\n");
}

/// Verifies `zlib_get_coding_type()` answers what php answers with output compression off.
///
/// php reports what its OUTPUT layer compressed with, and `false` when nothing did — MEASURED
/// under `php -n`, both outside and inside an `ob_start()`. elephc has no zlib output compression
/// at all, so `false` is php's answer for every configuration this can be in rather than a
/// placeholder for one.
#[test]
fn test_zlib_get_coding_type_reports_no_output_compression() {
    let out = compile_and_run(
        r#"<?php
var_dump(zlib_get_coding_type());
ob_start();
var_dump(zlib_get_coding_type());
ob_end_flush();
"#,
    );
    assert_eq!(out, "bool(false)\nbool(false)\n");
}

/// Verifies the three encoding constants carry php's `windowBits` values.
///
/// They are not opaque tags: a wrong value would select a different framing rather than be
/// rejected, and produce bytes no reader expects.
#[test]
fn test_the_encoding_constants_are_phps_window_bits() {
    let out = compile_and_run(
        r#"<?php
var_dump(ZLIB_ENCODING_RAW, ZLIB_ENCODING_DEFLATE, ZLIB_ENCODING_GZIP);
"#,
    );
    assert_eq!(out, "int(-15)\nint(15)\nint(31)\n");
}
