//! Purpose:
//! Interpreter tests for eval-supported stream wrapper URL handling.
//!
//! Called from:
//! - `cargo test -p elephc-magician` through Rust's test harness.
//!
//! Key details:
//! - PHAR fixtures are written through `elephc-phar` so tests exercise the same
//!   archive bridge used by generated-runtime paths.

use super::super::*;
use super::support::*;

/// Verifies eval `fopen()` and one-shot file builtins handle supported wrappers.
#[test]
fn execute_program_dispatches_supported_stream_wrapper_urls() {
    let pid = std::process::id();
    let local = format!("elephc_magician_wrapper_local_{pid}.txt");
    let archive = format!("elephc_magician_wrapper_{pid}.phar");
    let read_url = format!("phar://{archive}/dir/read.txt");
    let put_url = format!("phar://{archive}/dir/put.txt");
    let stream_url = format!("phar://{archive}/dir/stream.txt");
    let _ = std::fs::remove_file(&local);
    let _ = std::fs::remove_file(&archive);
    std::fs::write(&local, b"local").expect("write local wrapper fixture");
    elephc_phar::put_url_bytes(read_url.as_bytes(), b"from-phar")
        .expect("write phar wrapper fixture");
    let source = format!(
        r#"echo file_get_contents("file://{local}") === "local" ? "fileurl" : "bad"; echo ":";
$memory = fopen("php://memory", "w+");
fwrite($memory, "mem");
rewind($memory);
echo fread($memory, 3) === "mem" ? "memory" : "bad"; echo ":";
fclose($memory);
$data = fopen("data://text/plain;base64,SGVsbG8=", "r");
echo fread($data, 5) === "Hello" ? "data" : "bad"; echo ":";
fclose($data);
$phar = fopen("{read_url}", "r");
echo fread($phar, 32) === "from-phar" ? "pharopen" : "bad"; echo ":";
fclose($phar);
echo file_get_contents("{read_url}") === "from-phar" ? "pharget" : "bad"; echo ":";
echo file_exists("{read_url}") && is_file("{read_url}") && is_readable("{read_url}") ? "pharprobe" : "bad"; echo ":";
echo filetype("{read_url}") === "file" ? "phartype" : "bad"; echo ":";
echo filesize("{read_url}") === 9 ? "pharsize" : "bad"; echo ":";
echo file_put_contents("{put_url}", "put") === 3 ? "pharput" : "bad"; echo ":";
echo file_get_contents("{put_url}") === "put" ? "putread" : "bad"; echo ":";
$out = fopen("{stream_url}", "w");
fwrite($out, "stream");
echo fclose($out) ? "streamclose" : "bad"; echo ":";
echo file_get_contents("{stream_url}") === "stream" ? "streamread" : "bad"; echo ":";
echo unlink("{stream_url}") ? "unlink" : "bad"; echo ":";
echo file_get_contents("{stream_url}") === false ? "deleted" : "bad";
return true;"#
    );
    let program = parse_fragment(source.as_bytes()).expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    let _ = std::fs::remove_file(&local);
    let _ = std::fs::remove_file(&archive);
    assert_eq!(
        values.output,
        "fileurl:memory:data:pharopen:pharget:pharprobe:phartype:pharsize:pharput:putread:streamclose:streamread:unlink:deleted"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}

/// Verifies eval stream wrapper registration changes the visible wrapper list.
#[test]
fn execute_program_tracks_stream_wrapper_registry_state() {
    let program = parse_fragment(
        br#"$before = stream_get_wrappers();
echo in_array("evaltest", $before) ? "bad" : "missing"; echo ":";
echo stream_wrapper_register("evaltest", "stdClass") ? "reg" : "bad"; echo ":";
$after = stream_get_wrappers();
echo in_array("evaltest", $after) ? "listed" : "bad"; echo ":";
echo stream_wrapper_unregister("evaltest") ? "unreg" : "bad"; echo ":";
$removed = call_user_func("stream_get_wrappers");
echo in_array("evaltest", $removed) ? "bad" : "removed"; echo ":";
echo stream_wrapper_unregister("file") ? "unfile" : "bad"; echo ":";
$without_file = stream_get_wrappers();
echo in_array("file", $without_file) ? "bad" : "nofile"; echo ":";
echo stream_wrapper_restore("file") ? "restore" : "bad"; echo ":";
$restored = call_user_func_array("stream_get_wrappers", []);
echo in_array("file", $restored) ? "fileback" : "bad";
return true;"#,
    )
    .expect("parse eval fragment");
    let mut scope = ElephcEvalScope::new();
    let mut values = FakeOps::default();

    let result = execute_program(&program, &mut scope, &mut values).expect("execute eval ir");

    assert_eq!(
        values.output,
        "missing:reg:listed:unreg:removed:unfile:nofile:restore:fileback"
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
