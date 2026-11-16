//! Purpose:
//! Emits the x86_64 bounded JSON integer-tree probe, allocator, and writer.
//! Produces one exact transient string without using the shared concat scratch buffer.
//!
//! Called from:
//! - `super::emit_json_integer_tree()` and `super::emit_json_integer_tree_entry()`.
//!
//! Key details:
//! - The helper uses System V call alignment and the runtime's full x86_64 heap marker.
//! - Accepted trees are immutable between the pure probe and the no-callback writer.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::sentinels::{
    emit_branch_if_null_container, x86_64_heap_kind_word, CONCAT_TEMP_HEAP_KIND,
};

const PROBE_FRAME: usize = 64;
const WRITE_FRAME: usize = 96;
const TRY_FRAME: usize = 48;

/// Emits all x86_64 helper bodies used by the integer-tree fast path.
pub(super) fn emit_helpers(emitter: &mut Emitter) {
    emit_integer_width(emitter);
    emit_probe(emitter);
    emit_writer(emitter);
    emit_try(emitter);
}

/// Emits the top-level flag and depth guards plus the fast-path call site.
pub(super) fn emit_entry(emitter: &mut Emitter, prefix: &str) {
    let fallback = format!("{prefix}_fallback");
    let unsupported = format!("{prefix}_unsupported");
    let attempt = format!("{prefix}_attempt");
    let max_scratch_integer_slots = (emitter.concat_capacity - 1) / 21;

    // -- keep flags, recursive encodes, and unsupported trees on the general encoder --
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_active_flags", 0);
    emitter.instruction("test r10, r10");                                       // inspect the complete JSON option mask
    emitter.instruction(&format!("jnz {fallback}"));                            // flags need the general formatter semantics
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_active_depth", 0);
    emitter.instruction("test r10, r10");                                       // inspect the current recursive container depth
    emitter.instruction(&format!("jnz {fallback}"));                            // only the top-level container owns the exact result buffer

    // -- keep provably bounded direct integer arrays on the existing one-pass encoder --
    emitter.instruction("mov r10, QWORD PTR [rax - 8]");                        // load the packed array value type from the heap header
    emitter.instruction("shr r10, 8");                                          // move the value type into the low bits
    emitter.instruction("and r10, 0x7f");                                       // discard copy-on-write and other header flags
    emitter.instruction("test r10, r10");                                       // direct integer slots use value type zero
    emitter.instruction(&format!("jnz {attempt}"));                             // nested and boxed layouts still need the complete proof
    abi::emit_load_int_immediate(emitter, "r11", max_scratch_integer_slots as i64);
    emitter.instruction("cmp QWORD PTR [rax], r11");                            // test the twenty-byte-per-value upper bound against scratch capacity
    emitter.instruction(&format!("jbe {fallback}"));                            // bounded direct arrays retain the faster single-pass encoder
    emitter.label(&attempt);

    abi::emit_frame_prologue(emitter, 32);
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the source array across the eligibility attempt
    emitter.instruction("call __rt_json_integer_tree_try");                     // return rax/rdx on success or rax=0 when unsupported
    emitter.instruction("test rax, rax");                                       // did the bounded encoder accept the complete tree?
    emitter.instruction(&format!("jz {unsupported}"));                          // restore the source and continue through the old encoder on rejection
    emitter.instruction("mov rsp, rbp");                                        // release the attempt frame through its stable frame pointer
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer before returning
    emitter.instruction("ret");                                                 // return the exact heap-backed JSON slice

    emitter.label(&unsupported);
    emitter.instruction("mov rax, QWORD PTR [rbp - 8]");                        // restore the original array pointer for the existing encoder
    emitter.instruction("mov rsp, rbp");                                        // release the rejected attempt frame before the fallback path
    emitter.instruction("pop rbp");                                             // restore the caller frame pointer at the original stack depth
    emitter.label(&fallback);
}

