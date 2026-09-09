//! Purpose:
//! Maps packed or associative boxed PHP arrays through Mixed-input callbacks.
//!
//! Called from:
//! - The typed ArrayMap backend for PHP array parameters with runtime-selected storage.
//!
//! Key details:
//! - A fresh source box owns the payload snapshot so callback writes detach through COW.
//! - Keys retain insertion order and mapped values belong to a fresh Mixed-value hash.
//! - A native exception boundary releases the current input and partial result on throw.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::sentinels::emit_branch_if_null_container;
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

const FRAME_SIZE: usize = TRY_HANDLER_SLOT_SIZE + 160;
const HANDLER: usize = FRAME_SIZE - 16;
const CALLBACK: usize = 8;
const SOURCE: usize = 16;
const PAYLOAD: usize = 24;
const LAYOUT: usize = 32;
const ENV: usize = 40;
const RESULT_KIND: usize = 48;
const RESULT_TAG: usize = 56;
const RESULT: usize = 64;
const CURSOR: usize = 72;
const KEY_LO: usize = 80;
const KEY_HI: usize = 88;
const INPUT: usize = 96;
const OUTPUT: usize = 104;
const PREVIOUS: usize = 112;
const PENDING: usize = 120;
const RAW_RETURN: usize = 128;

/// Borrows callback, source box and environment in ABI args 0..2, with result kind/tag in args 3..4.
/// The callback borrows a Mixed cell and returns owned Mixed, int/bool, or a string pair.
/// Returns a fresh raw Hash<Mixed>, zero for invalid input, or propagates a callback exception.
pub fn emit_array_map_boxed(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global("__rt_array_map_boxed");
    // -- preserve the callback contract before validating and retaining the source payload --
    abi::emit_frame_prologue(emitter, FRAME_SIZE);
    for (index, offset) in [CALLBACK, SOURCE, ENV, RESULT_KIND, RESULT_TAG].into_iter().enumerate() {
        abi::store_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    for offset in [INPUT, OUTPUT, PENDING] { clear_slot(emitter, offset); }
    abi::load_at_offset(emitter, result, SOURCE);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    validate_source_and_acquire_snapshot(emitter);
    abi::store_at_offset(emitter, result, SOURCE);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 0), 8);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 1), 7);
    abi::emit_call_label(emitter, "__rt_hash_new");
    abi::store_at_offset(emitter, result, RESULT);
    initialize_cursor(emitter);
    install_boundary(emitter);

    // -- snapshot traversal keeps source keys alive across arbitrary callback mutations --
    emitter.label("__rt_array_map_boxed_loop");
    match emitter.target.arch {
        Arch::AArch64 => read_aarch64_entry(emitter),
        Arch::X86_64 => read_x86_64_entry(emitter),
    }
    emitter.label("__rt_array_map_boxed_value");
    acquire_input_cell(emitter);
    abi::store_at_offset(emitter, result, INPUT);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), INPUT);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), ENV);
    let callback = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, callback, CALLBACK);
    abi::emit_call_reg(emitter, callback);
    box_callback_result(emitter);
    abi::store_at_offset(emitter, result, OUTPUT);
    release_slot(emitter, INPUT, "__rt_decref_mixed");
    insert_mapped_value(emitter);
    abi::emit_jump(emitter, "__rt_array_map_boxed_loop");

    // -- release every helper-owned value before returning or escaping to the caller's catch --
    emitter.label("__rt_array_map_boxed_cleanup");
    release_slot(emitter, INPUT, "__rt_decref_mixed");
    release_slot(emitter, OUTPUT, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_map_boxed_release_source");
    release_slot(emitter, RESULT, "__rt_decref_hash");
    emitter.label("__rt_array_map_boxed_release_source");
    release_slot(emitter, SOURCE, "__rt_decref_mixed");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, "__rt_array_map_boxed_return");
    abi::load_at_offset(emitter, older_exception_reg(emitter), PREVIOUS);
    clear_slot(emitter, PREVIOUS);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::emit_frame_restore(emitter, FRAME_SIZE);
    abi::emit_jump(emitter, "__rt_throw_current");

    emitter.label("__rt_array_map_boxed_return");
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::load_at_offset(emitter, result, RESULT);
    abi::emit_frame_restore(emitter, FRAME_SIZE);
    abi::emit_return(emitter);

    emitter.label("__rt_array_map_boxed_caught");
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, older_exception_reg(emitter), PENDING);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::emit_jump(emitter, "__rt_array_map_boxed_cleanup");

    emitter.label("__rt_array_map_boxed_invalid");
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, FRAME_SIZE);
    abi::emit_return(emitter);
}

