//! Purpose:
//! Checks native query registration against live PHP reference storage.
//!
//! Called from:
//! - The query registration runtime-GC fixture module.
//!
//! Key details:
//! - The native shim borrows the caller's persistent reference through its existing child slot.
//! - Destructor retargeting and exception chaining execute real compiled PHP code.

use super::*;

/// Resolves a retargeted root before nesting removal and preserves exceptions from both destructors.
#[test]
fn test_mbstring_query_register_native_live_root_removal() {
    let source = r#"<?php
function query_test_reference(mixed &$root): int { return count($root); }
function query_test_retarget(array $replacement): int { return count($replacement); }
class QueryReplacement {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "new\n";
        if ($this->fail) { throw new RuntimeException("new root"); }
    }
}
class QueryRetarget {
    public function __construct(public bool $fail) {}
    public function __destruct() {
        echo "old\n";
        $replacement = ["key" => new QueryReplacement($this->fail), "keep" => "yes"];
        echo "retarget:", query_test_retarget($replacement), "\n";
        if ($this->fail) { throw new RuntimeException("old root"); }
    }
}
function run_query(mixed $root, bool $fail): void {
    $alias =& $root;
    $root = ["key" => new QueryRetarget($fail)];
    try { $status = query_test_reference($alias); echo "status:", $status, "\n"; }
    catch (Throwable $error) {
        echo "caught:", $error->getMessage(), ":", $error->getPrevious()->getMessage(), "\n";
    }
    echo json_encode($root), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) { run_query(null, false); run_query(null, true); }
echo "done\n";
"#;
    let expected = concat!(
        "old\nretarget:0\nnew\nstatus:10\n{\"keep\":\"yes\"}\n",
        "old\nretarget:0\nnew\ncaught:new root:old root\n{\"keep\":\"yes\"}\n",
    ).repeat(8) + "done\n";
    run_reference(source, &expected, true);
}

/// Ignores non-arrays, promotes unique dense roots, and reports shared dense roots without mutation.
#[test]
fn test_mbstring_query_register_native_root_representations() {
    let source = r#"<?php
function query_test_reference(mixed &$root): int { return count($root); }
function non_array(mixed $root): void {
    $alias =& $root;
    echo query_test_reference($alias), ":", json_encode($root), "\n";
}
function arrays(mixed $root): void {
    $alias =& $root;
    $root = ["one", "two"];
    echo query_test_reference($alias), ":", json_encode($root), "\n";
    $copy = ["before", "after"];
    $root = $copy;
    echo query_test_reference($alias), ":", json_encode($root), ":", json_encode($copy), "\n";
}
mb_strlen("");
for ($i = 0; $i < 8; $i++) {
    non_array(null); non_array(42); non_array(true); non_array("text"); arrays(null);
}
echo "done\n";
"#;
    let expected = concat!(
        "0:null\n0:42\n0:true\n0:\"text\"\n",
        "0:{\"0\":\"one\",\"1\":\"two\",\"key\":\"v\\u0000x\"}\n",
        "1:[\"before\",\"after\"]:[\"before\",\"after\"]\n",
    ).repeat(8) + "done\n";
    run_reference(source, &expected, false);
}

/// Runs shared registration with caller-owned references and an optional native retarget test helper.
fn run_reference(source: &str, expected: &str, remove: bool) {
    let directory = make_cli_test_dir("mbstring_query_reference_register");
    let (assembly, runtime, libraries) = compile_source_to_asm_with_options(
        source, &directory, 8_388_608, true, true);
    let mut patched = replace_function(&assembly, "query_test_reference", &reference_shim(remove));
    if remove {
        patched = replace_function(&patched, "query_test_retarget", &super::super::reference_fill::retarget_shim());
    }
    patched.push_str(&format!("\n{}\n", hash_slot_assembly("_capture_test_writer").2));
    patched.push_str("\n.data\n_query_reference_key:\n.byte 107,101,121\n_query_reference_value:\n.byte 118,0,120\n.p2align 3\n_query_reference_plan:\n");
    patched.push_str(if remove {
        ".quad 1,0,1,_query_reference_key,3\n.quad 3,0,1,_query_reference_key,3\n"
    } else { ".quad 2,0,1,_query_reference_key,3\n" });
    let output = assemble_and_run_capture(&patched, &runtime_obj_for_asm(&runtime), &directory,
        &libraries, &default_link_paths(), &[]);
    assert!(output.success, "{}: {}\n{}", directory.display(), output.stdout, output.stderr);
    assert_eq!(output.stdout, expected, "{}", directory.display());
    assert!(output.stderr.contains("HEAP DEBUG: leak summary: clean"), "{}: {}", directory.display(), output.stderr);
    std::fs::remove_dir_all(directory).unwrap();
}

/// Adapts the caller's child slot to a borrowed writer and forwards all seven registration arguments.
fn reference_shim(remove: bool) -> String {
    let count = if remove { 2 } else { 1 };
    let publish = hash_slot_assembly("_capture_test_writer").1;
    let done = format!(
        "{}query_reference_done",
        target().platform.local_label_prefix()
    );
    if target().arch == Arch::AArch64 {
        let address = |reg: &str, symbol: &str| if target().platform == Platform::Linux {
            format!("adrp {reg}, {symbol}\nadd {reg}, {reg}, :lo12:{symbol}")
        } else { format!("adrp {reg}, {symbol}@PAGE\nadd {reg}, {reg}, {symbol}@PAGEOFF") };
        let plan = address("x2", "_query_reference_plan");
        let value = address("x4", "_query_reference_value");
        return format!(r#"
    sub sp, sp, #32
    stp x29, x30, [sp, #16]
    sub x0, x0, #8
    {publish}
    mov x1, x0
    mov x0, #0
    {plan}
    mov x3, #{count}
    {value}
    mov x5, #3
    mov x6, sp
    bl __rt_mbstring_query_register
    mov x11, x0
    mov x0, #0
    {publish}
    mov x0, x11
    ldr x9, [sp]
    ldp x29, x30, [sp, #16]
    add sp, sp, #32
    cmp x0, #2
    b.ne {done}
    b __rt_throw_current
{done}:
    mov x10, #10
    madd x0, x9, x10, x0
    ret
"#);
    }
    format!(r#"
    push rbp
    mov rbp, rsp
    sub rsp, 16
    sub rdi, 8
    {publish}
    mov rsi, rdi
    xor edi, edi
    lea rdx, [rip + _query_reference_plan]
    mov ecx, {count}
    lea r8, [rip + _query_reference_value]
    mov r9d, 3
    lea r10, [rsp + 8]
    mov QWORD PTR [rsp], r10
    call __rt_mbstring_query_register
    xor edi, edi
    {publish}
    mov r10, QWORD PTR [rsp + 8]
    leave
    cmp eax, 2
    jne {done}
    jmp __rt_throw_current
{done}:
    imul r10, r10, 10
    add rax, r10
    ret
"#)
}