/// Emits a leaf helper that returns the decimal width of one signed integer.
fn emit_integer_width(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_width ---");
    emitter.label_global("__rt_json_integer_width");

    // -- derive the unsigned magnitude and optional sign width --
    emitter.instruction("mov r10, rax");                                        // keep the magnitude being counted
    emitter.instruction("xor r11d, r11d");                                      // initialize the optional sign width
    emitter.instruction("test r10, r10");                                       // inspect the input sign
    emitter.instruction("jns __rt_json_integer_width_positive");                // positive values already carry an unsigned magnitude
    emitter.instruction("neg r10");                                             // wrapping magnitude also handles PHP_INT_MIN
    emitter.instruction("mov r11, 1");                                          // reserve one byte for the minus sign
    emitter.label("__rt_json_integer_width_positive");

    // -- classify the magnitude without repeated integer division --
    for (digits, threshold) in (1_i64..=18).zip([
        10_i64,
        100,
        1_000,
        10_000,
        100_000,
        1_000_000,
        10_000_000,
        100_000_000,
        1_000_000_000,
        10_000_000_000,
        100_000_000_000,
        1_000_000_000_000,
        10_000_000_000_000,
        100_000_000_000_000,
        1_000_000_000_000_000,
        10_000_000_000_000_000,
        100_000_000_000_000_000,
        1_000_000_000_000_000_000,
    ]) {
        emitter.instruction(&format!("mov rcx, {digits}"));                     // retain the width selected when the next threshold contains the magnitude
        abi::emit_load_int_immediate(emitter, "r9", threshold);
        emitter.instruction("cmp r10, r9");                                     // compare the unsigned magnitude with the next decimal power
        emitter.instruction("jb __rt_json_integer_width_done_x86");             // finish as soon as the magnitude fits this width
    }
    emitter.instruction("mov rcx, 19");                                         // every remaining signed magnitude occupies nineteen digits
    emitter.label("__rt_json_integer_width_done_x86");
    emitter.instruction("lea rax, [rcx + r11]");                                // return digits plus the optional sign byte
    emitter.instruction("ret");                                                 // leaf helper preserves no frame state
}