/// Validates the unboxed source and acquires a new box, not another reference to its mutable box.
fn validate_source_and_acquire_snapshot(emitter: &mut Emitter) {
    let invalid = "__rt_array_map_boxed_invalid";
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub x9, x0, #4");                              // map array/hash tags onto zero and one
            emitter.instruction("cmp x9, #1");                                  // reject all scalar and object payloads
            emitter.instruction(&format!("b.hi {invalid}"));                    // return no result before acquiring any owner
            emit_branch_if_null_container(emitter, "x1", "x9", invalid);
            abi::store_at_offset(emitter, "x0", LAYOUT);
            abi::store_at_offset(emitter, "x1", PAYLOAD);
            emitter.instruction("mov x2, #0");                                  // array payloads have no high word
        }
        Arch::X86_64 => {
            emitter.instruction("lea r10, [rax - 4]");                          // map the two supported layout tags onto zero and one
            emitter.instruction("cmp r10, 1");                                  // validate before allocating a snapshot or result
            emitter.instruction(&format!("ja {invalid}"));                      // let the caller report a catchable argument TypeError
            emit_branch_if_null_container(emitter, "rdi", "r10", invalid);
            abi::store_at_offset(emitter, "rax", LAYOUT);
            abi::store_at_offset(emitter, "rdi", PAYLOAD);
            emitter.instruction("xor esi, esi");                                // array payloads have no high word
        }
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
}

/// Starts at packed index zero or at the hash's insertion-order head without changing its cursor.
fn initialize_cursor(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    let payload = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, result, LAYOUT);
    abi::load_at_offset(emitter, payload, PAYLOAD);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #4");                                  // packed sources use a numeric slot cursor
            emitter.instruction("mov x0, #0");                                  // begin with index zero for a packed source
            emitter.instruction("b.eq __rt_array_map_boxed_cursor");            // keep that index instead of reading a hash header
            emitter.instruction("ldr x0, [x10, #24]");                          // hashes start at their first live bucket
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 4");                                  // discriminate the source header layout
            emitter.instruction("mov eax, 0");                                  // seed the packed cursor without changing comparison flags
            emitter.instruction("je __rt_array_map_boxed_cursor");              // preserve zero for packed sources
            emitter.instruction("mov rax, QWORD PTR [r10 + 24]");               // begin at the insertion-order head for a hash
        }
    }
    emitter.label("__rt_array_map_boxed_cursor");
    abi::store_at_offset(emitter, result, CURSOR);
}

