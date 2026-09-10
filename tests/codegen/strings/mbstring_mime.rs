//! Purpose:
//! Exercises MIME header encoding and decoding through native EIR and opaque eval calls.
//!
//! Called from:
//! - The focused codegen string integration suite.
//!
//! Key details:
//! - Covers named and callable forms, binary output, state changes, and runtime diagnostics.

use crate::support::*;

/// Wraps the same PHP body as a native program or a runtime-unknown eval source.
fn program(body: &str, eval: bool) -> String {
    if !eval { return format!("<?php {body}"); }
    let escaped = body.replace('\\', "\\\\").replace('\'', "\\'");
    format!("<?php $source = $argc > 0 ? '{escaped}' : ''; eval($source);")
}

/// Shares MIME decoding across namespaced, named, first-class callable, and array calls.
#[test]
fn test_mbstring_mime_decode_public_calls() {
    let body = r#"
namespace MailHeaders;
mb_internal_encoding("UTF-8");
echo Mb_DeCoDe_MiMeHeAdEr(string: "Subject: =?UTF-8?Q?caf=C3=A9?="), "\n";
$decode = mb_decode_mimeheader(...);
echo $decode("=?UTF-8?Q?A?=\r\n\t=?ASCII?B?Qg==?= C"), "\n";
echo call_user_func_array("mb_decode_mimeheader", ["=?UTF-8?Q?A=zzB?="]), "\n";
echo bin2hex(mb_decode_mimeheader("=?UTF-8?Q?A=00B?=")), "\n";
echo mb_decode_mimeheader("=?missing?B?QQ==?="), "\n";
echo mb_decode_mimeheader("=?UTF-8?B?QQ?"), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "Subject: café\nAB C\nA=B\n410042\n=?missing?B?QQ==?=\nA\n");
    }
}

/// Uses request encoding after Stringable conversion and ignores replacement/error-counter settings.
#[test]
fn test_mbstring_mime_decode_request_state() {
    let body = r#"
class HeaderValue {
    public function __toString(): string {
        mb_internal_encoding("UTF-16LE");
        return "=?UTF-8?Q?A=C3=A9?=";
    }
}
mb_internal_encoding("UTF-8");
mb_substitute_character("none");
echo bin2hex(mb_decode_mimeheader(new HeaderValue())), "\n";
mb_internal_encoding("ASCII");
echo mb_decode_mimeheader("=?UTF-8?Q?=FF=C3=A9?="), "\n";
echo mb_check_encoding() ? "clean\n" : "dirty\n";
mb_internal_encoding("UTF-8");
echo mb_decode_mimeheader("=?UTF-16?B?//5BAA==?= =?UTF-16?B?QgA=?="), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "4100e900\n??\nclean\nAB\n");
    }
}

/// Rejects runtime arity/type mistakes and preserves the strict caller's string contract.
#[test]
fn test_mbstring_mime_decode_runtime_errors() {
    let body = r#"
$decode = $argc > 0 ? "mb_decode_mimeheader" : "strlen";
try { $decode(); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $decode("x", "y"); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { $decode([]); } catch (TypeError $e) { echo $e->getMessage(), "\n"; }
echo $decode(123), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "mb_decode_mimeheader() expects exactly 1 argument, 0 given\nmb_decode_mimeheader() expects exactly 1 argument, 2 given\nmb_decode_mimeheader(): Argument #1 ($string) must be of type string, array given\n123\n");
    }
    let strict = r#"$decode = "mb_decode_mimeheader";
try { echo $decode(123); } catch (TypeError $e) { echo $e->getMessage(); }"#;
    for eval in [false, true] {
        let source = program(strict, eval).replacen("<?php", "<?php declare(strict_types=1);", 1);
        let expected = if eval { "123" } else { "mb_decode_mimeheader(): Argument #1 ($string) must be of type string, int given" };
        assert_eq!(compile_and_run(&source), expected);
    }
}

/// Keeps known callable arity failures catchable instead of emitting invalid typed EIR.
#[test]
fn test_mbstring_mime_decode_known_callable_arity() {
    let body = r#"
function headerArgument(string $value): string { echo $value, ":"; return $value; }
$decode = "mb_decode_mimeheader";
try { $decode(); } catch (ArgumentCountError $e) { echo "empty\n"; }
try { $decode(headerArgument("A"), headerArgument("B")); } catch (ArgumentCountError $e) { echo "excess\n"; }
$first = mb_decode_mimeheader(...);
try { $first(...[]); } catch (ArgumentCountError $e) { echo "first\n"; }
$selected = $argc > 0 ? "mb_decode_mimeheader" : "mb_strlen";
try { call_user_func_array($selected, []); } catch (ArgumentCountError $e) { echo "callback\n"; }
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "empty\nA:B:excess\nfirst\ncallback\n");
    }
}

