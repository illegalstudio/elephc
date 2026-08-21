//! Purpose:
//! Materializes recursively flattened native-extension result trees into owned PHP values.
//!
//! Called from:
//! - `super::materialize_result_map()` for heterogeneous native map results.
//!
//! Key details:
//! - The complete implementation must validate flat value and byte ranges before allocation.
//! - Every returned nested string, container, and object wrapper must have one balanced owner.

use crate::codegen::abi;
use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{
    emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed, Result,
};
use crate::types::PhpType;

use super::{
    emit_bridge_failure_jump, emit_typed_wrapper_result, ARRAY_COUNT_OFFSET,
    ARRAY_INDEX_OFFSET, ARRAY_RESULT_OFFSET, ARRAY_VALUE_OFFSET, CALL_FRAME_SIZE,
    OBJECT_FIELD_COUNT_OFFSET, OBJECT_FIELDS_OFFSET,
};

const MATERIALIZER_FRAME_SIZE: usize = 96;
const MATERIALIZER_MAX_DEPTH: i64 = 64;
const WRAPPER_FRAME_SIZE: usize = CALL_FRAME_SIZE + 32;

/// Materializes one heterogeneous top-level map from the retained flat result frame.
pub(super) fn materialize_recursive_result_map(
    ctx: &mut FunctionContext<'_>,
    mixed_keys: bool,
) -> Result<()> {
    emit_prevalidate_result_map(ctx, mixed_keys);
    let materialize = emit_recursive_materializer(ctx)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_top_map_aarch64(ctx, mixed_keys, &materialize),
        Arch::X86_64 => emit_top_map_x86_64(ctx, mixed_keys, &materialize),
    }
    Ok(())
}

/// Invokes the target-neutral native validator before any PHP value allocation.
fn emit_prevalidate_result_map(ctx: &mut FunctionContext<'_>, mixed_keys: bool) {
    let valid = ctx.next_label("dom_result_tree_prevalidated");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", 112);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", 120);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", 96);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", 104);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "x4", 88);
            abi::emit_load_int_immediate(ctx.emitter, "x5", i64::from(mixed_keys));
        }
        Arch::X86_64 => {
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", 112);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", 120);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", 96);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", 104);
            abi::emit_load_temporary_stack_slot(ctx.emitter, "r8", 88);
            abi::emit_load_int_immediate(ctx.emitter, "r9", i64::from(mixed_keys));
        }
    }
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("elephc_dom_validate_result_map_tree");
    abi::emit_call_label(ctx.emitter, &symbol);
    abi::emit_branch_if_int_result_zero(ctx.emitter, &valid);
    emit_bridge_failure_jump(ctx);
    ctx.emitter.label(&valid);
}

/// Emits the local recursive value helper and its SimpleXML wrapper callback.
fn emit_recursive_materializer(ctx: &mut FunctionContext<'_>) -> Result<String> {
    let entry = ctx.next_label("dom_result_tree_value");
    let wrapper = ctx.next_label("dom_result_tree_wrapper");
    let value_object = ctx.next_label("dom_result_tree_value_object_callback");
    let body = ctx.next_label("dom_result_tree_body");
    abi::emit_jump(ctx.emitter, &body);
    ctx.emitter.label(&entry);
    match ctx.emitter.target.arch {
        Arch::AArch64 => emit_value_helper_aarch64(ctx, &entry, &wrapper, &value_object),
        Arch::X86_64 => emit_value_helper_x86_64(ctx, &entry, &wrapper, &value_object),
    }
    ctx.emitter.label(&wrapper);
    emit_wrapper_callback(ctx)?;
    ctx.emitter.label(&value_object);
    emit_value_object_callback(ctx)?;
    ctx.emitter.label(&body);
    Ok(entry)
}

/// Materializes one supported copied native PHP value-object descriptor.
fn emit_value_object_callback(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let libxml_error = ctx.next_label("dom_result_tree_libxml_error");
    let namespace_info = ctx.next_label("dom_result_tree_namespace_info");
    let done = ctx.next_label("dom_result_tree_value_object_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("sub sp, sp, #{}", WRAPPER_FRAME_SIZE));  // reserve an outer-compatible value-object materialization frame
            ctx.emitter
                .instruction(&format!("stp x29, x30, [sp, #{}]", CALL_FRAME_SIZE)); // preserve the caller frame and local-helper return address
            ctx.emitter
                .instruction(&format!("add x29, sp, #{}", CALL_FRAME_SIZE));    // establish the value-object callback frame pointer
            for offset in [16, 96, 104, 112, 120] {
                ctx.emitter
                    .instruction(&format!("ldr x9, [x0, #{}]", offset));        // copy retained flat-result storage into the callback frame
                ctx.emitter
                    .instruction(&format!("str x9, [sp, #{}]", offset));        // expose the retained storage at ordinary bridge offsets
            }
            ctx.emitter
                .instruction(&format!("str x2, [sp, #{}]", OBJECT_FIELDS_OFFSET)); // stage the copied object's direct-field range start
            ctx.emitter
                .instruction(&format!("str x3, [sp, #{}]", OBJECT_FIELD_COUNT_OFFSET)); // stage the copied object's exact field count
            ctx.emitter.instruction("cmp w1, #1");                              // is this a copied LibXMLError descriptor?
            ctx.emitter
                .instruction(&format!("b.eq {}", libxml_error));               // materialize the six-field LibXMLError object
            ctx.emitter.instruction("cmp w1, #2");                              // is this a copied Dom\NamespaceInfo descriptor?
            ctx.emitter
                .instruction(&format!("b.eq {}", namespace_info));             // materialize the three-field namespace object
            emit_bridge_failure_jump(ctx);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve the caller frame pointer for the local callback
            ctx.emitter.instruction("mov rbp, rsp");                            // establish a stable callback frame base
            ctx.emitter
                .instruction(&format!("sub rsp, {}", CALL_FRAME_SIZE));        // reserve the outer-compatible value-object frame
            for offset in [16, 96, 104, 112, 120] {
                ctx.emitter
                    .instruction(&format!("mov r10, QWORD PTR [rdi + {}]", offset)); // copy retained flat-result storage into the callback frame
                ctx.emitter
                    .instruction(&format!("mov QWORD PTR [rsp + {}], r10", offset)); // expose the retained storage at ordinary bridge offsets
            }
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], rdx", OBJECT_FIELDS_OFFSET)); // stage the copied object's direct-field range start
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], rcx", OBJECT_FIELD_COUNT_OFFSET)); // stage the copied object's exact field count
            ctx.emitter.instruction("cmp esi, 1");                              // is this a copied LibXMLError descriptor?
            ctx.emitter
                .instruction(&format!("je {}", libxml_error));                 // materialize the six-field LibXMLError object
            ctx.emitter.instruction("cmp esi, 2");                              // is this a copied Dom\NamespaceInfo descriptor?
            ctx.emitter
                .instruction(&format!("je {}", namespace_info));               // materialize the three-field namespace object
            emit_bridge_failure_jump(ctx);
        }
    }
    ctx.emitter.label(&libxml_error);
    super::materialize_libxml_error_value_object(ctx)?;
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&namespace_info);
    super::materialize_namespace_info_value_object(ctx)?;
    ctx.emitter.label(&done);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldp x29, x30, [sp, #{}]", CALL_FRAME_SIZE)); // restore the caller frame and local-helper return address
            ctx.emitter
                .instruction(&format!("add sp, sp, #{}", WRAPPER_FRAME_SIZE)); // release the value-object callback frame
            ctx.emitter.instruction("ret");                                     // return the owned ordinary PHP object
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("add rsp, {}", CALL_FRAME_SIZE));        // release the value-object callback frame
            ctx.emitter.instruction("pop rbp");                                 // restore the caller frame pointer
            ctx.emitter.instruction("ret");                                     // return the owned ordinary PHP object
        }
    }
    Ok(())
}

