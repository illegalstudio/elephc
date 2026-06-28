//! Purpose:
//! Interpreter tests for userspace stream-wrapper directory handles.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Wrapper directory resources hold the wrapper object across read, rewind,
//!   and close calls so cursor state stays on the userspace instance.

use super::super::*;
use super::support::*;

/// Verifies directory stream builtins dispatch to userspace wrapper methods.
#[test]
fn execute_program_dispatches_user_stream_wrapper_directories() {
    let program = parse_fragment(
        br#"class EvalDirWrapperW {
    public $entries;
    public $pos;
    public function dir_opendir($path, $options): bool {
        echo "O(" . $path . "," . $options . ")";
        $this->entries = ["one", "two"];
        $this->pos = 0;
        return $path === "dirw://ok";
    }
    public function dir_readdir(): string {
        if ($this->pos >= count($this->entries)) {
            return "";
        }
        $entry = $this->entries[$this->pos];
        $this->pos += 1;
        return $entry;
    }
    public function dir_rewinddir(): bool {
        echo "W";
        $this->pos = 0;
        return true;
    }
    public function dir_closedir(): bool {
        echo "C";
        return true;
    }
}
stream_wrapper_register("dirw", "EvalDirWrapperW");
$dh = opendir("dirw://ok");
echo is_resource($dh) ? "open" : "bad"; echo ":";
echo readdir($dh) === "one" ? "one" : "bad"; echo ":";
echo readdir($dh) === "two" ? "two" : "bad"; echo ":";
echo readdir($dh) === false ? "eof" : "bad"; echo ":";
rewinddir($dh);
echo readdir($dh) === "one" ? "rewind" : "bad"; echo ":";
call_user_func("rewinddir", $dh);
echo call_user_func("readdir", $dh) === "one" ? "callread" : "bad"; echo ":";
call_user_func("closedir", $dh);
echo readdir($dh) === false ? "closed" : "bad"; echo ":";
echo opendir("dirw://bad") === false ? "openfalse" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "O(dirw://ok,0)open:one:two:eof:Wrewind:Wcallread:Cclosed:O(dirw://bad,0)openfalse"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
