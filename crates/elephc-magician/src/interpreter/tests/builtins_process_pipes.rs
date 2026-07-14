//! Purpose:
//! Interpreter tests for eval process pipe stream builtins.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Tests use shell commands that exit immediately and clean up their temp file.
//! - `popen()` resources are normal eval streams until `pclose()` waits for the child.

use super::super::*;
use super::support::*;

/// Verifies `popen()` and `pclose()` support read/write pipes and dynamic calls.
#[test]
fn execute_program_dispatches_process_pipe_builtins() {
    let pid = std::process::id();
    let file = format!("elephc_magician_popen_{pid}.txt");
    let source = format!(
        r#"$h = popen("printf eval-popen", "r");
echo is_resource($h) ? "open" : "bad"; echo ":";
echo fread($h, 64) === "eval-popen" ? "read" : "bad"; echo ":";
echo pclose($h) === 0 ? "closed" : "bad"; echo ":";
$w = popen("cat > {file}", "w");
echo fwrite($w, "pipeout") === 7 ? "write" : "bad"; echo ":";
echo pclose($w) === 0 ? "wclosed" : "bad"; echo ":";
echo file_get_contents("{file}") === "pipeout" ? "file" : "bad"; echo ":";
$call = call_user_func("popen", "printf call-pipe", "r");
echo stream_get_contents($call) === "call-pipe" ? "callread" : "bad"; echo ":";
echo call_user_func("pclose", $call) === 0 ? "callclose" : "bad"; echo ":";
echo unlink("{file}") ? "cleanup" : "bad"; echo ":";
echo function_exists("popen"); echo function_exists("pclose");
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let _ = std::fs::remove_file(&file);
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&file);
    assert_eq!(
        values.output,
        "open:read:closed:write:wclosed:file:callread:callclose:cleanup:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
