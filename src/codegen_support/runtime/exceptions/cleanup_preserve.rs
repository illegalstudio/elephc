//! Purpose:
//! Completes one frame-local cleanup while preserving the exception already being unwound.
//!
//! Called from:
//! - Exceptional PHP frame callbacks through the shared typed ABI cleanup helpers.
//!
//! Key details:
//! - C arguments are a unary release entry and its owned payload.
//! - Destructor exceptions join the active chain and never escape this cleanup call.
//! - The integer result reports whether this cleanup caught a new throw.

use crate::codegen_support::{abi, emit::Emitter};

const FRAME: usize = 64;
const ENTRY: usize = 8;
const PAYLOAD: usize = 16;
const PENDING: usize = 24;
const CAUGHT: usize = 32;

/// Emits a bounded release that republishes the accumulated Throwable and returns its caught flag.
pub fn emit_cleanup_preserve_exception(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_cleanup_preserve_exception");
    abi::emit_frame_prologue(emitter, FRAME);
    for (index, offset) in [ENTRY, PAYLOAD].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), ENTRY);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), PAYLOAD);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), PENDING);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    abi::store_at_offset(emitter, result, CAUGHT);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::load_at_offset(emitter, result, CAUGHT);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Emits `__rt_release_slot_run`: releases `count` consecutive frame slots, walking DOWN from
/// `first` (the slot at the highest address), each through the unary release helper `entry`.
///
/// A slot holding `0` is skipped; any other is cleared BEFORE its release runs, exactly as the
/// per-slot epilogue sequence does, so a destructor that re-enters frame cleanup cannot release
/// it twice. Arguments: first slot address, slot count, release entry. The released value
/// travels in the integer result register, the release helpers' input.
///
/// One call replaces six instructions per slot in a function epilogue: compiler temporaries are
/// laid out next to each other, so a Symfony demo build spent about 2.5 M instructions on
/// unrolled `load / cbz / clear / bl __rt_decref_*` runs.
pub fn emit_release_slot_run(emitter: &mut Emitter) {
    const RUN_FRAME: usize = 48;
    const CURSOR: usize = 8;
    const LEFT: usize = 16;
    const RELEASE: usize = 24;
    emitter.blank();
    emitter.label_global("__rt_release_slot_run");
    abi::emit_frame_prologue(emitter, RUN_FRAME);
    for (index, offset) in [CURSOR, LEFT, RELEASE].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    emitter.label("__rt_release_slot_run_loop");
    match emitter.target.arch {
        crate::codegen_support::platform::Arch::AArch64 => {
            abi::load_at_offset(emitter, "x9", LEFT);
            emitter.instruction("cbz x9, __rt_release_slot_run_done");                 // every slot of the run is released
            emitter.instruction("sub x9, x9, #1");                                     // one slot fewer left
            abi::store_at_offset(emitter, "x9", LEFT);
            abi::load_at_offset(emitter, "x10", CURSOR);
            emitter.instruction("ldr x0, [x10]");                                      // the slot's owned value, or 0
            emitter.instruction("sub x11, x10, #8");                                   // the next slot sits one word lower
            abi::store_at_offset(emitter, "x11", CURSOR);
            emitter.instruction("cbz x0, __rt_release_slot_run_loop");                 // an empty slot owns nothing
            emitter.instruction("str xzr, [x10]");                                     // clear the owner before its release can re-enter
            abi::load_at_offset(emitter, "x9", RELEASE);
            emitter.instruction("blr x9");                                             // release through the slot type's helper
            emitter.instruction("b __rt_release_slot_run_loop");                       // next slot
        }
        crate::codegen_support::platform::Arch::X86_64 => {
            abi::load_at_offset(emitter, "rax", LEFT);
            emitter.instruction("test rax, rax");                                      // any slot left?
            emitter.instruction("jz __rt_release_slot_run_done");                      // every slot of the run is released
            emitter.instruction("dec rax");                                            // one slot fewer left
            abi::store_at_offset(emitter, "rax", LEFT);
            abi::load_at_offset(emitter, "r10", CURSOR);
            emitter.instruction("mov rax, QWORD PTR [r10]");                           // the slot's owned value, or 0
            emitter.instruction("lea r11, [r10 - 8]");                                 // the next slot sits one word lower
            abi::store_at_offset(emitter, "r11", CURSOR);
            emitter.instruction("test rax, rax");                                      // an empty slot owns nothing
            emitter.instruction("jz __rt_release_slot_run_loop");                      // skip it
            emitter.instruction("mov QWORD PTR [r10], 0");                             // clear the owner before its release can re-enter
            abi::load_at_offset(emitter, "r11", RELEASE);
            emitter.instruction("call r11");                                           // release through the slot type's helper
            emitter.instruction("jmp __rt_release_slot_run_loop");                     // next slot
        }
    }
    emitter.label("__rt_release_slot_run_done");
    abi::emit_frame_restore(emitter, RUN_FRAME);
    abi::emit_return(emitter);
}

