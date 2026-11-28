//! Purpose:
//! Emits core runtime helpers for scope-only literal eval AOT fragments.
//! Provides a small materialized name-to-Mixed-cell map without linking the
//! optional Rust eval interpreter bridge.
//!
//! Called from:
//! - `crate::codegen::runtime::emitters::emit_runtime()` when `eval_scope` is
//!   needed without `eval_bridge`.
//!
//! Key details:
//! - Entry names are generated static data labels, so the scope stores borrowed
//!   name pointers instead of copying name bytes.
//! - Owned Mixed cells are released on overwrite and scope free.

use elephc_memoir::values::eval_scope_storage::{self, EntryField};

use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

const EVAL_STATUS_OK: i64 = 0;
const EVAL_STATUS_RUNTIME_FATAL: i64 = 2;
const EVAL_SCOPE_FLAG_PRESENT: i64 = 1;
const EVAL_SCOPE_FLAG_DIRTY: i64 = 1 << 2;
const EVAL_SCOPE_FLAG_OWNED: i64 = 1 << 4;

/// Emits the target-specific eval-scope core helper surface.
pub(crate) fn emit_eval_scope_runtime(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: eval scope core helpers ---");
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64_eval_scope_runtime(emitter),
        Arch::X86_64 => emit_x86_64_eval_scope_runtime(emitter),
    }
}

/// Emits ARM64 eval-scope helpers. `__elephc_eval_value_null` comes from the
/// eval bridge value wrappers, which scope-only programs also emit.
fn emit_aarch64_eval_scope_runtime(emitter: &mut Emitter) {
    eval_scope_storage::emit_lifecycle(emitter, EVAL_SCOPE_FLAG_OWNED);
    emit_aarch64_eval_scope_set(emitter);
    emit_aarch64_eval_scope_get(emitter);
    emit_aarch64_eval_scope_native_type(emitter);
}

/// Emits the ARM64 scope setter.
fn emit_aarch64_eval_scope_set(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_set");
    emitter.instruction("cbz x0, __elephc_eval_scope_set_fatal");               // reject null scope handles
    emitter.instruction("cbz x2, __elephc_eval_scope_set_frame");               // empty names do not need a readable pointer
    emitter.instruction("cbz x1, __elephc_eval_scope_set_fatal");               // non-empty names must provide bytes
    emitter.label("__elephc_eval_scope_set_frame");
    emitter.instruction("sub sp, sp, #96");                                     // reserve a frame for inputs and callee-saved scan state
    emitter.instruction("stp x29, x30, [sp, #0]");                              // save frame pointer and return address across runtime calls
    emitter.instruction("stp x19, x20, [sp, #16]");                             // preserve scope and name pointer
    emitter.instruction("stp x21, x22, [sp, #32]");                             // preserve name length and cell pointer
    emitter.instruction("stp x23, x24, [sp, #48]");                             // preserve flags and previous-next pointer
    emitter.instruction("stp x25, x26, [sp, #64]");                             // preserve current entry and allocated entry pointer
    emitter.instruction("mov x29, sp");                                         // establish a stable frame for nested runtime calls
    emitter.instruction("mov x19, x0");                                         // save scope header pointer
    emitter.instruction("mov x20, x1");                                         // save name byte pointer
    emitter.instruction("mov x21, x2");                                         // save name byte length
    emitter.instruction("mov x22, x3");                                         // save Mixed cell pointer
    emitter.instruction("mov x23, x4");                                         // save caller-provided ABI flags
    emitter.instruction("mov x24, x19");                                        // previous-next pointer initially addresses scope->head
    eval_scope_storage::emit_load_link(emitter, "x25", "x24");
    emitter.label("__elephc_eval_scope_set_probe");
    emitter.instruction("cbz x25, __elephc_eval_scope_set_insert");             // insert when the name is absent
    eval_scope_storage::emit_load_entry_field(
        emitter,
        "x9",
        "x25",
        EntryField::NameLength,
    );
    emitter.instruction("cmp x9, x21");                                         // compare candidate length with requested length
    emitter.instruction("b.ne __elephc_eval_scope_set_next");                   // different lengths cannot match
    eval_scope_storage::emit_load_entry_field(emitter, "x10", "x25", EntryField::Name);
    emitter.instruction("mov x11, #0");                                         // byte index for the equality loop
    emitter.label("__elephc_eval_scope_set_cmp");
    emitter.instruction("cmp x11, x21");                                        // have all bytes matched?
    emitter.instruction("b.eq __elephc_eval_scope_set_update");                 // equal length and bytes select this entry
    emitter.instruction("ldrb w12, [x10, x11]");                                // load one existing name byte
    emitter.instruction("ldrb w13, [x20, x11]");                                // load the corresponding requested name byte
    emitter.instruction("cmp w12, w13");                                        // compare candidate and requested name bytes
    emitter.instruction("b.ne __elephc_eval_scope_set_next");                   // any byte mismatch means this entry is not the target
    emitter.instruction("add x11, x11, #1");                                    // advance to the next byte
    emitter.instruction("b __elephc_eval_scope_set_cmp");                       // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_set_next");
    emitter.instruction("mov x24, x25");                                        // previous-next pointer now addresses current->next
    eval_scope_storage::emit_load_entry_field(emitter, "x25", "x25", EntryField::Next);
    emitter.instruction("b __elephc_eval_scope_set_probe");                     // continue scanning the entry list
    eval_scope_storage::emit_replace(
        emitter, EVAL_SCOPE_FLAG_OWNED, EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY,
    );
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_OK));               // report successful scope write
    emitter.instruction("b __elephc_eval_scope_set_done");                      // restore the frame and return
    eval_scope_storage::emit_insert(emitter, EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY);
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_OK));               // report successful scope write
    emitter.label("__elephc_eval_scope_set_done");
    emitter.instruction("ldp x25, x26, [sp, #64]");                             // restore callee-saved entry registers
    emitter.instruction("ldp x23, x24, [sp, #48]");                             // restore callee-saved flag and link registers
    emitter.instruction("ldp x21, x22, [sp, #32]");                             // restore callee-saved name length and cell registers
    emitter.instruction("ldp x19, x20, [sp, #16]");                             // restore callee-saved scope and name registers
    emitter.instruction("ldp x29, x30, [sp, #0]");                              // restore frame pointer and return address
    emitter.instruction("add sp, sp, #96");                                     // release the setter frame
    emitter.instruction("ret");                                                 // return the eval status in x0
    emitter.label("__elephc_eval_scope_set_fatal");
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status without mutating scope
}