/// Materializes one bridge handle through the program's exact SimpleXML class table.
fn emit_wrapper_callback(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let context_reg = abi::secondary_scratch_reg(ctx.emitter).to_string();
    let handle_reg = abi::tertiary_scratch_reg(ctx.emitter).to_string();
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("sub sp, sp, #{}", WRAPPER_FRAME_SIZE));  // reserve an outer-compatible bridge frame and return-address slot
            ctx.emitter
                .instruction(&format!("stp x29, x30, [sp, #{}]", CALL_FRAME_SIZE)); // preserve the caller frame and local-helper return address
            ctx.emitter
                .instruction(&format!("add x29, sp, #{}", CALL_FRAME_SIZE));    // establish the wrapper callback frame pointer
            ctx.emitter.instruction("str x2, [sp, #88]");                       // stage the stable native wrapper discriminator
            ctx.emitter
                .instruction(&format!("ldr {}, [x0, #16]", context_reg));      // load the retained native DOM context from the outer call frame
            ctx.emitter
                .instruction(&format!("mov {}, x1", handle_reg));             // preserve the fresh bridge handle for wrapper allocation
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rbp");                                // preserve the caller frame pointer for the local callback
            ctx.emitter.instruction("mov rbp, rsp");                            // establish a stable callback frame base
            ctx.emitter
                .instruction(&format!("sub rsp, {}", CALL_FRAME_SIZE));        // reserve the outer-compatible bridge frame
            ctx.emitter.instruction("mov QWORD PTR [rsp + 88], rdx");           // stage the stable native wrapper discriminator
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [rdi + 16]", context_reg)); // load the retained native DOM context from the outer call frame
            ctx.emitter
                .instruction(&format!("mov {}, rsi", handle_reg));            // preserve the fresh bridge handle for wrapper allocation
        }
    }
    emit_typed_wrapper_result(
        ctx,
        "SimpleXMLElement",
        &context_reg,
        &handle_reg,
        false,
        None,
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("ldp x29, x30, [sp, #{}]", CALL_FRAME_SIZE)); // restore the caller frame and local-helper return address
            ctx.emitter
                .instruction(&format!("add sp, sp, #{}", WRAPPER_FRAME_SIZE)); // release the wrapper callback frame
            ctx.emitter.instruction("ret");                                     // return the owned concrete SimpleXML wrapper
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("add rsp, {}", CALL_FRAME_SIZE));        // release the wrapper callback frame
            ctx.emitter.instruction("pop rbp");                                 // restore the caller frame pointer
            ctx.emitter.instruction("ret");                                     // return the owned concrete SimpleXML wrapper
        }
    }
    Ok(())
}

/// Materializes the top-level heterogeneous map on AArch64.
fn emit_top_map_aarch64(
    ctx: &mut FunctionContext<'_>,
    mixed_keys: bool,
    materialize: &str,
) {
    let capacity_ready = ctx.next_label("dom_result_tree_top_capacity");
    let loop_head = ctx.next_label("dom_result_tree_top_loop");
    let loop_done = ctx.next_label("dom_result_tree_top_done");
    let integer_key = ctx.next_label("dom_result_tree_top_integer_key");
    let string_key = ctx.next_label("dom_result_tree_top_string_key");
    let key_ready = ctx.next_label("dom_result_tree_top_key_ready");
    let failure = ctx.next_label("dom_result_tree_top_failure");

    ctx.emitter.instruction("ldr x9, [sp, #80]");                               // load the top-level map's direct-child start
    ctx.emitter
        .instruction(&format!("cbnz x9, {}", failure));                        // top-level map children must begin at flat record zero
    ctx.emitter.instruction("ldr x10, [sp, #88]");                              // load the top-level map entry count
    ctx.emitter.instruction("adds x11, x10, x10");                              // compute the number of direct key/value records
    ctx.emitter
        .instruction(&format!("b.cs {}", failure));                            // reject an overflowing direct-child count
    ctx.emitter.instruction("ldr x12, [sp, #120]");                             // load the complete flat value count including descendants
    ctx.emitter.instruction("cmp x11, x12");                                    // do all direct children fit before their descendants?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject a truncated top-level map range
    ctx.emitter.instruction("mov x0, x11");                                     // use twice the entry count as the target hash capacity
    ctx.emitter.instruction("cmp x0, #16");                                     // enforce the runtime minimum hash capacity
    ctx.emitter
        .instruction(&format!("b.hs {}", capacity_ready));                     // retain a sufficiently large computed capacity
    ctx.emitter.instruction("mov x0, #16");                                     // raise small maps to the minimum capacity
    ctx.emitter.label(&capacity_ready);
    ctx.emitter.instruction("mov x1, #7");                                      // heterogeneous maps store owned boxed Mixed cells
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    abi::emit_store_to_sp(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
    super::store_stack_immediate(ctx, ARRAY_INDEX_OFFSET, 0);
    super::stage_result_word(ctx, 88, ARRAY_COUNT_OFFSET);

    ctx.emitter.label(&loop_head);
    ctx.emitter
        .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET));        // load the next top-level entry index
    ctx.emitter
        .instruction(&format!("ldr x10, [sp, #{}]", ARRAY_COUNT_OFFSET));      // load the top-level entry count
    ctx.emitter.instruction("cmp x9, x10");                                     // have all top-level entries been materialized?
    ctx.emitter
        .instruction(&format!("b.hs {}", loop_done));                          // finish after the final entry
    ctx.emitter.instruction("lsl x9, x9, #1");                                  // map the entry index to its flat key record
    ctx.emitter.instruction("ldr x11, [sp, #112]");                             // load the retained flat value-vector pointer
    ctx.emitter
        .instruction(&format!("cbz x11, {}", failure));                        // non-empty direct ranges require value storage
    ctx.emitter.instruction("add x10, x9, x9, lsl #1");                         // scale the key index by three ABI words
    ctx.emitter.instruction("add x11, x11, x10, lsl #3");                       // address the exact 24-byte key record
    ctx.emitter.instruction("ldr w10, [x11]");                                  // load the key record tag
    if mixed_keys {
        ctx.emitter.instruction("cmp w10, #2");                                 // is this an integer PHP array key?
        ctx.emitter
            .instruction(&format!("b.eq {}", integer_key));                    // retain the signed integer payload exactly
    }
    ctx.emitter.instruction("cmp w10, #4");                                     // is this a byte-string PHP array key?
    ctx.emitter
        .instruction(&format!("b.eq {}", string_key));                         // validate and normalize the string key
    ctx.emitter
        .instruction(&format!("b {}", failure));                               // reject every unsupported key tag

    ctx.emitter.label(&integer_key);
    ctx.emitter.instruction("ldr x1, [x11, #8]");                               // load the signed integer key bits
    ctx.emitter.instruction("mov x2, #-1");                                     // mark the key as integer for the hash ABI
    ctx.emitter
        .instruction(&format!("b {}", key_ready));                             // skip byte-range validation for integer keys

    ctx.emitter.label(&string_key);
    ctx.emitter.instruction("ldr x10, [x11, #8]");                              // load the borrowed result-byte offset
    ctx.emitter.instruction("ldr x2, [x11, #16]");                              // load the exact key byte length
    ctx.emitter.instruction("ldr x12, [sp, #104]");                             // load the retained result-byte length
    ctx.emitter.instruction("cmp x10, x12");                                    // is the key byte offset in bounds?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject an out-of-range key offset
    ctx.emitter.instruction("sub x12, x12, x10");                               // compute the bytes remaining after the offset
    ctx.emitter.instruction("cmp x2, x12");                                     // does the complete key fit in retained bytes?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject a truncated byte-string key
    ctx.emitter.instruction("ldr x1, [sp, #96]");                               // load the retained result-byte pointer
    ctx.emitter.instruction("add x1, x1, x10");                                 // address the borrowed key bytes
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");

    ctx.emitter.label(&key_ready);
    abi::emit_store_to_sp(ctx.emitter, "x1", OBJECT_FIELDS_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, "x2", OBJECT_FIELD_COUNT_OFFSET);
    ctx.emitter
        .instruction(&format!("ldr x0, [sp, #{}]", ARRAY_INDEX_OFFSET));       // reload the entry index for its value record
    ctx.emitter.instruction("lsl x0, x0, #1");                                  // map the entry index to its flat key record
    ctx.emitter.instruction("add x0, x0, #1");                                  // select the value immediately after the key
    ctx.emitter.instruction("mov x1, sp");                                      // pass the stable outer result frame to recursion
    ctx.emitter.instruction("mov x2, xzr");                                     // start bounded recursion at depth zero
    abi::emit_call_label(ctx.emitter, materialize);
    abi::emit_store_to_sp(ctx.emitter, "x0", ARRAY_VALUE_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x1", OBJECT_FIELDS_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x2", OBJECT_FIELD_COUNT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x3", ARRAY_VALUE_OFFSET);
    ctx.emitter.instruction("mov x4, xzr");                                     // boxed Mixed hash values use one payload word
    ctx.emitter.instruction("mov x5, #7");                                      // runtime value tag seven identifies boxed Mixed
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    abi::emit_store_to_sp(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
    ctx.emitter
        .instruction(&format!("ldr x9, [sp, #{}]", ARRAY_INDEX_OFFSET));       // reload the completed top-level entry index
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance to the next key/value pair
    ctx.emitter
        .instruction(&format!("str x9, [sp, #{}]", ARRAY_INDEX_OFFSET));       // persist the next top-level entry index
    abi::emit_jump(ctx.emitter, &loop_head);

    ctx.emitter.label(&loop_done);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "x0", ARRAY_RESULT_OFFSET);
    let complete = ctx.next_label("dom_result_tree_top_complete");
    abi::emit_jump(ctx.emitter, &complete);
    ctx.emitter.label(&failure);
    emit_bridge_failure_jump(ctx);
    ctx.emitter.label(&complete);
}