/// The release entries a slot-description table can name, by kind index.
///
/// Kind 0 frees a string's heap pointer; the others are the typed unary release helpers. The
/// compiler writes `offset | kind << 32` words and `__rt_release_slot_table` dispatches on the
/// index, so a function's cleanup is data instead of per-slot release code.
pub const RELEASE_SLOT_KINDS: [&str; 7] = [
    "__rt_heap_free_safe",
    "__rt_decref_mixed",
    "__rt_decref_any",
    "__rt_decref_array",
    "__rt_decref_hash",
    "__rt_decref_object",
    "__rt_callable_descriptor_release",
];

/// Emits `__rt_release_slot_table`: releases the frame slots a table describes.
///
/// Argument: the address of a zero-terminated table of 8-byte words, `offset | kind << 32`, where
/// `offset` is the slot's distance below the CALLER's frame pointer. The caller's frame pointer is
/// the saved one at `[x29]`/`[rbp]` of this helper's own frame. A slot holding `0` is skipped; any
/// other is cleared BEFORE its release runs, exactly as the inline per-slot sequence does, so a
/// destructor that re-enters frame cleanup cannot release it twice. The caller has already
/// preserved its return value.
pub fn emit_release_slot_table(emitter: &mut Emitter) {
    emit_release_slot_table_variant(emitter, "__rt_release_slot_table", false);
}

/// Emits `__rt_release_slot_table_preserving`: [`emit_release_slot_table`] for an exceptional
/// frame-cleanup callback, each release going through `__rt_cleanup_preserve_exception` so a
/// destructor's throw joins the exception being unwound instead of replacing it -- what the
/// callbacks' unrolled per-slot sequence does. It must be called from the callback itself, whose
/// frame pointer is the abandoned PHP frame. The callbacks were 7.4 MB of the Symfony console.
pub fn emit_release_slot_table_preserving(emitter: &mut Emitter) {
    emit_release_slot_table_variant(emitter, "__rt_release_slot_table_preserving", true);
}