/// Emits the ARM64 scope getter.
fn emit_aarch64_eval_scope_get(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_get");
    emitter.instruction("cbz x0, __elephc_eval_scope_get_fatal");               // reject null scope handles
    emitter.instruction("cbz x2, __elephc_eval_scope_get_search");              // empty names do not need a readable pointer
    emitter.instruction("cbz x1, __elephc_eval_scope_get_fatal");               // non-empty names must provide bytes
    emitter.label("__elephc_eval_scope_get_search");
    eval_scope_storage::emit_load_head(emitter, "x9", "x0");
    emitter.label("__elephc_eval_scope_get_probe");
    emitter.instruction("cbz x9, __elephc_eval_scope_get_missing");             // missing names produce null cell and zero flags
    eval_scope_storage::emit_load_entry_field(
        emitter,
        "x10",
        "x9",
        EntryField::NameLength,
    );
    emitter.instruction("cmp x10, x2");                                         // compare candidate length with requested length
    emitter.instruction("b.ne __elephc_eval_scope_get_next");                   // different lengths cannot match
    eval_scope_storage::emit_load_entry_field(emitter, "x10", "x9", EntryField::Name);
    emitter.instruction("mov x11, #0");                                         // byte index for the equality loop
    emitter.label("__elephc_eval_scope_get_cmp");
    emitter.instruction("cmp x11, x2");                                         // have all bytes matched?
    emitter.instruction("b.eq __elephc_eval_scope_get_found");                  // equal length and bytes select this entry
    emitter.instruction("ldrb w12, [x10, x11]");                                // load one existing name byte
    emitter.instruction("ldrb w13, [x1, x11]");                                 // load the corresponding requested name byte
    emitter.instruction("cmp w12, w13");                                        // compare candidate and requested name bytes
    emitter.instruction("b.ne __elephc_eval_scope_get_next");                   // any byte mismatch means this entry is not the target
    emitter.instruction("add x11, x11, #1");                                    // advance to the next byte
    emitter.instruction("b __elephc_eval_scope_get_cmp");                       // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_get_next");
    eval_scope_storage::emit_load_entry_field(emitter, "x9", "x9", EntryField::Next);
    emitter.instruction("b __elephc_eval_scope_get_probe");                     // continue scanning the scope list
    emitter.label("__elephc_eval_scope_get_found");
    emitter.instruction("cbz x3, __elephc_eval_scope_get_found_flags");         // skip cell output when caller passed null
    eval_scope_storage::emit_load_entry_field(emitter, "x10", "x9", EntryField::Cell);
    emitter.instruction("str x10, [x3]");                                       // write the output Mixed cell pointer
    emitter.label("__elephc_eval_scope_get_found_flags");
    emitter.instruction("cbz x4, __elephc_eval_scope_get_ok");                  // skip flags output when caller passed null
    eval_scope_storage::emit_load_entry_flags(emitter, "w10", "x9");
    emitter.instruction("str w10, [x4]");                                       // write the output ABI flags
    emitter.instruction("b __elephc_eval_scope_get_ok");                        // finish with success
    emitter.label("__elephc_eval_scope_get_missing");
    emitter.instruction("cbz x3, __elephc_eval_scope_get_missing_flags");       // skip cell output when caller passed null
    emitter.instruction("str xzr, [x3]");                                       // missing variables have no cell pointer
    emitter.label("__elephc_eval_scope_get_missing_flags");
    emitter.instruction("cbz x4, __elephc_eval_scope_get_ok");                  // skip flags output when caller passed null
    emitter.instruction("str wzr, [x4]");                                       // missing variables have zero ABI flags
    emitter.label("__elephc_eval_scope_get_ok");
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_OK));               // report successful scope lookup
    emitter.instruction("ret");                                                 // return the eval status in x0
    emitter.label("__elephc_eval_scope_get_fatal");
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status
}