/// Materializes the top-level heterogeneous map on x86_64 System V.
fn emit_top_map_x86_64(
    ctx: &mut FunctionContext<'_>,
    mixed_keys: bool,
    materialize: &str,
) {
    let capacity_ready = ctx.next_label("dom_result_tree_top_capacity");
    let loop_head = ctx.next_label("dom_result_tree_top_loop");
    let loop_done = ctx.next_label("dom_result_tree_top_done");
    let integer_key = ctx.next_label("dom_result_tree_top_integer_key");
    let string_key = ctx.next_label("dom_result_tree_top_string_key");
    let key_ready = ctx.next_label("dom_result_tree_top_key_ready");
    let failure = ctx.next_label("dom_result_tree_top_failure");
    let complete = ctx.next_label("dom_result_tree_top_complete");

    ctx.emitter.instruction("cmp QWORD PTR [rsp + 80], 0");                     // must top-level map children begin at flat record zero?
    ctx.emitter
        .instruction(&format!("jne {}", failure));                             // reject a shifted top-level direct range
    ctx.emitter.instruction("mov r10, QWORD PTR [rsp + 88]");                   // load the top-level map entry count
    ctx.emitter.instruction("mov r11, r10");                                    // preserve the entry count while scaling it
    ctx.emitter.instruction("shl r11, 1");                                      // compute the number of direct key/value records
    ctx.emitter
        .instruction(&format!("jc {}", failure));                              // reject an overflowing direct-child count
    ctx.emitter.instruction("cmp r11, QWORD PTR [rsp + 120]");                  // do all direct children fit before descendants?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject a truncated top-level map range
    ctx.emitter.instruction("mov rdi, r11");                                    // use twice the entry count as target hash capacity
    ctx.emitter.instruction("cmp rdi, 16");                                     // enforce the runtime minimum hash capacity
    ctx.emitter
        .instruction(&format!("jae {}", capacity_ready));                      // retain a sufficiently large computed capacity
    ctx.emitter.instruction("mov rdi, 16");                                     // raise small maps to the minimum capacity
    ctx.emitter.label(&capacity_ready);
    ctx.emitter.instruction("mov rsi, 7");                                      // heterogeneous maps store owned boxed Mixed cells
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    abi::emit_store_to_sp(ctx.emitter, "rax", ARRAY_RESULT_OFFSET);
    super::store_stack_immediate(ctx, ARRAY_INDEX_OFFSET, 0);
    super::stage_result_word(ctx, 88, ARRAY_COUNT_OFFSET);

    ctx.emitter.label(&loop_head);
    ctx.emitter
        .instruction(&format!("mov r9, QWORD PTR [rsp + {}]", ARRAY_INDEX_OFFSET)); // load the next top-level entry index
    ctx.emitter
        .instruction(&format!("cmp r9, QWORD PTR [rsp + {}]", ARRAY_COUNT_OFFSET)); // have all top-level entries been materialized?
    ctx.emitter
        .instruction(&format!("jae {}", loop_done));                           // finish after the final entry
    ctx.emitter.instruction("shl r9, 1");                                       // map the entry index to its flat key record
    ctx.emitter.instruction("mov r11, QWORD PTR [rsp + 112]");                  // load the retained flat value-vector pointer
    ctx.emitter.instruction("test r11, r11");                                   // is the non-empty direct range backed by storage?
    ctx.emitter
        .instruction(&format!("jz {}", failure));                              // reject a null flat value-vector pointer
    ctx.emitter.instruction("lea r10, [r9 + r9 * 2]");                          // scale the key index by three ABI words
    ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                        // address the exact 24-byte key record
    if mixed_keys {
        ctx.emitter.instruction("cmp DWORD PTR [r11], 2");                      // is this an integer PHP array key?
        ctx.emitter
            .instruction(&format!("je {}", integer_key));                      // retain the signed integer payload exactly
    }
    ctx.emitter.instruction("cmp DWORD PTR [r11], 4");                          // is this a byte-string PHP array key?
    ctx.emitter
        .instruction(&format!("je {}", string_key));                           // validate and normalize the string key
    ctx.emitter
        .instruction(&format!("jmp {}", failure));                             // reject every unsupported key tag

    ctx.emitter.label(&integer_key);
    ctx.emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                    // load the signed integer key bits
    ctx.emitter.instruction("mov rdx, -1");                                     // mark the key as integer for the hash ABI
    ctx.emitter
        .instruction(&format!("jmp {}", key_ready));                           // skip byte-range validation for integer keys

    ctx.emitter.label(&string_key);
    ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                    // load the borrowed result-byte offset
    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                   // load the exact key byte length
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 104]");                  // load the retained result-byte length
    ctx.emitter.instruction("cmp r10, rcx");                                    // is the key byte offset in bounds?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject an out-of-range key offset
    ctx.emitter.instruction("sub rcx, r10");                                    // compute the bytes remaining after the offset
    ctx.emitter.instruction("cmp rdx, rcx");                                    // does the complete key fit in retained bytes?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject a truncated byte-string key
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 96]");                   // load the retained result-byte pointer
    ctx.emitter.instruction("add rax, r10");                                    // address the borrowed key bytes
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
    ctx.emitter.instruction("mov rsi, rax");                                    // move the normalized key low word into the hash ABI register

    ctx.emitter.label(&key_ready);
    abi::emit_store_to_sp(ctx.emitter, "rsi", OBJECT_FIELDS_OFFSET);
    abi::emit_store_to_sp(ctx.emitter, "rdx", OBJECT_FIELD_COUNT_OFFSET);
    ctx.emitter
        .instruction(&format!("mov rdi, QWORD PTR [rsp + {}]", ARRAY_INDEX_OFFSET)); // reload the entry index for its value record
    ctx.emitter.instruction("shl rdi, 1");                                      // map the entry index to its flat key record
    ctx.emitter.instruction("add rdi, 1");                                      // select the value immediately after the key
    ctx.emitter.instruction("mov rsi, rsp");                                    // pass the stable outer result frame to recursion
    ctx.emitter.instruction("xor rdx, rdx");                                    // start bounded recursion at depth zero
    abi::emit_call_label(ctx.emitter, materialize);
    abi::emit_store_to_sp(ctx.emitter, "rax", ARRAY_VALUE_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdi", ARRAY_RESULT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rsi", OBJECT_FIELDS_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rdx", OBJECT_FIELD_COUNT_OFFSET);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rcx", ARRAY_VALUE_OFFSET);
    ctx.emitter.instruction("xor r8, r8");                                      // boxed Mixed hash values use one payload word
    ctx.emitter.instruction("mov r9, 7");                                       // runtime value tag seven identifies boxed Mixed
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    abi::emit_store_to_sp(ctx.emitter, "rax", ARRAY_RESULT_OFFSET);
    ctx.emitter
        .instruction(&format!("add QWORD PTR [rsp + {}], 1", ARRAY_INDEX_OFFSET)); // advance to the next top-level key/value pair
    abi::emit_jump(ctx.emitter, &loop_head);

    ctx.emitter.label(&loop_done);
    abi::emit_load_temporary_stack_slot(ctx.emitter, "rax", ARRAY_RESULT_OFFSET);
    abi::emit_jump(ctx.emitter, &complete);
    ctx.emitter.label(&failure);
    emit_bridge_failure_jump(ctx);
    ctx.emitter.label(&complete);
}

