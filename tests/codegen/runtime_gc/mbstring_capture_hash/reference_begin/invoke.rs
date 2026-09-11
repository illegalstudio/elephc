//! Purpose:
//! Runs the native V4 capture host through the real shared regex coordinator and PHP destructors.
//!
//! Called from:
//! - The native reference-initialization runtime-GC fixture.
//!
//! Key details:
//! - Only fixture entry bodies are replaced; coercion, Oniguruma, capture filling, and cleanup are real.
//! - Destructors change regex options before compilation and can replace the reference or throw.
//! - Displaced initialization owners survive per-call cleanup and cycle collection until request teardown.

use super::*;
use elephc_builtin_contract::RuntimeBuiltinId;

#[path = "invoke/aot_local.rs"]
mod aot_local;

#[path = "invoke/deferred.rs"]
pub(in crate::codegen::runtime_gc) mod deferred;

/// Completes shared captures after destructive initialization, retaining live settings and deferred owners.
#[test]
fn test_mbstring_capture_invoke_native_v4() {
    let source = r#"<?php
function capture_test_initialize(int &$mode): int {
    $old = capture_test_old_value($mode);
    return $old->mode;
}
function capture_test_observe(int &$mode): int { return $mode; }
function capture_test_release_root(int &$mode): int { return $mode; }
function capture_test_release_deferred(int &$mode): int { return $mode; }
function capture_test_collect(int &$mode): int { return $mode; }
function capture_test_retarget(array $replacement): int { return count($replacement); }
function capture_test_old_value(int $mode): NativeCaptureInvokeOldValue { return new NativeCaptureInvokeOldValue($mode); }
class NativeCaptureInvokeLateValue {
    public function __construct(public int $mode) {}
    public function __destruct() { echo "late:", $this->mode, "\n"; }
}
class NativeCaptureInvokeOldValue {
    public function __construct(public int $mode) {}
    public function __destruct() {
        echo "old:", $this->mode, "\n";
        $mode = $this->mode;
        echo "visible:", capture_test_observe($mode), "\n";
        mb_regex_set_options("i");
        if ($mode % 4 >= 2) {
            $replacement = ["late" => new NativeCaptureInvokeLateValue($mode)];
            echo "retarget:", capture_test_retarget($replacement), "\n";
        }
        if ($mode >= 4) { throw new RuntimeException("capture initialization"); }
    }
}
function run_capture(int $mode): void {
    mb_regex_set_options("r");
    try { $status = capture_test_initialize($mode); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo "after:", capture_test_observe($mode), "\n";
    echo "release root\n";
    $status = capture_test_release_root($mode);
    echo "root status:", $status, "\n";
    echo "release deferred\n";
    $status = capture_test_release_deferred($mode);
    echo "deferred status:", $status, "\nend\n";
}
mb_ereg_match("", "");
for ($i = 0; $i < 8; $i++) {
    for ($mode = 0; $mode < 8; $mode++) { run_capture($mode); }
}
$collect = 0;
echo "collection:", capture_test_collect($collect), "\n";
echo "done\n";
"#;
    let mut expected = String::new();
    for _ in 0..8 {
        for mode in 0..8 {
            let typed = mode % 2 == 1;
            let replacement = mode % 4 >= 2;
            expected.push_str(&format!("old:{mode}\nvisible:{}\n", if typed { 10 } else { 8 }));
            if replacement { expected.push_str("retarget:0\n"); }
            expected.push_str(if mode >= 4 { "caught:capture initialization\n" } else { "status:256\n" });
            expected.push_str(&format!("after:{}\nrelease root\n", if typed && replacement { 14 } else { 13 }));
            if typed && replacement { expected.push_str(&format!("late:{mode}\n")); }
            expected.push_str("root status:0\nrelease deferred\n");
            expected.push_str("deferred status:0\nend\n");
        }
    }
    expected.push_str("collection:0\ndone\n");
    expected.push_str(&"late:2\nlate:6\n".repeat(8));
    assert_capture(RuntimeBuiltinId::MbEreg, source, "(?<key>a)", 3, true, false, &expected, "");
}

/// Preserves the old output until initialization is allowed and supports two-argument calls without state.
#[test]
fn test_mbstring_capture_invoke_native_protocol() {
    use RuntimeBuiltinId::{MbEreg, MbEregi};
    let source = r#"<?php
function capture_test_initialize(int &$mode): int {
    $old = capture_test_old_value($mode);
    return $old->mode;
}
function capture_test_observe(int &$mode): int { return $mode; }
function capture_test_release_root(int &$mode): int { return $mode; }
function capture_test_release_deferred(int &$mode): int { return $mode; }
function capture_test_old_value(int $mode): NativeCaptureProtocolOld { return new NativeCaptureProtocolOld($mode); }
class NativeCaptureProtocolOld {
    public function __construct(public int $mode) {}
    public function __destruct() { echo "old\n"; }
}
function run_capture(int $mode): void {
    try { $status = capture_test_initialize($mode); echo "status:", $status, "\n"; }
    catch (Throwable $error) { echo get_class($error), ":", $error->getMessage(), "\n"; }
    echo "after:", capture_test_observe($mode), "\nrelease root\n";
    $status = capture_test_release_root($mode);
    echo "root status:", $status, "\n";
    $status = capture_test_release_deferred($mode);
    echo "deferred status:", $status, "\n";
}
mb_ereg_match("", "");
run_capture(0);
"#;
    for (operation, pattern, count, state, invalid_mode, prefix, original, warning) in [
        (MbEreg, "A", 3, true, false, "old\nstatus:256\nafter:11\n", false, ""),
        (MbEreg, "A", 2, false, false, "status:256\nafter:6\n", true, ""),
        (MbEreg, "a", 2, false, false, "status:0\nafter:6\n", true, ""),
        (MbEregi, "a", 2, false, false, "status:256\nafter:6\n", true, ""),
        (MbEregi, "a", 3, true, false, "old\nstatus:256\nafter:11\n", false, ""),
        (MbEreg, "", 3, true, false, "ValueError:mb_ereg(): Argument #1 ($pattern) must not be empty\nafter:6\n", true, ""),
        (MbEregi, "", 3, true, false, "ValueError:mb_eregi(): Argument #1 ($pattern) must not be empty\nafter:6\n", true, ""),
        (MbEreg, "A", 1, false, false, "ArgumentCountError:mb_ereg() expects at least 2 arguments, 1 given\nafter:6\n", true, ""),
        (MbEreg, "A", 3, false, false, "status:1\nafter:6\n", true, ""),
        (MbEreg, "A", 3, true, true, "status:1\nafter:6\n", true, ""),
        (MbEreg, "[", 3, true, false, "old\nstatus:0\nafter:10\n", false,
            "Warning: mb_ereg(): mbregex compile err: premature end of char-class\n"),
    ] {
        let expected = format!("{prefix}release root\n{}root status:0\ndeferred status:0\n",
            if original { "old\n" } else { "" });
        assert_capture(operation, source, pattern, count, state, invalid_mode, &expected, warning);
    }
}

/// Links one complete capture fixture and checks its exact PHP trace, diagnostics, and heap ownership.
fn assert_capture(operation: RuntimeBuiltinId, source: &str, pattern: &str, count: usize, state: bool, invalid_mode: bool,
    expected: &str, warning: &str) {
    let directory = make_cli_test_dir("mbstring_capture_v4");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let factory = function_symbol(&assembly, "capture_test_old_value");
    let initialize = begin_shim(&factory);
    assert_eq!(initialize.matches("__rt_mbstring_capture_reference_begin").count(), 1);
    let mut initialize = initialize.replace("__rt_mbstring_capture_reference_begin", "_capture_test_coordinator");
    if invalid_mode {
        initialize = initialize.replace("and x2, x2, #1", "mov x2, #2").replace("and edx, 1", "mov edx, 2");
    }
    let mut patched = replace_function(&assembly, "capture_test_initialize", &initialize);
    patched = replace_function(&patched, "capture_test_observe", &observe_shim());
    if source.contains("function capture_test_retarget(") {
        patched = replace_function(&patched, "capture_test_retarget", &reference_fill::retarget_shim());
    }
    patched = replace_function(&patched, "capture_test_release_root", &release_shim("_capture_test_writer"));
    patched = replace_function(&patched, "capture_test_release_deferred", &release_shim("_capture_test_discarded"));
    if source.contains("function capture_test_collect(") {
        patched = replace_function(&patched, "capture_test_collect", &deferred::collect_shim());
    }
    patched.push_str(&format!("\n.text\n_capture_test_coordinator:\n{}", coordinator_shim(operation, pattern.len(), count, state)));
    for name in ["_capture_test_writer", "_capture_test_discarded"] {
        patched.push_str(&format!("\n{}\n", hash_slot_assembly(name).2));
    }
    let bytes = pattern.bytes().chain(std::iter::once(0)).map(|byte| byte.to_string()).collect::<Vec<_>>().join(",");
    patched.push_str(&format!("\n.data\n_capture_test_pattern:\n.byte {bytes}\n_capture_test_subject:\n.ascii \"A\"\n"));
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    let _ = std::fs::remove_dir_all(directory);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    assert_eq!(output.stdout, expected);
    let diagnostics = output.stderr.lines().filter(|line| line.starts_with("Warning:")).collect::<Vec<_>>().join("\n");
    assert_eq!(diagnostics, warning.trim_end());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
}

/// Runs a complete capture call and reports status plus the boolean result shifted eight bits.
fn coordinator_shim(operation: RuntimeBuiltinId, pattern_len: usize, count: usize, state: bool) -> String {
    let operation = operation.as_u32();
    if target().arch == Arch::AArch64 {
        let address = |register: &str, symbol: &str| {
            if target().platform == Platform::Linux {
                format!("adrp {register}, {symbol}\nadd {register}, {register}, :lo12:{symbol}")
            } else {
                format!("adrp {register}, {symbol}@PAGE\nadd {register}, {register}, {symbol}@PAGEOFF")
            }
        };
        let pattern = address("x9", "_capture_test_pattern");
        let subject = address("x9", "_capture_test_subject");
        let capture_state = if state { "mov x5, sp" } else { "mov x5, #0" };
        return format!(r#"
    sub sp, sp, #128
    stp x29, x30, [sp, #112]
    str x2, [sp]
    str xzr, [sp, #8]
    str x1, [sp, #88]
    str x3, [sp, #96]
    add x9, sp, #40
    str x9, [sp, #16]
    add x9, sp, #64
    str x9, [sp, #24]
    str x1, [sp, #32]
    mov x10, #1
    str x10, [sp, #40]
    str x10, [sp, #64]
    str x10, [sp, #80]
    {pattern}
    str x9, [sp, #48]
    mov x10, #{pattern_len}
    str x10, [sp, #56]
    {subject}
    str x9, [sp, #72]
    mov x0, #{operation}
    add x1, sp, #16
    mov x2, #{count}
    mov x3, #0
    mov x4, #0
    {capture_state}
    bl __rt_mbstring_capture_invoke
    lsl x0, x0, #8
    orr x0, x0, x1
    str x0, [sp, #104]
    ldr x0, [sp, #88]
    bl __rt_incref
    ldr x9, [sp, #96]
    mov x10, #1
    str x10, [x9]
    str x0, [x9, #8]
    ldr x10, [sp, #8]
    str x10, [x9, #16]
    ldr x0, [sp, #104]
    ldp x29, x30, [sp, #112]
    add sp, sp, #128
    ret
"#);
    }
    let capture_state = if state { "mov r9, rsp" } else { "xor r9d, r9d" };
    format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 112
    mov QWORD PTR [rsp], rdx
    mov QWORD PTR [rsp + 8], 0
    mov QWORD PTR [rsp + 88], rsi
    mov QWORD PTR [rsp + 96], rcx
    lea r10, [rsp + 40]
    mov QWORD PTR [rsp + 16], r10
    lea r10, [rsp + 64]
    mov QWORD PTR [rsp + 24], r10
    mov QWORD PTR [rsp + 32], rsi
    mov QWORD PTR [rsp + 40], 1
    mov QWORD PTR [rsp + 64], 1
    mov QWORD PTR [rsp + 80], 1
    lea r10, [rip + _capture_test_pattern]
    mov QWORD PTR [rsp + 48], r10
    mov QWORD PTR [rsp + 56], {pattern_len}
    lea r10, [rip + _capture_test_subject]
    mov QWORD PTR [rsp + 72], r10
    mov edi, {operation}
    lea rsi, [rsp + 16]
    mov edx, {count}
    xor ecx, ecx
    xor r8d, r8d
    {capture_state}
    call __rt_mbstring_capture_invoke
    shl rax, 8
    or rax, rdx
    mov QWORD PTR [rsp + 104], rax
    mov rax, QWORD PTR [rsp + 88]
    call __rt_incref
    mov r10, QWORD PTR [rsp + 96]
    mov QWORD PTR [r10], 1
    mov QWORD PTR [r10 + 8], rax
    mov r11, QWORD PTR [rsp + 8]
    mov QWORD PTR [r10 + 16], r11
    mov rax, QWORD PTR [rsp + 104]
    leave
    ret
"#)
}
