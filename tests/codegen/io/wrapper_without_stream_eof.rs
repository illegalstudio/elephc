//! Purpose:
//! Integration tests for a user-registered wrapper that implements `stream_read` but NOT
//! `stream_eof` — the case where php refuses to hand the bytes over at all.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - A wrapper cannot set the stream's end-of-file state itself, so php asks `stream_eof()` after
//!   every `stream_read()`. A class that cannot answer gets a warning and its READ FAILS: the
//!   bytes it just returned are discarded. elephc used to keep them and answer the data, silently.
//! - php names the CALLER, not `feof()`, and every builtin's failure shape is that one refused
//!   read travelling out through its ordinary failure path — `false`, `""`, `[]`.
//! - Every expectation MEASURED on `php -n` 8.5.6.

use crate::support::*;

/// A wrapper with `stream_read` and no `stream_eof`. `stream_stat` is present so the whole-file
/// readers do not warn about THAT instead, which would hide the one being tested.
const WRAPPER: &str = r#"<?php
class NoEof {
    public $context;
    private int $pos = 0;
    private string $data = "abcdefghij";
    public function stream_open($path, $mode, $options, &$opened) { return true; }
    public function stream_read($count) {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("noeof", "NoEof");
"#;

/// Compiles `WRAPPER` followed by `body` and returns the program's captured output.
fn run_without_eof(body: &str) -> ProgramOutput {
    compile_and_run_capture(&format!("{WRAPPER}{body}\n"))
}

/// Asserts the value on stdout and the warning php names the given caller with.
fn assert_refusal(out: &ProgramOutput, caller: &str, value: &str) {
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(out.stdout, value);
    let expected = format!("Warning: {caller}(): NoEof::stream_eof is not implemented! Assuming EOF");
    assert!(
        out.diagnostics.contains(&expected),
        "expected {expected:?}, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `fread()` answers false, not the bytes, and names ITSELF in the warning.
///
/// elephc answered `string(4) "abcd"` here: the read happened, nobody asked whether it had
/// reached the end, and the data went out as if it had worked.
#[test]
fn test_fread_on_a_wrapper_without_stream_eof_answers_false() {
    let out =
        run_without_eof(r#"$h = fopen("noeof://x", "r"); var_dump(fread($h, 4)); fclose($h);"#);
    assert_refusal(&out, "fread", "bool(false)\n");
}

/// Verifies `fgetc()` does the same, under its own name.
#[test]
fn test_fgetc_on_a_wrapper_without_stream_eof_answers_false() {
    let out = run_without_eof(r#"$h = fopen("noeof://x", "r"); var_dump(fgetc($h)); fclose($h);"#);
    assert_refusal(&out, "fgetc", "bool(false)\n");
}

/// Verifies the whole-file reader answers the EMPTY STRING, not the file, under its own name.
///
/// `file_get_contents()` never holds a stream resource, so it names itself through the opener
/// rather than through the shared handle loader. Before that, it warned as `fgetcsv()` — the name
/// of whichever builtin had published last.
#[test]
fn test_file_get_contents_on_a_wrapper_without_stream_eof_answers_empty() {
    let out = run_without_eof(r#"var_dump(file_get_contents("noeof://x"));"#);
    assert_refusal(&out, "file_get_contents", "string(0) \"\"\n");
}

/// Verifies `file()` answers the EMPTY ARRAY, its own shape of the same refusal.
#[test]
fn test_file_on_a_wrapper_without_stream_eof_answers_an_empty_array() {
    let out = run_without_eof(r#"var_dump(file("noeof://x"));"#);
    assert_refusal(&out, "file", "array(0) {\n}\n");
}

/// Verifies the refused bytes are not left on the stream for the NEXT reader to serve.
///
/// The chunked reader put the whole chunk into the stream's holding area before judging the read,
/// so a second `fread()` answered bytes that php had already discarded — the refusal would have
/// been visible once and then quietly undone.
#[test]
fn test_a_refused_read_leaves_nothing_behind_for_the_next_reader() {
    let out = run_without_eof(
        r#"$h = fopen("noeof://x", "r"); fread($h, 4); var_dump(fread($h, 4)); fclose($h);"#,
    );
    assert_refusal(&out, "fread", "bool(false)\n");
    assert_eq!(
        out.diagnostics
            .matches("NoEof::stream_eof is not implemented")
            .count(),
        2,
        "one warning per refused read, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies a wrapper that DOES implement `stream_eof` is untouched by any of this.
///
/// The probe runs before `stream_read` and costs a vtable load; this is the test that says the
/// ordinary path still answers ordinary data, and warns about nothing.
#[test]
fn test_a_wrapper_with_stream_eof_still_reads_normally() {
    let out = compile_and_run_capture(
        r#"<?php
class WithEof {
    public $context;
    private int $pos = 0;
    private string $data = "abcdefghij";
    public function stream_open($path, $mode, $options, &$opened) { return true; }
    public function stream_read($count) {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_eof() { return $this->pos >= strlen($this->data); }
    public function stream_stat() { return []; }
    public function stream_close() {}
}
stream_wrapper_register("witheof", "WithEof");
$h = fopen("witheof://x", "r");
var_dump(fread($h, 4));
var_dump(fread($h, 3));
fclose($h);
var_dump(file_get_contents("witheof://y"));
"#,
    );
    assert!(out.success, "program failed: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "string(4) \"abcd\"\nstring(3) \"efg\"\nstring(10) \"abcdefghij\"\n"
    );
    assert!(
        !out.diagnostics.contains("stream_eof is not implemented"),
        "a class that implements stream_eof must not be warned about, got diagnostics={}",
        out.diagnostics
    );
}

/// Verifies `fgets()` answers false under its OWN name, not `feof()`.
///
/// elephc's `fgets` loop probes end-of-file before reading; php's does not — it reads, and the
/// read asks the wrapper. So the probe warned as `feof()` and the loop then answered the line php
/// refuses. The probe is silent now, and the read is what refuses.
#[test]
fn test_fgets_on_a_wrapper_without_stream_eof_answers_false() {
    let out = run_without_eof(r#"$h = fopen("noeof://x", "r"); var_dump(fgets($h)); fclose($h);"#);
    assert_refusal(&out, "fgets", "bool(false)\n");
}

/// Verifies `fgetcsv()` does the same, under its own name.
#[test]
fn test_fgetcsv_on_a_wrapper_without_stream_eof_answers_false() {
    let out = run_without_eof(r#"$h = fopen("noeof://x", "r"); var_dump(fgetcsv($h)); fclose($h);"#);
    assert_refusal(&out, "fgetcsv", "bool(false)\n");
}

/// Verifies `stream_get_contents()` answers the empty string, under its own name.
#[test]
fn test_stream_get_contents_on_a_wrapper_without_stream_eof_answers_empty() {
    let out = run_without_eof(
        r#"$h = fopen("noeof://x", "r"); var_dump(stream_get_contents($h)); fclose($h);"#,
    );
    assert_refusal(&out, "stream_get_contents", "string(0) \"\"\n");
}

/// Verifies `stream_get_line()` answers false, under its own name.
#[test]
fn test_stream_get_line_on_a_wrapper_without_stream_eof_answers_false() {
    let out = run_without_eof(
        "$h = fopen(\"noeof://x\", \"r\"); var_dump(stream_get_line($h, 100, \"\\n\")); fclose($h);",
    );
    assert_refusal(&out, "stream_get_line", "bool(false)\n");
}

/// Verifies `fpassthru()` answers -1 and prints NOTHING.
///
/// A refused read is not a short one: elephc counted the bytes it had managed and answered 0,
/// after printing them. php answers -1 and prints nothing, because it keeps none of them.
#[test]
fn test_fpassthru_on_a_wrapper_without_stream_eof_answers_minus_one() {
    let out =
        run_without_eof(r#"$h = fopen("noeof://x", "r"); var_dump(fpassthru($h)); fclose($h);"#);
    assert_refusal(&out, "fpassthru", "int(-1)\n");
}

/// Verifies `readfile()` answers -1 and prints NOTHING.
#[test]
fn test_readfile_on_a_wrapper_without_stream_eof_answers_minus_one() {
    let out = run_without_eof(r#"var_dump(readfile("noeof://x"));"#);
    assert_refusal(&out, "readfile", "int(-1)\n");
}

/// Verifies a `feof()` the PROGRAM wrote is still LOUD, and still names `feof()`.
///
/// The quiet probe exists for elephc's own loops. php warns for a `feof()` the user called, so
/// silencing that one would trade a wrong name for a missing warning.
#[test]
fn test_a_feof_the_program_called_still_warns() {
    let out = run_without_eof(r#"$h = fopen("noeof://x", "r"); var_dump(feof($h)); fclose($h);"#);
    assert_refusal(&out, "feof", "bool(true)\n");
}
