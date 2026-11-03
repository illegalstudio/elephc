//! Purpose:
//! Verifies canonical publication does not delay reentrant incoming-value destruction.
//!
//! Called from:
//! - The executable runtime ownership CI on every supported host target.
//!
//! Key details:
//! - The publisher consumes a fresh RHS without another caller-owned payload lease.
//! - The displaced PHP destructor overwrites the published value through a native callback.
//! - Exact output distinguishes destruction inside that callback from cleanup after return.

use crate::support::*;
use super::native_reference_validation_support::*;

/// Replaces a published object while its displaced destructor is still executing PHP.
#[test]
fn test_native_reference_publication_retires_rhs_lease_before_old_destructor() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_publication"));
    let source = r#"<?php
extern function native_publisher_overwrite(): void;
class NativePublisherIncoming {
    public function __destruct() { echo 'incoming|'; }
}
class NativePublisherOld {
    public function __destruct() {
        echo 'old-start|'; native_publisher_overwrite(); echo 'old-end|';
    }
}
function nativePublisherMakeOld(): mixed { return new NativePublisherOld(); }
function nativePublisherMakeIncoming(): mixed { return new NativePublisherIncoming(); }
function nativePublisherAssign(mixed &$target): void { $target = nativePublisherMakeIncoming(); }
if ($argc < 0) {
    $old = nativePublisherMakeOld(); $incoming = nativePublisherMakeIncoming();
    $target = null; nativePublisherAssign($target);
    unset($old, $incoming, $target);
}
"#;
    // Keep the two factories independently callable from the ABI harness.
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #128", "sub rsp, 128");
    for offset in [0, 8] {
        instruction(&mut asm, &format!("str xzr, [sp, #{offset}]"),
            &format!("mov QWORD PTR [rsp + {offset}], 0"));
    }
    call(&mut asm, &elephc::names::function_symbol("nativePublisherMakeOld"));
    store(&mut asm, 96);
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 88);
    address(&mut asm, "x9", "r10", "Lnative_publisher_target");
    instruction(&mut asm, "str x0, [x9]", "mov QWORD PTR [r10], rax");
    load(&mut asm, 96);
    call(&mut asm, "__rt_decref_mixed");
    call(&mut asm, &elephc::names::function_symbol("nativePublisherMakeIncoming"));
    store(&mut asm, 104);
    instruction(&mut asm, "mov x0, sp", "mov rdi, rsp");
    instruction(&mut asm, "ldr x1, [sp, #88]", "mov rsi, QWORD PTR [rsp + 88]");
    instruction(&mut asm, "ldr x2, [sp, #104]", "mov rdx, QWORD PTR [rsp + 104]");
    instruction(&mut asm, "add x3, sp, #16", "lea rcx, [rsp + 16]");
    call(&mut asm, "__rt_reference_publish_native");
    instruction(&mut asm, "cmp x0, #0", "test rax, rax");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(&mut asm, 88);
    call(&mut asm, "__rt_decref_mixed");
    // Repeat through real StoreRefCell lowering, including its moved source parking owner.
    call(&mut asm, &elephc::names::function_symbol("nativePublisherMakeOld"));
    store(&mut asm, 96);
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 88);
    address(&mut asm, "x9", "r10", "Lnative_publisher_target");
    instruction(&mut asm, "str x0, [x9]", "mov QWORD PTR [r10], rax");
    load(&mut asm, 96);
    call(&mut asm, "__rt_decref_mixed");
    instruction(&mut asm, "mov x0, #7", "mov edi, 7");
    call(&mut asm, "__rt_reference_cell_new");
    store(&mut asm, 104);
    instruction(&mut asm, "ldr x9, [sp, #88]", "mov r10, QWORD PTR [rsp + 88]");
    instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
    instruction(&mut asm, "mov x0, x0", "mov rdi, rax");
    call(&mut asm, &elephc::names::function_symbol("nativePublisherAssign"));
    load(&mut asm, 104);
    call(&mut asm, "__rt_reference_cell_release");
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    expect_zero(&mut asm, "_gc_live");
    append_status_output(&mut asm, 128);
    let mut user = inject_main_exit_harness(&user, &asm);
    append_overwrite_callback(&mut user);
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "old-start|incoming|old-end|old-start|incoming|old-end|1");
}

