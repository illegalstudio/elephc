//! Purpose:
//! Shares target-aware assembly helpers for native reference ABI regressions.
//!
//! Called from:
//! - Canonical publisher and invisible-root executable CI fixtures.
//!
//! Key details:
//! - Instructions use the host target dialect and fail through each fixture's common label.
//! - The directory guard removes only the test-owned isolated artifacts.

use crate::support::*;

/// Removes only this fixture's unique directory, including on assertion failure.
pub(super) struct FixtureDir(pub(super) std::path::PathBuf);

impl Drop for FixtureDir {
    /// Retires the generated source, assembly, objects and executable for this fixture.
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

/// Emits one target-dialect instruction for the executable runtime harness.
pub(super) fn instruction(out: &mut String, arm: &str, x86: &str) {
    out.push_str("    ");
    out.push_str(if target().arch == Arch::AArch64 { arm } else { x86 });
    out.push('\n');
}

/// Calls an internal runtime entry without applying C symbol mangling.
pub(super) fn call(out: &mut String, entry: &str) {
    instruction(out, &format!("bl {entry}"), &format!("call {entry}"));
}

/// Materializes one native code or data symbol without clobbering another test register.
pub(super) fn address(out: &mut String, arm: &str, x86: &str, symbol: &str) {
    if target().arch == Arch::AArch64 {
        if target().platform == Platform::Linux {
            out.push_str(&format!("    adrp {arm}, {symbol}\n    add {arm}, {arm}, :lo12:{symbol}\n"));
        } else { out.push_str(&format!("    adrp {arm}, {symbol}@PAGE\n    add {arm}, {arm}, {symbol}@PAGEOFF\n")); }
    } else { out.push_str(&format!("    lea {x86}, [rip + {symbol}]\n")); }
}

/// Loads a retained or borrowed test value from its stable harness stack slot.
pub(super) fn load(out: &mut String, offset: usize) {
    instruction(out, &format!("ldr x0, [sp, #{offset}]"), &format!("mov rax, QWORD PTR [rsp + {offset}]"));
}

/// Saves a newly acquired owner across subsequent runtime calls.
pub(super) fn store(out: &mut String, offset: usize) {
    instruction(out, &format!("str x0, [sp, #{offset}]"), &format!("mov QWORD PTR [rsp + {offset}], rax"));
}

/// Checks one zero runtime counter using only target-native symbol addressing.
pub(super) fn expect_zero(out: &mut String, symbol: &str) {
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

/// Adds a C-ABI count oracle that copies registry records without retaining PHP values.
pub(super) fn append_source_counter(user: &mut String) {
    let entry = target().extern_symbol("native_reference_source_count");
    user.push_str(&format!("\n.text\n.p2align 2\n.globl {entry}\n{entry}:\n"));
    if target().arch == Arch::AArch64 {
        user.push_str("    sub sp, sp, #80\n    stp x29, x30, [sp, #64]\n    add x29, sp, #64\n");
    } else { user.push_str("    push rbp\n    mov rbp, rsp\n    sub rsp, 64\n"); }
    instruction(user, "mov x0, #0", "xor eax, eax");
    store(user, 48);
    store(user, 56);
    user.push_str("Lnative_source_count_next:\n");
    instruction(user, "ldr x0, [sp, #56]", "mov rdi, QWORD PTR [rsp + 56]");
    instruction(user, "mov x1, sp", "mov rsi, rsp");
    call(user, &target().extern_symbol("__elephc_reference_sources_next_v1"));
    instruction(user, "cmp x0, #0", "test rax, rax");
    instruction(user, "b.eq Lnative_source_count_done", "je Lnative_source_count_done");
    instruction(user, "cmp x0, #1", "cmp rax, 1");
    instruction(user, "b.ne Lnative_source_count_failed", "jne Lnative_source_count_failed");
    load(user, 0);
    store(user, 56);
    load(user, 48);
    instruction(user, "add x0, x0, #1", "add rax, 1");
    store(user, 48);
    instruction(user, "b Lnative_source_count_next", "jmp Lnative_source_count_next");
    user.push_str("Lnative_source_count_failed:\n");
    instruction(user, "mov x0, #-1", "mov rax, -1");
    store(user, 48);
    user.push_str("Lnative_source_count_done:\n");
    load(user, 48);
    if target().arch == Arch::AArch64 {
        user.push_str("    ldp x29, x30, [sp, #64]\n    add sp, sp, #80\n    ret\n");
    } else { user.push_str("    leave\n    ret\n"); }
}