/// Releases callable argument containers, original object values, results, and arity throwables.
#[test]
fn test_mbstring_mime_decode_callable_ownership() {
    let mut failures = Vec::new();
    for eval in [false, true] {
        for (case, call) in [
            ("scalar", "$decode(123);"),
            ("success", "$decode(new OwnedMimeHeader());"),
            ("evaluation", "try { $decode(new OwnedMimeHeader(), failingMimeArgument()); } catch (Exception) {}"),
            ("spread_evaluation", "try { $decode(...[new OwnedMimeHeader(), new OwnedMimeHeader(), new OwnedMimeHeader(), new OwnedMimeHeader(), new OwnedMimeHeader()], failingMimeArgument()); } catch (Exception) {}"),
            ("arity", "try { $decode([], new OwnedMimeHeader()); } catch (ArgumentCountError) {}"),
            ("type", "try { $decode([new OwnedMimeHeader()]); } catch (TypeError) {}"),
        ] {
            let mut residual = Vec::new();
            for count in [1, 16] {
                let calls = call.repeat(count);
                let body = format!(r#"
class OwnedMimeHeader {{ public function __toString(): string {{ return "=?UTF-8?Q?caf=C3=A9?="; }} }}
function failingMimeArgument(): string {{ throw new Exception("argument failed"); }}
$decode = $argc > 0 ? "mb_decode_mimeheader" : "mb_strlen";
{calls}
echo "done";
"#);
                let output = compile_and_run_with_gc_stats(&program(&body, eval));
                assert!(output.success, "{}", output.stderr);
                assert_eq!(output.stdout, "done");
                let (allocated, freed) = parse_gc_stats(&output.stderr);
                residual.push(allocated as i64 - freed as i64);
            }
            if residual[0] != residual[1] {
                failures.push(format!("case={case}, eval={eval}: {residual:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "MIME callable owners leaked: {}", failures.join("; "));
}

/// Destroys temporary arguments in PHP parameter order after success, arity failure, and unpacking.
#[test]
fn test_mbstring_mime_decode_argument_destruction_order() {
    let body = r#"
class MimeArgumentOwner {
    public string $id;
    public function __construct(string $id) { $this->id = $id; }
    public function __toString(): string { return "UTF-8"; }
    public function __destruct() { echo $this->id; }
}
$decode = $argc > 0 ? "mb_decode_mimeheader" : "mb_strlen";
try { $decode(new MimeArgumentOwner("A"), new MimeArgumentOwner("B")); }
catch (ArgumentCountError) { echo "C"; }
echo "\n";
mb_strlen(encoding: new MimeArgumentOwner("B"), string: new MimeArgumentOwner("A"));
echo "\n";
mb_strlen(...[new MimeArgumentOwner("A"), new MimeArgumentOwner("B")]);
echo "\n";
mb_strlen(...["encoding" => new MimeArgumentOwner("B"), "string" => new MimeArgumentOwner("A")]);
echo "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "ABC\nAB\nAB\nAB\n", "eval={eval}");
    }
}

/// Keeps EIR-owned named containers alive until their single release after descriptor invocation.
#[test]
fn test_mbstring_mime_decode_named_container_heap_debug() {
    let body = r#"
$decode = $argc > 0 ? "mb_decode_mimeheader" : "mb_strlen";
echo $decode(string: "=?UTF-8?Q?A?="), "\n";
try { $decode(string: []); } catch (TypeError) { echo "type\n"; }
echo $decode(string: "=?UTF-8?Q?B?="), "\n";
"#;
    for eval in [false, true] {
        let output = compile_and_run_with_heap_debug(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "A\ntype\nB\n", "eval={eval}");
    }
}

/// Releases partial callback argument containers when a later source expression throws.
#[test]
fn test_mbstring_mime_decode_call_user_func_argument_failure_ownership() {
    let mut failures = Vec::new();
    for eval in [false, true] {
        for (case, call) in [
            ("indexed", "call_user_func($decode, new PartialMimeHeader(), failingMimeArgument());"),
            ("literal", "call_user_func_array($decode, [$retained, new PartialMimeHeader(), failingMimeArgument()]);"),
        ] {
            let mut residual = Vec::new();
            for count in [1, 4] {
                let calls = format!("try {{ {call} }} catch (Exception) {{}}").repeat(count);
                let body = format!(r#"
class PartialMimeHeader {{ public function __toString(): string {{ return "header"; }} }}
function failingMimeArgument(): string {{ throw new Exception("argument failed"); }}
$decode = $argc > 0 ? "mb_decode_mimeheader" : "mb_strlen";
$retained = 1;
{calls}
echo "done";
"#);
                let output = compile_and_run_with_gc_stats(&program(&body, eval));
                assert!(output.success, "case={case}, eval={eval}: {}", output.stderr);
                assert_eq!(output.stdout, "done");
                let (allocated, freed) = parse_gc_stats(&output.stderr);
                residual.push(allocated as i64 - freed as i64);
            }
            if residual[0] != residual[1] {
                failures.push(format!("case={case}, eval={eval}: {residual:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "Partial MIME callback owners leaked: {}", failures.join("; "));
}

/// Encodes the same header through named calls, closures, dynamic callbacks, and positional spreads.
#[test]
fn test_mbstring_mime_encode_public_calls() {
    let body = r#"
namespace MailHeaders;
mb_internal_encoding("UTF-8");
echo Mb_EnCoDe_MiMeHeAdEr(string: "café 猫", charset: "UTF-8", transfer_encoding: "Q"), "\n";
$encode = mb_encode_mimeheader(...);
echo $encode("café"), "\n";
echo $encode("café", "utf8", "Q"), "\n";
echo call_user_func_array("mb_encode_mimeheader", ["café", "UTF-8", "Q"]), "\n";
$selected = $argc > 0 ? "mb_encode_mimeheader" : "mb_strtoupper";
echo $selected("café", "UTF-8", "Q"), "\n";
$options = ["UTF-8", "Q"];
echo mb_encode_mimeheader("café", ...$options), "\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "=?UTF-8?Q?caf=C3=A9=20=E7=8C=AB?=\n=?UTF-8?B?Y2Fmw6k=?=\n=?UTF-8?Q?caf=C3=A9?=\n=?UTF-8?Q?caf=C3=A9?=\n=?UTF-8?Q?caf=C3=A9?=\n=?UTF-8?Q?caf=C3=A9?=\n");
    }
}

/// Reads language and encoding after Stringable callbacks and uses a fixed replacement policy.
#[test]
fn test_mbstring_mime_encode_request_state() {
    let body = r#"
class OutgoingHeader {
    public function __toString(): string {
        mb_language("German");
        mb_internal_encoding("UTF-16LE");
        return "c\0a\0f\0" . chr(233) . "\0";
    }
}
mb_language("Japanese");
mb_internal_encoding("UTF-8");
echo mb_encode_mimeheader(new OutgoingHeader()), "\n";
mb_internal_encoding("UTF-8");
echo mb_encode_mimeheader("café"), "\n";
echo mb_encode_mimeheader("café", "UTF-8"), "\n";
mb_substitute_character("none");
echo mb_encode_mimeheader(chr(255) . "猫", "ASCII", "Q"), "\n";
echo mb_check_encoding() ? "clean\n" : "dirty\n";
"#;
    for eval in [false, true] {
        assert_eq!(compile_and_run(&program(body, eval)), "=?ISO-8859-15?Q?caf=E9?=\n=?ISO-8859-15?Q?caf=E9?=\n=?UTF-8?B?Y2Fmw6k=?=\n=?US-ASCII?Q?=3F=3F?=\nclean\n");
    }
}

/// Preserves PHP's explicit-null diagnostics, named gaps, charset failures, and runtime arity.
#[test]
fn test_mbstring_mime_encode_runtime_errors() {
    let body = r#"
$encode = $argc > 0 ? "mb_encode_mimeheader" : "mb_strtoupper";
try { $encode(); } catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
try { $encode(1, 2, 3, 4, 5, 6); } catch (ArgumentCountError $error) { echo $error->getMessage(), "\n"; }
try { $encode([]); } catch (TypeError $error) { echo $error->getMessage(), "\n"; }
try { $encode("", "UTF7-IMAP"); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_encode_mimeheader("x", null); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
try { mb_encode_mimeheader(string: "x", newline: "\n"); } catch (ValueError $error) { echo $error->getMessage(), "\n"; }
echo mb_encode_mimeheader("café", "UTF-8", null), "\n";
"#;
    let expected = "mb_encode_mimeheader() expects at least 1 argument, 0 given\nmb_encode_mimeheader() expects at most 5 arguments, 6 given\nmb_encode_mimeheader(): Argument #1 ($string) must be of type string, array given\nmb_encode_mimeheader(): Argument #2 ($charset) \"UTF7-IMAP\" cannot be used for MIME header encoding\nmb_encode_mimeheader(): Passing null to parameter #2 ($charset) of type ?string is deprecated\nmb_encode_mimeheader(): Argument #2 ($charset) must be a valid encoding, \"\" given\nmb_encode_mimeheader(): Passing null to parameter #2 ($charset) of type ?string is deprecated\nmb_encode_mimeheader(): Passing null to parameter #3 ($transfer_encoding) of type ?string is deprecated\nmb_encode_mimeheader(): Argument #2 ($charset) must be a valid encoding, \"\" given\nmb_encode_mimeheader(): Passing null to parameter #3 ($transfer_encoding) of type ?string is deprecated\n=?UTF-8?B?Y2Fmw6k=?=\n";
    let stdout: String = expected.lines().filter(|line| !line.contains("Passing null")).map(|line| format!("{line}\n")).collect();
    let stderr: String = expected.lines().filter(|line| line.contains("Passing null")).map(|line| format!("Deprecated: {line}\n")).collect();
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "{}", output.stderr);
        assert_eq!(output.stdout, stdout, "eval={eval}");
        assert_eq!(output.stderr, stderr, "eval={eval}");
    }
    let strict = r#"<?php declare(strict_types=1);
try { mb_encode_mimeheader("x", null); } catch (TypeError $error) { echo $error->getMessage(); }
"#;
    assert_eq!(compile_and_run(strict), "mb_encode_mimeheader(): Argument #2 ($charset) must be of type string, null given");
}

/// Leaves trailing defaults absent when a runtime-selected callback receives named arguments.
#[test]
fn test_mbstring_mime_encode_dynamic_named_defaults() {
    let body = r#"
$encode = $argc > 0 ? "mb_encode_mimeheader" : "mb_strtoupper";
echo $encode(string: "café"), "\n";
echo $encode(charset: "UTF-8", string: "café"), "\n";
echo call_user_func_array($encode, ["string" => "café"]), "\n";
echo call_user_func_array($encode, ["charset" => "UTF-8", "string" => "café", "transfer_encoding" => "Q"]), "\n";
"#;
    for eval in [false, true] {
        let output = compile_and_run_capture(&program(body, eval));
        assert!(output.success, "eval={eval}: {}", output.stderr);
        assert_eq!(output.stdout, "=?UTF-8?B?Y2Fmw6k=?=\n=?UTF-8?B?Y2Fmw6k=?=\n=?UTF-8?B?Y2Fmw6k=?=\n=?UTF-8?Q?caf=C3=A9?=\n");
        assert!(output.stderr.is_empty(), "eval={eval}: {}", output.stderr);
    }
}

/// Checks one named-call ownership path without combining enough compile cycles to exceed CI limits.
fn check_mime_named_ownership(call: &str) {
    for eval in [false, true] {
        let mut residual = Vec::new();
        for count in [1, 4] {
            let body = format!(r#"
class OwnedOutgoingHeader {{ public function __toString(): string {{ return "café"; }} }}
class OutgoingCharset {{ public function __toString(): string {{ return "UTF-8"; }} }}
function failingOutgoingArgument(): string {{ throw new Exception("argument failed"); }}
$encode = $argc > 0 ? "mb_encode_mimeheader" : "mb_strtoupper";
{}
echo "done";
"#, call.repeat(count));
            let output = compile_and_run_with_gc_stats(&program(&body, eval));
            assert!(output.success, "eval={eval}: {}", output.stderr);
            assert_eq!(output.stdout, "done");
            let (allocated, freed) = parse_gc_stats(&output.stderr);
            residual.push(allocated as i64 - freed as i64);
        }
        assert_eq!(residual[0], residual[1], "eval={eval}: {call}");
    }
}

/// Releases original named argument objects after shared Stringable coercion.
#[test]
fn test_mbstring_mime_encode_named_ownership() {
    check_mime_named_ownership("$encode(string: new OwnedOutgoingHeader(), charset: new OutgoingCharset());");
}

/// Releases associative literal keys, values, and the successful shared result.
#[test]
fn test_mbstring_mime_encode_named_array_ownership() {
    check_mime_named_ownership("call_user_func_array($encode, [\"string\" => new OwnedOutgoingHeader(), \"charset\" => new OutgoingCharset()]); call_user_func_array($encode, [0 => new OwnedOutgoingHeader(), \"charset\" => new OutgoingCharset()]);");
}

/// Releases a named argument container when destination validation raises a PHP error.
#[test]
fn test_mbstring_mime_encode_named_error_ownership() {
    check_mime_named_ownership("try { $encode(string: new OwnedOutgoingHeader(), charset: \"missing\"); } catch (ValueError) {}");
}

/// Releases partial associative literals when a later argument expression throws.
#[test]
fn test_mbstring_mime_encode_named_array_failure_ownership() {
    check_mime_named_ownership("try { call_user_func_array($encode, [\"string\" => new OwnedOutgoingHeader(), \"charset\" => failingOutgoingArgument()]); } catch (Exception) {}");
}
