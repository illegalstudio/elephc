//! Purpose:
//! Integration tests for WHEN php asks a userspace wrapper about end-of-file, and when it answers
//! from what it was already told.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A wrapper cannot set the stream's end-of-file state itself, so php asks `stream_eof()`
//!   straight after every `stream_read()` and keeps the answer. `feof()` reads that rather than
//!   asking the class again; a seek is what clears it.
//! - elephc asked nothing after its read and asked the WRAPPER at every `feof()`. The values were
//!   always right — this is about the conversation the class actually sees, which is what a
//!   wrapper with side effects or a cost per call observes.
//! - Every expectation MEASURED on `php -n` 8.5.6.

use crate::support::*;

/// A wrapper that announces every call it receives, with the position it is at.
const WRAPPER: &str = r#"<?php
class T {
    public $context;
    public $pos = 0;
    public $data = "abcdefghij";
    public function stream_open($p, $m, $o, &$x) { return true; }
    public function stream_read($n) {
        $r = substr($this->data, $this->pos, $n);
        $this->pos += strlen($r);
        echo "read\n";
        return $r;
    }
    public function stream_eof() { echo "eof\n"; return $this->pos >= strlen($this->data); }
    public function stream_seek($o, $w) { echo "seek\n"; $this->pos = $o; return true; }
    public function stream_tell() { return $this->pos; }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("tw", "T");
"#;

/// Compiles `WRAPPER` followed by `body` and returns only the calls the class saw.
fn calls(body: &str) -> Vec<String> {
    let out = compile_and_run_capture(&format!("{WRAPPER}{body}\n"));
    assert!(out.success, "program failed: {}", out.stderr);
    out.stdout
        .lines()
        .filter(|l| matches!(*l, "read" | "eof" | "seek"))
        .map(str::to_string)
        .collect()
}

/// Verifies a read is followed by the question, and a second read served from the buffer is not.
///
/// elephc asked nothing here: the class was read and never told what php told it.
#[test]
fn test_a_read_is_followed_by_the_question() {
    assert_eq!(
        calls(r#"$h = fopen("tw://x", "r"); fread($h, 4); fread($h, 3); fclose($h);"#),
        vec!["read", "eof"],
    );
}

/// Verifies `feof()` answers from what the read was told, instead of asking again.
///
/// The whole-file readers drain the stream and then ask whether it is done; php already knows.
#[test]
fn test_feof_answers_from_what_the_read_was_told() {
    assert_eq!(
        calls(r#"$h = fopen("tw://x", "r"); fread($h, 20); var_dump(feof($h)); fclose($h);"#),
        vec!["read", "eof"],
    );
}

/// Verifies `feof()` on a stream nothing has read yet DOES ask — there is nothing to answer from.
#[test]
fn test_feof_before_any_read_asks_the_class() {
    assert_eq!(
        calls(r#"$h = fopen("tw://x", "r"); var_dump(feof($h)); fclose($h);"#),
        vec!["eof"],
    );
}

/// Verifies a seek makes the next `feof()` ask again.
///
/// The remembered answer describes a position the stream has left, so php discards it. Without
/// this the flag would outlive its own truth.
#[test]
fn test_a_seek_makes_the_next_feof_ask_again() {
    assert_eq!(
        calls(
            r#"$h = fopen("tw://x", "r"); fread($h, 20); fseek($h, 0); var_dump(feof($h)); fclose($h);"#
        ),
        vec!["read", "eof", "seek", "eof"],
    );
}

/// Verifies `file_get_contents()` sees one read and one question per fill.
#[test]
fn test_a_whole_file_read_asks_once_per_fill() {
    assert_eq!(
        calls(r#"file_get_contents("tw://y");"#),
        vec!["read", "eof", "read", "eof"],
    );
}

/// Verifies a wrapper that hands back SMALL pieces is not declared finished by the size of them.
///
/// The chunked reader judges a short read as end-of-file, which is the only thing backends that
/// cannot be asked have. A class CAN be asked, and answering from the guess instead would stop
/// `file_get_contents()` after the first three bytes.
#[test]
fn test_a_wrapper_that_hands_back_small_pieces_is_not_cut_short() {
    let out = compile_and_run_capture(
        r#"<?php
class S {
    public $context;
    public $pos = 0;
    public $data = "abcdefghij";
    public function stream_open($p, $m, $o, &$x) { return true; }
    public function stream_read($n) {
        $r = substr($this->data, $this->pos, min($n, 3));
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_eof() { return $this->pos >= strlen($this->data); }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("sw", "S");
var_dump(file_get_contents("sw://x"));
$h = fopen("sw://x", "r");
$out = "";
while (!feof($h)) { $out .= fread($h, 10); }
var_dump($out);
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(10) \"abcdefghij\"\nstring(10) \"abcdefghij\"\n"
    );
}

/// Verifies `fread($h, 4)` answers FOUR bytes while the source still has them.
///
/// php tops its holding area up before serving: when it holds less than the caller asked for, it
/// asks the source again. elephc answered whatever happened to be held, so a wrapper handing back
/// 6 bytes at a time turned this into 'abcd', 'ef', 'ghij', 'kl' — a silently short read, which
/// looks exactly like data. MEASURED on `php -n` 8.5.6: 'abcd', 'efgh', 'ijkl', 'mnop', 'qrst'.
#[test]
fn test_a_read_is_topped_up_rather_than_answered_short() {
    let out = compile_and_run_capture(
        r#"<?php
class P {
    public $context;
    public $pos = 0;
    public $data = "abcdefghijklmnopqrst";
    public function stream_open($p, $m, $o, &$x) { return true; }
    public function stream_read($n) {
        $r = substr($this->data, $this->pos, min($n, 6));
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_eof() { return $this->pos >= strlen($this->data); }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("pw", "P");
$h = fopen("pw://x", "r");
for ($i = 1; $i <= 5; $i++) { echo var_export(fread($h, 4), true), "\n"; }
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "'abcd'\n'efgh'\n'ijkl'\n'mnop'\n'qrst'\n");
}

/// Verifies the leftovers survive the top-up, and that ONE chunk is what a top-up adds.
///
/// Two rules in one line. The holding area is APPENDED to, not put over: `__rt_stream_pending_put`
/// frees what it replaces because its callers have drained it first, and the topping-up has not.
/// And php stops filling on a SHORT read rather than asking until satisfied — MEASURED, a source
/// handing back 3 bytes at a time answers `fread($h, 5)` with FOUR, one leftover plus one chunk.
#[test]
fn test_the_leftovers_survive_the_top_up() {
    let out = compile_and_run_capture(
        r#"<?php
class P {
    public $context;
    public $pos = 0;
    public $data = "0123456789";
    public function stream_open($p, $m, $o, &$x) { return true; }
    public function stream_read($n) {
        $r = substr($this->data, $this->pos, min($n, 3));
        $this->pos += strlen($r);
        return $r;
    }
    public function stream_eof() { return $this->pos >= strlen($this->data); }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("qw", "P");
$h = fopen("qw://x", "r");
echo fread($h, 2), "|", fread($h, 5), "|", fread($h, 5), "\n";
fclose($h);
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, "01|2345|678\n");
}

/// Verifies `fgets()` asks the class NOTHING: it fills, and reads what it already knows.
///
/// php's is `read`, `eof`, and that is all. elephc asked before reading and asked again when the
/// buffer emptied, so a class saw three questions where php puts one — and the last of them was a
/// read that came back empty. Same lines out either way.
#[test]
fn test_fgets_asks_the_class_nothing_before_it_reads() {
    assert_eq!(
        calls(r#"$h = fopen("tw://x", "r"); fgets($h); fclose($h);"#),
        vec!["read", "eof"],
    );
}

/// Verifies draining a stream with `fgets()` reads it ONCE, even where the last line has no
/// newline.
///
/// The probe it uses takes the HANDLE: driven by the descriptor it resolves no stream state, so it
/// could never answer yes, and every unterminated line cost an extra empty read.
#[test]
fn test_draining_with_fgets_reads_once() {
    assert_eq!(
        calls(r#"$h = fopen("tw://x", "r"); while (fgets($h) !== false) {} fclose($h);"#),
        vec!["read", "eof"],
    );
}
