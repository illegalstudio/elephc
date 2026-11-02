//! Purpose:
//! Interpreter tests for eval `proc_open()`, `proc_close()`, `proc_get_status()`,
//! `proc_terminate()`, and `exec()`'s by-reference outputs.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - Children are tiny `sh` commands that exit on their own or are signalled by the test.
//! - Expected values were measured on `php -n` 8.5.10.

use super::super::*;
use super::support::*;

/// Runs one eval fragment and returns its echoed output.
fn run(source: &str) -> String {
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();
    execute_program(&program, &mut scope, &mut values).expect("execute eval ir");
    values.output
}

/// Pipes in both directions, the process handle, and the exit code.
#[test]
fn execute_program_runs_proc_open_with_pipes() {
    let output = run(r#"$p = proc_open("cat", [0 => ["pipe", "r"], 1 => ["pipe", "w"], 2 => ["pipe", "w"]], $pipes);
echo is_resource($p) ? "open" : "bad"; echo ":";
echo count($pipes); echo ":";
echo get_resource_type($p); echo ":";
fwrite($pipes[0], "hello"); fclose($pipes[0]);
echo stream_get_contents($pipes[1]); echo ":";
echo strlen(stream_get_contents($pipes[2])); echo ":";
fclose($pipes[1]); fclose($pipes[2]);
proc_close($p); echo "closed";
return true;"#);
    assert_eq!(output, "open:3:process:hello:0:closed");
}

/// Array commands skip the shell; `cwd` and `env_vars` apply; failures return false.
#[test]
fn execute_program_proc_open_array_command_cwd_env_and_failure() {
    let output = run(r#"$p = proc_open(["echo", "a b", '$HOME'], [1 => ["pipe", "w"]], $pipes);
echo trim(stream_get_contents($pipes[1])); echo ":"; fclose($pipes[1]); proc_close($p);
$p = proc_open("pwd", [1 => ["pipe", "w"]], $pipes, "/");
echo trim(stream_get_contents($pipes[1])); echo ":"; fclose($pipes[1]); proc_close($p);
$p = proc_open('echo "$FOO|${HOME-unset}"', [1 => ["pipe", "w"]], $pipes, null, ["FOO" => "foo"]);
echo trim(stream_get_contents($pipes[1])); echo ":"; fclose($pipes[1]); proc_close($p);
echo @proc_open(["/nonexistent/binary"], [], $pipes) === false ? "false" : "bad";
return true;"#);
    assert_eq!(output, "a b $HOME:/:foo|unset:false");
}

/// Status keys and caching follow php 8.5; `proc_terminate()` signals without reaping.
#[test]
fn execute_program_proc_get_status_and_terminate() {
    let output = run(r#"$p = proc_open("exit 3", [], $pipes);
usleep(300000);
$s = proc_get_status($p);
echo implode(",", array_keys($s)); echo ":";
echo $s["running"] ? "running" : "done"; echo ":";
echo $s["exitcode"]; echo ":";
echo $s["cached"] ? "cached" : "fresh"; echo ":";
echo proc_get_status($p)["exitcode"]; echo ":";
echo proc_close($p); echo ":";
$p = proc_open("sleep 30", [], $pipes);
echo proc_terminate($p) ? "sent" : "bad"; echo ":";
usleep(300000);
$s = proc_get_status($p);
echo $s["signaled"] ? "signaled" : "bad"; echo ":";
echo $s["termsig"]; echo ":";
echo proc_get_status($p)["signaled"] ? "bad" : "reaped"; echo ":";
proc_close($p); echo "closed";
return true;"#);
    assert_eq!(
        output,
        "command,pid,cached,running,signaled,stopped,exitcode,termsig,stopsig:done:3:cached:3:3:sent:signaled:15:reaped:closed"
    );
}

/// `exec()` fills `$output` and `$result_code`, trims lines, and answers the last line.
#[test]
fn execute_program_exec_fills_output_and_result_code() {
    let output = run(r#"$last = exec("printf 'a\nb  \nc  '; exit 4", $lines, $code);
echo json_encode($lines); echo ":"; echo $code; echo ":"; echo json_encode($last); echo ":";
$more = ["pre"];
exec("echo x", $more);
echo json_encode($more); echo ":";
echo json_encode(exec("echo hi  "));
return true;"#);
    assert_eq!(output, "[\"a\",\"b\",\"c\"]:4:\"c\":[\"pre\",\"x\"]:\"hi\"");
}
