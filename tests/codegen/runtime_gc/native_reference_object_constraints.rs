//! Purpose:
//! Checks declared object constraints through the canonical native StoreRefCell publisher.
//!
//! Called from:
//! - Executable native reference CI on supported host targets.
//!
//! Key details:
//! - A generated nullable class validator accepts subclasses and null, preserving rejected stores.
//! - The ABI harness binds the physical source without changing ordinary activation policy.

use crate::support::*;
use super::native_reference_validation_support::*;

/// Shared nullable object constraints preserve subtype identity and reject unrelated instances.
#[test]
fn test_native_reference_object_declaration_constraints_preserve_identity() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_object_constraints"));
    let source = r#"<?php
class NativeObjectBase {}
class NativeObjectChild extends NativeObjectBase {}
class NativeObjectWrong {}
class NativeObjectSource { public ?NativeObjectBase $value = null; }
function nativeObjectConstraintAssign(mixed &$target): void {
    $target = new NativeObjectChild(); echo get_class($target), '|';
    try { $target = new NativeObjectWrong(); }
    catch (TypeError $error) { echo 'rejected|'; }
    echo get_class($target), '|';
    $target = null; echo isset($target) ? 'bad|' : 'null|';
}
if ($argc < 0) {
    $owner = new NativeObjectSource(); $alias =& $owner->value;
    $target = null; nativeObjectConstraintAssign($target); unset($target, $alias, $owner);
}
"#;
    let (user, runtime, requirements) = without_ir_opt(||
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true));
    let prefix = elephc::names::function_symbol("__eir_reference_property_");
    let suffix = elephc::names::mangle_fqn("_instance");
    let callback = user.lines().filter_map(|line| line.trim().strip_suffix(':'))
        .find(|line| line.starts_with(&prefix) && line.ends_with(&suffix))
        .expect("the declared object reference must have a generated native constraint");
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #80", "sub rsp, 80");
    address(&mut asm, "x0", "rax", "Lnative_object_constraint_null");
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 0);
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
    call(&mut asm, &elephc::names::function_symbol("nativeObjectConstraintAssign"));
    instruction(&mut asm, "add x0, sp, #8", "lea rdi, [rsp + 8]");
    call(&mut asm, &target().extern_symbol("__elephc_reference_sources_unbind_v1"));
    instruction(&mut asm, "cmp x0, #0", "test rax, rax");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(&mut asm, 8);
    call(&mut asm, "__rt_reference_cell_release");
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    expect_zero(&mut asm, "_exc_value");
    expect_zero(&mut asm, "_gc_live");
    append_status_output(&mut asm, 80);
    let mut user = inject_main_exit_harness(&user, &asm);
    user.push_str("\n.data\n.p2align 3\nLnative_object_constraint_null:\n    .quad 8\n    .quad 0\n    .quad 0\n");
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "NativeObjectChild|rejected|NativeObjectChild|null|1");
}
