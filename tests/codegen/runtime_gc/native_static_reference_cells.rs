//! Purpose:
//! Verifies managed static binding cells independently of their remaining lowering consumers.
//!
//! Called from:
//! - Executable native reference ownership tests in supported-host CI.
//!
//! Key details:
//! - A three-word static binding has no managed heap header of its own.
//! - Repeated promotion preserves the cell; an escaped alias outlives static retirement.
//! - Only runtime helpers execute in CI; no local compiler or generated executable is needed.

use crate::support::*;
use super::native_reference_validation_support::*;

/// A static binding transfers its wrapper once and a retained native alias survives its release.
#[test]
fn test_native_static_reference_cell_survives_binding_retirement() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_static_reference_cell"));
    let source = r#"<?php
function nativeStaticReferenceAssign(mixed &$value): void { $value = 42; echo $value, '|'; }
if ($argc < 0) { $value = 0; nativeStaticReferenceAssign($value); unset($value); }
"#;
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #32", "sub rsp, 32");
    initialize_boxed_binding(&mut asm, 41);
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_payload");
    address(&mut asm, "x9", "r10", "Lnative_static_binding");
    instruction(&mut asm, "cmp x0, x9", "cmp rax, r10");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_promote");
    store(&mut asm, 8);
    call(&mut asm, "__rt_reference_cell_owner");
    instruction(&mut asm, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]");
    instruction(&mut asm, "cmp x0, x9", "cmp rax, r10");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_promote");
    instruction(&mut asm, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]");
    instruction(&mut asm, "cmp x0, x9", "cmp rax, r10");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    call(&mut asm, "__rt_incref");
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_payload");
    instruction(&mut asm, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]");
    instruction(&mut asm, "cmp x0, x9", "cmp rax, r10");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_release");
    expect_zero(&mut asm, "Lnative_static_binding");
    address(&mut asm, "x9", "r10", "Lnative_static_binding");
    instruction(&mut asm, "ldr x9, [x9, #16]", "mov r10, QWORD PTR [r10 + 16]");
    instruction(&mut asm, "cmp x9, #0", "test r10, r10");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    // An already uninitialized binding has no remaining owner to release.
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_release");
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_promote");
    instruction(&mut asm, "cmp x0, #0", "test rax, rax");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    instruction(&mut asm, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
    call(&mut asm, &elephc::names::function_symbol("nativeStaticReferenceAssign"));
    load(&mut asm, 8);
    call(&mut asm, "__rt_reference_cell_release");
    initialize_boxed_binding(&mut asm, 0);
    address(&mut asm, "x0", "rdi", "Lnative_static_binding");
    call(&mut asm, "__rt_reference_static_mixed_release");
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    expect_zero(&mut asm, "_exc_value");
    expect_zero(&mut asm, "_gc_live");
    append_status_output(&mut asm, 32);
    let mut user = inject_main_exit_harness(&user, &asm);
    user.push_str("\n.data\n.p2align 3\nLnative_static_binding:\n    .quad 0\n    .quad 0\n    .quad 0\n");
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "42|1");
}

/// Gives the raw static binding exactly one freshly allocated boxed integer owner.
fn initialize_boxed_binding(asm: &mut String, value: i64) {
    instruction(asm, "mov x0, #0", "xor eax, eax");
    instruction(asm, &format!("mov x1, #{value}"), &format!("mov rdi, {value}"));
    instruction(asm, "mov x2, #0", "xor esi, esi");
    call(asm, "__rt_mixed_from_value");
    address(asm, "x9", "r10", "Lnative_static_binding");
    instruction(asm, "str x0, [x9]", "mov QWORD PTR [r10], rax");
    instruction(asm, "str xzr, [x9, #8]", "mov QWORD PTR [r10 + 8], 0");
}


/// Native inherited and Reflection readers observe the value inside a promoted static cell.
#[test]
fn test_native_static_reference_cell_readers_preserve_value_and_owner() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_static_reference_readers"));
    let source = r#"<?php
