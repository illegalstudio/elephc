//! Purpose:
//! Verifies native capture publication through real compiler-owned Mixed reference cells.
//!
//! Called from:
//! - The V4 capture invocation codegen tests.
//!
//! Key details:
//! - PHP lowering creates the local, alias, callback, and reference storage without test replacements.
//! - A native call shim checks the carrier and bridges its child address to the internal V4 host.
//! - Discarded scalar arrays move into request storage and are released during native teardown.

use super::*;

/// Publishes captures through caller aliases even when a callback retargets the cell or throws.
#[test]
fn test_mbstring_capture_invoke_aot_local_reference() {
    let source = r#"<?php
function capture_test_local(mixed &$matches): int { return is_array($matches) ? count($matches) : 0; }
class NativeAotCaptureOld {
    public function __construct(public Closure $observe, public bool $fail) {}
    public function __destruct() {
        ($this->observe)();
        mb_regex_set_options("i");
        if ($this->fail) { throw new RuntimeException("local capture"); }
    }
}
function run_capture(mixed $matches, bool $fail): void {
    $alias =& $matches;
    $observe = function() use (&$alias): void {
        echo $alias === null ? "visible:null\n" : "visible:other\n";
        $alias = ["during" => "callback"];
    };
    $matches = new NativeAotCaptureOld($observe, $fail);
    mb_regex_set_options("r");
    try { echo "status:", capture_test_local($alias), "\n"; }
    catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    echo count($matches), ":", $alias[0], ":", $matches["key"], ":", $alias[1], "\n";
}
mb_ereg_match("", "");
for ($i = 0; $i < 8; $i++) { run_capture(null, false); run_capture(null, true); }
echo "done\n";
"#;
    let directory = make_cli_test_dir("mbstring_capture_aot_local");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let mut patched = replace_function(&assembly, "capture_test_local", &local_shim());
    patched.push_str(&format!("\n.text\n_capture_test_coordinator:\n{}", coordinator_shim(RuntimeBuiltinId::MbEreg, 9, 3, true)));
    patched.push_str("\n.data\n_capture_test_pattern:\n.ascii \"(?<key>a)\"\n_capture_test_subject:\n.ascii \"A\"\n");
    std::fs::write(directory.join("caller.s"), &patched).unwrap();
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}\n{}\n{}", directory.display(), output.stdout, output.stderr);
    assert_eq!(output.stdout, format!("{}done\n",
        "status:visible:null\n256\n3:A:A:A\nstatus:visible:null\ncaught:local capture\n3:A:A:A\n".repeat(8)),
        "{}\n{}", directory.display(), output.stderr);
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    let _ = std::fs::remove_dir_all(directory);
}