/// Reads one ARM64 source entry into the Mixed constructor's tag/payload ABI, preserving its key.
fn read_aarch64_entry(emitter: &mut Emitter) {
    abi::load_at_offset(emitter, "x11", PAYLOAD);
    abi::load_at_offset(emitter, "x10", CURSOR);
    abi::load_at_offset(emitter, "x9", LAYOUT);
    emitter.instruction("cmp x9, #4");                                          // choose packed indexing or live hash traversal
    emitter.instruction("b.ne __rt_array_map_boxed_hash");                      // a hash cursor names a bucket instead of a dense slot
    emitter.instruction("ldr x9, [x11]");                                       // packed length bounds every read
    emitter.instruction("cmp x10, x9");                                         // stop before reading the last slot's successor
    emitter.instruction("b.hs __rt_array_map_boxed_cleanup");                   // return after finishing all packed entries
    abi::store_at_offset(emitter, "x10", KEY_LO);
    emitter.instruction("add x9, x10, #1");                                     // advance before the callback can clobber registers
    abi::store_at_offset(emitter, "x9", CURSOR);
    emitter.instruction("mov x9, #-1");                                         // integer keys carry a negative high word
    abi::store_at_offset(emitter, "x9", KEY_HI);
    emitter.instruction("ldr x0, [x11, #-8]");                                  // inspect the actual packed element representation
    emitter.instruction("ubfx x0, x0, #8, #7");                                 // isolate the value tag from heap flags
    emitter.instruction("ldr x12, [x11, #16]");                                 // read the physical slot width
    emitter.instruction("madd x11, x10, x12, x11");                             // address the current element before its header offset
    emitter.instruction("ldr x1, [x11, #24]");                                  // borrow its low payload word
    emitter.instruction("mov x2, #0");                                          // scalar and pointer slots have no high word
    emitter.instruction("cmp x12, #16");                                        // paired strings need their length word too
    emitter.instruction("b.ne __rt_array_map_boxed_value");                     // do not overread an eight-byte final slot
    emitter.instruction("ldr x2, [x11, #32]");                                  // borrow the paired high word
    emitter.instruction("b __rt_array_map_boxed_value");                        // box or retain the argument before invocation
    emitter.label("__rt_array_map_boxed_hash");
    emitter.instruction("tbnz x10, #63, __rt_array_map_boxed_cleanup");         // minus one ends the live insertion-order chain
    emitter.instruction("add x11, x11, x10, lsl #6");                           // each source hash bucket has sixty-four bytes
    emitter.instruction("add x11, x11, #40");                                   // skip the fixed hash header
    emitter.instruction("ldr x9, [x11, #56]");                                  // follow the next live bucket rather than a tombstone
    abi::store_at_offset(emitter, "x9", CURSOR);
    emitter.instruction("ldp x9, x10, [x11, #8]");                              // borrow the source key while the snapshot owns its bytes
    abi::store_at_offset(emitter, "x9", KEY_LO);
    abi::store_at_offset(emitter, "x10", KEY_HI);
    emitter.instruction("ldr x0, [x11, #40]");                                  // per-entry value tags govern heterogeneous hashes
    emitter.instruction("ldp x1, x2, [x11, #24]");                              // borrow both value words
}

/// Reads an x86_64 source entry into rdx/rdi/rsi before the input boxer moves its tag to rax.
fn read_x86_64_entry(emitter: &mut Emitter) {
    abi::load_at_offset(emitter, "r11", PAYLOAD);
    abi::load_at_offset(emitter, "r10", CURSOR);
    abi::load_at_offset(emitter, "rax", LAYOUT);
    emitter.instruction("cmp rax, 4");                                          // select packed or associative traversal
    emitter.instruction("jne __rt_array_map_boxed_hash");                       // hash cursors identify buckets
    emitter.instruction("cmp r10, QWORD PTR [r11]");                            // bound packed reads by the saved payload's length
    emitter.instruction("jae __rt_array_map_boxed_cleanup");                    // finish after the last live packed slot
    abi::store_at_offset(emitter, "r10", KEY_LO);
    emitter.instruction("lea r9, [r10 + 1]");                                   // advance before callback invocation
    abi::store_at_offset(emitter, "r9", CURSOR);
    emitter.instruction("mov r9, -1");                                          // mark the preserved key as an integer
    abi::store_at_offset(emitter, "r9", KEY_HI);
    emitter.instruction("mov rdx, QWORD PTR [r11 - 8]");                        // inspect the packed element metadata
    emitter.instruction("shr rdx, 8");                                          // move the element tag to the low bits
    emitter.instruction("and rdx, 127");                                        // discard heap flags outside the value tag
    emitter.instruction("mov rcx, QWORD PTR [r11 + 16]");                       // read the actual eight- or sixteen-byte stride
    emitter.instruction("imul r10, rcx");                                       // convert the logical index to a payload offset
    emitter.instruction("add r11, r10");                                        // locate the current packed entry
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // borrow the low value word
    emitter.instruction("xor esi, esi");                                        // one-word slots have no high value word
    emitter.instruction("cmp rcx, 16");                                         // only paired slots contain a second word
    emitter.instruction("jne __rt_array_map_boxed_value");                      // avoid reading beyond single-word storage
    emitter.instruction("mov rsi, QWORD PTR [r11 + 32]");                       // retain the string length or paired high word
    emitter.instruction("jmp __rt_array_map_boxed_value");                      // normalize the callback's boxed input
    emitter.label("__rt_array_map_boxed_hash");
    emitter.instruction("test r10, r10");                                       // inspect the insertion-order end sentinel
    emitter.instruction("js __rt_array_map_boxed_cleanup");                     // no more live source buckets remain
    emitter.instruction("shl r10, 6");                                          // each hash bucket occupies sixty-four bytes
    emitter.instruction("lea r11, [r11 + r10 + 40]");                           // skip the fixed header and preceding buckets
    emitter.instruction("mov r9, QWORD PTR [r11 + 56]");                        // follow the next live insertion-order link
    abi::store_at_offset(emitter, "r9", CURSOR);
    emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                         // preserve the key's low word
    abi::store_at_offset(emitter, "r9", KEY_LO);
    emitter.instruction("mov r9, QWORD PTR [r11 + 16]");                        // preserve its string length or integer marker
    abi::store_at_offset(emitter, "r9", KEY_HI);
    emitter.instruction("mov rdx, QWORD PTR [r11 + 40]");                       // heterogeneous entries carry their own value tag
    emitter.instruction("mov rdi, QWORD PTR [r11 + 24]");                       // borrow the low payload word
    emitter.instruction("mov rsi, QWORD PTR [r11 + 32]");                       // borrow the high payload word
}

