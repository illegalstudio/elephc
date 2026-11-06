//! Purpose:
//! Avoids a protected callback boundary for destruction of boxed PHP scalars.
//!
//! Called from:
//! - `super::emit_cleanup_invoke()` before its ordinary exception-handler setup.
//!
//! Key details:
//! - Only the exact native decref dispatcher and a live managed Mixed cell qualify.
//! - Int, float, bool and null own no child that can execute PHP cleanup hooks.
//! - The original dispatcher still enforces refcounts and GC sweep suppression.
//! - The validated callback pointer is invoked unchanged, including in PIC modules.
//! - Pending exceptions and the caller's pending-owner slot remain untouched.
//! - Eval retirement invalidates weak tokens; mbstring retirement releases Rust identities only.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::value_boxing::runtime_value_tag;
use crate::types::PhpType;

pub(super) const PROTECTED: &str = "__rt_cleanup_invoke_protected";
pub(super) const DIRECT: &str = "__rt_cleanup_invoke_direct";

/// Emits a conservative scalar gate, then resumes the existing protected path on rejection.
pub(super) fn emit_fast_path(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => emit_aarch64_gate(emitter),
        Arch::X86_64 => emit_x86_gate(emitter),
    }

    // -- proved values cannot call PHP, so retain the original exception state --
    emitter.label(DIRECT);
    abi::emit_frame_prologue(emitter, 16);
    let result = abi::int_result_reg(emitter);
    let callback = if emitter.target.arch == Arch::AArch64 { "x9" } else { "r11" };
    abi::emit_reg_move(emitter, callback, abi::int_arg_reg_name(emitter.target, 0));
    let payload = abi::int_arg_reg_name(emitter.target, 1);
    abi::emit_reg_move(emitter, result, payload);
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rdi, rax");                                    // preserve the dispatcher's native and C unary conventions
    }
    abi::emit_call_reg(emitter, callback);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, 16);
    abi::emit_return(emitter);
    emitter.label(PROTECTED);
}

/// Validates the AAPCS64 callback and payload without changing any incoming argument.
fn emit_aarch64_gate(emitter: &mut Emitter) {
    // -- authenticate the only release entry with a proven scalar path --
    abi::emit_symbol_address(emitter, "x9", "__rt_decref_any");
    emitter.instruction("cmp x0, x9");                                          // arbitrary cleanup callbacks retain their exception boundary
    emitter.instruction(&format!("b.ne {}", super::empty_array::CANDIDATE));    // other callbacks need their own proof before any header read

    // -- require a live managed Mixed cell before reading its runtime tag --
    abi::emit_symbol_address(emitter, "x9", "_heap_buf");
    emitter.instruction("add x10, x9, #16");                                    // the first payload begins after the uniform heap header
    emitter.instruction("cmp x1, x10");                                         // null, literals and sentinels cannot be scalar heap cells
    emitter.instruction(&format!("b.lo {PROTECTED}"));                          // preserve the original handling of foreign payloads
    abi::emit_symbol_address(emitter, "x10", "_heap_off");
    emitter.instruction("ldr x10, [x10]");                                      // preserve the heap base in x9 while loading its bump offset
    emitter.instruction("add x10, x9, x10");                                    // derive the current managed heap end
    emitter.instruction("cmp x1, x10");                                         // reject addresses outside allocated heap storage
    emitter.instruction(&format!("b.hs {PROTECTED}"));                          // leave invalid payload handling to the protected dispatcher
    emitter.instruction("ldr w10, [x1, #-12]");                                 // inspect real ownership without changing the refcount
    emitter.instruction(&format!("cbz w10, {PROTECTED}"));                      // freed or doomed cells retain the ordinary dispatch path
    emitter.instruction("ldr w10, [x1, #-16]");                                 // confirm storage can contain the complete Mixed cell
    emitter.instruction("cmp w10, #24");                                        // current boxed cells contain three words
    emitter.instruction(&format!("b.lo {PROTECTED}"));                          // malformed or older short cells do not use this optimization
    emitter.instruction("ldr x10, [x1, #-8]");                                  // load storage kind while retaining all collector flags
    emitter.instruction("and x10, x10, #0xff");                                 // collector metadata cannot change the underlying heap kind
    emitter.instruction("cmp x10, #5");                                         // only a Mixed cell has the runtime tag inspected below
    emitter.instruction(&format!("b.ne {PROTECTED}"));                          // containers, objects, strings and reference cells stay protected
    emitter.instruction("ldr x10, [x1]");                                       // borrow the canonical runtime value tag
    for ty in [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::Void] {
        emitter.instruction(&format!("cmp x10, #{}", runtime_value_tag(&ty)));  // admit only PHP scalar tags with no child ownership
        emitter.instruction(&format!("b.eq {DIRECT}"));                         // the original decref dispatcher can release this cell without PHP callbacks
    }
    emitter.instruction(&format!("b {PROTECTED}"));                             // unknown and heap-backed tags keep full exception protection
}

