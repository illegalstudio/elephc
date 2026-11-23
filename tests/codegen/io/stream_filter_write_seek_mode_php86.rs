//! Purpose:
//! PHP 8.6 stream-filter `$params` fixtures: the `write_seek_mode` validation and
//! `stream_select()` on a filtered stream.
//!
//! Called from:
//! - `cargo test --test codegen_tests stream_filter_write_seek_mode_php86`.
//!
//! Key details:
//! - The invalid-mode fixture captures the two php-src warnings through
//!   `set_error_handler`, so stdout stays deterministic.
//! - The array-form compression level is observed through the compressed file size:
//!   level 0 stores the payload, level 9 shrinks it.

use elephc::php_version::PhpVersion;

use crate::support::*;

/// Compiles and runs one fixture with the PHP 8.6 profile.
fn run86(source: &str) -> String {
    compile_and_run_with_php_version(source, PhpVersion::Php86)
}

/// An invalid `write_seek_mode` emits php-src's two warnings and rejects the filter.
#[test]
fn test_stream_filter_invalid_write_seek_mode_php86() {
    let out = run86(
        r#"<?php
$f = fopen("seek.bin", "w");
set_error_handler(static function (int $severity, string $message): bool {
    echo $message, "\n";
    return true;
});
$result = stream_filter_append($f, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'rewind']);
restore_error_handler();
echo $result === false ? "false" : "true";
fclose($f);
"#,
    );
    assert_eq!(
        out,
        "stream_filter_append(): \"write_seek_mode\" filter parameter must be one of \"preserve\", \"reset\", or \"strict\"\nstream_filter_append(): Unable to create or locate filter \"zlib.deflate\"\nfalse"
    );
}

/// Every accepted `write_seek_mode` value keeps the filter attachable.
#[test]
fn test_stream_filter_valid_write_seek_modes_php86() {
    let out = run86(
        r#"<?php
$results = [];
foreach (['preserve', 'reset', 'strict'] as $mode) {
    $f = fopen($mode . ".bin", "w");
    $filter = stream_filter_append($f, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => $mode]);
    $results[] = $filter !== false ? "ok" : "bad";
    fclose($f);
}
echo implode(",", $results);
"#,
    );
    assert_eq!(out, "ok,ok,ok");
}

/// The associative `$params` compression level reaches the deflate initializer.
#[test]
fn test_stream_filter_array_level_is_honored_php86() {
    let out = run86(
        r#"<?php
$payload = str_repeat("abcdefgh", 128);
$f = fopen("level0.bin", "w");
stream_filter_append($f, "zlib.deflate", STREAM_FILTER_WRITE, ['level' => 0]);
fwrite($f, $payload);
fclose($f);
$g = fopen("level9.bin", "w");
stream_filter_append($g, "zlib.deflate", STREAM_FILTER_WRITE, ['level' => 9]);
fwrite($g, $payload);
fclose($g);
echo filesize("level0.bin") > filesize("level9.bin") ? "stored>deflated" : "equal";
"#,
    );
    assert_eq!(out, "stored>deflated");
}

/// A filtered stream can still be cast to its descriptor for `stream_select()`.
#[test]
fn test_stream_select_on_filtered_stream_php86() {
    let out = run86(
        r#"<?php
$pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, 0);
$reader = $pair[0];
$writer = $pair[1];
stream_filter_append($reader, "string.toupper", STREAM_FILTER_READ);
fwrite($writer, "ping");
$read = [$reader];
$write = [];
$except = [];
$ready = stream_select($read, $write, $except, 1, 0);
echo $ready, ":", fread($reader, 4);
fclose($reader);
fclose($writer);
"#,
    );
    assert_eq!(out, "1:PING");
}

