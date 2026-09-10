//! Purpose:
//! Exercises indexed capture destinations through real compiler-owned Mixed references.
//!
//! Called from:
//! - The runtime-GC capture reference-fill integration tests.
//!
//! Key details:
//! - Only the placeholder invocation is replaced; PHP creates all arrays, aliases, and destructors.
//! - Shared indexed payloads are rejected before mutation until their identity adapter exists.

use super::*;

/// Promotes unique indexed arrays before mixed capture writes and completes writes after a destructor throws.
#[test]
fn test_mbstring_capture_reference_fill_unique_indexed() {
    let source = r#"<?php
function capture_test_indexed(mixed &$matches): int { return count($matches); }
class IndexedCaptureValue {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "destroy\n";
        if ($this->fail) { throw new RuntimeException("indexed capture"); }
    }
}
function run_capture(mixed $matches, bool $fail): void {
    $alias =& $matches;
    $matches = [new IndexedCaptureValue($fail), 2, "keep"];
    try { $status = capture_test_indexed($alias); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo count($matches), ":", $matches[0], ":";
    var_export($alias[1]);
    echo ":", $matches[2], ":", bin2hex($alias["key"]), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_capture(null, false); run_capture(null, true); }
echo "done\n";
"#;
    let expected = "destroy\nstatus:0\n4:whole:false:keep:6100ff7a\ndestroy\ncaught:indexed capture\n4:whole:false:keep:6100ff7a\n".repeat(8);
    assert_indexed_fill(source, &format!("{expected}done\n"));
}

/// Preserves independently owned indexed aliases instead of silently converting only one observer.
#[test]
fn test_mbstring_capture_reference_fill_shared_indexed_unchanged() {
    let source = r#"<?php
function capture_test_indexed(mixed &$matches): int { return count($matches); }
function run_capture(mixed $matches): void {
    $alias =& $matches;
    $original = [41, 42, 43];
    $matches = $original;
    echo "status:", capture_test_indexed($alias), "\n";
    echo $original[0], ":", $original[1], ":", count($original), "\n";
    echo $matches[0], ":", $matches[1], ":", count($matches), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_capture(null); }
echo "done\n";
"#;
    assert_indexed_fill(source, &format!("{}done\n", "status:1\n41:42:3\n41:42:3\n".repeat(8)));
}

/// Preserves string strides and callable capture owners when indexed storage becomes heterogeneous.
#[test]
fn test_mbstring_capture_reference_fill_indexed_strings_and_callables() {
    let source = r#"<?php
function capture_test_indexed(mixed &$matches): int { return count($matches); }
class IndexedCaptureOwner {
    public function __construct(public int $number) {}
    public function __destruct() { echo "drop:", $this->number, "\n"; }
}
function make_capture(int $number): callable {
    $owner = new IndexedCaptureOwner($number);
    return function() use ($owner): int { return $owner->number; };
}
function run_strings(mixed $matches): void {
    $alias =& $matches;
    $matches = ["one", "two", "binary\0tail"];
    echo "strings:", capture_test_indexed($alias), ":", bin2hex($matches[2]), "\n";
}
function run_callables(mixed $matches): void {
    $alias =& $matches;
    $matches = [make_capture(1), make_capture(2), make_capture(3), null];
    $status = capture_test_indexed($alias);
    $callback = $matches[2];
    echo "callables:", $status, ":", $callback(), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_strings(null); run_callables(null); }
echo "done\n";
"#;
    let expected = "strings:0:62696e617279007461696c\ndrop:1\ndrop:2\ncallables:0:3\ndrop:3\n".repeat(8);
    assert_indexed_fill(source, &format!("{expected}done\n"));
}

/// Keeps the per-element null or integer tag when a nullable scalar array changes representation.
#[test]
fn test_mbstring_capture_reference_fill_indexed_nullable_scalars() {
    let source = r#"<?php
function capture_test_indexed(mixed &$matches): int { return count($matches); }
function capture_number(bool $present): ?int { return $present ? 7 : null; }
function run_capture(mixed $matches, bool $present): void {
    $alias =& $matches;
    $matches = [capture_number($present), capture_number(!$present), capture_number($present)];
    echo "status:", capture_test_indexed($alias), ":", count($matches), ":";
    var_export($matches[2]);
    echo "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_capture(null, true); run_capture(null, false); }
echo "done\n";
"#;
    assert_indexed_fill(source, &format!("{}done\n", "status:0:4:7\nstatus:0:4:NULL\n".repeat(8)));
}

/// Runs a native capture graph against caller reference storage and requires a clean final heap.
fn assert_indexed_fill(source: &str, expected: &str) {
    let graph = ArrayGraph::new(0, vec![vec![
        (Key::Int(0), Value::String(b"whole".to_vec())),
        (Key::Int(1), Value::Bool(false)),
        (Key::String(b"key".to_vec()), Value::String(b"a\0\xffz".to_vec())),
    ]]).unwrap().encode();
    let directory = make_cli_test_dir("mbstring_capture_indexed");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let mut patched = replace_function(&assembly, "capture_test_indexed", &indexed_shim(graph.len()));
    let bytes = graph.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
    patched.push_str(&format!("\n.data\n_capture_test_graph:\n.byte {bytes}\n"));
    std::fs::write(directory.join("caller.s"), &patched).unwrap();
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}\n{}\n{}", directory.display(), output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}\n{}", directory.display(), output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    let _ = std::fs::remove_dir_all(directory);
}

/// Adapts a native Mixed child-slot address to the retained reference expected by capture filling.
fn indexed_shim(length: usize) -> String {
    if target().arch == Arch::AArch64 {
        let graph = if target().platform == Platform::Linux {
            "adrp x2, _capture_test_graph\nadd x2, x2, :lo12:_capture_test_graph"
        } else {
            "adrp x2, _capture_test_graph@PAGE\nadd x2, x2, _capture_test_graph@PAGEOFF"
        };
        return format!(r#"
    stp x29, x30, [sp, #-16]!
    sub x1, x0, #8
    mov x0, #0
    {graph}
    mov x3, #{length}
    bl __rt_mbstring_capture_reference_fill
    ldp x29, x30, [sp], #16
    cmp x0, #2
    b.eq __rt_throw_current
    ret
"#);
    }
    format!(r#"
    push rbp
    mov rbp, rsp
    lea rsi, [rdi - 8]
    xor edi, edi
    lea rdx, [rip + _capture_test_graph]
    mov ecx, {length}
    call __rt_mbstring_capture_reference_fill
    pop rbp
    cmp eax, 2
    je __rt_throw_current
    ret
"#)
}
