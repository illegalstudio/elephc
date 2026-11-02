//! Purpose:
//! Exercises real native validation roots independently of unfinished typed publishers.
//!
//! Called from:
//! - Runtime GC executable CI on the three host targets.
//!
//! Key details:
//! - Nested pins must be invisible to array copies and canonical native cell cloning.
//! - Interleaved activations can remove an older record before the current head.
//! - Every acquired owner is retired before checking actual live heap accounting.

use crate::support::*;

/// Removes only this fixture's unique directory, including on assertion failure.
struct FixtureDir(std::path::PathBuf);

impl Drop for FixtureDir {
    /// Retires the generated source, assembly, objects and executable for this fixture.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Emits one target-dialect instruction for the executable runtime harness.
fn instruction(out: &mut String, arm: &str, x86: &str) {
    out.push_str("    ");
    out.push_str(if target().arch == Arch::AArch64 { arm } else { x86 });
    out.push('\n');
}

/// Calls an internal runtime entry without applying C symbol mangling.
fn call(out: &mut String, entry: &str) {
    instruction(out, &format!("bl {entry}"), &format!("call {entry}"));
}

/// Loads a retained or borrowed test value from its stable harness stack slot.
fn load(out: &mut String, offset: usize) {
    instruction(out, &format!("ldr x0, [sp, #{offset}]"), &format!("mov rax, QWORD PTR [rsp + {offset}]"));
}

/// Saves a newly acquired owner across subsequent runtime calls.
fn store(out: &mut String, offset: usize) {
    instruction(out, &format!("str x0, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], rax"));
}

/// Registers one caller-owned native record using the pin helper's explicit C ABI.
fn enter(out: &mut String, record: usize, value: usize) {
    instruction(out, &format!("add x0, sp, #{record}"), &format!("lea rdi, [rsp + {record}]"));
    instruction(out, &format!("ldr x1, [sp, #{value}]"), &format!("mov rsi, QWORD PTR [rsp + {value}]"));
    call(out, "__rt_reference_validation_pin_enter");
}

/// Removes one native record before its protected owner retirement.
fn leave(out: &mut String, record: usize) {
    instruction(out, &format!("add x0, sp, #{record}"), &format!("lea rdi, [rsp + {record}]"));
    call(out, "__rt_reference_validation_pin_leave");
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
}

/// Verifies that artificial owners never turn a singleton array reference into a shared alias.
fn expect_detached_array_copy(out: &mut String) {
    load(out, 0);
    call(out, "__rt_reference_array_copy");
    instruction(out, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
    instruction(out, "cmp x0, x9", "cmp rax, r10");
    instruction(out, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    call(out, "__rt_decref_mixed");
}

/// Checks one zero runtime counter using only target-native symbol addressing.
fn expect_zero(out: &mut String, symbol: &str) {
    if target().arch == Arch::AArch64 {
        let address = if target().platform == Platform::Linux {
            format!("    adrp x9, {symbol}\n    add x9, x9, :lo12:{symbol}\n")
        } else { format!("    adrp x9, {symbol}@PAGE\n    add x9, x9, {symbol}@PAGEOFF\n") };
        out.push_str(&address);
        instruction(out, "ldr x9, [x9]", "");
        instruction(out, "cmp x9, #0", "");
        instruction(out, "b.ne Lvalidation_pin_fail", "");
    } else {
        instruction(out, "", &format!("cmp QWORD PTR [rip + {symbol}], 0"));
        instruction(out, "", "jne Lvalidation_pin_fail");
    }
}

/// Real nested pins preserve singleton copy semantics, unlink in any order and leave no heap owners.
#[test]
fn test_native_reference_validation_pins_preserve_copy_semantics_and_retire() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_validation_pins"));
    let (user, runtime, requirements) =
        compile_source_to_asm_with_options("<?php", &dir.0, 65_536, false, true);
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #160", "sub rsp, 160");
    // Create a canonical native cell whose PHPRef has exactly one real owner, the cell.
    instruction(&mut asm, "mov x0, #0", "mov eax, 0");
    instruction(&mut asm, "mov x1, #41", "mov edi, 41");
    instruction(&mut asm, "mov x2, #0", "xor esi, esi");
    call(&mut asm, "__rt_mixed_from_value");
    store(&mut asm, 8);
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 0);
    load(&mut asm, 8);
    call(&mut asm, "__rt_decref_mixed");
    instruction(&mut asm, "mov x0, #7", "mov edi, 7");
    call(&mut asm, "__rt_reference_cell_new");
    store(&mut asm, 8);
    instruction(&mut asm, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
    instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
    load(&mut asm, 0);
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 16);
    enter(&mut asm, 32, 0);
    enter(&mut asm, 64, 0);
    enter(&mut asm, 96, 16);
    expect_detached_array_copy(&mut asm);
    // A singleton canonical clone must contain a different wrapper with the same value.
    load(&mut asm, 8);
    call(&mut asm, "__rt_reference_cell_clone");
    store(&mut asm, 24);
    instruction(&mut asm, "ldr x0, [x0]", "mov rax, QWORD PTR [rax]");
    instruction(&mut asm, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
    instruction(&mut asm, "cmp x0, x9", "cmp rax, r10");
    instruction(&mut asm, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    instruction(&mut asm, "ldr x9, [x0]", "mov r10, QWORD PTR [rax]");
    instruction(&mut asm, "cmp x9, #7", "cmp r10, 7");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    instruction(&mut asm, "ldr x9, [x0, #16]", "mov r10, QWORD PTR [rax + 16]");
    instruction(&mut asm, "cmp x9, #1", "cmp r10, 1");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    call(&mut asm, "__rt_mixed_deref");
    instruction(&mut asm, "ldr x9, [x0, #8]", "mov r10, QWORD PTR [rax + 8]");
    instruction(&mut asm, "cmp x9, #41", "cmp r10, 41");
    instruction(&mut asm, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(&mut asm, 24);
    call(&mut asm, "__rt_reference_cell_release");
    // Retire a middle record and the oldest record while the unrelated head remains live.
    leave(&mut asm, 64);
    expect_detached_array_copy(&mut asm);
    leave(&mut asm, 32);
    expect_detached_array_copy(&mut asm);
    leave(&mut asm, 96);
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
    load(&mut asm, 16);
    call(&mut asm, "__rt_decref_mixed");
    load(&mut asm, 8);
    call(&mut asm, "__rt_reference_cell_release");
    expect_zero(&mut asm, "_gc_live");
    instruction(&mut asm, "mov x0, #1", "mov eax, 1");
    instruction(&mut asm, "b Lvalidation_pin_print", "jmp Lvalidation_pin_print");
    asm.push_str("Lvalidation_pin_fail:\n");
    instruction(&mut asm, "mov x0, #0", "xor eax, eax");
    asm.push_str("Lvalidation_pin_print:\n");
    call(&mut asm, "__rt_itoa");
    if target().arch == Arch::AArch64 {
        instruction(&mut asm, "mov x0, #1", "");
        if target().platform == Platform::Linux {
            instruction(&mut asm, "mov x8, #64", "");
            instruction(&mut asm, "svc #0", "");
        } else {
            instruction(&mut asm, "mov x16, #4", "");
            instruction(&mut asm, "svc #0x80", "");
        }
    } else {
        asm.push_str("    mov rsi, rax\n    mov edi, 1\n    mov eax, 1\n    syscall\n");
    }
    instruction(&mut asm, "add sp, sp, #160", "add rsp, 160");
    let user = inject_main_exit_harness(&user, &asm);
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "1");
}
