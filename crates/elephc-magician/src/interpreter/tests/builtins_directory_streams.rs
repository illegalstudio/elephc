//! Purpose:
//! Interpreter tests for eval local directory resource builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Each test uses a process-unique directory and removes it before and after execution.
//! - Directory resources share eval's generic resource cell representation.

use super::super::*;
use super::support::*;

/// Verifies eval directory resources support open/read/rewind/close operations.
#[test]
fn execute_program_dispatches_directory_stream_builtins() {
    let pid = std::process::id();
    let dir = format!("elephc_magician_dir_stream_{pid}");
    let source = format!(
        r#"mkdir("{dir}");
file_put_contents("{dir}/a.txt", "a");
file_put_contents("{dir}/b.txt", "b");
$dh = opendir("{dir}");
echo is_resource($dh) ? "open" : "bad"; echo ":";
echo get_resource_type($dh) === "stream" ? "rtype" : "bad"; echo ":";
echo readdir($dh) === "." ? "dot" : "bad"; echo ":";
echo readdir($dh) === ".." ? "dotdot" : "bad"; echo ":";
$entries = [readdir($dh), readdir($dh)];
echo in_array("a.txt", $entries) && in_array("b.txt", $entries) ? "entries" : "bad"; echo ":";
echo readdir($dh) === false ? "eof" : "bad"; echo ":";
rewinddir($dh);
echo readdir($dh) === "." ? "rewind" : "bad"; echo ":";
call_user_func("rewinddir", $dh);
echo call_user_func("readdir", $dh) === "." ? "callread" : "bad"; echo ":";
call_user_func("closedir", $dh);
echo readdir($dh) === false ? "closed" : "bad"; echo ":";
$call = call_user_func_array("opendir", ["directory" => "{dir}"]);
echo call_user_func("readdir", $call) === "." ? "callopen" : "bad"; echo ":";
closedir($call);
echo unlink("{dir}/a.txt") && unlink("{dir}/b.txt") && rmdir("{dir}") ? "cleanup" : "bad"; echo ":";
echo function_exists("opendir"); echo function_exists("readdir");
echo function_exists("rewinddir"); echo function_exists("closedir");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_dir_all(&dir);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(
        values.output,
        "open:rtype:dot:dotdot:entries:eof:rewind:callread:closed:callopen:cleanup:1111"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