/// The walker behind both entries; `preserving` routes each release through the exception-
/// preserving cleanup call.
fn emit_release_slot_table_variant(emitter: &mut Emitter, entry: &str, preserving: bool) {
    const FRAME: usize = 32;
    const CURSOR: usize = 8;
    let loop_label = format!("{entry}_loop");
    let done_label = format!("{entry}_done");
    emitter.blank();
    emitter.label_global(entry);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), CURSOR);
    emitter.label(&loop_label);
    let is_arm = emitter.target.arch == crate::codegen_support::platform::Arch::AArch64;
    if is_arm {
        abi::load_at_offset(emitter, "x9", CURSOR);
        emitter.instruction("ldr x10, [x9], #8");                               // the next table word, cursor advanced
        abi::store_at_offset(emitter, "x9", CURSOR);
        emitter.instruction(&format!("cbz x10, {done_label}"));                 // the table ends with a zero word
        emitter.instruction("ldr x13, [x29]");                                  // the caller's frame pointer
        emitter.instruction("and x11, x10, #0xffffffff");                       // the slot's distance below it
        emitter.instruction("sub x13, x13, x11");                               // the slot address
        emitter.instruction("lsr x12, x10, #32");                               // the release kind
        emitter.instruction("ldr x0, [x13]");                                   // the slot's owned value, or 0
        emitter.instruction(&format!("cbz x0, {loop_label}"));                  // an empty slot owns nothing
        emitter.instruction("str xzr, [x13]");                                  // clear the owner before its release can re-enter
        for (index, helper) in RELEASE_SLOT_KINDS.iter().enumerate() {
            let last = index + 1 == RELEASE_SLOT_KINDS.len();
            let next = format!("{entry}_kind_{}", index + 1);
            if !last {
                emitter.instruction(&format!("cmp x12, #{}", index));           // is this the kind?
                emitter.instruction(&format!("b.ne {}", next));
            }
            if preserving {
                emitter.instruction("mov x1, x0");                              // the owned value is the release payload
                abi::emit_symbol_address(emitter, "x0", helper);                // the slot type's release entry
                emitter.instruction("bl __rt_cleanup_preserve_exception");      // release, keeping the unwound exception
            } else {
                emitter.instruction(&format!("bl {}", helper));                 // release through the slot type's helper
            }
            emitter.instruction(&format!("b {loop_label}"));                    // next slot
            if !last {
                emitter.label(&next);
            }
        }
    } else {
        abi::load_at_offset(emitter, "r9", CURSOR);
        emitter.instruction("mov r10, QWORD PTR [r9]");                         // the next table word
        emitter.instruction("add r9, 8");                                       // advance the cursor
        abi::store_at_offset(emitter, "r9", CURSOR);
        emitter.instruction("test r10, r10");                                   // the table ends with a zero word
        emitter.instruction(&format!("jz {done_label}"));
        emitter.instruction("mov r11, QWORD PTR [rbp]");                        // the caller's frame pointer
        emitter.instruction("mov rcx, r10");
        emitter.instruction("mov eax, ecx");                                    // the slot's distance below it
        emitter.instruction("sub r11, rax");                                    // the slot address
        emitter.instruction("shr r10, 32");                                     // the release kind
        emitter.instruction("mov rax, QWORD PTR [r11]");                        // the slot's owned value, or 0
        emitter.instruction("test rax, rax");                                   // an empty slot owns nothing
        emitter.instruction(&format!("jz {loop_label}"));
        emitter.instruction("mov QWORD PTR [r11], 0");                          // clear the owner before its release can re-enter
        for (index, helper) in RELEASE_SLOT_KINDS.iter().enumerate() {
            let last = index + 1 == RELEASE_SLOT_KINDS.len();
            let next = format!("{entry}_kind_{}", index + 1);
            if !last {
                emitter.instruction(&format!("cmp r10, {}", index));            // is this the kind?
                emitter.instruction(&format!("jne {}", next));
            }
            if preserving {
                emitter.instruction("mov rsi, rax");                            // the owned value is the release payload
                abi::emit_symbol_address(emitter, "rdi", helper);               // the slot type's release entry
                emitter.instruction("call __rt_cleanup_preserve_exception");    // release, keeping the unwound exception
            } else {
                emitter.instruction(&format!("call {}", helper));               // release through the slot type's helper
            }
            emitter.instruction(&format!("jmp {loop_label}"));                  // next slot
            if !last {
                emitter.label(&next);
            }
        }
    }
    emitter.label(&done_label);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Emits `__rt_retire_slot` and `__rt_retire_mixed_slot`: clear one frame slot and release the