/// Defines the foreign callback used by the real PHP destructor, with no extra RHS owner.
fn append_overwrite_callback(user: &mut String) {
    let entry = target().extern_symbol("native_publisher_overwrite");
    user.push_str(&format!("\n.text\n.p2align 2\n.globl {entry}\n{entry}:\n"));
    if target().arch == Arch::AArch64 {
        user.push_str("    sub sp, sp, #32\n    stp x29, x30, [sp, #16]\n    add x29, sp, #16\n");
    } else { user.push_str("    push rbp\n    mov rbp, rsp\n    sub rsp, 16\n"); }
    address(user, "x9", "r10", "Lnative_publisher_target");
    instruction(user, "ldr x0, [x9]", "mov rax, QWORD PTR [r10]");
    address(user, "x1", "rdi", "Lnative_publisher_null");
    call(user, "__rt_reference_replace");
    call(user, "__rt_decref_mixed");
    if target().arch == Arch::AArch64 {
        user.push_str("    ldp x29, x30, [sp, #16]\n    add sp, sp, #32\n    ret\n");
    } else { user.push_str("    leave\n    ret\n"); }
    user.push_str(".data\n.p2align 3\nLnative_publisher_target:\n    .quad 0\n\
        .p2align 3\nLnative_publisher_null:\n    .quad 8\n    .quad 0\n    .quad 0\n");
}