/// Emits the scope-only native-type validator stub.
///
/// Literal AOT eval does not enter Magician, so no interpreter assignment can violate a native
/// local's declared object type. The shared barrier still calls the bridge ABI, and this stub
/// keeps that surface link-complete while reporting success.
fn emit_aarch64_eval_scope_native_type(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_native_type_v1");
    emitter.instruction(&format!("mov x0, #{}", EVAL_STATUS_OK));               // scope-only barriers have no interpreter write to validate
    emitter.instruction("ret");                                                 // return the successful eval status
}

/// Emits x86_64 eval-scope helpers. `__elephc_eval_value_null` comes from the
/// eval bridge value wrappers, which scope-only programs also emit.
fn emit_x86_64_eval_scope_runtime(emitter: &mut Emitter) {
    eval_scope_storage::emit_lifecycle(emitter, EVAL_SCOPE_FLAG_OWNED);
    emit_x86_64_eval_scope_set(emitter);
    emit_x86_64_eval_scope_get(emitter);
    emit_x86_64_eval_scope_native_type(emitter);
}

/// Emits the x86_64 scope setter.
fn emit_x86_64_eval_scope_set(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_set");
    emitter.instruction("test rdi, rdi");                                       // reject null scope handles
    emitter.instruction("jz __elephc_eval_scope_set_fatal");                    // return a runtime-fatal status for null scope
    emitter.instruction("test rdx, rdx");                                       // empty names do not need a readable pointer
    emitter.instruction("jz __elephc_eval_scope_set_frame");                    // skip the pointer check for empty names
    emitter.instruction("test rsi, rsi");                                       // non-empty names must provide bytes
    emitter.instruction("jz __elephc_eval_scope_set_fatal");                    // return a runtime-fatal status for invalid names
    emitter.label("__elephc_eval_scope_set_frame");
    emitter.instruction("push rbp");                                            // preserve the caller frame pointer before scope mutation
    emitter.instruction("mov rbp, rsp");                                        // establish a stable frame for saved inputs and scan state
    emitter.instruction("sub rsp, 64");                                         // reserve scope, name, length, cell, flags, previous-next, current, and index slots
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save scope header pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save name byte pointer
    emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                       // save name byte length
    emitter.instruction("mov QWORD PTR [rbp - 32], rcx");                       // save Mixed cell pointer
    emitter.instruction("mov QWORD PTR [rbp - 40], r8");                        // save caller-provided ABI flags
    emitter.instruction("mov QWORD PTR [rbp - 48], rdi");                       // previous-next pointer initially addresses scope->head
    eval_scope_storage::emit_load_head(emitter, "r10", "rdi");
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the current candidate entry
    emitter.label("__elephc_eval_scope_set_probe");
    emitter.instruction("mov r9, QWORD PTR [rbp - 56]");                        // reload the current candidate entry
    emitter.instruction("test r9, r9");                                         // is the requested name absent?
    emitter.instruction("jz __elephc_eval_scope_set_insert");                   // insert when the scan reaches the end
    eval_scope_storage::emit_load_entry_field(
        emitter,
        "r10",
        "r9",
        EntryField::NameLength,
    );
    emitter.instruction("cmp r10, QWORD PTR [rbp - 24]");                       // compare candidate length with requested length
    emitter.instruction("jne __elephc_eval_scope_set_next");                    // different lengths cannot match
    eval_scope_storage::emit_load_entry_field(emitter, "r10", "r9", EntryField::Name);
    emitter.instruction("xor r11, r11");                                        // byte index for the equality loop
    emitter.label("__elephc_eval_scope_set_cmp");
    emitter.instruction("cmp r11, QWORD PTR [rbp - 24]");                       // have all bytes matched?
    emitter.instruction("je __elephc_eval_scope_set_update");                   // equal length and bytes select this entry
    emitter.instruction("mov al, BYTE PTR [r10 + r11]");                        // load one existing name byte
    emitter.instruction("mov r8, QWORD PTR [rbp - 16]");                        // reload requested name pointer
    emitter.instruction("cmp al, BYTE PTR [r8 + r11]");                         // compare candidate and requested name bytes
    emitter.instruction("jne __elephc_eval_scope_set_next");                    // any byte mismatch means this entry is not the target
    emitter.instruction("add r11, 1");                                          // advance to the next byte
    emitter.instruction("jmp __elephc_eval_scope_set_cmp");                     // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_set_next");
    emitter.instruction("mov QWORD PTR [rbp - 48], r9");                        // previous-next pointer now addresses current->next
    eval_scope_storage::emit_load_entry_field(emitter, "r10", "r9", EntryField::Next);
    emitter.instruction("mov QWORD PTR [rbp - 56], r10");                       // save the next candidate as current
    emitter.instruction("jmp __elephc_eval_scope_set_probe");                   // continue scanning the entry list
    eval_scope_storage::emit_replace(
        emitter, EVAL_SCOPE_FLAG_OWNED, EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY,
    );
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_OK));               // report successful scope write
    emitter.instruction("jmp __elephc_eval_scope_set_done");                    // restore the frame and return
    eval_scope_storage::emit_insert(emitter, EVAL_SCOPE_FLAG_PRESENT | EVAL_SCOPE_FLAG_DIRTY);
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_OK));               // report successful scope write
    emitter.label("__elephc_eval_scope_set_done");
    emitter.instruction("add rsp, 64");                                         // release saved input and scan slots
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer
    emitter.instruction("ret");                                                 // return the eval status in rax
    emitter.label("__elephc_eval_scope_set_fatal");
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status without mutating scope
}