/// value it owned, bounding any destructor throw. Both return the newly-caught flag, so the caller
/// is `call; branch-if-nonzero __rt_throw_current`.
///
/// `__rt_retire_slot(slot, entry)` releases through the unary helper `entry`.
/// `__rt_retire_mixed_slot(slot)` releases a boxed `Mixed`: a null container or a scalar payload
/// (tags 0-3 and 8) owns nothing that can run PHP code, so it is released directly, without the
/// exception-preserving boundary. These replace an inline sequence of up to twenty instructions at
/// every deferred `release_local_slot` site (2.6 M instructions in the Symfony demo).
pub fn emit_retire_slot(emitter: &mut Emitter) {
    const RETIRE_FRAME: usize = 32;
    const SLOT: usize = 8;
    const ENTRY_WORD: usize = 16;
    let arch = emitter.target.arch;
    let result = abi::int_result_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    for mixed in [false, true] {
        let name = if mixed { "__rt_retire_mixed_slot" } else { "__rt_retire_slot" };
        emitter.blank();
        emitter.label_global(name);
        abi::emit_frame_prologue(emitter, RETIRE_FRAME);
        abi::store_at_offset(emitter, arg0, SLOT);
        if !mixed {
            abi::store_at_offset(emitter, arg1, ENTRY_WORD);
        }
        let none = format!("{name}_none");
        let direct = format!("{name}_direct");
        match arch {
            crate::codegen_support::platform::Arch::AArch64 => {
                emitter.instruction("ldr x0, [x0]");                                   // the slot's owned value, or 0
                emitter.instruction(&format!("cbz x0, {none}"));                       // an empty slot owns nothing
                abi::load_at_offset(emitter, "x9", SLOT);
                emitter.instruction("str xzr, [x9]");                                  // clear the owner before its release can re-enter
                if mixed {
                    abi::emit_load_int_immediate(emitter, "x10", crate::codegen_support::sentinels::NULL_SENTINEL);
                    emitter.instruction("cmp x0, x10");                                // the in-band null container owns nothing
                    emitter.instruction(&format!("b.eq {none}"));
                    emitter.instruction("ldr x10, [x0]");                              // the boxed payload's tag
                    emitter.instruction("cmp x10, #3");                                // int, string, float and bool payloads
                    emitter.instruction(&format!("b.ls {direct}"));                    // cannot run a destructor
                    emitter.instruction("cmp x10, #8");                                // nor can the canonical null
                    emitter.instruction(&format!("b.eq {direct}"));
                    emitter.instruction("mov x1, x0");                                 // payload for the bounded release
                    abi::emit_symbol_address(emitter, "x0", "__rt_decref_mixed");
                } else {
                    emitter.instruction("mov x1, x0");                                 // payload for the bounded release
                    abi::load_at_offset(emitter, "x0", ENTRY_WORD);
                }
            }
            crate::codegen_support::platform::Arch::X86_64 => {
                emitter.instruction("mov rax, QWORD PTR [rdi]");                       // the slot's owned value, or 0
                emitter.instruction("test rax, rax");                                  // an empty slot owns nothing
                emitter.instruction(&format!("jz {none}"));
                emitter.instruction("mov QWORD PTR [rdi], 0");                         // clear the owner before its release can re-enter
                if mixed {
                    abi::emit_load_int_immediate(emitter, "r10", crate::codegen_support::sentinels::NULL_SENTINEL);
                    emitter.instruction("cmp rax, r10");                               // the in-band null container owns nothing
                    emitter.instruction(&format!("je {none}"));
                    emitter.instruction("mov r10, QWORD PTR [rax]");                   // the boxed payload's tag
                    emitter.instruction("cmp r10, 3");                                 // int, string, float and bool payloads
                    emitter.instruction(&format!("jbe {direct}"));                     // cannot run a destructor
                    emitter.instruction("cmp r10, 8");                                 // nor can the canonical null
                    emitter.instruction(&format!("je {direct}"));
                    emitter.instruction("mov rsi, rax");                               // payload for the bounded release
                    abi::emit_symbol_address(emitter, "rdi", "__rt_decref_mixed");
                } else {
                    emitter.instruction("mov rsi, rax");                               // payload for the bounded release
                    abi::load_at_offset(emitter, "rdi", ENTRY_WORD);
                }
            }
        }
        abi::emit_call_label(emitter, "__rt_cleanup_preserve_exception");
        abi::emit_frame_restore(emitter, RETIRE_FRAME);
        abi::emit_return(emitter);
        if mixed {
            emitter.label(&direct);
            abi::emit_call_label(emitter, "__rt_decref_mixed");
        }
        emitter.label(&none);
        abi::emit_load_int_immediate(emitter, result, 0);
        abi::emit_frame_restore(emitter, RETIRE_FRAME);
        abi::emit_return(emitter);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI republishes the pending chain and returns the independent newly-caught flag.
    #[test]
    fn frame_cleanup_preserves_the_pending_chain_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_cleanup_preserve_exception(&mut emitter);
            let arch = emitter.target.arch;
            let asm = emitter.output();
            let invoke = asm.find("__rt_cleanup_invoke").unwrap();
            let publish = asm.rfind("_exc_value").unwrap();
            let (caught_store, caught_reload) = match arch {
                crate::codegen_support::platform::Arch::AArch64 => (
                    "stur x0, [x29, #-32]",
                    "ldur x0, [x29, #-32]",
                ),
                crate::codegen_support::platform::Arch::X86_64 => {
                    (
                        "mov QWORD PTR [rbp - 32], rax",
                        "mov rax, QWORD PTR [rbp - 32]",
                    )
                }
            };
            let save = asm.find(caught_store).unwrap();
            let reload = asm.rfind(caught_reload).unwrap();
            assert!(asm.find("_exc_value").unwrap() < invoke, "{name}");
            assert!(invoke < save && save < publish && publish < reload, "{name}");
            assert!(!asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