class NativeStaticReadBase { public static mixed $value = 0; }
class NativeStaticReadChild extends NativeStaticReadBase {}
function nativeStaticReadViews(): void {
    echo NativeStaticReadBase::$value, '|', NativeStaticReadChild::$value, '|';
    $class = new ReflectionClass(NativeStaticReadChild::class);
    echo $class->getStaticPropertyValue('value'), '|';
    $values = $class->getStaticProperties(); echo $values['value'], '|';
}
if ($argc < 0) { nativeStaticReadViews(); }
"#;
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let symbol = elephc::names::static_property_symbol("NativeStaticReadBase", "value");
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #32", "sub rsp, 32");
    instruction(&mut asm, "mov x0, #0", "xor eax, eax");
    instruction(&mut asm, "mov x1, #41", "mov edi, 41");
    instruction(&mut asm, "mov x2, #0", "xor esi, esi");
    call(&mut asm, "__rt_mixed_from_value");
    address(&mut asm, "x9", "r10", &symbol);
    instruction(&mut asm, "str x0, [x9]", "mov QWORD PTR [r10], rax");
    instruction(&mut asm, "str xzr, [x9, #8]", "mov QWORD PTR [r10 + 8], 0");
    address(&mut asm, "x0", "rdi", &symbol);
    call(&mut asm, "__rt_reference_static_mixed_promote");
    store(&mut asm, 8);
    call(&mut asm, &elephc::names::function_symbol("nativeStaticReadViews"));
    // Borrowed readers and Reflection snapshots cannot consume the binding's owner.
    address(&mut asm, "x0", "rdi", &symbol);
    call(&mut asm, "__rt_reference_static_mixed_payload");
    instruction(&mut asm, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]");
    instruction(&mut asm, "cmp x0, x9", "cmp rax, r10");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    address(&mut asm, "x0", "rdi", &symbol);
    call(&mut asm, "__rt_reference_static_mixed_release");
    expect_zero(&mut asm, &symbol);
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    expect_zero(&mut asm, "_exc_value");
    expect_zero(&mut asm, "_gc_live");
    append_status_output(&mut asm, 32);
    let user = inject_main_exit_harness(&user, &asm);
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "41|41|41|41|1");
}


/// Fresh boxed static writes transfer their unwind pin exactly once through both native surfaces.
#[test]
fn test_native_static_reference_cell_writers_transfer_fresh_heap_boxes_bundle() {
    for optimize in [false, true] {
        let dir = FixtureDir(make_cli_test_dir("elephc_native_static_reference_writers"));
        let source = r#"<?php
class NativeStaticWriteBase { public static mixed $value = 0; }
class NativeStaticWriteChild extends NativeStaticWriteBase {}
function nativeStaticWriteFresh(): mixed { return str_repeat('ab', 2); }
function nativeStaticWriteViews(): void {
    NativeStaticWriteBase::$value = nativeStaticWriteFresh();
    echo NativeStaticWriteChild::$value, '|';
    $class = new ReflectionClass(NativeStaticWriteChild::class);
    $class->setStaticPropertyValue('value', nativeStaticWriteFresh());
    echo NativeStaticWriteBase::$value, '|';
}
if ($argc < 0) { nativeStaticWriteViews(); }
"#;
        let compile = || compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true);
        let (user, runtime, requirements) = if optimize { compile() } else { without_ir_opt(compile) };
        let symbol = elephc::names::static_property_symbol("NativeStaticWriteBase", "value");
        let mut asm = String::new();
        instruction(&mut asm, "sub sp, sp, #32", "sub rsp, 32");
        instruction(&mut asm, "mov x0, #0", "xor eax, eax");
        instruction(&mut asm, "mov x1, #0", "xor edi, edi");
        instruction(&mut asm, "mov x2, #0", "xor esi, esi");
        call(&mut asm, "__rt_mixed_from_value");
        address(&mut asm, "x9", "r10", &symbol);
        instruction(&mut asm, "str x0, [x9]", "mov QWORD PTR [r10], rax");
        instruction(&mut asm, "str xzr, [x9, #8]", "mov QWORD PTR [r10 + 8], 0");
        address(&mut asm, "x0", "rdi", &symbol);
        call(&mut asm, "__rt_reference_static_mixed_promote");
        store(&mut asm, 8);
        call(&mut asm, &elephc::names::function_symbol("nativeStaticWriteViews"));
        address(&mut asm, "x0", "rdi", &symbol);
        call(&mut asm, "__rt_reference_static_mixed_payload");
        instruction(&mut asm, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]");
        instruction(&mut asm, "cmp x0, x9", "cmp rax, r10");
        instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
        address(&mut asm, "x0", "rdi", &symbol);
        call(&mut asm, "__rt_reference_static_mixed_release");
        expect_zero(&mut asm, &symbol);
        expect_zero(&mut asm, "_rt_reference_validation_pins_top");
        expect_zero(&mut asm, "_exc_value");
        expect_zero(&mut asm, "_gc_live");
        append_status_output(&mut asm, 32);
        let user = inject_main_exit_harness(&user, &asm);
        let runtime = runtime_obj_for_asm(&runtime);
        let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
        assert_eq!(output, "abab|abab|1", "IR optimization={optimize}");
    }
}