/// Emits the x86_64 scope getter.
fn emit_x86_64_eval_scope_get(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_get");
    emitter.instruction("test rdi, rdi");                                       // reject null scope handles
    emitter.instruction("jz __elephc_eval_scope_get_fatal");                    // return a runtime-fatal status for null scope
    emitter.instruction("test rdx, rdx");                                       // empty names do not need a readable pointer
    emitter.instruction("jz __elephc_eval_scope_get_search");                   // skip the pointer check for empty names
    emitter.instruction("test rsi, rsi");                                       // non-empty names must provide bytes
    emitter.instruction("jz __elephc_eval_scope_get_fatal");                    // return a runtime-fatal status for invalid names
    emitter.label("__elephc_eval_scope_get_search");
    eval_scope_storage::emit_load_head(emitter, "r9", "rdi");
    emitter.label("__elephc_eval_scope_get_probe");
    emitter.instruction("test r9, r9");                                         // has the scan reached the end of the entry list?
    emitter.instruction("jz __elephc_eval_scope_get_missing");                  // missing names produce null cell and zero flags
    eval_scope_storage::emit_load_entry_field(
        emitter,
        "r10",
        "r9",
        EntryField::NameLength,
    );
    emitter.instruction("cmp r10, rdx");                                        // compare candidate length with requested length
    emitter.instruction("jne __elephc_eval_scope_get_next");                    // different lengths cannot match
    eval_scope_storage::emit_load_entry_field(emitter, "r10", "r9", EntryField::Name);
    emitter.instruction("xor r11, r11");                                        // byte index for the equality loop
    emitter.label("__elephc_eval_scope_get_cmp");
    emitter.instruction("cmp r11, rdx");                                        // have all bytes matched?
    emitter.instruction("je __elephc_eval_scope_get_found");                    // equal length and bytes select this entry
    emitter.instruction("mov al, BYTE PTR [r10 + r11]");                        // load one existing name byte
    emitter.instruction("cmp al, BYTE PTR [rsi + r11]");                        // compare candidate and requested name bytes
    emitter.instruction("jne __elephc_eval_scope_get_next");                    // any byte mismatch means this entry is not the target
    emitter.instruction("add r11, 1");                                          // advance to the next byte
    emitter.instruction("jmp __elephc_eval_scope_get_cmp");                     // continue comparing this candidate name
    emitter.label("__elephc_eval_scope_get_next");
    eval_scope_storage::emit_load_entry_field(emitter, "r9", "r9", EntryField::Next);
    emitter.instruction("jmp __elephc_eval_scope_get_probe");                   // continue scanning the scope list
    emitter.label("__elephc_eval_scope_get_found");
    emitter.instruction("test rcx, rcx");                                       // did the caller request cell output?
    emitter.instruction("jz __elephc_eval_scope_get_found_flags");              // skip cell output for null out_cell
    eval_scope_storage::emit_load_entry_field(emitter, "r10", "r9", EntryField::Cell);
    emitter.instruction("mov QWORD PTR [rcx], r10");                            // write the output Mixed cell pointer
    emitter.label("__elephc_eval_scope_get_found_flags");
    emitter.instruction("test r8, r8");                                         // did the caller request flag output?
    emitter.instruction("jz __elephc_eval_scope_get_ok");                       // skip flags output for null out_flags
    eval_scope_storage::emit_load_entry_flags(emitter, "r10d", "r9");
    emitter.instruction("mov DWORD PTR [r8], r10d");                            // write the output ABI flags
    emitter.instruction("jmp __elephc_eval_scope_get_ok");                      // finish with success
    emitter.label("__elephc_eval_scope_get_missing");
    emitter.instruction("test rcx, rcx");                                       // did the caller request cell output?
    emitter.instruction("jz __elephc_eval_scope_get_missing_flags");            // skip cell output for null out_cell
    emitter.instruction("mov QWORD PTR [rcx], 0");                              // missing variables have no cell pointer
    emitter.label("__elephc_eval_scope_get_missing_flags");
    emitter.instruction("test r8, r8");                                         // did the caller request flag output?
    emitter.instruction("jz __elephc_eval_scope_get_ok");                       // skip flags output for null out_flags
    emitter.instruction("mov DWORD PTR [r8], 0");                               // missing variables have zero ABI flags
    emitter.label("__elephc_eval_scope_get_ok");
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_OK));               // report successful scope lookup
    emitter.instruction("ret");                                                 // return the eval status in rax
    emitter.label("__elephc_eval_scope_get_fatal");
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_RUNTIME_FATAL));    // report invalid scope/name inputs
    emitter.instruction("ret");                                                 // return the fatal eval status
}