/// Emits the recursive flat-record materializer for AArch64.
fn emit_value_helper_aarch64(
    ctx: &mut FunctionContext<'_>,
    entry: &str,
    wrapper: &str,
    value_object: &str,
) {
    let null = ctx.next_label("dom_result_tree_null");
    let boolean = ctx.next_label("dom_result_tree_bool");
    let integer = ctx.next_label("dom_result_tree_int");
    let float = ctx.next_label("dom_result_tree_float");
    let bytes = ctx.next_label("dom_result_tree_bytes");
    let array = ctx.next_label("dom_result_tree_array");
    let map = ctx.next_label("dom_result_tree_map");
    let bridge_handle = ctx.next_label("dom_result_tree_bridge_handle");
    let callable = ctx.next_label("dom_result_tree_callable");
    let object = ctx.next_label("dom_result_tree_object");
    let array_capacity = ctx.next_label("dom_result_tree_array_capacity");
    let array_loop = ctx.next_label("dom_result_tree_array_loop");
    let array_done = ctx.next_label("dom_result_tree_array_done");
    let map_capacity = ctx.next_label("dom_result_tree_map_capacity");
    let map_loop = ctx.next_label("dom_result_tree_map_loop");
    let map_done = ctx.next_label("dom_result_tree_map_done");
    let map_integer_key = ctx.next_label("dom_result_tree_map_integer_key");
    let map_string_key = ctx.next_label("dom_result_tree_map_string_key");
    let map_key_ready = ctx.next_label("dom_result_tree_map_key_ready");
    let epilogue = ctx.next_label("dom_result_tree_epilogue");
    let failure = ctx.next_label("dom_result_tree_failure");

    ctx.emitter
        .instruction(&format!("sub sp, sp, #{}", MATERIALIZER_FRAME_SIZE));     // reserve recursive materializer locals and a saved return address
    ctx.emitter.instruction("stp x29, x30, [sp, #80]");                         // preserve the caller frame and recursive return address
    ctx.emitter.instruction("add x29, sp, #80");                                // establish the materializer frame pointer
    ctx.emitter.instruction("str x0, [sp]");                                    // retain the selected flat record index
    ctx.emitter.instruction("str x1, [sp, #8]");                                // retain the stable outer bridge frame pointer
    ctx.emitter.instruction("str x2, [sp, #16]");                               // retain the current recursion depth
    ctx.emitter
        .instruction(&format!("cmp x2, #{}", MATERIALIZER_MAX_DEPTH));         // has malformed input exceeded the bounded recursion depth?
    ctx.emitter
        .instruction(&format!("b.hs {}", failure));                            // contain cycles or pathologically deep native result trees
    ctx.emitter.instruction("ldr x9, [x1, #120]");                              // load the complete retained flat value count
    ctx.emitter.instruction("cmp x0, x9");                                      // is the selected record index in bounds?
    ctx.emitter
        .instruction(&format!("b.hs {}", failure));                            // reject an out-of-range descendant reference
    ctx.emitter.instruction("ldr x10, [x1, #112]");                             // load the retained flat value-vector pointer
    ctx.emitter
        .instruction(&format!("cbz x10, {}", failure));                        // an addressable record requires backing storage
    ctx.emitter.instruction("add x11, x0, x0, lsl #1");                         // scale the record index by three ABI words
    ctx.emitter.instruction("add x11, x10, x11, lsl #3");                       // address the exact 24-byte flat record
    ctx.emitter.instruction("str x11, [sp, #24]");                              // retain the record pointer across allocations and recursion
    ctx.emitter.instruction("ldr w9, [x11]");                                   // load the flat ABI value tag
    for (tag, label) in [
        (0, &null),
        (1, &boolean),
        (2, &integer),
        (3, &float),
        (4, &bytes),
        (5, &array),
        (6, &map),
        (8, &bridge_handle),
        (9, &callable),
        (11, &object),
    ] {
        ctx.emitter
            .instruction(&format!("cmp w9, #{}", tag));                       // compare one supported recursive result tag
        ctx.emitter
            .instruction(&format!("b.eq {}", label));                         // dispatch to the matching owned materializer
    }
    ctx.emitter
        .instruction(&format!("b {}", failure));                               // reject unsupported resource descriptors

    ctx.emitter.label(&null);
    abi::emit_load_int_immediate(ctx.emitter, "x0", crate::codegen::NULL_SENTINEL);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&boolean);
    ctx.emitter.instruction("ldr x0, [x11, #8]");                               // load the exact native boolean payload
    ctx.emitter.instruction("cmp x0, #1");                                      // is the boolean payload canonical?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject non-zero/non-one boolean payloads
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&integer);
    ctx.emitter.instruction("ldr x0, [x11, #8]");                               // load the exact signed integer bits
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&float);
    ctx.emitter.instruction("ldr x9, [x11, #8]");                               // load the exact IEEE-754 result bits
    ctx.emitter.instruction("fmov d0, x9");                                     // move the bits into the floating-point result register
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&bytes);
    ctx.emitter.instruction("ldr x10, [x11, #8]");                              // load the borrowed result-byte offset
    ctx.emitter.instruction("ldr x2, [x11, #16]");                              // load the exact byte length
    ctx.emitter.instruction("ldr x12, [x1, #104]");                             // load the retained result-byte length
    ctx.emitter.instruction("cmp x10, x12");                                    // is the byte offset in bounds?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject an out-of-range string offset
    ctx.emitter.instruction("sub x12, x12, x10");                               // compute the bytes remaining after the offset
    ctx.emitter.instruction("cmp x2, x12");                                     // does the complete string fit in retained bytes?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject a truncated string payload
    ctx.emitter.instruction("ldr x1, [x1, #96]");                               // load the retained result-byte pointer
    ctx.emitter.instruction("add x1, x1, x10");                                 // address the borrowed string bytes
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&bridge_handle);
    ctx.emitter.instruction("ldr x0, [sp, #8]");                                // pass the stable outer bridge frame to wrapper materialization
    ctx.emitter.instruction("ldr x1, [x11, #8]");                               // pass the fresh native bridge handle
    ctx.emitter.instruction("ldr x2, [x11, #16]");                              // pass the registered wrapper discriminator
    abi::emit_call_label(ctx.emitter, wrapper);
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Object("SimpleXMLElement".to_string()),
    );
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&callable);
    ctx.emitter.instruction("ldr x0, [x11, #8]");                               // load the native callable descriptor
    ctx.emitter
        .instruction(&format!("cbz x0, {}", failure));                         // reject a null callable descriptor
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Callable);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&object);
    ctx.emitter.instruction("ldr x0, [sp, #8]");                                // pass the stable outer bridge frame to copied-object materialization
    ctx.emitter.instruction("ldr x11, [sp, #24]");                              // reload the copied value-object descriptor after tag dispatch
    ctx.emitter.instruction("ldr w1, [x11, #4]");                               // pass the validated copied value-object subtype
    ctx.emitter.instruction("ldr x2, [x11, #8]");                               // pass the direct field-range start
    ctx.emitter.instruction("ldr x3, [x11, #16]");                              // pass the exact direct field count
    abi::emit_call_label(ctx.emitter, value_object);
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Object("LibXMLError".to_string()),
    );
    abi::emit_jump(ctx.emitter, &epilogue);

    emit_array_value_aarch64(
        ctx,
        entry,
        &array,
        &array_capacity,
        &array_loop,
        &array_done,
        &epilogue,
        &failure,
    );
    emit_map_value_aarch64(
        ctx,
        entry,
        &map,
        &map_capacity,
        &map_loop,
        &map_done,
        &map_integer_key,
        &map_string_key,
        &map_key_ready,
        &epilogue,
        &failure,
    );

    ctx.emitter.label(&epilogue);
    ctx.emitter.instruction("ldp x29, x30, [sp, #80]");                         // restore the caller frame and recursive return address
    ctx.emitter
        .instruction(&format!("add sp, sp, #{}", MATERIALIZER_FRAME_SIZE));    // release recursive materializer locals
    ctx.emitter.instruction("ret");                                             // return one owned boxed Mixed value
    ctx.emitter.label(&failure);
    emit_bridge_failure_jump(ctx);
}

