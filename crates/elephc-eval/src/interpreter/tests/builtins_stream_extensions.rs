//! Purpose:
//! Interpreter tests for eval stream wrapper, filter, and bucket helper builtins.
//!
//! Called from:
//! - `cargo test -p elephc-eval` through Rust's test harness.
//!
//! Key details:
//! - Filter resources are eval-local handles and do not transform stream bytes.
//! - Buckets use stdClass properties so user-filter style code can inspect them.

use super::super::*;
use super::support::*;

/// Verifies stream wrapper/filter/bucket helper builtins are callable in eval.
#[test]
fn execute_program_dispatches_stream_extension_builtins() {
    let pid = std::process::id();
    let file = format!("elephc_eval_stream_extensions_{pid}.txt");
    let source = format!(
        r#"file_put_contents("{file}", "abc");
$h = fopen("{file}", "r+");
echo stream_wrapper_register("evaltest", "stdClass") ? "wreg" : "bad"; echo ":";
echo stream_wrapper_unregister("evaltest") ? "wunreg" : "bad"; echo ":";
echo stream_wrapper_restore("evaltest") ? "wrestore" : "bad"; echo ":";
echo stream_filter_register("eval.filter", "stdClass") ? "freg" : "bad"; echo ":";
$filter = stream_filter_append($h, "string.toupper");
echo is_resource($filter) ? "fappend" : "bad"; echo ":";
echo stream_filter_remove($filter) ? "fremove" : "bad"; echo ":";
$filter = call_user_func("stream_filter_prepend", $h, "string.tolower");
echo is_resource($filter) ? "fprepend" : "bad"; echo ":";
echo call_user_func("stream_filter_remove", $filter) ? "fcallremove" : "bad"; echo ":";
$bucket = stream_bucket_new($h, "payload");
echo is_object($bucket) && $bucket->data === "payload" && $bucket->datalen === 7 ? "bucket" : "bad"; echo ":";
$brigade = new stdClass();
stream_bucket_append($brigade, $bucket);
$out = stream_bucket_make_writeable($brigade);
echo is_object($out) && $out->data === "payload" ? "make" : "bad"; echo ":";
$brigade2 = new stdClass();
$first = stream_bucket_new($h, "first");
$second = stream_bucket_new($h, "second");
stream_bucket_append($brigade2, $second);
stream_bucket_prepend($brigade2, $first);
$out = stream_bucket_make_writeable($brigade2);
echo is_object($out) && $out->data === "first" ? "prepend" : "bad"; echo ":";
fclose($h);
echo unlink("{file}") ? "cleanup" : "bad"; echo ":";
echo function_exists("stream_bucket_append"); echo function_exists("stream_bucket_make_writeable");
echo function_exists("stream_bucket_new"); echo function_exists("stream_bucket_prepend");
echo function_exists("stream_filter_append"); echo function_exists("stream_filter_prepend");
echo function_exists("stream_filter_register"); echo function_exists("stream_filter_remove");
echo function_exists("stream_wrapper_register"); echo function_exists("stream_wrapper_restore");
echo function_exists("stream_wrapper_unregister");
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
        concat!(
            "wreg:wunreg:wrestore:freg:fappend:fremove:fprepend:fcallremove:",
            "bucket:make:prepend:cleanup:11111111111"
        )
    );
    assert_eq!(values.get(result), FakeValue::Bool(true));
}