/// Keeps an identity pin alive after ordinary and protected destructor frames release their local owners.
#[test]
fn test_mbstring_capture_aot_reference_pin_survives_frame() {
    let source = r#"<?php
function capture_test_hold(mixed &$value): int { return is_array($value) ? count($value) : 0; }
function capture_test_read(int &$marker): int { return $marker; }
function capture_test_drop(int &$marker): int { return $marker; }
class NativePinnedCaptureValue {
    public function __destruct() { echo "destroy value\n"; }
}
class NativePinnedCaptureScope {
    public function __construct(public mixed $seed, public bool $fail) {}
    public function __destruct() {
        $value = $this->seed;
        $alias =& $value;
        $value = new NativePinnedCaptureValue();
        echo "hold:", capture_test_hold($alias), "\n";
        if ($this->fail) { throw new RuntimeException("scope"); }
    }
}
function hold_local(mixed $value): void {
    $alias =& $value;
    $value = new NativePinnedCaptureValue();
    echo "hold:", capture_test_hold($alias), "\n";
}
function check_held(): void {
    $marker = 0;
    echo "after frame:", capture_test_read($marker), "\n";
    echo "drop:", capture_test_drop($marker), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    hold_local(null); check_held();
    $scope = new NativePinnedCaptureScope(null, false); unset($scope); check_held();
    $scope = new NativePinnedCaptureScope(null, true);
    try { unset($scope); } catch (Throwable $error) { echo "caught:", $error->getMessage(), "\n"; }
    check_held();
}
echo "done\n";
"#;
    let directory = make_cli_test_dir("mbstring_capture_aot_pin");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(source, &directory, 8_388_608, true, true);
    let publish = hash_slot_assembly("_capture_test_writer").1;
    let hold = if target().arch == Arch::AArch64 {
        format!("stp x29, x30, [sp, #-16]!\nsub x0, x0, #8\nbl __rt_incref\n{publish}\nmov x0, #0\nldp x29, x30, [sp], #16\nret\n")
    } else {
        format!("sub rsp, 8\nlea rax, [rdi - 8]\ncall __rt_incref\nmov rdi, rax\n{publish}\nxor eax, eax\nadd rsp, 8\nret\n")
    };
    let mut patched = replace_function(&assembly, "capture_test_hold", &hold);
    patched = replace_function(&patched, "capture_test_read", &observe_shim());
    patched = replace_function(&patched, "capture_test_drop", &release_shim("_capture_test_writer"));
    patched.push_str(&format!("\n{}\n", hash_slot_assembly("_capture_test_writer").2));
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}\n{}", output.stdout, output.stderr);
    let ordinary = "hold:0\nafter frame:6\ndrop:destroy value\n0\n";
    let throwing = "hold:0\ncaught:scope\nafter frame:6\ndrop:destroy value\n0\n";
    assert_eq!(output.stdout, format!("{}done\n", format!("{ordinary}{ordinary}{throwing}").repeat(8)));
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}", output.stderr);
    let _ = std::fs::remove_dir_all(directory);
}

/// Borrows a verified native child-slot address without copying or retaining its current PHP value.
fn local_shim() -> String {
    if target().arch == Arch::AArch64 {
        return r#"
    sub sp, sp, #64
    stp x29, x30, [sp, #48]
    sub x0, x0, #8
    str x0, [sp, #32]
    bl __rt_heap_kind
    cmp x0, #5
    b.ne .L_capture_local_invalid
    ldr x0, [sp, #32]
    bl __rt_reference_is
    cbz x0, .L_capture_local_invalid
    mov x0, #0
    ldr x1, [sp, #32]
    mov x2, #0
    mov x3, sp
    bl _capture_test_coordinator
    str x0, [sp, #24]
    ldr x0, [sp, #8]
    bl __rt_decref_any
    ldr x0, [sp, #16]
    bl __rt_decref_any
    ldr x0, [sp, #24]
    b .L_capture_local_return
.L_capture_local_invalid:
    mov x0, #1
.L_capture_local_return:
    ldp x29, x30, [sp, #48]
    add sp, sp, #64
    and x9, x0, #255
    cmp x9, #2
    b.ne .L_capture_local_done
    b __rt_throw_current
.L_capture_local_done:
    ret
"#.to_string();
    }
    r#"
    push rbp
    mov rbp, rsp
    sub rsp, 48
    lea rax, [rdi - 8]
    mov QWORD PTR [rsp + 32], rax
    call __rt_heap_kind
    cmp eax, 5
    jne .L_capture_local_invalid
    mov rax, QWORD PTR [rsp + 32]
    call __rt_reference_is
    test eax, eax
    jz .L_capture_local_invalid
    xor edi, edi
    mov rsi, QWORD PTR [rsp + 32]
    xor edx, edx
    mov rcx, rsp
    call _capture_test_coordinator
    mov QWORD PTR [rsp + 24], rax
    mov rax, QWORD PTR [rsp + 8]
    call __rt_decref_any
    mov rax, QWORD PTR [rsp + 16]
    call __rt_decref_any
    mov rax, QWORD PTR [rsp + 24]
    jmp .L_capture_local_return
.L_capture_local_invalid:
    mov eax, 1
.L_capture_local_return:
    leave
    cmp al, 2
    je __rt_throw_current
    ret
"#.to_string()
}