/// Emits recursive indexed-array materialization inside the AArch64 helper.
#[allow(clippy::too_many_arguments)]
fn emit_array_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    entry: &str,
    array: &str,
    capacity_ready: &str,
    loop_head: &str,
    loop_done: &str,
    epilogue: &str,
    failure: &str,
) {
    ctx.emitter.label(array);
    ctx.emitter.instruction("ldr x11, [sp, #24]");                              // reload the indexed-array flat record
    ctx.emitter.instruction("ldr x9, [x11, #8]");                               // load the direct-child range start
    ctx.emitter.instruction("ldr x10, [x11, #16]");                             // load the direct-child count
    ctx.emitter.instruction("ldr x12, [sp, #8]");                               // reload the stable outer result frame
    ctx.emitter.instruction("ldr x13, [x12, #120]");                            // load the complete retained flat value count
    ctx.emitter.instruction("cmp x9, x13");                                     // is the direct-child start in bounds?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject an out-of-range indexed-array start
    ctx.emitter.instruction("sub x13, x13, x9");                                // compute the records remaining after the start
    ctx.emitter.instruction("cmp x10, x13");                                    // do all indexed children fit in the retained vector?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject a truncated indexed-array range
    ctx.emitter.instruction("str x9, [sp, #48]");                               // retain the direct-child start across recursion
    ctx.emitter.instruction("str x10, [sp, #56]");                              // retain the direct-child count across recursion
    ctx.emitter.instruction("mov x0, x10");                                     // use the exact child count as requested capacity
    ctx.emitter.instruction("cmp x0, #4");                                      // enforce the runtime minimum indexed capacity
    ctx.emitter
        .instruction(&format!("b.hs {}", capacity_ready));                     // retain a sufficiently large child count
    ctx.emitter.instruction("mov x0, #4");                                      // raise small arrays to the minimum capacity
    ctx.emitter.label(capacity_ready);
    ctx.emitter.instruction("mov x1, #8");                                      // boxed Mixed arrays store one heap pointer per slot
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, "x0", &PhpType::Mixed);
    ctx.emitter.instruction("str x0, [sp, #32]");                               // retain the owned destination array across child recursion
    ctx.emitter.instruction("str xzr, [sp, #40]");                              // initialize the direct-child loop index

    ctx.emitter.label(loop_head);
    ctx.emitter.instruction("ldr x9, [sp, #40]");                               // load the next direct-child offset
    ctx.emitter.instruction("ldr x10, [sp, #56]");                              // load the indexed-array child count
    ctx.emitter.instruction("cmp x9, x10");                                     // have all indexed children been materialized?
    ctx.emitter
        .instruction(&format!("b.hs {}", loop_done));                          // finish after the final indexed child
    ctx.emitter.instruction("ldr x0, [sp, #48]");                               // load the direct-child range start
    ctx.emitter.instruction("add x0, x0, x9");                                  // select the next child's flat record
    ctx.emitter.instruction("ldr x1, [sp, #8]");                                // pass the stable outer result frame to recursion
    ctx.emitter.instruction("ldr x2, [sp, #16]");                               // load the current recursion depth
    ctx.emitter.instruction("add x2, x2, #1");                                  // descend exactly one container level
    abi::emit_call_label(ctx.emitter, entry);
    ctx.emitter.instruction("str x0, [sp, #72]");                               // retain the owned boxed child across append
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // pass the destination indexed array to append
    ctx.emitter.instruction("ldr x1, [sp, #72]");                               // pass the borrowed boxed child to refcounted append
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    ctx.emitter.instruction("str x0, [sp, #32]");                               // retain the possibly grown destination array
    ctx.emitter.instruction("ldr x0, [sp, #72]");                               // reload the recursive owner's boxed child
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction("ldr x9, [sp, #40]");                               // reload the completed direct-child offset
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance to the next indexed child
    ctx.emitter.instruction("str x9, [sp, #40]");                               // persist the next direct-child offset
    abi::emit_jump(ctx.emitter, loop_head);

    ctx.emitter.label(loop_done);
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // restore the completed owned indexed array
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Array(Box::new(PhpType::Mixed)),
    );
    abi::emit_jump(ctx.emitter, epilogue);
}

/// Emits recursive associative-map materialization inside the AArch64 helper.
#[allow(clippy::too_many_arguments)]
fn emit_map_value_aarch64(
    ctx: &mut FunctionContext<'_>,
    entry: &str,
    map: &str,
    capacity_ready: &str,
    loop_head: &str,
    loop_done: &str,
    integer_key: &str,
    string_key: &str,
    key_ready: &str,
    epilogue: &str,
    failure: &str,
) {
    ctx.emitter.label(map);
    ctx.emitter.instruction("ldr x11, [sp, #24]");                              // reload the associative-map flat record
    ctx.emitter.instruction("ldr x9, [x11, #8]");                               // load the direct key/value range start
    ctx.emitter.instruction("ldr x10, [x11, #16]");                             // load the map entry count
    ctx.emitter.instruction("adds x12, x10, x10");                              // compute the direct alternating-record count
    ctx.emitter
        .instruction(&format!("b.cs {}", failure));                            // reject an overflowing pair count
    ctx.emitter.instruction("ldr x13, [sp, #8]");                               // reload the stable outer result frame
    ctx.emitter.instruction("ldr x13, [x13, #120]");                            // load the complete retained flat value count
    ctx.emitter.instruction("cmp x9, x13");                                     // is the direct key/value start in bounds?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject an out-of-range map start
    ctx.emitter.instruction("sub x13, x13, x9");                                // compute the records remaining after the start
    ctx.emitter.instruction("cmp x12, x13");                                    // do all direct pairs fit in the retained vector?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject a truncated associative-map range
    ctx.emitter.instruction("str x9, [sp, #48]");                               // retain the direct pair start across recursion
    ctx.emitter.instruction("str x10, [sp, #56]");                              // retain the entry count across recursion
    ctx.emitter.instruction("mov x0, x12");                                     // use twice the entry count as requested hash capacity
    ctx.emitter.instruction("cmp x0, #16");                                     // enforce the runtime minimum hash capacity
    ctx.emitter
        .instruction(&format!("b.hs {}", capacity_ready));                     // retain a sufficiently large computed capacity
    ctx.emitter.instruction("mov x0, #16");                                     // raise small maps to the minimum capacity
    ctx.emitter.label(capacity_ready);
    ctx.emitter.instruction("mov x1, #7");                                      // nested maps store owned boxed Mixed cells
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    ctx.emitter.instruction("str x0, [sp, #32]");                               // retain the owned destination hash across recursion
    ctx.emitter.instruction("str xzr, [sp, #40]");                              // initialize the map entry loop index

    ctx.emitter.label(loop_head);
    ctx.emitter.instruction("ldr x9, [sp, #40]");                               // load the next nested map entry index
    ctx.emitter.instruction("ldr x10, [sp, #56]");                              // load the nested map entry count
    ctx.emitter.instruction("cmp x9, x10");                                     // have all nested pairs been materialized?
    ctx.emitter
        .instruction(&format!("b.hs {}", loop_done));                          // finish after the final nested pair
    ctx.emitter.instruction("ldr x10, [sp, #48]");                              // load the direct pair range start
    ctx.emitter.instruction("add x9, x10, x9, lsl #1");                         // select the current key record
    ctx.emitter.instruction("str x9, [sp]");                                    // retain the key record index across key normalization
    ctx.emitter.instruction("ldr x11, [sp, #8]");                               // reload the stable outer result frame
    ctx.emitter.instruction("ldr x11, [x11, #112]");                            // load the retained flat value-vector pointer
    ctx.emitter.instruction("add x10, x9, x9, lsl #1");                         // scale the key index by three ABI words
    ctx.emitter.instruction("add x11, x11, x10, lsl #3");                       // address the exact 24-byte key record
    ctx.emitter.instruction("ldr w10, [x11]");                                  // load the nested map key tag
    ctx.emitter.instruction("cmp w10, #2");                                     // is this an integer PHP array key?
    ctx.emitter
        .instruction(&format!("b.eq {}", integer_key));                        // preserve the exact signed integer payload
    ctx.emitter.instruction("cmp w10, #4");                                     // is this a byte-string PHP array key?
    ctx.emitter
        .instruction(&format!("b.eq {}", string_key));                         // validate and normalize the byte-string key
    ctx.emitter
        .instruction(&format!("b {}", failure));                               // reject every unsupported nested map key tag

    ctx.emitter.label(integer_key);
    ctx.emitter.instruction("ldr x1, [x11, #8]");                               // load the signed integer key bits
    ctx.emitter.instruction("mov x2, #-1");                                     // mark the key as integer for the hash ABI
    ctx.emitter
        .instruction(&format!("b {}", key_ready));                             // skip byte-range validation for integer keys

    ctx.emitter.label(string_key);
    ctx.emitter.instruction("ldr x10, [x11, #8]");                              // load the borrowed result-byte offset
    ctx.emitter.instruction("ldr x2, [x11, #16]");                              // load the exact key byte length
    ctx.emitter.instruction("ldr x12, [sp, #8]");                               // reload the stable outer result frame
    ctx.emitter.instruction("ldr x13, [x12, #104]");                            // load the retained result-byte length
    ctx.emitter.instruction("cmp x10, x13");                                    // is the nested key byte offset in bounds?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject an out-of-range nested key offset
    ctx.emitter.instruction("sub x13, x13, x10");                               // compute the bytes remaining after the offset
    ctx.emitter.instruction("cmp x2, x13");                                     // does the complete nested key fit in retained bytes?
    ctx.emitter
        .instruction(&format!("b.hi {}", failure));                            // reject a truncated nested byte-string key
    ctx.emitter.instruction("ldr x1, [x12, #96]");                              // load the retained result-byte pointer
    ctx.emitter.instruction("add x1, x1, x10");                                 // address the borrowed nested key bytes
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");

    ctx.emitter.label(key_ready);
    ctx.emitter.instruction("str x1, [sp, #64]");                               // retain the normalized key low word across recursion
    ctx.emitter.instruction("str x2, [sp, #72]");                               // retain the normalized key high word across recursion
    ctx.emitter.instruction("ldr x9, [sp]");                                    // reload the key record index after normalization
    ctx.emitter.instruction("add x0, x9, #1");                                  // select the value record immediately after the key
    ctx.emitter.instruction("ldr x1, [sp, #8]");                                // pass the stable outer result frame to recursion
    ctx.emitter.instruction("ldr x2, [sp, #16]");                               // load the current recursion depth
    ctx.emitter.instruction("add x2, x2, #1");                                  // descend exactly one container level
    abi::emit_call_label(ctx.emitter, entry);
    ctx.emitter.instruction("mov x3, x0");                                      // transfer the owned boxed child to hash insertion
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // reload the destination associative map
    ctx.emitter.instruction("ldr x1, [sp, #64]");                               // reload the normalized key low word
    ctx.emitter.instruction("ldr x2, [sp, #72]");                               // reload the normalized key high word
    ctx.emitter.instruction("mov x4, xzr");                                     // boxed Mixed hash values use one payload word
    ctx.emitter.instruction("mov x5, #7");                                      // runtime value tag seven identifies boxed Mixed
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    ctx.emitter.instruction("str x0, [sp, #32]");                               // retain the possibly grown destination hash
    ctx.emitter.instruction("ldr x9, [sp, #40]");                               // reload the completed nested map entry index
    ctx.emitter.instruction("add x9, x9, #1");                                  // advance to the next nested key/value pair
    ctx.emitter.instruction("str x9, [sp, #40]");                               // persist the next nested map entry index
    abi::emit_jump(ctx.emitter, loop_head);

    ctx.emitter.label(loop_done);
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // restore the completed owned associative map
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
    abi::emit_jump(ctx.emitter, epilogue);
}

