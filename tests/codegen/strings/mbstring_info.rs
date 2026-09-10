//! Purpose:
//! Exercises mbstring information results through native EIR and opaque eval calls.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - Null, false, scalar, indexed, and associative results share one engine operation.
//! - Snapshots and temporary selector objects must release all ownership after each call.

use crate::support::*;

/// Wraps a PHP body as a native program or a runtime-unknown eval source.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Shares all result shapes through namespaced, named, first-class, and dynamic callable forms.
#[test]
fn test_mbstring_info_public_calls() {
    let body = r#"
namespace RequestInformation;
echo Mb_GeT_InFo(type: "LaNgUaGe"), "\n";
$read = mb_get_info(...);
echo is_null($read("http_input")) ? "null\n" : "wrong\n";
echo json_encode($read("detect_order")), "\n";
$dynamic = $argc > 0 ? "mb_get_info" : "mb_language";
echo $dynamic(type: "internal_encoding"), "\n";
echo call_user_func_array($dynamic, ["type" => "http_output"]), "\n";
$before = mb_get_info();
mb_language("Japanese");
mb_detect_order(["SJIS", "ASCII"]);
mb_http_output("pass");
mb_substitute_character("entity");
echo json_encode(mb_get_info("detect_order")), "\n";
echo mb_get_info("mail_charset"), ":", mb_get_info("mail_header_encoding"), ":", mb_get_info("mail_body_encoding"), "\n";
echo json_encode($before), "\n";
"#;
    let expected = concat!("neutral\nnull\n[\"ASCII\",\"UTF-8\"]\nUTF-8\nUTF-8\n[\"SJIS\",\"ASCII\"]\nISO-2022-JP:BASE64:7bit\n",
        "{\"internal_encoding\":\"UTF-8\",\"http_output\":\"UTF-8\",\"http_output_conv_mimetypes\":\"^(text\\/|application\\/xhtml\\\\+xml)\",",
        "\"mail_charset\":\"UTF-8\",\"mail_header_encoding\":\"BASE64\",\"mail_body_encoding\":\"BASE64\",\"illegal_chars\":0,",
        "\"encoding_translation\":\"Off\",\"language\":\"neutral\",\"detect_order\":[\"ASCII\",\"UTF-8\"],\"substitute_character\":63,\"strict_detection\":\"Off\"}\n");
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, expected, "eval={eval}");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}

/// Reads state after Stringable selector callbacks and distinguishes invalid selectors from null.
#[test]
fn test_mbstring_info_state_and_errors() {
    let body = r#"
class InformationSelector {
    public function __toString(): string { mb_scrub(chr(255) . chr(255), "UTF-8"); return "illegal_chars"; }
}
echo mb_get_info(new InformationSelector()), "\n";
echo mb_get_info("illegal_chars"), "\n";
$read = $argc > 0 ? "mb_get_info" : "mb_language";
try { $read("all", "language"); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $read([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo $read("all\0") === false ? "invalid\n" : "wrong\n";
echo $read(null) === false ? "null argument\n" : "wrong\n";
echo @mb_get_info("missing") === false ? "suppressed\n" : "wrong\n";
echo mb_get_info("http_input") === null ? "unset\n" : "wrong\n";
"#;
    for eval in [false, true] {
        // Eval's parser does not implement @ yet; retain unsuppressed warning coverage there.
        let body = if eval { body.replace("@mb_get_info", "mb_get_info") } else { body.to_owned() };
        let output = compile_and_run_capture(&program(&body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "2\n2\nmb_get_info() expects at most 1 argument, 2 given\nmb_get_info(): Argument #1 ($type) must be of type string, array given\ninvalid\nnull argument\nsuppressed\nunset\n", "eval={eval}");
        assert_eq!(output.stderr.matches("mb_get_info(): argument #1 ($type) must be a valid type").count(), if eval { 3 } else { 2 }, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stderr.matches("Passing null to parameter #1 ($type) of type string is deprecated").count(), 1, "eval={eval}: {}", output.stderr);
    }
}

/// Releases every snapshot child, boxed null, and coerced temporary across repeated native/eval calls.
#[test]
fn test_mbstring_info_result_ownership() {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 4] {
            let calls = "mb_get_info(); $read(type: new OwnedInformationSelector()); call_user_func_array($read, [\"type\" => \"detect_order\"]); $read(\"http_input\"); mb_http_input(\"I\");";
            let body = format!(r#"
class OwnedInformationSelector {{ public function __toString(): string {{ return "all"; }} }}
$read = $argc > 0 ? "mb_get_info" : "mb_language";
{}
echo "done";
"#, calls.repeat(count));
            let output = compile_and_run_with_gc_stats(&program(&body, eval));
            assert!(output.success, "eval={eval}: {}", output.stderr);
            assert_eq!(output.stdout, "done");
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "eval={eval}");
    }
}