/// Acquires an existing Mixed input cell or boxes a raw entry without consuming the source owner.
fn acquire_input_cell(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #7");                                  // existing boxed cells need only another owner
            emitter.instruction("b.ne __rt_array_map_boxed_box_input");         // raw values must be boxed using their real tag
            emitter.instruction("mov x0, x1");                                  // retain the source's Mixed pointer
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rdx, 7");                                  // distinguish boxed cells from raw values
            emitter.instruction("mov rax, rdx");                                // Mixed construction receives the element tag in rax
            emitter.instruction("jne __rt_array_map_boxed_box_input");          // construct a new box for raw entries
            emitter.instruction("mov rax, rdi");                                // the retain helper receives the Mixed pointer in rax
        }
    }
    abi::emit_call_label(emitter, "__rt_incref");
    abi::emit_jump(emitter, "__rt_array_map_boxed_input_ready");
    emitter.label("__rt_array_map_boxed_box_input");
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    emitter.label("__rt_array_map_boxed_input_ready");
}

/// Normalizes descriptor Mixed results or the existing direct scalar/string callback return ABI.
fn box_callback_result(emitter: &mut Emitter) {
    let ready = "__rt_array_map_boxed_output_ready";
    let result = abi::int_result_reg(emitter);
    abi::store_at_offset(emitter, result, RAW_RETURN);
    let tag = abi::secondary_scratch_reg(emitter);
    abi::load_at_offset(emitter, tag, RESULT_TAG);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x10, #7");                                 // descriptor callbacks already return an owned Mixed cell
            emitter.instruction(&format!("b.eq {ready}"));                      // transfer that owner without nesting another box
            emitter.instruction("cmp x10, #1");                                 // string callbacks return their payload in x1/x2
            emitter.instruction("b.eq __rt_array_map_boxed_box_output");        // preserve the existing string pair
            emitter.instruction("mov x1, x0");                                  // move an integer or boolean into the box payload
            emitter.instruction("mov x2, #0");                                  // scalar results have no high word
            emitter.label("__rt_array_map_boxed_box_output");
            emitter.instruction("mov x0, x10");                                 // use the direct callback's declared scalar or string tag
        }
        Arch::X86_64 => {
            emitter.instruction("cmp r10, 7");                                  // descriptor results already own their returned box
            emitter.instruction(&format!("je {ready}"));                        // preserve that exact Mixed result owner
            emitter.instruction("mov rdi, rax");                                // both scalars and strings use rax for their low word
            emitter.instruction("xor esi, esi");                                // scalar results have no high word
            emitter.instruction("cmp r10, 1");                                  // strings also carry their length in rdx
            emitter.instruction("jne __rt_array_map_boxed_box_output");         // skip string-only return registers for scalar callbacks
            emitter.instruction("mov rsi, rdx");                                // preserve the returned string length
            emitter.label("__rt_array_map_boxed_box_output");
            emitter.instruction("mov rax, r10");                                // pass the scalar or string tag to Mixed construction
        }
    }
    let string_pointer = abi::string_result_regs(emitter).0;
    if emitter.target.arch == Arch::AArch64 {
        abi::store_at_offset(emitter, string_pointer, RAW_RETURN);
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, OUTPUT);
    abi::load_at_offset(emitter, result, RESULT_KIND);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("cmp x0, #2");                                  // owned direct strings must release their previous payload copy
            emitter.instruction("b.ne __rt_array_map_boxed_reload_output");     // borrowed strings and scalar values have no owner to release
        }
        Arch::X86_64 => {
            emitter.instruction("cmp rax, 2");                                  // kind two denotes an owned direct string pair
            emitter.instruction("jne __rt_array_map_boxed_reload_output");      // borrowed strings and scalar results need no release
        }
    }
    abi::load_at_offset(emitter, result, RAW_RETURN);
    abi::emit_call_label(emitter, "__rt_heap_free_safe");
    emitter.label("__rt_array_map_boxed_reload_output");
    abi::load_at_offset(emitter, result, OUTPUT);
    emitter.label(ready);
}