/// Emits the recursive flat-record materializer for x86_64 System V.
fn emit_value_helper_x86_64(
    ctx: &mut FunctionContext<'_>,
    entry: &str,
    wrapper: &str,
    value_object: &str,
) {
    let null = ctx.next_label("dom_result_tree_null");
    let boolean = ctx.next_label("dom_result_tree_bool");
    let integer = ctx.next_label("dom_result_tree_int");
    let float = ctx.next_label("dom_result_tree_float");
    let bytes = ctx.next_label("dom_result_tree_bytes");
    let array = ctx.next_label("dom_result_tree_array");
    let map = ctx.next_label("dom_result_tree_map");
    let bridge_handle = ctx.next_label("dom_result_tree_bridge_handle");
    let callable = ctx.next_label("dom_result_tree_callable");
    let object = ctx.next_label("dom_result_tree_object");
    let array_capacity = ctx.next_label("dom_result_tree_array_capacity");
    let array_loop = ctx.next_label("dom_result_tree_array_loop");
    let array_done = ctx.next_label("dom_result_tree_array_done");
    let map_capacity = ctx.next_label("dom_result_tree_map_capacity");
    let map_loop = ctx.next_label("dom_result_tree_map_loop");
    let map_done = ctx.next_label("dom_result_tree_map_done");
    let map_integer_key = ctx.next_label("dom_result_tree_map_integer_key");
    let map_string_key = ctx.next_label("dom_result_tree_map_string_key");
    let map_key_ready = ctx.next_label("dom_result_tree_map_key_ready");
    let epilogue = ctx.next_label("dom_result_tree_epilogue");
    let failure = ctx.next_label("dom_result_tree_failure");

    ctx.emitter.instruction("push rbp");                                        // preserve the caller frame pointer before recursive materialization
    ctx.emitter.instruction("mov rbp, rsp");                                    // establish a stable local frame base
    ctx.emitter
        .instruction(&format!("sub rsp, {}", MATERIALIZER_FRAME_SIZE));        // reserve recursive materializer locals with call alignment
    ctx.emitter.instruction("mov QWORD PTR [rbp - 88], r12");                   // preserve the first System V callee-saved scratch register
    ctx.emitter.instruction("mov QWORD PTR [rbp - 96], r13");                   // preserve the second System V callee-saved scratch register
    ctx.emitter.instruction("mov QWORD PTR [rbp - 8], rdi");                    // retain the selected flat record index
    ctx.emitter.instruction("mov QWORD PTR [rbp - 16], rsi");                   // retain the stable outer bridge frame pointer
    ctx.emitter.instruction("mov QWORD PTR [rbp - 24], rdx");                   // retain the current recursion depth
    ctx.emitter
        .instruction(&format!("cmp rdx, {}", MATERIALIZER_MAX_DEPTH));         // has malformed input exceeded the bounded recursion depth?
    ctx.emitter
        .instruction(&format!("jae {}", failure));                             // contain cycles or pathologically deep native result trees
    ctx.emitter.instruction("mov r9, QWORD PTR [rsi + 120]");                   // load the complete retained flat value count
    ctx.emitter.instruction("cmp rdi, r9");                                     // is the selected record index in bounds?
    ctx.emitter
        .instruction(&format!("jae {}", failure));                             // reject an out-of-range descendant reference
    ctx.emitter.instruction("mov r10, QWORD PTR [rsi + 112]");                  // load the retained flat value-vector pointer
    ctx.emitter.instruction("test r10, r10");                                   // is the addressable vector backed by storage?
    ctx.emitter
        .instruction(&format!("jz {}", failure));                              // reject a null value-vector pointer
    ctx.emitter.instruction("lea r11, [rdi + rdi * 2]");                        // scale the record index by three ABI words
    ctx.emitter.instruction("lea r11, [r10 + r11 * 8]");                        // address the exact 24-byte flat record
    ctx.emitter.instruction("mov QWORD PTR [rbp - 32], r11");                   // retain the record pointer across allocations and recursion
    for (tag, label) in [
        (0, &null),
        (1, &boolean),
        (2, &integer),
        (3, &float),
        (4, &bytes),
        (5, &array),
        (6, &map),
        (8, &bridge_handle),
        (9, &callable),
        (11, &object),
    ] {
        ctx.emitter
            .instruction(&format!("cmp DWORD PTR [r11], {}", tag));            // compare one supported recursive result tag
        ctx.emitter
            .instruction(&format!("je {}", label));                            // dispatch to the matching owned materializer
    }
    ctx.emitter
        .instruction(&format!("jmp {}", failure));                             // reject unsupported resource descriptors

    ctx.emitter.label(&null);
    abi::emit_load_int_immediate(ctx.emitter, "rax", crate::codegen::NULL_SENTINEL);
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Void);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&boolean);
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + 8]");                    // load the exact native boolean payload
    ctx.emitter.instruction("cmp rax, 1");                                      // is the boolean payload canonical?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject non-zero/non-one boolean payloads
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&integer);
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + 8]");                    // load the exact signed integer bits
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Int);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&float);
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + 8]");                    // load the exact IEEE-754 result bits
    ctx.emitter.instruction("movq xmm0, rax");                                  // restore the bits into the floating-point result register
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Float);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&bytes);
    ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                    // load the borrowed result-byte offset
    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                   // load the exact byte length
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsi + 104]");                  // load the retained result-byte length
    ctx.emitter.instruction("cmp r10, rcx");                                    // is the byte offset in bounds?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject an out-of-range string offset
    ctx.emitter.instruction("sub rcx, r10");                                    // compute the bytes remaining after the offset
    ctx.emitter.instruction("cmp rdx, rcx");                                    // does the complete string fit in retained bytes?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject a truncated string payload
    ctx.emitter.instruction("mov rax, QWORD PTR [rsi + 96]");                   // load the retained result-byte pointer
    ctx.emitter.instruction("add rax, r10");                                    // address the borrowed string bytes
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    emit_box_current_owned_value_as_mixed(ctx.emitter, &PhpType::Str);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&bridge_handle);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // pass the stable outer bridge frame to wrapper materialization
    ctx.emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                    // pass the fresh native bridge handle
    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                   // pass the registered wrapper discriminator
    abi::emit_call_label(ctx.emitter, wrapper);
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Object("SimpleXMLElement".to_string()),
    );
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&callable);
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + 8]");                    // load the native callable descriptor
    ctx.emitter.instruction("test rax, rax");                                   // is the callable descriptor valid?
    ctx.emitter
        .instruction(&format!("jz {}", failure));                              // reject a null callable descriptor
    emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Callable);
    abi::emit_jump(ctx.emitter, &epilogue);

    ctx.emitter.label(&object);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rbp - 16]");                   // pass the stable outer bridge frame to copied-object materialization
    ctx.emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                   // reload the copied value-object descriptor after tag dispatch
    ctx.emitter.instruction("mov esi, DWORD PTR [r11 + 4]");                    // pass the validated copied value-object subtype
    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 8]");                    // pass the direct field-range start
    ctx.emitter.instruction("mov rcx, QWORD PTR [r11 + 16]");                   // pass the exact direct field count
    abi::emit_call_label(ctx.emitter, value_object);
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Object("LibXMLError".to_string()),
    );
    abi::emit_jump(ctx.emitter, &epilogue);

    emit_array_value_x86_64(
        ctx,
        entry,
        &array,
        &array_capacity,
        &array_loop,
        &array_done,
        &epilogue,
        &failure,
    );
    emit_map_value_x86_64(
        ctx,
        entry,
        &map,
        &map_capacity,
        &map_loop,
        &map_done,
        &map_integer_key,
        &map_string_key,
        &map_key_ready,
        &epilogue,
        &failure,
    );

    ctx.emitter.label(&epilogue);
    ctx.emitter.instruction("mov r12, QWORD PTR [rbp - 88]");                   // restore the first System V callee-saved scratch register
    ctx.emitter.instruction("mov r13, QWORD PTR [rbp - 96]");                   // restore the second System V callee-saved scratch register
    ctx.emitter
        .instruction(&format!("add rsp, {}", MATERIALIZER_FRAME_SIZE));        // release recursive materializer locals
    ctx.emitter.instruction("pop rbp");                                         // restore the caller frame pointer
    ctx.emitter.instruction("ret");                                             // return one owned boxed Mixed value
    ctx.emitter.label(&failure);
    emit_bridge_failure_jump(ctx);
}