/// Validates System V inputs while preserving the callback, payload and pending-owner address.
fn emit_x86_gate(emitter: &mut Emitter) {
    // -- authenticate the exact native release callback --
    abi::emit_symbol_address(emitter, "r9", "__rt_decref_any");
    emitter.instruction("cmp rdi, r9");                                         // arbitrary callbacks cannot inherit the scalar release proof
    emitter.instruction(&format!("jne {}", super::empty_array::CANDIDATE));     // other callbacks need their own proof before any header read

    // -- validate the payload header without changing the incoming arguments --
    abi::emit_symbol_address(emitter, "r9", "_heap_buf");
    emitter.instruction("lea r10, [r9 + 16]");                                  // locate the first valid managed payload
    emitter.instruction("cmp rsi, r10");                                        // reject null, static and sentinel payloads
    emitter.instruction(&format!("jb {PROTECTED}"));                            // leave foreign values on the ordinary protected path
    abi::emit_load_symbol_to_reg(emitter, "r10", "_heap_off", 0);
    emitter.instruction("add r10, r9");                                         // derive the current heap end from its bump offset
    emitter.instruction("cmp rsi, r10");                                        // require a payload inside allocated storage
    emitter.instruction(&format!("jae {PROTECTED}"));                           // out-of-range values cannot qualify
    emitter.instruction("cmp DWORD PTR [rsi - 12], 0");                         // inspect the actual owner count
    emitter.instruction(&format!("je {PROTECTED}"));                            // freed and collector-doomed cells retain the original dispatcher
    emitter.instruction("cmp DWORD PTR [rsi - 16], 24");                        // require the complete three-word Mixed storage
    emitter.instruction(&format!("jb {PROTECTED}"));                            // short or invalid storage cannot be interpreted as a current cell
    emitter.instruction("mov r10, QWORD PTR [rsi - 8]");                        // preserve storage kind and collector flags while authenticating the marker
    emitter.instruction("mov r11, r10");                                        // keep the complete kind word for the low-byte check
    emitter.instruction("shr r11, 32");                                         // isolate the x86 managed-heap marker
    emitter.instruction(&format!("cmp r11, {}", crate::codegen_support::sentinels::X86_64_HEAP_MAGIC_HI32)); // reject foreign headers inside the managed address range
    emitter.instruction(&format!("jne {PROTECTED}"));                           // only stamped native allocations can qualify
    emitter.instruction("and r10d, 0xff");                                      // ignore transient marks and cached incoming counts
    emitter.instruction("cmp r10d, 5");                                         // only Mixed storage begins with this value-tag layout
    emitter.instruction(&format!("jne {PROTECTED}"));                           // all other heap kinds remain protected
    emitter.instruction("mov r10, QWORD PTR [rsi]");                            // borrow the current runtime tag before releasing the cell
    for ty in [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::Void] {
        emitter.instruction(&format!("cmp r10, {}", runtime_value_tag(&ty)));   // admit only scalars that cannot own a throwing child
        emitter.instruction(&format!("je {DIRECT}"));                           // use the same decref semantics without a fresh jump buffer
    }
    emitter.instruction(&format!("jmp {PROTECTED}"));                           // strings, resources, containers and unknown tags stay protected
}
