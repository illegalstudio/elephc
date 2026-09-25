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
#[cfg(unix)]
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

/// Verifies Windows `popen()` uses `cmd.exe` pipes without Unix descriptor conversion.
#[test]
#[cfg(windows)]
fn execute_program_dispatches_windows_process_pipe_builtins() {
    let program = parse_fragment(
        br#"$h = popen("echo|set /p=eval-popen", "r");
echo is_resource($h) ? "open" : "bad"; echo ":";
echo fread($h, 64) === "eval-popen" ? "read" : "bad"; echo ":";
echo pclose($h) === 0 ? "closed" : "bad"; echo ":";
echo function_exists("popen"); echo function_exists("pclose");
return true;"#,
    )
    .expect("parse Windows eval process pipe fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "open:read:closed:11");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval `proc_open` owns a real child process, writes the pipes output,
/// and `proc_close` returns the child's exit status.
#[test]
#[cfg(unix)]
fn execute_program_dispatches_proc_open_and_close() {
    let root = std::env::temp_dir().join(format!(
        "elephc_magician_proc_open_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).expect("create proc_open cwd");
    let root = std::fs::canonicalize(&root).expect("canonicalize proc_open cwd");
    let output = root.join("redirected.txt");
    let source = format!(
        r#"$pipes = [];
$process = proc_open('read line; printf "%s|%s|%s" "$MAGIC" "$PWD" "$line"; printf "stderr" >&2; exit 9', [0 => ["pipe", "r"], 1 => ["pipe", "w"], 2 => ["pipe", "w"]], $pipes, "{}", ["MAGIC" => "env"]);
echo is_resource($process) ? "open" : "bad"; echo ":";
echo fwrite($pipes[0], "hello\n"); echo ":";
echo fclose($pipes[0]) ? "closed" : "bad"; echo ":";
echo stream_get_contents($pipes[1]); echo ":";
echo stream_get_contents($pipes[2]); echo ":";
echo fclose($pipes[1]) ? "outclosed" : "bad"; echo ":";
echo fclose($pipes[2]) ? "errclosed" : "bad"; echo ":";
echo proc_close($process); echo ":";
$redirected = [];
$second = proc_open('printf "file-out"; printf "%s" "-err" >&2; exit 7', [1 => ["file", "{}", "w"], 2 => 1], $redirected);
echo count($redirected); echo ":";
echo proc_close($second); echo ":";
echo file_get_contents("{}"); echo ":";
$direct_pipes = [];
$direct = proc_open('/usr/bin/printf bypass', [1 => ["pipe", "w"]], $direct_pipes, null, null, ["bypass_shell" => true]);
echo stream_get_contents($direct_pipes[1]); echo ":";
echo proc_close($direct); echo ":";
echo function_exists("proc_open"); echo function_exists("proc_close");
return true;"#,
        root.to_string_lossy(),
        output.to_string_lossy(),
        output.to_string_lossy(),
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval proc_open fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_dir_all(&root);
    assert_eq!(
        values.output,
        format!(
            "open:6:closed:env|{}|hello:stderr:outclosed:errclosed:9:0:7:file-out-err:bypass:0:11",
            root.to_string_lossy()
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval process status is non-consuming and the new process builtins are registered.
#[test]
#[cfg(unix)]
fn execute_program_dispatches_proc_status_and_terminate_builtins() {
    let program = parse_fragment(
        br#"$pipes = [];
$process = proc_open('sleep 1', [1 => ["pipe", "w"]], $pipes);
$status = proc_get_status($process);
echo $status["running"] ? "running" : "stopped"; echo ":";
echo $status["cached"] ? "cached" : "fresh"; echo ":";
echo is_int($status["pid"]) ? "pid" : "bad"; echo ":";
echo call_user_func("proc_get_status", $process)["command"] === "sleep 1" ? "command" : "bad"; echo ":";
echo proc_close($process) === 0 ? "closed" : "bad"; echo ":";
echo proc_terminate($process) ? "bad" : "notfound"; echo ":";
return function_exists("proc_get_status") && function_exists("proc_terminate");"#,
    )
    .expect("parse eval proc status fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(values.output, "running:fresh:pid:command:closed:notfound:");
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies Windows eval `proc_open` materializes all three pipe directions and
/// preserves the child exit code under `cmd.exe`.
#[test]
#[cfg(windows)]
fn execute_program_dispatches_windows_proc_open_descriptors() {
    let program = parse_fragment(
        br#"$pipes = [];
$process = proc_open("more >NUL&<nul set /p=%MAGIC%&1>&2<nul set /p=stderr&exit /b 9", [0 => ["pipe", "r"], 1 => ["pipe", "w"], 2 => ["pipe", "w"]], $pipes, null, ["MAGIC" => "env"]);
echo is_resource($process) ? "open" : "bad"; echo ":";
echo fwrite($pipes[0], "hello\r\n"); echo ":";
echo fclose($pipes[0]) ? "closed" : "bad"; echo ":";
echo stream_get_contents($pipes[1]); echo ":";
echo stream_get_contents($pipes[2]); echo ":";
echo fclose($pipes[1]) ? "outclosed" : "bad"; echo ":";
echo fclose($pipes[2]) ? "errclosed" : "bad"; echo ":";
echo proc_close($process); echo ":";
echo function_exists("proc_open"); echo function_exists("proc_close");
return true;"#,
    )
    .expect("parse Windows eval proc_open fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "open:7:closed:env:stderr:outclosed:errclosed:9:11"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