/// Emits recursive indexed-array materialization inside the x86_64 helper.
#[allow(clippy::too_many_arguments)]
fn emit_array_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    entry: &str,
    array: &str,
    capacity_ready: &str,
    loop_head: &str,
    loop_done: &str,
    epilogue: &str,
    failure: &str,
) {
    ctx.emitter.label(array);
    ctx.emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                   // reload the indexed-array flat record
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                     // load the direct-child range start
    ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                   // load the direct-child count
    ctx.emitter.instruction("mov r12, QWORD PTR [rbp - 16]");                   // reload the stable outer result frame
    ctx.emitter.instruction("mov r13, QWORD PTR [r12 + 120]");                  // load the complete retained flat value count
    ctx.emitter.instruction("cmp r9, r13");                                     // is the direct-child start in bounds?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject an out-of-range indexed-array start
    ctx.emitter.instruction("sub r13, r9");                                     // compute the records remaining after the start
    ctx.emitter.instruction("cmp r10, r13");                                    // do all indexed children fit in the retained vector?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject a truncated indexed-array range
    ctx.emitter.instruction("mov QWORD PTR [rbp - 56], r9");                    // retain the direct-child start across recursion
    ctx.emitter.instruction("mov QWORD PTR [rbp - 64], r10");                   // retain the direct-child count across recursion
    ctx.emitter.instruction("mov rdi, r10");                                    // use the exact child count as requested capacity
    ctx.emitter.instruction("cmp rdi, 4");                                      // enforce the runtime minimum indexed capacity
    ctx.emitter
        .instruction(&format!("jae {}", capacity_ready));                      // retain a sufficiently large child count
    ctx.emitter.instruction("mov rdi, 4");                                      // raise small arrays to the minimum capacity
    ctx.emitter.label(capacity_ready);
    ctx.emitter.instruction("mov rsi, 8");                                      // boxed Mixed arrays store one heap pointer per slot
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    crate::codegen::emit_array_value_type_stamp(ctx.emitter, "rax", &PhpType::Mixed);
    ctx.emitter.instruction("mov QWORD PTR [rbp - 40], rax");                   // retain the owned destination array across child recursion
    ctx.emitter.instruction("mov QWORD PTR [rbp - 48], 0");                     // initialize the direct-child loop index

    ctx.emitter.label(loop_head);
    ctx.emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                    // load the next direct-child offset
    ctx.emitter.instruction("cmp r9, QWORD PTR [rbp - 64]");                    // have all indexed children been materialized?
    ctx.emitter
        .instruction(&format!("jae {}", loop_done));                           // finish after the final indexed child
    ctx.emitter.instruction("mov rdi, QWORD PTR [rbp - 56]");                   // load the direct-child range start
    ctx.emitter.instruction("add rdi, r9");                                     // select the next child's flat record
    ctx.emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                   // pass the stable outer result frame to recursion
    ctx.emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                   // load the current recursion depth
    ctx.emitter.instruction("add rdx, 1");                                      // descend exactly one container level
    abi::emit_call_label(ctx.emitter, entry);
    ctx.emitter.instruction("mov QWORD PTR [rbp - 80], rax");                   // retain the owned boxed child across append
    ctx.emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                   // pass the destination indexed array to append
    ctx.emitter.instruction("mov rsi, QWORD PTR [rbp - 80]");                   // pass the borrowed boxed child to refcounted append
    abi::emit_call_label(ctx.emitter, "__rt_array_push_refcounted");
    ctx.emitter.instruction("mov QWORD PTR [rbp - 40], rax");                   // retain the possibly grown destination array
    ctx.emitter.instruction("mov rax, QWORD PTR [rbp - 80]");                   // reload the recursive owner's boxed child
    abi::emit_call_label(ctx.emitter, "__rt_decref_mixed");
    ctx.emitter.instruction("add QWORD PTR [rbp - 48], 1");                     // advance to the next indexed child
    abi::emit_jump(ctx.emitter, loop_head);

    ctx.emitter.label(loop_done);
    ctx.emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // restore the completed owned indexed array
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::Array(Box::new(PhpType::Mixed)),
    );
    abi::emit_jump(ctx.emitter, epilogue);
}