/// Emits the x86_64 scope-only native-type validator stub.
fn emit_x86_64_eval_scope_native_type(emitter: &mut Emitter) {
    label_c_global(emitter, "__elephc_eval_scope_native_type_v1");
    emitter.instruction(&format!("mov eax, {}", EVAL_STATUS_OK));               // scope-only barriers have no interpreter write to validate
    emitter.instruction("ret");                                                 // return the successful eval status
}

/// Emits a global label with platform C-symbol mangling.
fn label_c_global(emitter: &mut Emitter, name: &str) {
    let symbol = emitter.target.extern_symbol(name);
    emitter.label_global(&symbol);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// Eval-scope flag loads use the 32-bit C ABI width on every supported target.
    #[test]
    fn eval_scope_flag_load_uses_abi_width_on_all_targets() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_eval_scope_runtime(&mut emitter);
            let arch = emitter.target.arch;
            let assembly = emitter.output();
            let expected = match arch {
                Arch::AArch64 => "ldr w10, [x9, #32]",
                Arch::X86_64 => "mov r10d, DWORD PTR [r9 + 32]",
            };

            assert!(assembly.contains(expected), "{name}");
            assert!(!assembly.contains("mov r10d, QWORD PTR [r9 + 32]"), "{name}");
        }
    }
}