/// Emits the recursive, side-effect-free shape and exact-size probe.
fn emit_probe(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_tree_probe ---");
    emitter.label_global("__rt_json_integer_tree_probe");

    // -- validate the root container and initialize its exact byte count --
    abi::emit_frame_prologue(emitter, PROBE_FRAME);
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the candidate indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save its one-based JSON nesting depth
    emit_branch_if_null_container(emitter, "rdi", "r10", "__rt_json_integer_tree_probe_unsupported_x86");
    abi::emit_load_symbol_to_reg(emitter, "r10", "_json_depth_limit", 0);
    emitter.instruction("cmp rsi, r10");                                        // would this container exceed the PHP depth budget?
    emitter.instruction("jg __rt_json_integer_tree_probe_unsupported_x86");     // let the general encoder publish the depth error
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load packed heap kind and indexed-array value type
    emitter.instruction("mov r10, r9");                                         // copy the packed metadata for kind isolation
    emitter.instruction("and r10, 0xff");                                       // isolate the uniform top-level heap kind
    emitter.instruction("cmp r10, 2");                                          // kind 2 identifies indexed-array storage
    emitter.instruction("jne __rt_json_integer_tree_probe_unsupported_x86");    // hashes and other containers need the general encoder
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the logical array length
    emitter.instruction("test r10, r10");                                       // reject invalid negative metadata defensively
    emitter.instruction("js __rt_json_integer_tree_probe_unsupported_x86");     // negative lengths indicate invalid metadata
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // retain the length for the element loop
    emitter.instruction("mov QWORD PTR [rbp - 24], 2");                         // every accepted array contributes '[' and ']'
    emitter.instruction("test r10, r10");                                       // every valid empty indexed array serializes as []
    emitter.instruction("jz __rt_json_integer_tree_probe_success_x86");         // every valid empty indexed array serializes as []
    emitter.instruction("cmp QWORD PTR [rdi + 16], 8");                         // accepted payload families occupy one machine word per slot
    emitter.instruction("jne __rt_json_integer_tree_probe_unsupported_x86");    // wider layouts belong to the general encoder
    emitter.instruction("shr r9, 8");                                           // move the packed value type into the low bits
    emitter.instruction("and r9, 0x7f");                                        // discard copy-on-write and other header flags
    emitter.instruction("cmp r9, 0");                                           // direct integer slots are eligible
    emitter.instruction("je __rt_json_integer_tree_probe_type_ok_x86");         // accept direct integer storage without a per-slot tag
    emitter.instruction("cmp r9, 4");                                           // direct indexed-array slots are eligible
    emitter.instruction("je __rt_json_integer_tree_probe_type_ok_x86");         // accept direct nested indexed-array storage
    emitter.instruction("cmp r9, 7");                                           // boxed Mixed slots need per-cell tag checks
    emitter.instruction("jne __rt_json_integer_tree_probe_unsupported_x86");    // reject every remaining payload layout
    emitter.label("__rt_json_integer_tree_probe_type_ok_x86");
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the accepted array value type
    emitter.instruction("mov QWORD PTR [rbp - 48], 0");                         // initialize the element index

    // -- classify and size every accepted element without changing runtime state --
    emitter.label("__rt_json_integer_tree_probe_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 48]");                       // reload the current element index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all elements been sized?
    emitter.instruction("jae __rt_json_integer_tree_probe_success_x86");        // finish after sizing the complete array
    emitter.instruction("test r10, r10");                                       // the first element has no comma prefix
    emitter.instruction("jz __rt_json_integer_tree_probe_value_x86");           // the first element has no comma prefix
    emitter.instruction("mov r11, QWORD PTR [rbp - 24]");                       // reload the accumulated byte count
    emitter.instruction("add r11, 1");                                          // include one comma separator
    emitter.instruction("jc __rt_json_integer_tree_probe_unsupported_x86");     // reject wrapped totals before allocation
    emitter.instruction("js __rt_json_integer_tree_probe_unsupported_x86");     // heap sizes must remain non-negative
    emitter.instruction("mov QWORD PTR [rbp - 24], r11");                       // publish the updated byte count

    emitter.label("__rt_json_integer_tree_probe_value_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the array base
    emitter.instruction("mov r8, QWORD PTR [r11 + r10 * 8 + 24]");              // load the one-word element payload
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the packed array value type
    emitter.instruction("test r9, r9");                                         // direct integer slots need only decimal width
    emitter.instruction("jz __rt_json_integer_tree_probe_int_x86");             // direct integer slots need only decimal width
    emitter.instruction("cmp r9, 4");                                           // direct nested arrays recurse without boxing
    emitter.instruction("je __rt_json_integer_tree_probe_array_x86");           // direct nested arrays recurse without boxing

    emit_branch_if_null_container(emitter, "r8", "r11", "__rt_json_integer_tree_probe_unsupported_x86");
    emitter.instruction("mov r11, QWORD PTR [r8 - 8]");                         // load the boxed-cell heap kind
    emitter.instruction("and r11, 0xff");                                       // isolate the uniform kind byte
    emitter.instruction("cmp r11, 5");                                          // accepted boxed elements must be real Mixed cells
    emitter.instruction("jne __rt_json_integer_tree_probe_unsupported_x86");    // reference cells and other owners keep general semantics
    emitter.instruction("mov r11, QWORD PTR [r8]");                             // load the Mixed runtime value tag
    emitter.instruction("test r11, r11");                                       // boxed integers are eligible
    emitter.instruction("jz __rt_json_integer_tree_probe_mixed_int_x86");       // boxed integers are eligible
    emitter.instruction("cmp r11, 4");                                          // boxed indexed arrays are eligible
    emitter.instruction("jne __rt_json_integer_tree_probe_unsupported_x86");    // strings, floats, bools, nulls and objects fall back
    emitter.instruction("mov r8, QWORD PTR [r8 + 8]");                          // load the boxed nested-array pointer
    emitter.instruction("jmp __rt_json_integer_tree_probe_array_x86");          // reuse the nested-array probe path for the boxed payload

    emitter.label("__rt_json_integer_tree_probe_mixed_int_x86");
    emitter.instruction("mov rax, QWORD PTR [r8 + 8]");                         // load the boxed integer payload
    emitter.instruction("call __rt_json_integer_width");                        // compute its exact decimal byte width
    emitter.instruction("jmp __rt_json_integer_tree_probe_add_x86");            // add the boxed integer width to the parent total

    emitter.label("__rt_json_integer_tree_probe_int_x86");
    emitter.instruction("mov rax, r8");                                         // pass the direct integer payload to the width helper
    emitter.instruction("call __rt_json_integer_width");                        // compute its exact decimal byte width
    emitter.instruction("jmp __rt_json_integer_tree_probe_add_x86");            // add the direct integer width to the parent total

    emitter.label("__rt_json_integer_tree_probe_array_x86");
    emit_branch_if_null_container(emitter, "r8", "r11", "__rt_json_integer_tree_probe_unsupported_x86");
    emitter.instruction("mov rdi, r8");                                         // pass the nested indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // reload the current one-based nesting depth
    emitter.instruction("add rsi, 1");                                          // recurse at the child depth
    emitter.instruction("call __rt_json_integer_tree_probe");                   // return the child size or zero when unsupported
    emitter.instruction("test rax, rax");                                       // zero requests the general encoder
    emitter.instruction("jz __rt_json_integer_tree_probe_unsupported_x86");     // propagate child rejection to the root attempt

    emitter.label("__rt_json_integer_tree_probe_add_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the accumulated byte count
    emitter.instruction("add r10, rax");                                        // include this scalar or child container
    emitter.instruction("jc __rt_json_integer_tree_probe_unsupported_x86");     // reject unsigned addition overflow
    emitter.instruction("js __rt_json_integer_tree_probe_unsupported_x86");     // reject sizes the allocator reads as negative
    emitter.instruction("mov QWORD PTR [rbp - 24], r10");                       // save the new exact total
    emitter.instruction("add QWORD PTR [rbp - 48], 1");                         // advance to the next element
    emitter.instruction("jmp __rt_json_integer_tree_probe_loop_x86");           // size the next array slot

    // -- return the exact size or a marker that preserves the general encoder --
    emitter.label("__rt_json_integer_tree_probe_success_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the exact non-zero serialized size
    abi::emit_frame_restore(emitter, PROBE_FRAME);
    emitter.instruction("ret");                                                 // return the admitted tree size to the caller
    emitter.label("__rt_json_integer_tree_probe_unsupported_x86");
    emitter.instruction("xor eax, eax");                                        // zero tells the caller to use the general encoder
    abi::emit_frame_restore(emitter, PROBE_FRAME);
    emitter.instruction("ret");                                                 // return the rejection marker to the caller
}

/// Emits the accepted-tree writer into a caller-owned exact output allocation.
fn emit_writer(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_tree_write ---");
    emitter.label_global("__rt_json_integer_tree_write");

    // -- initialize the caller-owned output cursor and array traversal state --
    abi::emit_frame_prologue(emitter, WRITE_FRAME);
    emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                        // save the source indexed-array pointer
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // save the current output cursor
    emitter.instruction("mov BYTE PTR [rsi], 91");                              // write ASCII '['
    emitter.instruction("add rsi, 1");                                          // advance the output cursor
    emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                       // persist the advanced cursor
    emitter.instruction("mov r10, QWORD PTR [rdi]");                            // load the source array length
    emitter.instruction("mov QWORD PTR [rbp - 32], r10");                       // save it for the element loop
    emitter.instruction("mov r9, QWORD PTR [rdi - 8]");                         // load packed indexed-array metadata
    emitter.instruction("shr r9, 8");                                           // move the value type into the low bits
    emitter.instruction("and r9, 0x7f");                                        // isolate the stable value type
    emitter.instruction("mov QWORD PTR [rbp - 40], r9");                        // save the value type for dispatch
    emitter.instruction("mov QWORD PTR [rbp - 24], 0");                         // initialize the element index

    // -- serialize each previously accepted slot into the exact allocation --
    emitter.label("__rt_json_integer_tree_write_loop_x86");
    emitter.instruction("mov r10, QWORD PTR [rbp - 24]");                       // reload the current element index
    emitter.instruction("cmp r10, QWORD PTR [rbp - 32]");                       // have all elements been written?
    emitter.instruction("jae __rt_json_integer_tree_write_close_x86");          // close the array after its final slot
    emitter.instruction("test r10, r10");                                       // the first element has no comma prefix
    emitter.instruction("jz __rt_json_integer_tree_write_value_x86");           // the first element has no comma prefix
    emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                       // reload the current output cursor
    emitter.instruction("mov BYTE PTR [r11], 44");                              // write ASCII ','
    emitter.instruction("add r11, 1");                                          // advance past the comma
    emitter.instruction("mov QWORD PTR [rbp - 16], r11");                       // persist the new cursor

    emitter.label("__rt_json_integer_tree_write_value_x86");
    emitter.instruction("mov r11, QWORD PTR [rbp - 8]");                        // reload the source array pointer
    emitter.instruction("mov r8, QWORD PTR [r11 + r10 * 8 + 24]");              // load the one-word element payload
    emitter.instruction("mov r9, QWORD PTR [rbp - 40]");                        // reload the array value type
    emitter.instruction("test r9, r9");                                         // direct integer slots format in place
    emitter.instruction("jz __rt_json_integer_tree_write_int_x86");             // direct integer slots format in place
    emitter.instruction("cmp r9, 4");                                           // direct nested arrays recurse into the same destination
    emitter.instruction("je __rt_json_integer_tree_write_array_x86");           // direct nested arrays recurse into the same destination
    emitter.instruction("cmp QWORD PTR [r8], 0");                               // distinguish the two already-probed Mixed tags
    emitter.instruction("jne __rt_json_integer_tree_write_mixed_array_x86");    // the only other accepted Mixed tag is indexed array 4
    emitter.instruction("mov r8, QWORD PTR [r8 + 8]");                          // load the boxed integer payload
    emitter.instruction("jmp __rt_json_integer_tree_write_int_x86");            // format the boxed integer through the shared path
    emitter.label("__rt_json_integer_tree_write_mixed_array_x86");
    emitter.instruction("mov r8, QWORD PTR [r8 + 8]");                          // load the boxed nested-array pointer

    emitter.label("__rt_json_integer_tree_write_array_x86");
    emitter.instruction("mov rdi, r8");                                         // pass the nested indexed-array pointer
    emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                       // pass the current output cursor
    emitter.instruction("call __rt_json_integer_tree_write");                   // return the cursor after the child closing bracket
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // continue writing after the nested array
    emitter.instruction("jmp __rt_json_integer_tree_write_next_x86");           // advance after the nested writer returned

    emitter.label("__rt_json_integer_tree_write_int_x86");
    emitter.instruction("mov rdi, r8");                                         // pass the accepted integer payload to the formatter
    abi::emit_frame_slot_address(emitter, "rsi", 72);
    emitter.instruction("call __rt_itoa_into");                                 // return a decimal slice inside the local 21-byte window
    emitter.instruction("mov r10, QWORD PTR [rbp - 16]");                       // reload the destination cursor
    emitter.instruction("xor ecx, ecx");                                        // initialize the formatted-slice copy index
    emitter.label("__rt_json_integer_tree_write_copy_x86");
    emitter.instruction("cmp rcx, rdx");                                        // have all formatted bytes been copied?
    emitter.instruction("jae __rt_json_integer_tree_write_copy_done_x86");      // stop after copying the complete decimal slice
    emitter.instruction("mov r11b, BYTE PTR [rax + rcx]");                      // load the next decimal byte
    emitter.instruction("mov BYTE PTR [r10 + rcx], r11b");                      // write it into the exact result allocation
    emitter.instruction("add rcx, 1");                                          // advance the copy index
    emitter.instruction("jmp __rt_json_integer_tree_write_copy_x86");           // copy the next decimal byte
    emitter.label("__rt_json_integer_tree_write_copy_done_x86");
    emitter.instruction("add r10, rdx");                                        // advance past the formatted integer
    emitter.instruction("mov QWORD PTR [rbp - 16], r10");                       // persist the new output cursor

    emitter.label("__rt_json_integer_tree_write_next_x86");
    emitter.instruction("add QWORD PTR [rbp - 24], 1");                         // advance to the next slot
    emitter.instruction("jmp __rt_json_integer_tree_write_loop_x86");           // encode the next array slot

    // -- close the container and return its final output cursor --
    emitter.label("__rt_json_integer_tree_write_close_x86");
    emitter.instruction("mov rax, QWORD PTR [rbp - 16]");                       // reload the final cursor before the closing bracket
    emitter.instruction("mov BYTE PTR [rax], 93");                              // write ASCII ']'
    emitter.instruction("add rax, 1");                                          // return the cursor just after the closing byte
    abi::emit_frame_restore(emitter, WRITE_FRAME);
    emitter.instruction("ret");                                                 // return the final output cursor to the parent writer
}

/// Emits the allocating public attempt used by both indexed-array encoders.
fn emit_try(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: json_integer_tree_try ---");
    emitter.label_global("__rt_json_integer_tree_try");

    // -- admit the complete tree before allocating and filling one result string --
    abi::emit_frame_prologue(emitter, TRY_FRAME);
    emitter.instruction("mov QWORD PTR [rbp - 8], rax");                        // preserve the root array across the size probe
    emitter.instruction("mov rdi, rax");                                        // pass the root array to the internal probe ABI
    emitter.instruction("mov rsi, 1");                                          // the root array occupies JSON depth one
    emitter.instruction("call __rt_json_integer_tree_probe");                   // return the exact size or zero when the tree is unsupported
    emitter.instruction("test rax, rax");                                       // zero means the existing encoder must handle this value
    emitter.instruction("jz __rt_json_integer_tree_try_unsupported_x86");       // allocate only after the entire tree is admitted
    emitter.instruction("mov QWORD PTR [rbp - 16], rax");                       // preserve the exact byte count across allocation and writing
    emitter.instruction("call __rt_heap_alloc");                                // allocate one exact output payload
    emitter.instruction(&format!(
        "mov r10, 0x{:x}",
        x86_64_heap_kind_word(CONCAT_TEMP_HEAP_KIND)
    ));                                                                         // materialize the transient string kind with the allocator marker
    emitter.instruction("mov QWORD PTR [rax - 8], r10");                        // publish the transient string heap kind
    emitter.instruction("mov QWORD PTR [rbp - 24], rax");                       // preserve the output base across the recursive writer
    emitter.instruction("mov rsi, rax");                                        // pass the exact output base as the initial cursor
    emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                        // reload the admitted root array
    emitter.instruction("call __rt_json_integer_tree_write");                   // fill the allocation without further allocation or callbacks
    emitter.instruction("mov rax, QWORD PTR [rbp - 24]");                       // return the string base in the JSON string ABI
    emitter.instruction("mov rdx, QWORD PTR [rbp - 16]");                       // return the exact serialized length
    abi::emit_frame_restore(emitter, TRY_FRAME);
    emitter.instruction("ret");                                                 // return the completed JSON string pair

    // -- leave unsupported trees to the existing stateful encoder --
    emitter.label("__rt_json_integer_tree_try_unsupported_x86");
    emitter.instruction("xor eax, eax");                                        // a zero result pointer requests the general encoder
    emitter.instruction("xor edx, edx");                                        // unsupported attempts have no result bytes
    abi::emit_frame_restore(emitter, TRY_FRAME);
    emitter.instruction("ret");                                                 // return the unsupported marker to the guarded call site
}