/// PHP 8.6 `write_seek_mode` is enforced by `fseek()`: strict rejects every seek, reset
/// allows only the start position, and preserve allows any offset.
#[test]
fn test_stream_filter_write_seek_mode_seek_rules_php86() {
    let out = run86(
        r#"<?php
$text = str_repeat("abcdefgh", 64);
$f = fopen("strict.bin", "w+");
stream_filter_append($f, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'strict']);
fwrite($f, $text);
echo @fseek($f, 0, 0), "|";
echo @fseek($f, 5, 0), "|";
fclose($f);
$g = fopen("reset.bin", "w+");
stream_filter_append($g, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'reset']);
fwrite($g, $text);
echo @fseek($g, 5, 0), "|";
echo @fseek($g, 0, 0), "|";
fclose($g);
$h = fopen("preserve.bin", "w+");
stream_filter_append($h, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'preserve']);
fwrite($h, $text);
echo @fseek($h, 5, 0);
fclose($h);
"#,
    );
    assert_eq!(out, "-1|-1|-1|0|0");
}

/// The rejected seeks emit php-src's filter-labelled warnings.
#[test]
fn test_stream_filter_write_seek_mode_warnings_php86() {
    let out = run86(
        r#"<?php
set_error_handler(static function (int $severity, string $message): bool {
    echo $message, "\n";
    return true;
});
$f = fopen("messages.bin", "w+");
stream_filter_append($f, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'strict']);
fseek($f, 0, 0);
fclose($f);
$g = fopen("messages2.bin", "w+");
stream_filter_append($g, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'reset']);
fseek($g, 5, 0);
fclose($g);
restore_error_handler();
"#,
    );
    assert_eq!(
        out,
        "fseek(): Stream filter zlib.deflate is never seekable\nfseek(): Stream filter zlib.deflate is seekable only to start position\n"
    );
}

/// An allowed seek resets the deflate state: after `ftruncate(0)` + `fseek(0)`, writes form
/// a fresh compressed stream, as php-src's `php_zlib_deflate_seek` does with `deflateReset`.
#[test]
fn test_stream_filter_write_seek_mode_resets_deflate_state_php86() {
    let out = run86(
        r#"<?php
$text1 = "First message that will be discarded.";
$text2 = "Second message that replaces the first.";
$fp = fopen("reset.bin", "w+");
stream_filter_append($fp, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'reset']);
fwrite($fp, $text1);
ftruncate($fp, 0);
echo fseek($fp, 0, 0) === 0 ? "seeked|" : "bad|";
fwrite($fp, $text2);
fclose($fp);
$rd = fopen("reset.bin", "r");
stream_filter_append($rd, "zlib.inflate", STREAM_FILTER_READ);
echo stream_get_contents($rd) === $text2 ? "reset-ok" : "reset-bad";
fclose($rd);
"#,
    );
    assert_eq!(out, "seeked|reset-ok");
}

/// The same reset semantics apply to `bzip2.compress`: an allowed seek ends and
/// reinitializes the compression stream, so the new writes decode to the new payload.
#[test]
fn test_stream_filter_write_seek_mode_resets_bzip2_state_php86() {
    let out = run86(
        r#"<?php
$text1 = "First message that will be discarded.";
$text2 = "Second message that replaces the first.";
$fp = fopen("reset.bz2", "w+");
stream_filter_append($fp, "bzip2.compress", STREAM_FILTER_WRITE, ['write_seek_mode' => 'reset']);
fwrite($fp, $text1);
ftruncate($fp, 0);
echo fseek($fp, 0, 0) === 0 ? "seeked|" : "bad|";
fwrite($fp, $text2);
fclose($fp);
$rd = fopen("reset.bz2", "r");
stream_filter_append($rd, "bzip2.decompress", STREAM_FILTER_READ);
echo stream_get_contents($rd) === $text2 ? "reset-ok" : "reset-bad";
fclose($rd);
"#,
    );
    assert_eq!(out, "seeked|reset-ok");
}

/// String, convert and iconv write filters record the same seekability rule.
#[test]
fn test_stream_filter_write_seek_mode_string_convert_iconv_php86() {
    let out = run86(
        r#"<?php
$f = fopen("str.bin", "w+");
stream_filter_append($f, "string.toupper", STREAM_FILTER_WRITE, ['write_seek_mode' => 'strict']);
fwrite($f, "abc");
echo @fseek($f, 0, 0), "|";
fclose($f);
$g = fopen("conv.bin", "w+");
stream_filter_append($g, "convert.base64-encode", STREAM_FILTER_WRITE, ['write_seek_mode' => 'reset']);
fwrite($g, "abc");
echo @fseek($g, 5, 0), "|";
echo @fseek($g, 0, 0), "|";
fclose($g);
$h = fopen("iconv.bin", "w+");
stream_filter_append($h, "convert.iconv.UTF-8/UTF-16LE", STREAM_FILTER_WRITE, ['write_seek_mode' => 'strict']);
fwrite($h, "abc");
echo @fseek($h, 0, 0);
fclose($h);
"#,
    );
    assert_eq!(out, "-1|-1|0|-1");
}

