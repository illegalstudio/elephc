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

/// Materializes one native code or data symbol without clobbering another test register.
fn address(out: &mut String, arm: &str, x86: &str, symbol: &str) {
    if target().arch == Arch::AArch64 {
        if target().platform == Platform::Linux {
            out.push_str(&format!("    adrp {arm}, {symbol}\n    add {arm}, {arm}, :lo12:{symbol}\n"));
        } else { out.push_str(&format!("    adrp {arm}, {symbol}@PAGE\n    add {arm}, {arm}, {symbol}@PAGEOFF\n")); }
    } else { out.push_str(&format!("    lea {x86}, [rip + {symbol}]\n")); }
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

/// Invokes the actual protected coercion entry with the fixture's context and descriptor.
fn coerce(out: &mut String, status: usize) {
    instruction(out, "add x0, sp, #128", "lea rdi, [rsp + 128]");
    instruction(out, "add x1, sp, #144", "lea rsi, [rsp + 144]");
    instruction(out, "ldr x2, [sp, #8]", "mov rdx, QWORD PTR [rsp + 8]");
    instruction(out, "add x3, sp, #160", "lea rcx, [rsp + 160]");
    call(out, "__rt_reference_validation_coerce");
    instruction(out, &format!("cmp x0, #{status}"), &format!("cmp rax, {status}"));
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
}

/// Checks original-input acceptance, contained throws and a separately parked caller exception.
fn protected_coercion(out: &mut String) {
    for offset in [128, 136] {
        instruction(out, &format!("str xzr, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], 0"));
    }
    call(out, "__rt_unmanaged_reference_escape_error_new");
    store(out, 176);
    address(out, "x9", "r10", "_exc_value");
    instruction(out, "str x0, [x9]", "mov QWORD PTR [r10], rax");
    address(out, "x0", "rax", "Lvalidation_pin_accept");
    store(out, 144);
    coerce(out, 0);
    load(out, 160);
    instruction(out, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]");
    instruction(out, "cmp x0, x9", "cmp rax, r10");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    call(out, "__rt_decref_mixed");
    address(out, "x0", "rax", "Lvalidation_pin_throw");
    store(out, 144);
    coerce(out, 2);
    load(out, 160);
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(out, 136);
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    // A pending validation exception stops before dereferencing even an empty descriptor.
    instruction(out, "str xzr, [sp, #144]", "mov QWORD PTR [rsp + 144], 0");
    coerce(out, 2);
    load(out, 136);
    instruction(out, "str xzr, [sp, #136]", "mov QWORD PTR [rsp + 136], 0");
    call(out, "__rt_decref_any");
    address(out, "x9", "r10", "_exc_value");
    instruction(out, "ldr x9, [x9]", "mov r10, QWORD PTR [r10]");
    load(out, 176);
    instruction(out, "cmp x0, x9", "cmp rax, r10");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    address(out, "x9", "r10", "_exc_value");
    instruction(out, "str xzr, [x9]", "mov QWORD PTR [r10], 0");
    call(out, "__rt_decref_any");
}

/// Calls a generated float declaration with the same original bool in weak and strict modes.
fn generated_declaration_coercion(out: &mut String, callback: &str) {
    load(out, 8);
    store(out, 184);
    instruction(out, "mov x0, #3", "mov eax, 3");
    instruction(out, "mov x1, #1", "mov edi, 1");
    instruction(out, "mov x2, #0", "xor esi, esi");
    call(out, "__rt_mixed_from_value");
    store(out, 8);
    address(out, "x0", "rax", callback);
    store(out, 144);
    coerce(out, 0);
    load(out, 160);
    call(out, "__rt_mixed_unbox");
    instruction(out, "cmp x0, #2", "cmp rax, 2");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(out, 160);
    call(out, "__rt_mixed_cast_int");
    instruction(out, "cmp x0, #1", "cmp rax, 1");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(out, 160);
    call(out, "__rt_decref_mixed");
    instruction(out, "mov x0, #1", "mov eax, 1");
    store(out, 128);
    coerce(out, 2);
    load(out, 160);
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(out, 136);
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    instruction(out, "str xzr, [sp, #136]", "mov QWORD PTR [rsp + 136], 0");
    call(out, "__rt_decref_any");
    instruction(out, "str xzr, [sp, #128]", "mov QWORD PTR [rsp + 128], 0");
    load(out, 8);
    call(out, "__rt_decref_mixed");
    load(out, 184);
    store(out, 8);
}

/// Exercises C-int output width and protected candidate retirement with a surviving RHS owner.
fn validation_operation_callbacks(out: &mut String) {
    instruction(out, "str xzr, [sp, #192]", "mov QWORD PTR [rsp + 192], 0");
    instruction(out, "mov w9, #23", "mov r10d, 23");
    instruction(out, "str w9, [sp, #196]", "mov DWORD PTR [rsp + 196], r10d");
    instruction(out, "add x0, sp, #128", "lea rdi, [rsp + 128]");
    instruction(out, "ldr x1, [sp, #8]", "mov rsi, QWORD PTR [rsp + 8]");
    instruction(out, "ldr x2, [sp, #8]", "mov rdx, QWORD PTR [rsp + 8]");
    instruction(out, "add x3, sp, #192", "lea rcx, [rsp + 192]");
    call(out, "__rt_reference_validation_identical");
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    instruction(out, "ldr w9, [sp, #192]", "mov eax, DWORD PTR [rsp + 192]");
    instruction(out, "cmp w9, #1", "cmp eax, 1");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    instruction(out, "ldr w9, [sp, #196]", "mov eax, DWORD PTR [rsp + 196]");
    instruction(out, "cmp w9, #23", "cmp eax, 23");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(out, 8);
    call(out, "__rt_incref");
    instruction(out, "ldr x1, [sp, #8]", "mov rsi, QWORD PTR [rsp + 8]");
    instruction(out, "add x0, sp, #128", "lea rdi, [rsp + 128]");
    call(out, "__rt_reference_validation_release");
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
}

/// Physical source probes accept the current cell and reject replaced copied metadata.
fn validation_physical_source(out: &mut String) {
    instruction(out, "mov x0, #1", "mov eax, 1");
    store(out, 128);
    instruction(out, "add x0, sp, #8", "lea rax, [rsp + 8]");
    store(out, 136);
    load(out, 8);
    store(out, 144);
    for expected in [1, 0] {
        if expected == 0 {
            instruction(out, "str xzr, [sp, #144]", "mov QWORD PTR [rsp + 144], 0");
        }
        instruction(out, "mov x0, #0", "xor edi, edi");
        instruction(out, "add x1, sp, #128", "lea rsi, [rsp + 128]");
        instruction(out, "ldr x2, [sp]", "mov rdx, QWORD PTR [rsp]");
        call(out, "__rt_reference_validation_is_current");
        instruction(out, &format!("cmp x0, #{expected}"), &format!("cmp rax, {expected}"));
        instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    }
}

/// Registers a real Rust type source and validates through generated native callbacks.
fn native_coordinator(out: &mut String, callback: &str) {
    load(out, 0);
    call(out, "__rt_mixed_clone");
    store(out, 304);
    for offset in [128, 136] {
        instruction(out, &format!("str xzr, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], 0"));
    }
    address(out, "x0", "rax", callback);
    store(out, 144);
    instruction(out, "add x0, sp, #8", "lea rdi, [rsp + 8]");
    instruction(out, "ldr x1, [sp, #8]", "mov rsi, QWORD PTR [rsp + 8]");
    instruction(out, "add x2, sp, #144", "lea rdx, [rsp + 144]");
    call(out, &target().extern_symbol("__elephc_reference_sources_bind_v1"));
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
    instruction(out, "ldr x0, [sp, #8]", "mov rdi, QWORD PTR [rsp + 8]");
    instruction(out, "ldr x1, [sp]", "mov rsi, QWORD PTR [rsp]");
    call(out, &target().extern_symbol("__elephc_reference_sources_set_reference_v1"));
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    instruction(out, "mov x0, #3", "mov eax, 3");
    instruction(out, "mov x1, #1", "mov edi, 1");
    instruction(out, "mov x2, #0", "xor esi, esi");
    call(out, "__rt_mixed_from_value");
    store(out, 208);
    for (strict, expected) in [(0, 0), (1, 2)] {
        instruction(out, &format!("mov x0, #{strict}"), &format!("mov eax, {strict}"));
        store(out, 128);
        instruction(out, "ldr x0, [sp]", "mov rdi, QWORD PTR [rsp]");
        instruction(out, "ldr x1, [sp, #208]", "mov rsi, QWORD PTR [rsp + 208]");
        instruction(out, "add x2, sp, #128", "lea rdx, [rsp + 128]");
        instruction(out, "add x3, sp, #224", "lea rcx, [rsp + 224]");
        call(out, "__rt_reference_validate_native");
        instruction(out, &format!("cmp x0, #{expected}"), &format!("cmp rax, {expected}"));
        instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
        load(out, 224);
        if strict == 0 {
            call(out, "__rt_mixed_unbox");
            instruction(out, "cmp x0, #2", "cmp rax, 2");
            instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
            load(out, 224);
            call(out, "__rt_mixed_cast_int");
            instruction(out, "cmp x0, #1", "cmp rax, 1");
            instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
            native_commit(out, 224, 0);
        } else {
            instruction(out, "cmp x0, #0", "test rax, rax");
            instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
            load(out, 136);
            instruction(out, "cmp x0, #0", "test rax, rax");
            instruction(out, "b.eq Lvalidation_pin_fail", "je Lvalidation_pin_fail");
            // A pending validation error consumes a supplied candidate without publishing it.
            load(out, 208);
            call(out, "__rt_mixed_clone");
            store(out, 224);
            native_commit(out, 224, 2);
            load(out, 136);
            instruction(out, "str xzr, [sp, #136]", "mov QWORD PTR [rsp + 136], 0");
            call(out, "__rt_decref_any");
        }
    }
    load(out, 0);
    call(out, "__rt_mixed_unbox");
    instruction(out, "cmp x0, #2", "cmp rax, 2");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    load(out, 0);
    call(out, "__rt_mixed_cast_int");
    instruction(out, "cmp x0, #1", "cmp rax, 1");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    instruction(out, "add x0, sp, #8", "lea rdi, [rsp + 8]");
    call(out, &target().extern_symbol("__elephc_reference_sources_unbind_v1"));
    instruction(out, "cmp x0, #0", "test rax, rax");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    // With no source left, restore the original detached owner for the later clone checks.
    native_commit(out, 304, 0);
    load(out, 208);
    call(out, "__rt_decref_mixed");
}

/// Transfers one candidate owner, then clears the consumed output instead of releasing it twice.
fn native_commit(out: &mut String, candidate: usize, expected: usize) {
    instruction(out, "add x0, sp, #128", "lea rdi, [rsp + 128]");
    instruction(out, "ldr x1, [sp]", "mov rsi, QWORD PTR [rsp]");
    instruction(out, &format!("ldr x2, [sp, #{candidate}]"), &format!("mov rdx, QWORD PTR [rsp + {candidate}]"));
    call(out, "__rt_reference_commit_native");
    instruction(out, &format!("cmp x0, #{expected}"), &format!("cmp rax, {expected}"));
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    instruction(out, &format!("str xzr, [sp, #{candidate}]"), &format!("mov QWORD PTR [rsp + {candidate}], 0"));
}

/// A real native binding or independent PHPRef owner keeps a canonical clone shared.
fn expect_shared_native_clone(out: &mut String) {
    load(out, 8);
    call(out, "__rt_reference_cell_clone");
    instruction(out, "ldr x9, [sp, #8]", "mov r10, QWORD PTR [rsp + 8]");
    instruction(out, "cmp x0, x9", "cmp rax, r10");
    instruction(out, "b.ne Lvalidation_pin_fail", "jne Lvalidation_pin_fail");
    call(out, "__rt_reference_cell_release");
}

/// Real nested pins preserve singleton copy semantics, unlink in any order and leave no heap owners.
#[test]
fn test_native_reference_validation_pins_preserve_copy_semantics_and_retire() {
    let dir = FixtureDir(make_cli_test_dir("elephc_native_reference_validation_pins"));
    let source = "<?php class NativeValidationDeclarationProbe { public float $value = 0.0; } \
        $object = new NativeValidationDeclarationProbe(); $alias = &$object->value; \
        unset($alias, $object);";
    let (user, runtime, requirements) =
        compile_source_to_asm_with_native_reference_validation(source, &dir.0, 65_536, true);
    let callback_prefix = elephc::names::function_symbol("__eir_reference_property_");
    let callback_suffix = elephc::names::mangle_fqn("_instance");
    let callback = user.lines().filter_map(|line| line.trim().strip_suffix(':'))
        .find(|line| line.starts_with(&callback_prefix) && line.ends_with(&callback_suffix))
        .expect("the typed physical reference declaration must produce a native callback");
    let mut asm = String::new();
    instruction(&mut asm, "sub sp, sp, #320", "sub rsp, 320");
    // Create a canonical native cell whose PHPRef has exactly one real owner, the cell.
    instruction(&mut asm, "mov x0, #0", "mov eax, 0");
    instruction(&mut asm, "mov x1, #41", "mov edi, 41");
    instruction(&mut asm, "mov x2, #0", "xor esi, esi");
    call(&mut asm, "__rt_mixed_from_value");
    store(&mut asm, 8);
    protected_coercion(&mut asm);
    generated_declaration_coercion(&mut asm, callback);
    validation_operation_callbacks(&mut asm);
    load(&mut asm, 8);
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 0);
    load(&mut asm, 8);
    call(&mut asm, "__rt_decref_mixed");
    instruction(&mut asm, "mov x0, #7", "mov edi, 7");
    call(&mut asm, "__rt_reference_cell_new");
    store(&mut asm, 8);
    instruction(&mut asm, "ldr x9, [sp]", "mov r10, QWORD PTR [rsp]");
    instruction(&mut asm, "str x9, [x0]", "mov QWORD PTR [rax], r10");
    validation_physical_source(&mut asm);
    native_coordinator(&mut asm, callback);
    load(&mut asm, 0);
    call(&mut asm, "__rt_reference_new");
    store(&mut asm, 16);
    enter(&mut asm, 32, 0);
    enter(&mut asm, 64, 0);
    enter(&mut asm, 96, 16);
    enter(&mut asm, 128, 8);
    // Two invisible staging leases on another cell must hide its child owner only once.
    instruction(&mut asm, "mov x0, #7", "mov edi, 7");
    call(&mut asm, "__rt_reference_cell_new");
    store(&mut asm, 304);
    load(&mut asm, 0);
    call(&mut asm, "__rt_incref");
    instruction(&mut asm, "ldr x9, [sp, #304]", "mov r10, QWORD PTR [rsp + 304]");
    instruction(&mut asm, "str x0, [x9]", "mov QWORD PTR [r10], rax");
    enter(&mut asm, 160, 304);
    enter(&mut asm, 192, 304);
    // A borrowed frame with forged cell-like words cannot contribute an invisible child owner.
    instruction(&mut asm, "mov x9, #0x708", "mov r10, 0x708");
    instruction(&mut asm, "str x9, [sp, #264]", "mov QWORD PTR [rsp + 264], r10");
    instruction(&mut asm, "mov w9, #1", "mov r10d, 1");
    instruction(&mut asm, "str w9, [sp, #260]", "mov DWORD PTR [rsp + 260], r10d");
    load(&mut asm, 0);
    store(&mut asm, 272);
    instruction(&mut asm, "add x9, sp, #272", "lea r10, [rsp + 272]");
    instruction(&mut asm, "str x9, [sp, #288]", "mov QWORD PTR [rsp + 288], r10");
    enter(&mut asm, 224, 288);
    expect_shared_native_clone(&mut asm);
    leave(&mut asm, 224);
    load(&mut asm, 304);
    call(&mut asm, "__rt_reference_cell_release");
    // A separate PHP alias remains visible despite both roots naming the same staging cell.
    load(&mut asm, 0);
    call(&mut asm, "__rt_incref");
    expect_shared_native_clone(&mut asm);
    load(&mut asm, 0);
    call(&mut asm, "__rt_decref_mixed");
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
    leave(&mut asm, 160);
    expect_detached_array_copy(&mut asm);
    leave(&mut asm, 192);
    // Retire a middle record and the oldest record while the unrelated head remains live.
    leave(&mut asm, 64);
    expect_detached_array_copy(&mut asm);
    leave(&mut asm, 32);
    expect_detached_array_copy(&mut asm);
    leave(&mut asm, 96);
    load(&mut asm, 16);
    call(&mut asm, "__rt_decref_mixed");
    load(&mut asm, 8);
    call(&mut asm, "__rt_reference_cell_release");
    instruction(&mut asm, "str xzr, [sp, #8]", "mov QWORD PTR [rsp + 8], 0");
    // The final invisible native-cell owner must dispatch by heap kind and release its child.
    leave(&mut asm, 128);
    expect_zero(&mut asm, "_rt_reference_validation_pins_top");
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
    instruction(&mut asm, "add sp, sp, #320", "add rsp, 320");
    instruction(&mut asm, "b Lvalidation_pin_callbacks_end", "jmp Lvalidation_pin_callbacks_end");
    asm.push_str("Lvalidation_pin_accept:\n");
    instruction(&mut asm, "mov x0, x1", "mov rax, rsi");
    instruction(&mut asm, "b __rt_incref", "jmp __rt_incref");
    asm.push_str("Lvalidation_pin_throw:\n");
    instruction(&mut asm, "b __rt_unmanaged_reference_escape_error", "jmp __rt_unmanaged_reference_escape_error");
    asm.push_str("Lvalidation_pin_callbacks_end:\n");
    let user = inject_main_exit_harness(&user, &asm);
    let runtime = runtime_obj_for_asm(&runtime);
    let output = assemble_and_run(&user, &runtime, &dir.0, &requirements, &default_link_paths(), &[]);
    assert_eq!(output, "1");
}