/// Direct assignment transfers its RHS, while Reflection retains its argument until return.
#[test]
fn test_native_static_reference_publication_retires_concrete_rhs_before_old_destructor() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_static_reference_destructors"));
    let source = r#"<?php
class NativeStaticDestructSlot { public static mixed $value = null; }
class NativeStaticIncoming { public function __destruct() { echo 'incoming|'; } }
class NativeStaticDisplaced {
    public function __destruct() {
        echo 'old-start|'; NativeStaticDestructSlot::$value = 0; echo 'old-end|';
    }
}
function nativeStaticInitializeOld(): void { NativeStaticDestructSlot::$value = new NativeStaticDisplaced(); }
function nativeStaticReplaceDirect(): void { NativeStaticDestructSlot::$value = new NativeStaticIncoming(); }
function nativeStaticReplaceReflection(): void {
    $class = new ReflectionClass(NativeStaticDestructSlot::class);
    $class->setStaticPropertyValue('value', new NativeStaticIncoming());
}
if ($argc < 0) { nativeStaticInitializeOld(); nativeStaticReplaceDirect(); nativeStaticReplaceReflection(); }
"#;
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let symbol = elephc::names::static_property_symbol("NativeStaticDestructSlot", "value");
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #32", "sub rsp, 32");
    call(&mut asm, &elephc::names::function_symbol("nativeStaticInitializeOld"));
    address(&mut asm, "x0", "rdi", &symbol);
    call(&mut asm, "__rt_reference_static_mixed_promote");
    call(&mut asm, &elephc::names::function_symbol("nativeStaticReplaceDirect"));
    call(&mut asm, &elephc::names::function_symbol("nativeStaticInitializeOld"));
    call(&mut asm, &elephc::names::function_symbol("nativeStaticReplaceReflection"));
    address(&mut asm, "x0", "rdi", &symbol);
    call(&mut asm, "__rt_reference_static_mixed_release");
    expect_zero(&mut asm, &symbol);
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    expect_zero(&mut asm, "_exc_value");
    expect_zero(&mut asm, "_gc_live");
    append_status_output(&mut asm, 32);
    let user = inject_main_exit_harness(&user, &asm);
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    let reflection = user.split_once("@fn name=nativeStaticReplaceReflection ")
        .map(|(_, body)| body.split_once("@endfn name=nativeStaticReplaceReflection")
            .map_or(body, |(body, _)| body))
        .unwrap_or("native Reflection function metadata missing");
    assert_eq!(output, "old-start|incoming|old-end|old-start|old-end|incoming|1",
        "native Reflection ownership sequence:\n{reflection}");
}