/// Transfers the mapped owner to its original source key, persisting string-key bytes in the hash.
fn insert_mapped_value(emitter: &mut Emitter) {
    for (index, offset) in [RESULT, KEY_LO, KEY_HI, OUTPUT].into_iter().enumerate() {
        abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, index), offset);
    }
    clear_slot(emitter, OUTPUT);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 4), 0);
    abi::emit_load_int_immediate(emitter, abi::int_arg_reg_name(emitter.target, 5), 7);
    abi::emit_call_label(emitter, "__rt_hash_set");
    abi::store_at_offset(emitter, abi::int_result_reg(emitter), RESULT);
}

/// Saves the caller's state and installs a local handler while all map-owned slots remain reachable.
fn install_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_value", PREVIOUS),
        ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, offset);
    }
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), HANDLER - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                     // preserve map-owned inputs and partial results when a callback throws
    abi::emit_branch_if_int_result_nonzero(emitter, "__rt_array_map_boxed_caught");
}

/// Restores the caller's handler chain and diagnostic depth after successful or failed mapping.
fn restore_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, offset) in [
        ("_exc_handler_top", HANDLER),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::load_at_offset(emitter, result, offset);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
}

/// Clears a helper-owned slot without destroying a loaded value or outgoing hash arguments.
fn clear_slot(emitter: &mut Emitter, offset: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, offset);
}

/// Releases an owner only after clearing its slot, making subsequent cleanup safe to resume.
fn release_slot(emitter: &mut Emitter, offset: usize, helper: &str) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), offset);
    clear_slot(emitter, offset);
    abi::emit_call_label(emitter, helper);
}

/// Returns the previous-owner argument register of the shared raw-Throwable chain helper.
fn older_exception_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdi",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Every ABI retains a payload snapshot and installs cleanup before invoking a boxed callback.
    #[test]
    fn boxed_array_map_snapshots_and_exception_boundaries_cover_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit_array_map_boxed(&mut emitter);
            let asm = emitter.output();
            let snapshot = asm.find("__rt_mixed_from_value").unwrap();
            let boundary = asm.find(&target.extern_symbol("setjmp")).unwrap();
            let call = asm.find(if target.arch == Arch::AArch64 { "blr x10" } else { "call r10" }).unwrap();
            assert!(snapshot < boundary && boundary < call, "{name}");
            assert!(asm.contains("__rt_array_map_boxed_hash:"), "{name}");
            assert!(asm.contains("__rt_array_map_boxed_caught:"), "{name}");
            assert!(asm.contains("__rt_exception_chain"), "{name}");
            assert!(asm.contains("__rt_decref_hash"), "{name}");
            assert!(asm.contains("__rt_throw_current"), "{name}");
        }
    }
}
