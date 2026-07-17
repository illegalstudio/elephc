//! Purpose:
//! Interpreter tests for one-shot file I/O through userspace stream wrappers.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - `file_get_contents()`, `file()`, `readfile()`, and `file_put_contents()`
//!   should use `stream_open()` plus stream read/write/close methods.

use super::super::*;
use super::support::*;

/// Verifies one-shot file I/O builtins dispatch through userspace stream wrappers.
#[test]
fn execute_program_dispatches_user_stream_wrapper_file_io() {
    let program = parse_fragment(
        br#"class EvalFileIoWrapperW {
    public $data;
    public $pos;
    public function stream_open($path, $mode, $options, &$opened_path): bool {
        echo "O(" . $mode . ")";
        $this->data = "aa\nbb";
        $this->pos = 0;
        return true;
    }
    public function stream_read($count): string {
        $chunk = substr($this->data, $this->pos, $count);
        $this->pos += strlen($chunk);
        return $chunk;
    }
    public function stream_write($data): int {
        echo "W(" . $data . ")";
        return strlen($data);
    }
    public function stream_eof(): bool {
        return $this->pos >= strlen($this->data);
    }
    public function stream_close(): void {
        echo "C";
    }
}
stream_wrapper_register("fio", "EvalFileIoWrapperW");
echo file_get_contents("fio://read") === "aa\nbb" ? "fgc" : "bad"; echo ":";
$lines = file("fio://read");
echo count($lines) === 2 && $lines[0] === "aa\n" && $lines[1] === "bb" ? "file" : "bad"; echo ":";
echo readfile("fio://read") === 5 ? "readfile" : "bad"; echo ":";
echo file_put_contents("fio://write", "xyz") === 3 ? "put" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "O(r)Cfgc:O(r)Cfile:O(r)aa\nbbCreadfile:O(w)W(xyz)Cput"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
