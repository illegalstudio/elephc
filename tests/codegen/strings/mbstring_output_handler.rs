//! Purpose:
//! Verifies public mb_output_handler calls and output-buffer callbacks in native and opaque eval code.
//!
//! Called from:
//! - The focused codegen string integration harness.
//!
//! Key details:
//! - Output remains buffered until conversion completes so MIME metadata stays mutable.
//! - Binary results, decoder phases, argument planning, and native ownership are observable.

use crate::support::*;

/// Exercises direct, named, unpacked, and callable conversion with a split UTF-8 decoder sequence.
#[test]
fn test_mbstring_output_handler_native_calls() {
    clean(r#"<?php
namespace OutputCalls;
header("Content-Type: text/plain");
mb_internal_encoding("UTF-8"); mb_http_output("UTF-16LE");
ob_start();
echo bin2hex(Mb_Output_Handler("é\0😀", 9)), "\n";
echo bin2hex(mb_output_handler("\xe2", 1)), ":", bin2hex(mb_output_handler("\x82\xac", 8)), "\n";
function phase(): int { echo "P"; return 9; }
function text(): string { echo "T"; return "é"; }
echo bin2hex(mb_output_handler(status: phase(), string: text())), "\n";
echo bin2hex(mb_output_handler(...["status" => 9, "string" => "€"])), "\n";
$handler = \mb_output_handler(...);
echo bin2hex($handler("é", 9)), "\n";
echo bin2hex(call_user_func("mb_output_handler", "€", 9)), "\n";
ob_end_flush();
"#, "e90000003dd800de\n3f00:3f003f00\nPTe900\nac20\ne900\nac20\n");
}

/// Uses the registered shared runtime through real eval, including dynamic arity and type errors.
#[test]
fn test_mbstring_output_handler_eval_calls() {
    clean(r#"<?php
mb_internal_encoding("UTF-8"); mb_http_output("UTF-16LE");
$source = $argc > 0 ? '
header("Content-Type: text/plain"); ob_start();
echo bin2hex(mb_output_handler(status: 9, string: "é")), "\n";
$handler = mb_output_handler(...);
echo bin2hex($handler(...["string" => "€", "status" => 9])), "\n";
echo bin2hex(call_user_func("mb_output_handler", "é", 9)), "\n";
try { mb_output_handler(); } catch (ArgumentCountError $e) { echo "arity\n"; }
try { mb_output_handler([], 9); } catch (TypeError $e) { echo "string-type\n"; }
try { mb_output_handler("a", []); } catch (TypeError $e) { echo "status-type\n"; }
ob_end_flush();
' : '';
eval($source);
"#, "e900\nac20\ne900\narity\nstring-type\nstatus-type\n");
}

/// Runs native string-name and first-class handlers with incremental flush and final phases.
#[test]
fn test_mbstring_output_handler_native_buffers() {
    clean(r#"<?php
header("Content-Type: text/plain");
mb_internal_encoding("UTF-8"); mb_http_output("UTF-16LE");
ob_start();
ob_start("mb_output_handler");
echo "\xe2"; ob_flush(); echo "\x82\xac"; ob_end_flush();
$first = ob_get_clean();
ob_start(); ob_start(mb_output_handler(...));
echo "é\0😀"; ob_end_flush(); $second = ob_get_clean();
echo bin2hex($first), ":", bin2hex($second), "\n";
"#, "3f003f003f00:e90000003dd800de\n");
}

/// Invokes a builtin handler installed by opaque eval using the native request's encoding state.
#[test]
fn test_mbstring_output_handler_eval_buffer() {
    clean(r#"<?php
mb_internal_encoding("UTF-8"); mb_http_output("UTF-16LE");
$source = $argc > 0 ? '
header("Content-Type: text/plain"); ob_start(); ob_start("mb_output_handler");
echo "é"; ob_flush(); echo "€"; ob_end_flush();
$converted = ob_get_clean(); echo bin2hex($converted), "\n";
' : '';
eval($source);
"#, "e900ac20\n");
}

/// Requires the expected PHP-visible bytes and balanced native owners after every complete fixture.
fn clean(source: &str, expected: &str) {
    let output = compile_and_run_with_heap_debug(source);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    assert!(!output.stderr.contains("Warning:"), "{}", output.stderr);
}

/// Runs one ownership boundary independently so the focused CI timeout applies to a single program.
fn ownership_case(name: &str) {
    let cases = [
        ("native_buffer", "ob_start();ob_start(\"mb_output_handler\");echo \"é\";ob_end_flush();$b=ob_get_clean();echo bin2hex($b);", "e900"),
        ("native_identity", "ob_start();ob_start(function($b,$p){return $b;});echo \"é\";ob_end_flush();$b=ob_get_clean();echo bin2hex($b);", "c3a9"),
        ("eval_direct", "echo bin2hex(mb_output_handler(\"é\",9));", "e900"),
        ("eval_callable", "$h=mb_output_handler(...);echo bin2hex($h(\"é\",9));", "e900"),
        ("eval_array", "$h=mb_output_handler(...);echo bin2hex($h(...[\"string\"=>\"é\",\"status\"=>9]));", "e900"),
        ("eval_error", "try {mb_output_handler([],9);}catch(TypeError $e){echo \"caught\";}", "caught"),
        ("eval_buffer", "ob_start();ob_start(\"mb_output_handler\");echo \"é\";ob_end_flush();$b=ob_get_clean();echo bin2hex($b);", "e900"),
        ("eval_plain", "ob_start();echo \"é\";$b=ob_get_clean();echo bin2hex($b);", "c3a9"),
        ("eval_header", "header(\"Content-Type: text/plain\");echo \"ready\";", "ready"),
    ];
    let (_, body, expected) = cases.into_iter().find(|(case, _, _)| *case == name).unwrap();
    let body = if name.starts_with("eval_") {
        let body = body.replace('\\', "\\\\").replace('\'', "\\'");
        format!("$source=$argc>0?'{body}':'';eval($source);")
    } else { body.into() };
    let source = format!("<?php mb_internal_encoding(\"UTF-8\"); mb_http_output(\"UTF-16LE\"); {body}");
    let output = compile_and_run_with_heap_debug(&source);
    assert!(output.success, "{name}: {}", output.stderr);
    assert_eq!(output.stdout, expected, "{name}");
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{name}: {}", output.stderr);
}

/// Retires the native builtin handler's result copy after buffer publication.
#[test]
fn test_mbstring_output_handler_ownership_native_buffer() { ownership_case("native_buffer"); }

/// Preserves an identity handler's argument alias while releasing its descriptor and result copy.
#[test]
fn test_mbstring_output_handler_ownership_native_identity() { ownership_case("native_identity"); }

/// Consumes a shared output result passed directly into eval bin2hex.
#[test]
fn test_mbstring_output_handler_ownership_eval_direct() { ownership_case("eval_direct"); }

/// Consumes the output result returned through an eval first-class callable.
#[test]
fn test_mbstring_output_handler_ownership_eval_callable() { ownership_case("eval_callable"); }

/// Releases named unpacking storage and the first-class callable's temporary result.
#[test]
fn test_mbstring_output_handler_ownership_eval_array() { ownership_case("eval_array"); }

/// Releases invalid argument arrays and the caught parameter exception.
#[test]
fn test_mbstring_output_handler_ownership_eval_error() { ownership_case("eval_error"); }

/// Retires callback invocation values and the owning eval context's registered handler.
#[test]
fn test_mbstring_output_handler_ownership_eval_buffer() { ownership_case("eval_buffer"); }

/// Preserves ordinary eval buffer ownership alongside the output-handler integration.
#[test]
fn test_mbstring_output_handler_ownership_eval_plain() { ownership_case("eval_plain"); }

/// Releases the evaluated header argument after response metadata has copied its bytes.
#[test]
fn test_mbstring_output_handler_ownership_eval_header() { ownership_case("eval_header"); }
