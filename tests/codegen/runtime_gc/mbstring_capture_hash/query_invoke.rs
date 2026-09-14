//! Purpose:
//! Exercises the complete native V5 mb_parse_str host against real compiled PHP references.
//!
//! Called from:
//! - Runtime-GC tests using the actual Rust query parser, native callbacks, and PHP unwinder.
//!
//! Key details:
//! - A small C embedding host supplies live configuration and optional SAPI filtering.
//! - Only the fixture entry is replaced; PHP coercion, initialization, parsing, and cleanup stay real.
//! - These tests do not claim public INI or mb_parse_str lvalue binding support.

use super::*;
use std::process::Command;

/// Coerces Stringable input before destructive output initialization and completes writes after throws.
#[test]
fn test_mbstring_query_invoke_native_v5() {
    let source = r#"<?php
function query_test_parse(mixed $source, mixed &$output): bool { return strlen((string)$source) > 0; }
class QueryInvokeSource {
    public function __toString(): string {
        echo "string\n";
        return "word=caf%C3%A9&nested[x]=ok&list[]=one&list[]=two&bin=a%00z";
    }
}
class QueryInvokeOld {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "old\n";
        if ($this->fail) { throw new RuntimeException("query initialize"); }
    }
}
function run_query(mixed $output, bool $fail, mixed $source): void {
    $alias =& $output;
    $output = new QueryInvokeOld($fail);
    try { $status = query_test_parse($source, $alias); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo json_encode($output), ":", mb_http_input(), ":";
    var_export(mb_http_input("S"));
    echo "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    run_query(null, false, new QueryInvokeSource());
    run_query(null, true, new QueryInvokeSource());
}
echo "done\n";
"#;
    let value = "{\"word\":\"caf\\u00e9\",\"nested\":{\"x\":\"ok\"},\"list\":[\"one\",\"two\"],\"bin\":\"a\\u0000z\"}:UTF-8:false\n";
    let expected = (format!("string\nold\nstatus:1\n{value}string\nold\ncaught:query initialize\n{value}")).repeat(8) + "done\n";
    for regex in [false, true] {
        let source = if regex { source.replace("mb_strlen(\"\");", "mb_strlen(\"\"); mb_ereg_match(\"\", \"\");") }
            else { source.to_owned() };
        run(&source, &expected, &[], false);
    }
}

/// Uses independent configuration/filter context and rereads nesting limits after filter callbacks.
#[test]
fn test_mbstring_query_invoke_native_filter_policy() {
    let source = r#"<?php
function query_test_parse(mixed $source, mixed &$output): bool { return strlen((string)$source) > 0; }
function query_test_observe(int &$mode): int { return $mode; }
function run_query(mixed $output): void {
    $alias =& $output;
    $output = "before";
    echo query_test_parse("keep=a%00z;skip=no&replace=old&limit=1&deep[x][y]=v&last=ok", $alias), "\n";
    $mode = 0;
    echo json_encode($output), ":", query_test_observe($mode), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_query(null); }
echo "done\n";
"#;
    let expected = "1\n{\"keep\":\"a\\u0000z\",\"replace\":\"filtered\",\"limit\":\"1\",\"last\":\"ok\"}:10501\n".repeat(8) + "done\n";
    run(source, &expected, &["-DFILTERED=1"], true);
}

/// Reads the real Core request provider after a native filter changes diagnostic visibility.
#[test]
fn test_mbstring_query_invoke_shared_core_policy() {
    let query = format!("keep=a%00z&skip=no&replace=old&limit=1&deep{}=v&last=ok", "[x]".repeat(66));
    let source = r#"<?php
function query_test_parse(mixed $source, mixed &$output): bool { return strlen((string)$source) > 0; }
function query_test_observe(int &$mode): int { return $mode; }
function run_query(mixed $output): void {
    $alias =& $output;
    echo query_test_parse("QUERY", $alias), "\n";
    $mode = 0;
    echo json_encode($output), ":", query_test_observe($mode), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_query(null); }
echo "done\n";
"#.replace("QUERY", &query);
    let expected = "1\n{\"keep\":\"a\\u0000z\",\"replace\":\"filtered\",\"limit\":\"1\",\"last\":\"ok\"}:10501\n".repeat(8) + "done\n";
    run(&source, &expected, &["-DFILTERED=1", "-DCORE_POLICY=1"], true);
}

