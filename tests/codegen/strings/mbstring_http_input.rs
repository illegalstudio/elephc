//! Purpose:
//! Exercises HTTP input information through native and runtime-unknown eval calls.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - CLI inputs begin unidentified while their configured candidate list defaults to UTF-8.
//! - Live language, detection, and internal encoding setters do not replace that list.

use crate::support::*;

/// Wraps one PHP body as native code or a runtime-unknown eval fragment.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Preserves false/null distinctions and input-list independence across every callable entry form.
#[test]
fn test_mbstring_http_input_public_calls() {
    let body = r#"
namespace RequestEncodings;
echo json_encode([Mb_HtTp_InPuT(), mb_http_input(null), mb_http_input("g"), mb_http_input("P"),
    mb_http_input("c"), mb_http_input("S"), mb_http_input(type: "i"), mb_http_input("l")]), "\n";
echo mb_get_info("http_input") === null ? "info null\n" : "wrong\n";
mb_internal_encoding("ASCII");
mb_detect_order(["SJIS", "ASCII"]);
mb_language("Japanese");
$read = mb_http_input(...);
echo $read("L"), "\n";
$dynamic = $argc > 0 ? "mb_http_input" : "mb_get_info";
echo json_encode($dynamic(type: "I")), "\n";
echo call_user_func_array($dynamic, ["type" => null]) === false ? "unidentified\n" : "wrong\n";
class HttpInputSelector {
    public function __toString(): string { mb_internal_encoding("UTF-16LE"); return "i"; }
}
echo json_encode(mb_http_input(new HttpInputSelector())), ":", mb_internal_encoding(), "\n";
"#;
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "[false,false,false,false,false,false,[\"UTF-8\"],\"UTF-8\"]\ninfo null\nUTF-8\n[\"UTF-8\"]\nunidentified\n[\"UTF-8\"]:UTF-16LE\n", "eval={eval}");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}

/// Keeps invalid selectors catchable and nullable input free of deprecated-null diagnostics.
#[test]
fn test_mbstring_http_input_runtime_errors() {
    let body = r#"
$read = $argc > 0 ? "mb_http_input" : "mb_get_info";
try { $read("G", "P"); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $read([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
try { $read("G\0"); } catch (ValueError $e) { echo $e->getMessage(), "\n"; }
try { $read(false); } catch (ValueError $e) { echo "empty invalid\n"; }
echo $read(null) === false ? "null accepted\n" : "wrong\n";
"#;
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "mb_http_input() expects at most 1 argument, 2 given\nmb_http_input(): Argument #1 ($type) must be of type ?string, array given\nmb_http_input(): Argument #1 ($type) must be one of \"G\", \"P\", \"C\", \"S\", \"I\", or \"L\"\nempty invalid\nnull accepted\n", "eval={eval}");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}