/// `rewind()` consults the mode too: strict rejects it, reset allows it.
#[test]
fn test_stream_filter_write_seek_mode_rewind_php86() {
    let out = run86(
        r#"<?php
$f = fopen("rw.bin", "w+");
stream_filter_append($f, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'strict']);
fwrite($f, "abc");
echo @rewind($f) ? "true" : "false", "|";
fclose($f);
$g = fopen("rw2.bin", "w+");
stream_filter_append($g, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'reset']);
fwrite($g, "abc");
echo @rewind($g) ? "true" : "false";
fclose($g);
"#,
    );
    assert_eq!(out, "false|true");
}

/// Read-side seekability: the decompressors and the convert/iconv read filters are
/// start-only, matching php-src's `PSFS_SEEKABLE_START` read flag.
#[test]
fn test_stream_filter_read_seekability_php86() {
    let out = run86(
        r#"<?php
file_put_contents("x.txt", "hello world");
$f = fopen("x.txt", "r");
stream_filter_append($f, "convert.iconv.UTF-8/UTF-16LE");
fread($f, 4);
echo @fseek($f, 4, 0), "|";
echo @fseek($f, 0, 0), "|";
fclose($f);
$w = fopen("z.bin", "w");
stream_filter_append($w, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($w, "hello");
fclose($w);
$r = fopen("z.bin", "r");
stream_filter_append($r, "zlib.inflate", STREAM_FILTER_READ);
echo @fseek($r, 5, 0), "|";
echo @fseek($r, 0, 0);
fclose($r);
"#,
    );
    assert_eq!(out, "-1|0|-1|0");
}

/// The start-only read rejection names the filter's php-src label.
#[test]
fn test_stream_filter_read_seekability_warnings_php86() {
    let out = run86(
        r#"<?php
set_error_handler(static function (int $severity, string $message): bool {
    echo $message, "\n";
    return true;
});
file_put_contents("x.txt", "hello world");
$f = fopen("x.txt", "r");
stream_filter_append($f, "convert.iconv.UTF-8/UTF-16LE");
fseek($f, 4, 0);
fclose($f);
$w = fopen("z.bin", "w");
stream_filter_append($w, "zlib.deflate", STREAM_FILTER_WRITE);
fwrite($w, "hello");
fclose($w);
$r = fopen("z.bin", "r");
stream_filter_append($r, "zlib.inflate", STREAM_FILTER_READ);
fseek($r, 5, 0);
fclose($r);
restore_error_handler();
"#,
    );
    assert_eq!(
        out,
        "fseek(): Stream filter convert.iconv.* is seekable only to start position\nfseek(): Stream filter zlib.inflate is seekable only to start position\n"
    );
}

/// An allowed rewind also resets the filter state, so the replacement payload written
/// after `ftruncate(0)` + `rewind()` decodes exactly.
#[test]
fn test_stream_filter_write_seek_mode_resets_state_on_rewind_php86() {
    let out = run86(
        r#"<?php
$text1 = "First message that will be discarded.";
$text2 = "Second message that replaces the first.";
$fp = fopen("rwreset.bin", "w+");
stream_filter_append($fp, "zlib.deflate", STREAM_FILTER_WRITE, ['write_seek_mode' => 'reset']);
fwrite($fp, $text1);
ftruncate($fp, 0);
echo rewind($fp) ? "rewound|" : "bad|";
fwrite($fp, $text2);
fclose($fp);
$rd = fopen("rwreset.bin", "r");
stream_filter_append($rd, "zlib.inflate", STREAM_FILTER_READ);
echo stream_get_contents($rd) === $text2 ? "reset-ok" : "reset-bad";
fclose($rd);
"#,
    );
    assert_eq!(out, "rewound|reset-ok");
}