/// Real StoreRefCell calls use source-file strictness and propagate a protected declaration error.
#[test]
fn test_native_reference_store_assignment_strictness_bundle() {
    for strict in [false, true] {
        let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_store_mode"));
        let source = format!(r#"<?php
{}
class NativeReferenceModeProbe {{ public float $value = 2.0; }}
function nativeReferenceApplyMode(mixed &$target): void {{
    try {{ $target = true; echo 'accepted|'; }}
    catch (TypeError $error) {{ echo 'rejected|'; }}
}}
if ($argc < 0) {{
    $probe = new NativeReferenceModeProbe(); $target = 2.0;
    $alias =& $probe->value;
    nativeReferenceApplyMode($target); unset($alias, $probe, $target);
}}
"#, if strict { "declare(strict_types=1);" } else { "" });
        let (user, runtime, requirements) = without_ir_opt(||
            compile_source_to_asm_with_native_reference_validation(&source, &dir.0, 65_536, true));
        let prefix = elephc::names::function_symbol("__eir_reference_property_");
        let suffix = elephc::names::mangle_fqn("_instance");
        let callback = user.lines().filter_map(|line| line.trim().strip_suffix(':'))
            .find(|line| line.starts_with(&prefix) && line.ends_with(&suffix)).unwrap();
        let mut asm = String::new();
        instruction(&mut asm, "sub sp, sp, #160", "sub rsp, 160");
        instruction(&mut asm, "mov x0, #2", "mov eax, 2");
        instruction(&mut asm, "mov x1, #0x4000000000000000", "mov rdi, 0x4000000000000000");
        instruction(&mut asm, "mov x2, #0", "xor esi, esi");
        call(&mut asm, "__rt_mixed_from_value");
        store(&mut asm, 16);
        call(&mut asm, "__rt_reference_new");
        store(&mut asm, 0);
        load(&mut asm, 16);
        call(&mut asm, "__rt_decref_mixed");
        instruction(&mut asm, "mov x0, #7", "mov edi, 7");
        call(&mut asm, "__rt_reference_cell_new");
        store(&mut asm, 8);
        instruction(&mut asm, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
        instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
        address(&mut asm, "x0", "rax", callback);
        store(&mut asm, 32);
        instruction(&mut asm, "add x0, sp, #8", "lea rdi, [rsp + 8]");
        instruction(&mut asm, "ldr x1, [sp, #8]", "mov rsi, QWORD PTR [rsp + 8]");
        instruction(&mut asm, "add x2, sp, #32", "lea rdx, [rsp + 32]");
        call(&mut asm, &target().extern_symbol("__elephc_reference_sources_bind_v1"));
        instruction(&mut asm, "cmp x0, #0", "test rax, rax");
        instruction(&mut asm, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
        instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
        call(&mut asm, "__rt_reference_source_associate_native");
        instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
        call(&mut asm, &elephc::names::function_symbol("nativeReferenceApplyMode"));
        load(&mut asm, 8);
        instruction(&mut asm, "ldr x0, [x0]", "mov rax, QWORD PTR [rax]");
        call(&mut asm, "__rt_mixed_unbox");
        instruction(&mut asm, "cmp x0, #2", "cmp rax, 2");
        instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
        load(&mut asm, 8);
        instruction(&mut asm, "ldr x0, [x0]", "mov rax, QWORD PTR [rax]");
        call(&mut asm, "__rt_mixed_cast_int");
        let expected = if strict { 2 } else { 1 };
        instruction(&mut asm, &format!("cmp x0, #{expected}"), &format!("cmp rax, {expected}"));
        instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
        instruction(&mut asm, "add x0, sp, #8", "lea rdi, [rsp + 8]");
        call(&mut asm, &target().extern_symbol("__elephc_reference_sources_unbind_v1"));
        instruction(&mut asm, "cmp x0, #0", "test rax, rax");
        instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
        load(&mut asm, 8);
        call(&mut asm, "__rt_reference_cell_release");
        expect_zero(&mut asm, "_rt_reference_validation_pins_top");
        expect_zero(&mut asm, "_gc_live");
        append_status_output(&mut asm, 160);
        let user = inject_main_exit_harness(&user, &asm);
        let runtime = runtime_obj_for_asm(&runtime);
        let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
        assert_eq!(output, if strict { "rejected|1" } else { "accepted|1" }, "strict={strict}");
    }
}

/// A coercion snapshot's destructor observes publication and its throw survives later clean releases.
#[test]
fn test_native_reference_publication_preserves_snapshot_destructor_throw() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_publication_throw"));
    let source = r#"<?php
extern function native_publisher_verify_new(): void;
class NativePublisherStringSource { public string $value = 'old'; }
class NativePublisherStringable {
    public function __toString(): string { echo 'convert|'; return 'new'; }
    public function __destruct() {
        echo 'rhs-dtor|'; native_publisher_verify_new(); throw new Exception('retired');
    }
}
function nativePublisherMakeStringable(): mixed { return new NativePublisherStringable(); }
$object = new NativePublisherStringSource(); $alias =& $object->value; unset($alias, $object);
if ($argc < 0) { $rhs = nativePublisherMakeStringable(); unset($rhs); }
"#;
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let prefix = elephc::names::function_symbol("__eir_reference_property_");
    let suffix = elephc::names::mangle_fqn("_instance");
    let callback = user.lines().filter_map(|line| line.trim().strip_suffix(':'))
        .find(|line| line.starts_with(&prefix) && line.ends_with(&suffix)).unwrap();
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #192", "sub rsp, 192");
    for offset in [0, 8] {
        instruction(&mut asm, &format!("str xzr, [sp, #{offset}]"),
            &format!("mov QWORD PTR [rsp + {offset}], 0"));
    }
    address(&mut asm, "x0", "rax", "Lnative_publisher_old_string");
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 104);
    address(&mut asm, "x9", "r10", "Lnative_publisher_target");
    instruction(&mut asm, "str x0, [x9]", "mov QWORD PTR [r10], rax");
    instruction(&mut asm, "mov x0, #7", "mov edi, 7");
    call(&mut asm, "__rt_reference_cell_new");
    store(&mut asm, 96);
    instruction(&mut asm, "ldr x9, [sp, #104]", "mov r10, QWORD PTR [rsp + 104]");
    instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
    address(&mut asm, "x0", "rax", callback);
    store(&mut asm, 128);
    instruction(&mut asm, "add x0, sp, #96", "lea rdi, [rsp + 96]");
    instruction(&mut asm, "ldr x1, [sp, #96]", "mov rsi, QWORD PTR [rsp + 96]");
    instruction(&mut asm, "add x2, sp, #128", "lea rdx, [rsp + 128]");
    call(&mut asm, &target().extern_symbol("__elephc_reference_sources_bind_v1"));
    instruction(&mut asm, "cmp x0, #0", "test rax, rax");
    instruction(&mut asm, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    instruction(&mut asm, "ldr x0, [sp, #96]", "mov rdi, QWORD PTR [rsp + 96]");
    instruction(&mut asm, "ldr x1, [sp, #104]", "mov rsi, QWORD PTR [rsp + 104]");
    call(&mut asm, &target().extern_symbol("__elephc_reference_sources_set_reference_v1"));
    instruction(&mut asm, "cmp x0, #0", "test rax, rax");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    call(&mut asm, &elephc::names::function_symbol("nativePublisherMakeStringable"));
    store(&mut asm, 112);
    instruction(&mut asm, "mov x0, sp", "mov rdi, rsp");
    instruction(&mut asm, "ldr x1, [sp, #104]", "mov rsi, QWORD PTR [rsp + 104]");
    instruction(&mut asm, "ldr x2, [sp, #112]", "mov rdx, QWORD PTR [rsp + 112]");
    instruction(&mut asm, "add x3, sp, #16", "lea rcx, [rsp + 16]");
    call(&mut asm, "__rt_reference_publish_native");
    instruction(&mut asm, "cmp x0, #2", "cmp rax, 2");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    address(&mut asm, "x9", "r10", "Lnative_publisher_observed");
    instruction(&mut asm, "ldr x9, [x9]", "mov r10, QWORD PTR [r10]");
    instruction(&mut asm, "cmp x9, #1", "cmp r10, 1");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(&mut asm, 8);
    instruction(&mut asm, "cmp x0, #0", "test rax, rax");
    instruction(&mut asm, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    instruction(&mut asm, "str xzr, [sp, #8]", "mov QWORD PTR [rsp + 8], 0");
    call(&mut asm, "__rt_decref_any");
    instruction(&mut asm, "add x0, sp, #96", "lea rdi, [rsp + 96]");
    call(&mut asm, &target().extern_symbol("__elephc_reference_sources_unbind_v1"));
    instruction(&mut asm, "cmp x0, #0", "test rax, rax");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(&mut asm, 96);
    call(&mut asm, "__rt_reference_cell_release");
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    expect_zero(&mut asm, "_gc_live");
    append_status_output(&mut asm, 192);
    let mut user = inject_main_exit_harness(&user, &asm);
    append_verify_callback(&mut user);
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "convert|rhs-dtor|1");
}

/// A failed shared conversion remains the previous exception when the last RHS owner throws.
#[test]
fn test_native_reference_publication_chains_conflict_before_rhs_destructor() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_conflict_throw"));
    let source = r#"<?php
class NativeConflictInteger { public int|string $value = ''; }
class NativeConflictFloat { public float|string $value = ''; }
class NativeConflictInput {
    public static int $calls = 0;
    public function __toString(): string {
        echo 'convert|'; return 'value' . ++self::$calls;
    }
    public function __destruct() {
        echo 'destroy|'; if (self::$calls === 2) { throw new Exception('destructor'); }
    }
}
function nativeConflictAssign(mixed &$target): void {
    try { $target = new NativeConflictInput(); }
    catch (Throwable $error) {
        echo get_class($error), ':';
        $previous = $error->getPrevious();
        echo $previous === null ? 'none' : get_class($previous), '|';
    }
}
if ($argc < 0) {
    $integer = new NativeConflictInteger(); $float = new NativeConflictFloat();
    $integerAlias =& $integer->value; $floatAlias =& $float->value;
    $target = ''; nativeConflictAssign($target);
    unset($target, $integerAlias, $floatAlias, $integer, $float);
}
"#;
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let prefix = elephc::names::function_symbol("__eir_reference_property_");
    let suffix = elephc::names::mangle_fqn("_instance");
    let callbacks: Vec<_> = user.lines().filter_map(|line| line.trim().strip_suffix(':'))
        .filter(|line| line.starts_with(&prefix) && line.ends_with(&suffix)).collect();
    assert_eq!(callbacks.len(), 2, "both physical declarations must constrain the same reference");
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #144", "sub rsp, 144");
    address(&mut asm, "x0", "rax", "Lnative_conflict_initial");
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 0);
    instruction(&mut asm, "mov x0, #7", "mov edi, 7");
    call(&mut asm, "__rt_reference_cell_new");
    store(&mut asm, 8);
    instruction(&mut asm, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
    instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
    // Each physical binding independently owns the shared canonical cell.
    call(&mut asm, "__rt_incref");
    store(&mut asm, 16);
    for (index, callback) in callbacks.iter().enumerate() {
        let binding = 8 + index * 8;
        let descriptor = 32 + index * 32;
        address(&mut asm, "x0", "rax", callback);
        store(&mut asm, descriptor);
        instruction(&mut asm, &format!("add x0, sp, #{binding}"), &format!("lea rdi, [rsp + {binding}]"));
        instruction(&mut asm, &format!("ldr x1, [sp, #{binding}]"), &format!("mov rsi, QWORD PTR [rsp + {binding}]"));
        instruction(&mut asm, &format!("add x2, sp, #{descriptor}"), &format!("lea rdx, [rsp + {descriptor}]"));
        call(&mut asm, &target().extern_symbol("__elephc_reference_sources_bind_v1"));
        instruction(&mut asm, "cmp x0, #0", "test rax, rax");
        instruction(&mut asm, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    }
    instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
    call(&mut asm, "__rt_reference_source_associate_native");
    for _ in 0..2 {
        instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
        call(&mut asm, &elephc::names::function_symbol("nativeConflictAssign"));
    }
    for binding in [8, 16] {
        instruction(&mut asm, &format!("add x0, sp, #{binding}"), &format!("lea rdi, [rsp + {binding}]"));
        call(&mut asm, &target().extern_symbol("__elephc_reference_sources_unbind_v1"));
        instruction(&mut asm, "cmp x0, #0", "test rax, rax");
        instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
        load(&mut asm, binding);
        call(&mut asm, "__rt_reference_cell_release");
    }
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    expect_zero(&mut asm, "_exc_value");
    expect_zero(&mut asm, "_gc_live");
    append_status_output(&mut asm, 144);
    let mut user = inject_main_exit_harness(&user, &asm);
    user.push_str("\n.data\n.p2align 3\nLnative_conflict_initial:\n    .quad 1\n    .quad Lnative_conflict_bytes\n    .quad 0\nLnative_conflict_bytes:\n    .byte 0\n");
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "convert|convert|destroy|Exception:TypeError|convert|convert|destroy|TypeError:none|1");
}

/// Fresh heap strings survive both the raw store and canonical source-parking retirement.
#[test]
fn test_native_reference_store_fresh_heap_string_ownership_bundle() {
    let source = r#"<?php
function nativeFreshHeapString(): string { return str_repeat('ab', 2); }
function nativeFreshHeapAssign(string &$value): void {
    $value = nativeFreshHeapString(); echo $value, '|';
}
if ($argc < 0) { $value = ''; nativeFreshHeapAssign($value); unset($value); }
"#;
    for canonical in [false, true] {
        for optimize in [false, true] {
            let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_heap_string"));
            let compile = || if canonical {
                compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true)
            } else {
                compile_source_to_asm_with_options(source, &dir.0, 65_536, false, true)
            };
            let (user, runtime, requirements) = if optimize { compile() } else { without_ir_opt(compile) };
            let mut asm = String::new();
            instruction(&mut asm, "sub sp, sp, #32", "sub rsp, 32");
            if canonical {
                address(&mut asm, "x0", "rax", "Lnative_heap_string_initial");
                call(&mut asm, "__rt_reference_new");
                store(&mut asm, 16);
            }
            let tag = if canonical { 7 } else { 1 };
            instruction(&mut asm, &format!("mov x0, #{tag}"), &format!("mov edi, {tag}"));
            call(&mut asm, "__rt_reference_cell_new");
            store(&mut asm, 8);
            if canonical {
                instruction(&mut asm, "ldr x9, [sp, #16]", "mov r10, QWORD PTR [rsp + 16]");
                instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
            } else {
                address(&mut asm, "x9", "r10", "Lnative_heap_string_bytes");
                instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
                instruction(&mut asm, "mov x9, #7", "mov r10, 7");
                instruction(&mut asm, "str x9, [x0, #8]", "mov QWORD PTR [rax + 8], r10");
            }
            instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
            call(&mut asm, &elephc::names::function_symbol("nativeFreshHeapAssign"));
            load(&mut asm, 8);
            call(&mut asm, "__rt_reference_cell_release");
            expect_zero(&mut asm, "_rt_reference_validation_pins_top");
            expect_zero(&mut asm, "_gc_live");
            append_status_output(&mut asm, 32);
            let mut user = inject_main_exit_harness(&user, &asm);
            user.push_str("\n.data\n.p2align 3\nLnative_heap_string_initial:\n    .quad 1\n    .quad Lnative_heap_string_bytes\n    .quad 7\nLnative_heap_string_bytes:\n    .ascii \"initial\"\n");
            let runtime = runtime_obj_for_asm(&runtime);
            let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
            assert_eq!(output, "abab|1", "canonical={canonical} optimize={optimize}");
        }
    }
}

