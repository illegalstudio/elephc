//! Purpose:
//! Integration tests for php's per-stream READ BUFFER as a user-registered wrapper sees it: what a
//! read did not need stays on the stream, and every other reader sees it.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Two defects, both silent, both only reachable through a wrapper. `fgets($h, $n)` ignored its
//!   length bound entirely on that path and answered the WHOLE line — a wrong value that still
//!   looks like a line. And the bytes a bounded read left behind were invisible to `fread`,
//!   `fgetc` and `stream_get_contents`, which answered `""` and `false` for bytes the stream was
//!   holding, while `ftell()` reported the right position throughout.
//! - The same calls on a plain file and on `php://memory` were always correct, which is why this
//!   file drives a wrapper: the defect lives on the one path that reaches `stream_read()`.
//! - Every expectation was MEASURED on `php -n` 8.5.6.
//! - The wrapper deliberately declares `stream_seek` as failing, so nothing here can be answered
//!   by seeking back — the buffer is the only way these bytes can be produced.

use crate::support::*;

/// The wrapper every test below opens: a fixed payload served through `stream_read($count)`.
const WRAPPER: &str = r#"<?php
class Src {
    public $context;
    private int $pos = 0;
    private string $data = "";
    public function stream_open($path, $mode, $options, &$opened) {
        $this->data = "abcdefghij\nklmnopqrst\nuvwxyz\n";
        $this->pos = 0;
        return true;
    }
    public function stream_read($count) {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_eof() { return $this->pos >= strlen($this->data); }
    public function stream_stat() { return []; }
    public function stream_tell() { return $this->pos; }
    public function stream_seek($offset, $whence) { return false; }
}
stream_wrapper_register("src", "Src");
"#;

/// Compiles `WRAPPER` followed by `body` and returns the program's output.
fn run_with_wrapper(body: &str) -> (String, std::path::PathBuf) {
    compile_and_run_in_dir(&format!("{WRAPPER}{body}\n"))
}

/// Verifies `fgets($h, $n)` stops at `$n - 1` bytes on a wrapper, as it does everywhere else.
///
/// It answered the whole 10-byte line plus its newline. `php -n` 8.5.6 answers `"abcde"`.
#[test]
fn test_fgets_honours_its_length_bound_through_a_wrapper() {
    let (out, dir) = run_with_wrapper(r#"$h = fopen("src://x", "r"); var_dump(fgets($h, 6)); fclose($h);"#);
    assert_eq!(out, "string(5) \"abcde\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a second bounded `fgets` continues where the first stopped.
#[test]
fn test_a_second_bounded_fgets_continues_from_the_buffer() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h, 6); var_dump(fgets($h, 6)); fclose($h);"#,
    );
    assert_eq!(out, "string(5) \"fghij\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies an UNBOUNDED `fgets` still reads the whole line — the bound must not become a cap.
#[test]
fn test_an_unbounded_fgets_still_reads_the_whole_line() {
    let (out, dir) = run_with_wrapper(r#"$h = fopen("src://x", "r"); var_dump(fgets($h)); fclose($h);"#);
    assert_eq!(out, "string(11) \"abcdefghij\n\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `fread()` sees what a bounded `fgets()` left on the stream.
///
/// This is the shape that answered `""`: the wrapper branch tail-called `stream_read` without ever
/// consulting the stream's own buffer.
#[test]
fn test_fread_sees_what_a_bounded_fgets_left_behind() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); var_dump(fgets($h, 6)); var_dump(fread($h, 5)); fclose($h);"#,
    );
    assert_eq!(out, "string(5) \"abcde\"\nstring(5) \"fghij\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `fgetc()` sees it too, where it answered php `false`.
#[test]
fn test_fgetc_sees_what_a_bounded_fgets_left_behind() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h, 6); var_dump(fgetc($h)); fclose($h);"#,
    );
    assert_eq!(out, "string(1) \"f\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_contents()` sees it, and reads the rest of the stream after it.
#[test]
fn test_stream_get_contents_sees_the_buffer_then_the_rest() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h, 6); var_dump(stream_get_contents($h)); fclose($h);"#,
    );
    assert_eq!(out, "string(24) \"fghij\nklmnopqrst\nuvwxyz\n\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `ftell()` reports the bytes HANDED BACK, not the bytes pulled from the wrapper.
///
/// This one was already right, and it is what made the others confusing: the position said five
/// while the readers behaved as if the stream were exhausted.
#[test]
fn test_ftell_reports_the_consumed_position_not_the_read_one() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h, 6); var_dump(ftell($h)); fclose($h);"#,
    );
    assert_eq!(out, "int(5)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies a plain `fread` sequence with no `fgets` in it is unchanged.
///
/// The drain was added inside the wrapper branch, so this pins that the ordinary path through it
/// still reaches `stream_read()` and still answers the same bytes.
#[test]
fn test_consecutive_freads_through_a_wrapper_are_unchanged() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); var_dump(fread($h, 5)); var_dump(fread($h, 5)); fclose($h);"#,
    );
    assert_eq!(out, "string(5) \"abcde\"\nstring(5) \"fghij\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `feof()` is FALSE while the stream still holds buffered bytes.
///
/// `stream_eof()` reports the wrapper's OWN position, which a buffered read has already moved
/// past, so this answered `true` on a stream with 24 bytes left. php answers `false`, and the next
/// read proves it by returning them.
#[test]
fn test_feof_is_false_while_the_wrapper_stream_still_holds_bytes() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h, 6); var_dump(feof($h)); var_dump(fgets($h)); fclose($h);"#,
    );
    assert_eq!(out, "bool(false)\nstring(6) \"fghij\n\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `fpassthru()` writes the buffered bytes and everything after them.
///
/// Its wrapper loop drove `feof`/`fread` with the synthetic FD where both take a HANDLE. That
/// worked only because the descriptor lookup maps a synthetic fd to itself — and it hid the stream
/// STATE, so the loop saw EOF at once and answered `int(0)` with 24 bytes still on the stream.
#[test]
fn test_fpassthru_writes_the_buffered_bytes_and_the_rest() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h, 6); var_dump(fpassthru($h)); fclose($h);"#,
    );
    assert_eq!(out, "fghij\nklmnopqrst\nuvwxyz\nint(24)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `fpassthru()` with nothing buffered is unchanged.
///
/// The loop's driving value changed from the fd to the handle, so the ordinary path through it is
/// pinned as well as the buffered one.
#[test]
fn test_fpassthru_from_the_start_is_unchanged() {
    let (out, dir) =
        run_with_wrapper(r#"$h = fopen("src://x", "r"); var_dump(fpassthru($h)); fclose($h);"#);
    assert_eq!(out, "abcdefghij\nklmnopqrst\nuvwxyz\nint(29)\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_line()` reads one delimited record from a wrapper.
#[test]
fn test_stream_get_line_reads_one_record_from_a_wrapper() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); var_dump(stream_get_line($h, 100, "\n")); fclose($h);"#,
    );
    assert_eq!(out, "string(10) \"abcdefghij\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_line()` after an UNBOUNDED `fgets()` reads the NEXT record.
///
/// This answered the right byte COUNT over the wrong buffer, including uninitialised bytes: the
/// entry drain wrote the held bytes into the reserved window, and the wrapper entry then replaced
/// that window with `_user_wrapper_drain_buf` while the running total kept counting them. The
/// unbounded spelling is the one that proves the defect predates the length-bound fix.
#[test]
fn test_stream_get_line_after_an_unbounded_fgets_reads_the_next_record() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h); var_dump(stream_get_line($h, 100, "\n")); fclose($h);"#,
    );
    assert_eq!(out, "string(10) \"klmnopqrst\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies `stream_get_line()` after a BOUNDED `fgets()` starts from the buffered bytes.
///
/// The bytes it takes now pass the delimiter scan one at a time, so the record stops where php's
/// does instead of swallowing the delimiter and everything after it.
#[test]
fn test_stream_get_line_after_a_bounded_fgets_starts_from_the_buffer() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); fgets($h, 6); var_dump(stream_get_line($h, 100, "\n")); fclose($h);"#,
    );
    assert_eq!(out, "string(5) \"fghij\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies two consecutive `stream_get_line()` calls walk consecutive records.
#[test]
fn test_two_stream_get_line_calls_walk_consecutive_records() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); var_dump(stream_get_line($h, 100, "\n")); var_dump(stream_get_line($h, 100, "\n")); fclose($h);"#,
    );
    assert_eq!(out, "string(10) \"abcdefghij\"\nstring(10) \"klmnopqrst\"\n");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Verifies three unbounded `fgets` calls walk the whole stream, line by line.
#[test]
fn test_unbounded_fgets_walks_every_line() {
    let (out, dir) = run_with_wrapper(
        r#"$h = fopen("src://x", "r"); var_dump(fgets($h)); var_dump(fgets($h)); var_dump(fgets($h)); fclose($h);"#,
    );
    assert_eq!(
        out,
        "string(11) \"abcdefghij\n\"\nstring(11) \"klmnopqrst\n\"\nstring(7) \"uvwxyz\n\"\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
