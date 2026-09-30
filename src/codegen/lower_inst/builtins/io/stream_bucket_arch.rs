//! Purpose:
//! AArch64 and x86_64 stream bucket object lowering.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Emits the AArch64 body for `stream_bucket_new`.
pub(super) fn lower_stream_bucket_new_aarch64(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldr x1, [sp, #16]");                               // reload the bucket data string pointer
    ctx.emitter.instruction("ldr x2, [sp, #24]");                               // reload the bucket data string length
    ctx.emitter.instruction("mov x0, #1");                                      // runtime tag 1 = string
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov x3, x0");                                      // pass boxed data as the stdClass property value
    abi::emit_pop_reg(ctx.emitter, "x0");
    abi::emit_push_reg(ctx.emitter, "x0");
    let (data_sym, data_len) = ctx.data.add_string(b"data");
    abi::emit_symbol_address(ctx.emitter, "x1", &data_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", data_len));                 // pass the `data` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    ctx.emitter.instruction("ldr x1, [sp, #24]");                               // use the original string length as datalen
    ctx.emitter.instruction("mov x2, #0");                                      // integer Mixed payloads do not use the high word
    ctx.emitter.instruction("mov x0, #0");                                      // runtime tag 0 = int
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov x3, x0");                                      // pass boxed datalen as the property value
    abi::emit_pop_reg(ctx.emitter, "x0");
    let (datalen_sym, datalen_len) = ctx.data.add_string(b"datalen");
    abi::emit_symbol_address(ctx.emitter, "x1", &datalen_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", datalen_len));              // pass the `datalen` property-name length
    abi::emit_push_reg(ctx.emitter, "x0");
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_pop_reg(ctx.emitter, "x0");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("mov x1, x0");                                      // pass the bucket object pointer as the Mixed payload
    ctx.emitter.instruction("mov x2, #0");                                      // object Mixed payloads do not use the high word
    ctx.emitter.instruction("mov x0, #6");                                      // runtime tag 6 = object
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Emits the x86_64 body for `stream_bucket_new`.
pub(super) fn lower_stream_bucket_new_x86_64(ctx: &mut FunctionContext<'_>) {
    abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_new");
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                   // reload the bucket data string pointer
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");                   // reload the bucket data string length
    ctx.emitter.instruction("mov rax, 1");                                      // runtime tag 1 = string
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov rcx, rax");                                    // pass boxed data as the stdClass property value
    abi::emit_pop_reg(ctx.emitter, "rax");
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the bucket object as the first stdClass argument
    let (data_sym, data_len) = ctx.data.add_string(b"data");
    abi::emit_symbol_address(ctx.emitter, "rsi", &data_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", data_len));                 // pass the `data` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");                   // use the original string length as datalen
    ctx.emitter.instruction("xor esi, esi");                                    // integer Mixed payloads do not use the high word
    ctx.emitter.instruction("mov rax, 0");                                      // runtime tag 0 = int
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov rcx, rax");                                    // pass boxed datalen as the property value
    abi::emit_pop_reg(ctx.emitter, "rdi");
    abi::emit_push_reg(ctx.emitter, "rdi");
    let (datalen_sym, datalen_len) = ctx.data.add_string(b"datalen");
    abi::emit_symbol_address(ctx.emitter, "rsi", &datalen_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", datalen_len));              // pass the `datalen` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_pop_reg(ctx.emitter, "rdi");
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    ctx.emitter.instruction("xor esi, esi");                                    // object Mixed payloads do not use the high word
    ctx.emitter.instruction("mov rax, 6");                                      // runtime tag 6 = object
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
}

/// Emits the AArch64 body for stream bucket insertion at the requested end.
pub(super) fn lower_stream_bucket_insert_aarch64(
    ctx: &mut FunctionContext<'_>,
    bucket: ValueId,
    brigade_is_mixed: bool,
    prepend: bool,
    buckets_sym: &str,
    buckets_len: usize,
    done: &str,
    init: &str,
    existing: &str,
) -> Result<()> {
    if brigade_is_mixed {
        ctx.emitter.instruction(&format!("cbz x0, {}", done));                  // null Mixed means there is no brigade to mutate
        ctx.emitter.instruction("ldr x9, [x0]");                                // load the Mixed runtime tag
        ctx.emitter.instruction("cmp x9, #6");                                  // tag 6 identifies object values
        ctx.emitter.instruction(&format!("b.ne {}", done));                     // non-object brigades are ignored
        ctx.emitter.instruction("ldr x0, [x0, #8]");                            // unbox the stdClass object pointer
    }
    ctx.emitter.instruction(&format!("cbz x0, {}", done));                      // null brigade objects are ignored
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_result(bucket)?;
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the brigade object for `_buckets` lookup
    abi::emit_symbol_address(ctx.emitter, "x1", buckets_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    ctx.emitter.instruction(&format!("cbz x0, {}", init));                      // missing `_buckets` property allocates a fresh array
    ctx.emitter.instruction("ldr x9, [x0]");                                    // load the property Mixed tag
    ctx.emitter.instruction("cmp x9, #4");                                      // tag 4 identifies indexed arrays
    ctx.emitter.instruction(&format!("b.ne {}", init));                         // non-array `_buckets` allocates a fresh array
    ctx.emitter.instruction("ldr x9, [x0, #8]");                                // unbox the indexed-array pointer
    ctx.emitter.instruction(&format!("cbz x9, {}", init));                      // null array payload allocates a fresh array
    ctx.emitter.instruction("mov x0, x9");                                      // use the existing `_buckets` array
    ctx.emitter.instruction(&format!("b {}", existing));                        // skip fresh-array allocation

    ctx.emitter.label(init);
    ctx.emitter.instruction("mov x0, #4");                                      // initial bucket-array capacity
    ctx.emitter.instruction("mov x1, #8");                                      // bucket-array elements are Mixed-cell pointers
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    ctx.emitter.instruction("ldr x10, [x0, #-8]");                              // load the array metadata word
    ctx.emitter.instruction("mov x12, #0x80ff");                                // preserve kind and COW bits while changing value type
    ctx.emitter.instruction("and x10, x10, x12");                               // keep only the persistent array metadata bits
    ctx.emitter.instruction("mov x11, #7");                                     // value_type 7 = boxed Mixed pointer
    ctx.emitter.instruction("lsl x11, x11, #8");                                // move the value type into the metadata byte lane
    ctx.emitter.instruction("orr x10, x10, x11");                               // merge the boxed-Mixed value type
    ctx.emitter.instruction("str x10, [x0, #-8]");                              // store the updated array metadata word

    ctx.emitter.label(existing);
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the bucket Mixed cell for retention
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_pop_reg(ctx.emitter, "x0");
    // A brigade holds each bucket AT MOST ONCE: php's buckets are a linked list and appending one
    // that is already linked MOVES it. MEASURED — appending the same bucket three times answers
    // `'ABC'` in php and answered `'ABCABCABC'` here. The take-out sits after the incref so the
    // count cannot reach zero on a bucket that is about to go straight back in.
    ctx.emitter.instruction("ldr x1, [sp, #0]");                                // the bucket Mixed cell
    abi::emit_call_label(ctx.emitter, "__rt_brigade_remove");
    ctx.emitter.instruction("ldr x1, [sp, #0]");                                // pass the bucket Mixed cell to array_push
    abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
    if prepend {
        let shift = ctx.next_label("sbp_shift");
        let insert = ctx.next_label("sbp_insert");
        ctx.emitter.instruction("ldr x9, [x0]");                                // load the post-append brigade length
        ctx.emitter.instruction("sub x9, x9, #1");                              // point at the appended bucket's last slot
        ctx.emitter.instruction("add x10, x0, #24");                            // compute the brigade payload base
        ctx.emitter.instruction("ldr x11, [x10, x9, lsl #3]");                  // preserve the appended bucket while shifting
        ctx.emitter.label(&shift);
        ctx.emitter.instruction("cmp x9, #0");                                  // check whether the front slot is now available
        ctx.emitter.instruction(&format!("b.eq {}", insert));                   // finish once every prior bucket moved right
        ctx.emitter.instruction("sub x12, x9, #1");                             // select the preceding bucket slot
        ctx.emitter.instruction("ldr x13, [x10, x12, lsl #3]");                 // load the preceding bucket pointer
        ctx.emitter.instruction("str x13, [x10, x9, lsl #3]");                  // shift the preceding bucket one slot right
        ctx.emitter.instruction("mov x9, x12");                                 // continue toward the brigade head
        ctx.emitter.instruction(&format!("b {}", shift));                       // shift the next preceding bucket
        ctx.emitter.label(&insert);
        ctx.emitter.instruction("str x11, [x10]");                              // install the prepended bucket at index zero
    }
    ctx.emitter.instruction("mov x1, x0");                                      // pass the bucket array as the Mixed payload
    ctx.emitter.instruction("mov x2, #0");                                      // indexed-array Mixed payloads do not use the high word
    ctx.emitter.instruction("mov x0, #4");                                      // runtime tag 4 = indexed array
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov x3, x0");                                      // pass the boxed array as the stdClass property value
    ctx.emitter.instruction("ldr x0, [sp, #16]");                               // reload the brigade object
    abi::emit_symbol_address(ctx.emitter, "x1", buckets_sym);
    ctx.emitter.instruction(&format!("mov x2, #{}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(done);
    Ok(())
}

/// Emits the x86_64 body for stream bucket insertion at the requested end.
pub(super) fn lower_stream_bucket_insert_x86_64(
    ctx: &mut FunctionContext<'_>,
    bucket: ValueId,
    brigade_is_mixed: bool,
    prepend: bool,
    buckets_sym: &str,
    buckets_len: usize,
    done: &str,
    init: &str,
    existing: &str,
) -> Result<()> {
    if brigade_is_mixed {
        ctx.emitter.instruction("test rax, rax");                               // null Mixed means there is no brigade to mutate
        ctx.emitter.instruction(&format!("jz {}", done));                       // skip mutation when the brigade is null
        ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                    // load the Mixed runtime tag
        ctx.emitter.instruction("cmp r10, 6");                                  // tag 6 identifies object values
        ctx.emitter.instruction(&format!("jne {}", done));                      // non-object brigades are ignored
        ctx.emitter.instruction("mov rax, QWORD PTR [rax + 8]");                // unbox the stdClass object pointer
    }
    ctx.emitter.instruction("test rax, rax");                                   // null brigade objects are ignored
    ctx.emitter.instruction(&format!("jz {}", done));                           // skip mutation when the brigade object is null
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_result(bucket)?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                   // reload the brigade object for `_buckets` lookup
    abi::emit_symbol_address(ctx.emitter, "rsi", buckets_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_get");
    ctx.emitter.instruction("test rax, rax");                                   // missing `_buckets` property allocates a fresh array
    ctx.emitter.instruction(&format!("jz {}", init));                           // branch to fresh-array allocation
    ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                        // load the property Mixed tag
    ctx.emitter.instruction("cmp r10, 4");                                      // tag 4 identifies indexed arrays
    ctx.emitter.instruction(&format!("jne {}", init));                          // non-array `_buckets` allocates a fresh array
    ctx.emitter.instruction("mov r10, QWORD PTR [rax + 8]");                    // unbox the indexed-array pointer
    ctx.emitter.instruction("test r10, r10");                                   // null array payload allocates a fresh array
    ctx.emitter.instruction(&format!("jz {}", init));                           // branch to fresh-array allocation
    ctx.emitter.instruction("mov rax, r10");                                    // use the existing `_buckets` array
    ctx.emitter.instruction(&format!("jmp {}", existing));                      // skip fresh-array allocation

    ctx.emitter.label(init);
    ctx.emitter.instruction("mov rdi, 4");                                      // initial bucket-array capacity
    ctx.emitter.instruction("mov rsi, 8");                                      // bucket-array elements are Mixed-cell pointers
    abi::emit_call_label(ctx.emitter, "__rt_array_new");
    ctx.emitter.instruction("mov r10, QWORD PTR [rax - 8]");                    // load the array metadata word
    ctx.emitter.instruction("mov r11, 0xffffffff000080ff");                     // preserve magic, kind, and COW bits while changing value type
    ctx.emitter.instruction("and r10, r11");                                    // keep only the persistent array metadata bits
    ctx.emitter.instruction("mov r11, 7");                                      // value_type 7 = boxed Mixed pointer
    ctx.emitter.instruction("shl r11, 8");                                      // move the value type into the metadata byte lane
    ctx.emitter.instruction("or r10, r11");                                     // merge the boxed-Mixed value type
    ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");                    // store the updated array metadata word

    ctx.emitter.label(existing);
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");                   // reload the bucket Mixed cell for retention
    abi::emit_call_label(ctx.emitter, "__rt_incref");
    abi::emit_pop_reg(ctx.emitter, "rax");
    // See the AArch64 counterpart: a brigade holds each bucket AT MOST ONCE, so an append of one
    // it already holds MOVES it. The take-out sits after the incref for the same reason.
    ctx.emitter.instruction("mov rdi, rax");                                    // the `_buckets` array
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // the bucket Mixed cell
    abi::emit_call_label(ctx.emitter, "__rt_brigade_remove");                   // answers the same array in rax
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the `_buckets` array to array_push
    ctx.emitter.instruction("mov rsi, QWORD PTR [rsp]");                        // pass the bucket Mixed cell to array_push
    abi::emit_call_label(ctx.emitter, "__rt_array_push_int");
    if prepend {
        let shift = ctx.next_label("sbp_shift");
        let insert = ctx.next_label("sbp_insert");
        ctx.emitter.instruction("mov r10, QWORD PTR [rax]");                    // load the post-append brigade length
        ctx.emitter.instruction("sub r10, 1");                                  // point at the appended bucket's last slot
        ctx.emitter.instruction("lea r11, [rax + 24]");                         // compute the brigade payload base
        ctx.emitter.instruction("mov r8, QWORD PTR [r11 + r10 * 8]");           // preserve the appended bucket while shifting
        ctx.emitter.label(&shift);
        ctx.emitter.instruction("test r10, r10");                               // check whether the front slot is now available
        ctx.emitter.instruction(&format!("jz {}", insert));                     // finish once every prior bucket moved right
        ctx.emitter.instruction("lea rcx, [r10 - 1]");                          // select the preceding bucket slot
        ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + rcx * 8]");          // load the preceding bucket pointer
        ctx.emitter.instruction("mov QWORD PTR [r11 + r10 * 8], rdx");          // shift the preceding bucket one slot right
        ctx.emitter.instruction("mov r10, rcx");                                // continue toward the brigade head
        ctx.emitter.instruction(&format!("jmp {}", shift));                     // shift the next preceding bucket
        ctx.emitter.label(&insert);
        ctx.emitter.instruction("mov QWORD PTR [r11], r8");                     // install the prepended bucket at index zero
    }
    ctx.emitter.instruction("mov rdi, rax");                                    // pass the bucket array as the Mixed payload
    ctx.emitter.instruction("xor esi, esi");                                    // indexed-array Mixed payloads do not use the high word
    ctx.emitter.instruction("mov rax, 4");                                      // runtime tag 4 = indexed array
    abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
    ctx.emitter.instruction("mov rcx, rax");                                    // pass the boxed array as the stdClass property value
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");                   // reload the brigade object
    abi::emit_symbol_address(ctx.emitter, "rsi", buckets_sym);
    ctx.emitter.instruction(&format!("mov rdx, {}", buckets_len));              // pass the `_buckets` property-name length
    abi::emit_call_label(ctx.emitter, "__rt_stdclass_set");
    abi::emit_release_temporary_stack(ctx.emitter, 32);
    ctx.emitter.label(done);
    Ok(())
}