/// Rejects an absent required host capability before Stringable coercion or output initialization.
#[test]
fn test_mbstring_query_invoke_native_missing_configuration() {
    let source = r#"<?php
function query_test_parse(mixed $source, mixed &$output): int { return strlen((string)$source); }
class QueryMissingSource {
    public function __toString(): string { echo "unexpected coercion\n"; return "key=value"; }
}
class QueryMissingOutput {
    public int $value = 42;
    public function __destruct() { echo "old\n"; }
}
function run_query(mixed $output): void {
    $alias =& $output;
    $output = new QueryMissingOutput();
    echo query_test_parse(new QueryMissingSource(), $alias), ":", $output->value, "\n";
    echo "before cleanup\n";
}
mb_strlen("");
run_query(null);
echo "done\n";
"#;
    run(source, "1:42\nbefore cleanup\nold\ndone\n", &["-DCONFIGURED=0"], false);
}

/// Checks arity before coercion and preserves the original reference on strict or structural type errors.
#[test]
fn test_mbstring_query_invoke_native_argument_failures() {
    let source = r#"<?php
function query_test_parse(mixed $source, mixed &$output): bool { return strlen((string)$source) > 0; }
class QueryInvalidInput {
    public function __toString(): string { echo "unexpected coercion\n"; return "key=value"; }
}
class QueryPreservedOutput {
    public int $value = 42;
    public function __destruct() { echo "old\n"; }
}
function run_query(mixed $output, mixed $source): void {
    $alias =& $output;
    $output = new QueryPreservedOutput();
    try { query_test_parse($source, $alias); echo "unexpected success\n"; }
    catch (Throwable $error) { echo get_class($error), ":", $output->value, "\n"; }
    echo "before cleanup\n";
}
mb_strlen("");
run_query(null, QUERY_INPUT);
echo "done\n";
"#;
    for (input, flags, error) in [
        ("new QueryInvalidInput()", vec!["-DARG_COUNT=0"], "ArgumentCountError"),
        ("new QueryInvalidInput()", vec!["-DARG_COUNT=1"], "ArgumentCountError"),
        ("new QueryInvalidInput()", vec!["-DARG_COUNT=3"], "ArgumentCountError"),
        ("[]", vec![], "TypeError"),
        ("42", vec!["-DSTRICT_TYPES=1"], "TypeError"),
    ] {
        run(&source.replace("QUERY_INPUT", input), &format!("{error}:42\nbefore cleanup\nold\ndone\n"), &flags, false);
    }
}

/// Compiles a real C host and links its assembly beside the original PHP callers and runtime.
fn run(source: &str, expected: &str, defines: &[&str], observe: bool) {
    // The replacement invokes PHP and changes request state, so retain those effects in the fixture's EIR.
    let source = source.replace("{ return strlen((string)$source)",
        "{ mb_internal_encoding(\"UTF-8\"); if (is_null($source)) { throw new RuntimeException(\"fixture\"); } return strlen((string)$source)");
    let directory = make_cli_test_dir("mbstring_query_native_invoke");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(
        &source, &directory, 8_388_608, true, true);
    let jump = if target().arch == Arch::AArch64 { "b" } else { "jmp" };
    let mut patched = replace_function(&assembly, "query_test_parse", &format!("{jump} _query_invoke_fixture\n"));
    if observe { patched = replace_function(&patched, "query_test_observe", &format!("{jump} _query_invoke_observe\n")); }
    let provider = directory.join("provider.c");
    let provider_asm = directory.join("provider.s");
    std::fs::write(&provider, include_str!("query_invoke/provider.c")).unwrap();
    let mut compiler = Command::new("cc");
    compiler.args(["-S", "-O2", "-Wall", "-Wextra", "-Werror"]);
    if target().arch == Arch::X86_64 { compiler.arg("-masm=intel"); }
    let built = compiler.args(defines).arg(&provider).arg("-o").arg(&provider_asm).output().unwrap();
    assert!(built.status.success(), "{}: {}", directory.display(), String::from_utf8_lossy(&built.stderr));
    patched.push_str(&format!("\n.text\n{}\n", status_shim()));
    patched.push_str(&std::fs::read_to_string(provider_asm).unwrap());
    std::fs::write(directory.join("caller.s"), &patched).unwrap();
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}: {}\n{}", directory.display(), output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", directory.display());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {}", directory.display(), output.stderr);
    std::fs::remove_dir_all(directory).unwrap();
}

/// Returns only the native status for the intentionally incomplete host test without exiting PHP.
fn status_shim() -> &'static str {
    if target().arch == Arch::AArch64 {
        ".globl _query_fixture_invoke_status\n_query_fixture_invoke_status:\nstp x29, x30, [sp, #-16]!\nbl __rt_mbstring_query_invoke\nmov x0, x1\nldp x29, x30, [sp], #16\nret\n"
    } else {
        ".globl _query_fixture_invoke_status\n_query_fixture_invoke_status:\nsub rsp, 8\ncall __rt_mbstring_query_invoke\nmov rax, rdx\nadd rsp, 8\nret\n"
    }
}