/// Emits recursive associative-map materialization inside the x86_64 helper.
#[allow(clippy::too_many_arguments)]
fn emit_map_value_x86_64(
    ctx: &mut FunctionContext<'_>,
    entry: &str,
    map: &str,
    capacity_ready: &str,
    loop_head: &str,
    loop_done: &str,
    integer_key: &str,
    string_key: &str,
    key_ready: &str,
    epilogue: &str,
    failure: &str,
) {
    ctx.emitter.label(map);
    ctx.emitter.instruction("mov r11, QWORD PTR [rbp - 32]");                   // reload the associative-map flat record
    ctx.emitter.instruction("mov r9, QWORD PTR [r11 + 8]");                     // load the direct key/value range start
    ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 16]");                   // load the map entry count
    ctx.emitter.instruction("mov r12, r10");                                    // preserve the entry count while scaling it
    ctx.emitter.instruction("shl r12, 1");                                      // compute the direct alternating-record count
    ctx.emitter
        .instruction(&format!("jc {}", failure));                              // reject an overflowing pair count
    ctx.emitter.instruction("mov r13, QWORD PTR [rbp - 16]");                   // reload the stable outer result frame
    ctx.emitter.instruction("mov r13, QWORD PTR [r13 + 120]");                  // load the complete retained flat value count
    ctx.emitter.instruction("cmp r9, r13");                                     // is the direct key/value start in bounds?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject an out-of-range map start
    ctx.emitter.instruction("sub r13, r9");                                     // compute the records remaining after the start
    ctx.emitter.instruction("cmp r12, r13");                                    // do all direct pairs fit in the retained vector?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject a truncated associative-map range
    ctx.emitter.instruction("mov QWORD PTR [rbp - 56], r9");                    // retain the direct pair start across recursion
    ctx.emitter.instruction("mov QWORD PTR [rbp - 64], r10");                   // retain the entry count across recursion
    ctx.emitter.instruction("mov rdi, r12");                                    // use twice the entry count as requested hash capacity
    ctx.emitter.instruction("cmp rdi, 16");                                     // enforce the runtime minimum hash capacity
    ctx.emitter
        .instruction(&format!("jae {}", capacity_ready));                      // retain a sufficiently large computed capacity
    ctx.emitter.instruction("mov rdi, 16");                                     // raise small maps to the minimum capacity
    ctx.emitter.label(capacity_ready);
    ctx.emitter.instruction("mov rsi, 7");                                      // nested maps store owned boxed Mixed cells
    abi::emit_call_label(ctx.emitter, "__rt_hash_new");
    ctx.emitter.instruction("mov QWORD PTR [rbp - 40], rax");                   // retain the owned destination hash across recursion
    ctx.emitter.instruction("mov QWORD PTR [rbp - 48], 0");                     // initialize the map entry loop index

    ctx.emitter.label(loop_head);
    ctx.emitter.instruction("mov r9, QWORD PTR [rbp - 48]");                    // load the next nested map entry index
    ctx.emitter.instruction("cmp r9, QWORD PTR [rbp - 64]");                    // have all nested pairs been materialized?
    ctx.emitter
        .instruction(&format!("jae {}", loop_done));                           // finish after the final nested pair
    ctx.emitter.instruction("mov r10, QWORD PTR [rbp - 56]");                   // load the direct pair range start
    ctx.emitter.instruction("lea r9, [r10 + r9 * 2]");                          // select the current key record
    ctx.emitter.instruction("mov QWORD PTR [rbp - 8], r9");                     // retain the key record index across key normalization
    ctx.emitter.instruction("mov r11, QWORD PTR [rbp - 16]");                   // reload the stable outer result frame
    ctx.emitter.instruction("mov r11, QWORD PTR [r11 + 112]");                  // load the retained flat value-vector pointer
    ctx.emitter.instruction("lea r10, [r9 + r9 * 2]");                          // scale the key index by three ABI words
    ctx.emitter.instruction("lea r11, [r11 + r10 * 8]");                        // address the exact 24-byte key record
    ctx.emitter.instruction("cmp DWORD PTR [r11], 2");                          // is this an integer PHP array key?
    ctx.emitter
        .instruction(&format!("je {}", integer_key));                          // preserve the exact signed integer payload
    ctx.emitter.instruction("cmp DWORD PTR [r11], 4");                          // is this a byte-string PHP array key?
    ctx.emitter
        .instruction(&format!("je {}", string_key));                           // validate and normalize the byte-string key
    ctx.emitter
        .instruction(&format!("jmp {}", failure));                             // reject every unsupported nested map key tag

    ctx.emitter.label(integer_key);
    ctx.emitter.instruction("mov rsi, QWORD PTR [r11 + 8]");                    // load the signed integer key bits
    ctx.emitter.instruction("mov rdx, -1");                                     // mark the key as integer for the hash ABI
    ctx.emitter
        .instruction(&format!("jmp {}", key_ready));                           // skip byte-range validation for integer keys

    ctx.emitter.label(string_key);
    ctx.emitter.instruction("mov r10, QWORD PTR [r11 + 8]");                    // load the borrowed result-byte offset
    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + 16]");                   // load the exact key byte length
    ctx.emitter.instruction("mov r12, QWORD PTR [rbp - 16]");                   // reload the stable outer result frame
    ctx.emitter.instruction("mov rcx, QWORD PTR [r12 + 104]");                  // load the retained result-byte length
    ctx.emitter.instruction("cmp r10, rcx");                                    // is the nested key byte offset in bounds?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject an out-of-range nested key offset
    ctx.emitter.instruction("sub rcx, r10");                                    // compute the bytes remaining after the offset
    ctx.emitter.instruction("cmp rdx, rcx");                                    // does the complete nested key fit in retained bytes?
    ctx.emitter
        .instruction(&format!("ja {}", failure));                              // reject a truncated nested byte-string key
    ctx.emitter.instruction("mov rax, QWORD PTR [r12 + 96]");                   // load the retained result-byte pointer
    ctx.emitter.instruction("add rax, r10");                                    // address the borrowed nested key bytes
    abi::emit_call_label(ctx.emitter, "__rt_hash_normalize_key");
    ctx.emitter.instruction("mov rsi, rax");                                    // move the normalized key low word into the hash ABI register

    ctx.emitter.label(key_ready);
    ctx.emitter.instruction("mov QWORD PTR [rbp - 72], rsi");                   // retain the normalized key low word across recursion
    ctx.emitter.instruction("mov QWORD PTR [rbp - 80], rdx");                   // retain the normalized key high word across recursion
    ctx.emitter.instruction("mov r9, QWORD PTR [rbp - 8]");                     // reload the key record index after normalization
    ctx.emitter.instruction("lea rdi, [r9 + 1]");                               // select the value record immediately after the key
    ctx.emitter.instruction("mov rsi, QWORD PTR [rbp - 16]");                   // pass the stable outer result frame to recursion
    ctx.emitter.instruction("mov rdx, QWORD PTR [rbp - 24]");                   // load the current recursion depth
    ctx.emitter.instruction("add rdx, 1");                                      // descend exactly one container level
    abi::emit_call_label(ctx.emitter, entry);
    ctx.emitter.instruction("mov rcx, rax");                                    // transfer the owned boxed child to hash insertion
    ctx.emitter.instruction("mov rdi, QWORD PTR [rbp - 40]");                   // reload the destination associative map
    ctx.emitter.instruction("mov rsi, QWORD PTR [rbp - 72]");                   // reload the normalized key low word
    ctx.emitter.instruction("mov rdx, QWORD PTR [rbp - 80]");                   // reload the normalized key high word
    ctx.emitter.instruction("xor r8, r8");                                      // boxed Mixed hash values use one payload word
    ctx.emitter.instruction("mov r9, 7");                                       // runtime value tag seven identifies boxed Mixed
    abi::emit_call_label(ctx.emitter, "__rt_hash_set");
    ctx.emitter.instruction("mov QWORD PTR [rbp - 40], rax");                   // retain the possibly grown destination hash
    ctx.emitter.instruction("add QWORD PTR [rbp - 48], 1");                     // advance to the next nested key/value pair
    abi::emit_jump(ctx.emitter, loop_head);

    ctx.emitter.label(loop_done);
    ctx.emitter.instruction("mov rax, QWORD PTR [rbp - 40]");                   // restore the completed owned associative map
    emit_box_current_owned_value_as_mixed(
        ctx.emitter,
        &PhpType::AssocArray {
            key: Box::new(PhpType::Mixed),
            value: Box::new(PhpType::Mixed),
        },
    );
    abi::emit_jump(ctx.emitter, epilogue);
}