/// Prints the harness predicate after any failure jumps to the shared zero-result label.
fn append_status_output(asm: &mut String, bytes: usize) {
    instruction(asm, "mov x0, #1", "mov eax, 1");
    instruction(asm, "b Lnative_publisher_print", "jmp Lnative_publisher_print");
    asm.push_str("Lvalidation_pin_fail:\n");
    instruction(asm, "mov x0, #0", "xor eax, eax");
    asm.push_str("Lnative_publisher_print:\n");
    call(asm, "__rt_itoa");
    if target().arch == Arch::AArch64 {
        instruction(asm, "mov x0, #1", "");
        if target().platform == Platform::Linux {
            instruction(asm, "mov x8, #64", ""); instruction(asm, "svc #0", "");
        } else { instruction(asm, "mov x16, #4", ""); instruction(asm, "svc #0x80", ""); }
    } else { asm.push_str("    mov rsi, rax\n    mov edi, 1\n    mov eax, 1\n    syscall\n"); }
    instruction(asm, &format!("add sp, sp, #{bytes}"), &format!("add rsp, {bytes}"));
}

/// Observes the real published string from inside the Stringable RHS destructor.
fn append_verify_callback(user: &mut String) {
    let entry = target().extern_symbol("native_publisher_verify_new");
    user.push_str(&format!("\n.text\n.p2align 2\n.globl {entry}\n{entry}:\n"));
    if target().arch == Arch::AArch64 {
        user.push_str("    stp x29, x30, [sp, #-16]!\n    mov x29, sp\n");
    } else { user.push_str("    push rbp\n    mov rbp, rsp\n"); }
    address(user, "x9", "r10", "Lnative_publisher_target");
    instruction(user, "ldr x0, [x9]", "mov rax, QWORD PTR [r10]");
    call(user, "__rt_mixed_unbox");
    instruction(user, "cmp x0, #1", "cmp rax, 1");
    instruction(user, "b.ne Lnative_publisher_verify_done", "jne Lnative_publisher_verify_done");
    instruction(user, "cmp x2, #3", "cmp rdx, 3");
    instruction(user, "b.ne Lnative_publisher_verify_done", "jne Lnative_publisher_verify_done");
    for (index, byte) in b"new".iter().enumerate() {
        instruction(user, &format!("ldrb w9, [x1, #{index}]"), &format!("movzx r10d, BYTE PTR [rdi + {index}]"));
        instruction(user, &format!("cmp w9, #{byte}"), &format!("cmp r10d, {byte}"));
        instruction(user, "b.ne Lnative_publisher_verify_done", "jne Lnative_publisher_verify_done");
    }
    address(user, "x9", "r10", "Lnative_publisher_observed");
    instruction(user, "mov x10, #1", "mov r11, 1");
    instruction(user, "str x10, [x9]", "mov QWORD PTR [r10], r11");
    user.push_str("Lnative_publisher_verify_done:\n");
    if target().arch == Arch::AArch64 {
        user.push_str("    ldp x29, x30, [sp], #16\n    ret\n");
    } else { user.push_str("    leave\n    ret\n"); }
    user.push_str(".data\n.p2align 3\nLnative_publisher_target:\n    .quad 0\n\
        Lnative_publisher_observed:\n    .quad 0\n.p2align 3\nLnative_publisher_old_string:\n\
        .quad 1\n    .quad Lnative_publisher_old_bytes\n    .quad 3\n\
        Lnative_publisher_old_bytes:\n    .asciz \"old\"\n");
}
