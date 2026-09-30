//! Purpose:
//! Interpreter tests for userspace stream-wrapper filesystem path operations.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Path operations instantiate wrapper objects per operation, matching the
//!   generated runtime path-op dispatch instead of open stream resource dispatch.

use super::super::*;
use super::support::*;

/// Verifies path mutation builtins dispatch to userspace stream-wrapper methods.
#[test]
fn execute_program_dispatches_user_stream_wrapper_path_ops() {
    let program = parse_fragment(
        br#"class EvalPathOpWrapperW {
    public function unlink($path): bool {
        echo "U(" . $path . ")";
        return $path === "pathop://delete-ok";
    }
    public function mkdir($path, $mode, $options): bool {
        echo "M(" . $path . "," . $mode . "," . $options . ")";
        return true;
    }
    public function rmdir($path, $options): bool {
        echo "R(" . $path . "," . $options . ")";
        return false;
    }
    public function rename($from, $to): bool {
        echo "N(" . $from . "," . $to . ")";
        return $to === "pathop://dest";
    }
}
stream_wrapper_register("pathop", "EvalPathOpWrapperW");
echo unlink("pathop://delete-ok") ? "unlink" : "bad"; echo ":";
echo call_user_func("unlink", "pathop://delete-no") === false ? "unlinkfalse" : "bad"; echo ":";
echo mkdir("pathop://dir") ? "mkdir" : "bad"; echo ":";
echo mkdir("pathop://deep", 493, true) ? "mkdirrec" : "bad"; echo ":";
echo rmdir("pathop://dir") === false ? "rmdirfalse" : "bad"; echo ":";
echo rename("pathop://source", "pathop://dest") ? "rename" : "bad"; echo ":";
echo call_user_func("rename", "pathop://source2", "pathop://dest") ? "callrename" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    // The `$mode`/`$options` pairs are php's, measured on 8.5.6: `mkdir($p)` arrives as
    // (511, 8) and `mkdir($p, 0755, true)` as (493, 9) — STREAM_REPORT_ERRORS with
    // STREAM_MKDIR_RECURSIVE folded in — while `rmdir($p)` arrives as (8). This assertion used to
    // read (0, 0) and (0), which was the implementation writing its own expectation.
    assert_eq!(
        values.output,
        "U(pathop://delete-ok)unlink:U(pathop://delete-no)unlinkfalse:M(pathop://dir,511,8)mkdir:M(pathop://deep,493,9)mkdirrec:R(pathop://dir,8)rmdirfalse:N(pathop://source,pathop://dest)rename:N(pathop://source2,pathop://dest)callrename"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
