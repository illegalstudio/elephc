//! Purpose:
//! Integration tests for php's rule that a stream asks its SOURCE for a whole chunk, whatever the
//! caller asked for, and keeps the surplus in the stream's own read buffer.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - The wrapper here PRINTS every `$count` it is handed, and that output is the property under
//!   test. Asserting bytes alone would prove nothing: `fread($h, 5)` answered the right five bytes
//!   before this rule was implemented — it simply asked the wrapper for five, twice, where php
//!   asks for 8192 once and serves the second call from its buffer with no call at all.
//! - A wrapper can see the difference. It may count calls, size its own reads against the count,
//!   or pay per call — a network wrapper asked for one byte at a time is a different program from
//!   one asked for 8192.
//! - 8192 is php's default chunk. `fgets` already asked for it; these tests pin `fread`, `fgetc`
//!   and a bounded `stream_get_contents` doing the same, and `fgets` alongside them so the four
//!   cannot drift apart.
//! - Every expectation was MEASURED on `php -n` 8.5.6, comparing the wrapper's log line for line.

use crate::support::*;

/// A wrapper that logs the byte count of every `stream_read()` it is asked for.
const LOUD: &str = r#"<?php
class Loud {
    public $context;
    private int $pos = 0;
    private string $data = "";
    public function stream_open($path, $mode, $options, &$opened) {
        $this->data = str_repeat("abcdefghij", 10);
        $this->pos = 0;
        return true;
    }
    public function stream_read($count) {
        echo "read(", $count, ")\n";
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_eof() { return $this->pos >= strlen($this->data); }
    public function stream_stat() { return []; }
    public function stream_tell() { return $this->pos; }
    public function stream_seek($offset, $whence) { return false; }
}
stream_wrapper_register("loud", "Loud");
"#;

/// Compiles `LOUD` followed by `body` and returns the program's output.
fn run_loud(body: &str) -> (String, std::path::PathBuf) {
    compile_and_run_in_dir(&format!("{LOUD}{body}\n"))
}

/// Verifies a short `fread()` asks the wrapper for a whole chunk.
#[test]
fn test_a_short_fread_asks_the_wrapper_for_a_whole_chunk() {
    let (out, dir) = run_loud(r#"$h = fopen("loud://x", "r"); var_dump(fread($h, 5)); fclose($h);"#);
    assert_eq!(out, "read(8192)\nstring(5) \"abcde\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies the SECOND short `fread()` is served from the buffer, with no second call.
///
/// This is the half that a byte-count assertion alone would miss: both calls answered the right
/// bytes before, at the cost of two trips into user code.
#[test]
fn test_a_second_short_fread_makes_no_further_wrapper_call() {
    let (out, dir) = run_loud(
        r#"$h = fopen("loud://x", "r"); var_dump(fread($h, 5)); var_dump(fread($h, 5)); fclose($h);"#,
    );
    assert_eq!(
        out,
        "read(8192)\nstring(5) \"abcde\"\nstring(5) \"fghij\"\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `fgetc()` asks for a chunk rather than one byte, and the next one costs no call.
#[test]
fn test_fgetc_asks_for_a_chunk_not_a_byte() {
    let (out, dir) = run_loud(
        r#"$h = fopen("loud://x", "r"); var_dump(fgetc($h)); var_dump(fgetc($h)); fclose($h);"#,
    );
    assert_eq!(out, "read(8192)\nstring(1) \"a\"\nstring(1) \"b\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a bounded `stream_get_contents()` asks for a chunk, not for its bound.
#[test]
fn test_a_bounded_stream_get_contents_asks_for_a_chunk() {
    let (out, dir) = run_loud(
        r#"$h = fopen("loud://x", "r"); var_dump(strlen(stream_get_contents($h, 12))); fclose($h);"#,
    );
    assert_eq!(out, "read(8192)\nint(12)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a SEEK throws the buffered bytes away.
///
/// Buffering a chunk gives the stream bytes from AHEAD of wherever the caller seeks to next, and
/// serving them afterwards is a wrong answer made of valid bytes: `fread($h, 5)` then
/// `fseek($h, 3)` then `fread($h, 4)` answered bytes 7..10 out of the stale buffer. php discards
/// the buffer and reads again, which the SECOND `read(8192)` in the expected output is the
/// evidence for — this wrapper cannot actually seek, so that read finds nothing left.
#[test]
fn test_a_seek_throws_the_buffered_bytes_away() {
    let (out, dir) = run_loud(
        r#"$h = fopen("loud://x", "r"); var_dump(fread($h, 5)); fseek($h, 3); var_dump(fread($h, 4)); fclose($h);"#,
    );
    assert_eq!(
        out,
        "read(8192)\nstring(5) \"abcde\"\nread(8192)\nstring(0) \"\"\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `rewind()` throws the buffered bytes away too, since it IS a seek.
#[test]
fn test_a_rewind_throws_the_buffered_bytes_away() {
    let (out, dir) = run_loud(
        r#"$h = fopen("loud://x", "r"); var_dump(fread($h, 5)); rewind($h); var_dump(fread($h, 4)); fclose($h);"#,
    );
    assert_eq!(
        out,
        "read(8192)\nstring(5) \"abcde\"\nread(8192)\nstring(0) \"\"\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `fgets()` still asks for a chunk, so the four readers agree.
#[test]
fn test_a_bounded_fgets_still_asks_for_a_chunk() {
    let (out, dir) = run_loud(r#"$h = fopen("loud://x", "r"); var_dump(fgets($h, 6)); fclose($h);"#);
    assert_eq!(out, "read(8192)\nstring(5) \"abcde\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}
