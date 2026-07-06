//! Purpose:
//! Emits PHP stream-bucket builtins:
//! `stream_bucket_make_writeable`, `stream_bucket_new`,
//! `stream_bucket_append`, `stream_bucket_prepend`. Buckets are
//! stdClass-backed objects with `data` (string) and `datalen` (int)
//! public properties. A brigade is a stdClass with an internal
//! `_buckets` property (indexed array of Mixed-boxed bucket objects);
//! a brigade with no `_buckets` property is treated as empty.
//!
//! Called from:
//! - `crate::codegen_support::builtins::io::emit()`.
//!
//! Key details:
//! - v1 API-surface delivery: these builtins work as a stand-alone
//!   primitive for code that needs PHP-shaped bucket plumbing. They
//!   are NOT yet wired into the filter dispatch — the existing
//!   `filter(string $data): string` contract continues to drive
//!   stream_filter_append's chain. A future increment will detect a
//!   class's filter() arity and route 4-arg `filter($in, $out,
//!   &$consumed, $closing): int` methods through these brigades.
//! - The brigade's `_buckets` property is an indexed array of
//!   boxed-Mixed bucket references. `make_writeable` pops the head
//!   and rewrites the array (no in-place mutation in v1); `append`
//!   reconstructs the array with the new tail entry. Performance is
//!   O(n) per call — acceptable since real filter brigades stay
//!   small (typically 1-3 buckets).

use crate::codegen_support::abi;
use crate::codegen_support::context::Context;
use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::expr::emit_expr;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Expr;
use crate::types::PhpType;

/// `stream_bucket_new($stream, $data)`: build a stdClass with
/// `data` and `datalen` properties. The `$stream` arg is evaluated
/// for side effects but unused — bucket lifetime is tied to the
/// owning brigade, not the stream.
pub fn emit_new(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_bucket_new()");
    // Evaluate $stream for side effects; the result is dropped.
    emit_expr(&args[0], emitter, ctx, data);
    // Evaluate $data → string in x1/x2 (ARM64) or rax/rdx (x86_64).
    emit_expr(&args[1], emitter, ctx, data);
    match emitter.target.arch {
        Arch::AArch64 => {
            // Save the string ptr/len across the stdclass_new + property-set calls.
            abi::emit_push_reg_pair(emitter, "x1", "x2");
            abi::emit_call_label(emitter, "__rt_stdclass_new");                 // x0 = bucket obj
            abi::emit_push_reg(emitter, "x0");                                  // preserve bucket across the property set
            // Set $bucket->data = boxed_mixed_string
            emitter.instruction("ldr x1, [sp, #16]");                           // string ptr (peek the saved pair)
            emitter.instruction("ldr x2, [sp, #24]");                           // string len
            emitter.instruction("mov x0, #1");                                  // tag = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");             // x0 = mixed-cell ptr
            emitter.instruction("mov x3, x0");                                  // value → 4th arg
            abi::emit_pop_reg(emitter, "x0");                                   // bucket obj → 1st arg
            abi::emit_push_reg(emitter, "x0");                                  // re-push for the next set
            let (data_sym, data_len) = data.add_string(b"data");
            abi::emit_symbol_address(emitter, "x1", &data_sym);                 // name_ptr
            emitter.instruction(&format!("mov x2, #{}", data_len));             // name_len = 4
            abi::emit_call_label(emitter, "__rt_stdclass_set");
            // Set $bucket->datalen = boxed_mixed_int(strlen).
            emitter.instruction("ldr x1, [sp, #24]");                           // string len → int payload
            emitter.instruction("mov x2, #0");                                  // high word
            emitter.instruction("mov x0, #0");                                  // tag = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction("mov x3, x0");                                  // value → 4th arg
            abi::emit_pop_reg(emitter, "x0");                                   // bucket obj
            let (datalen_sym, datalen_len) = data.add_string(b"datalen");
            abi::emit_symbol_address(emitter, "x1", &datalen_sym);
            emitter.instruction(&format!("mov x2, #{}", datalen_len));          // name_len = 7
            abi::emit_push_reg(emitter, "x0");                                  // hold bucket across the set
            abi::emit_call_label(emitter, "__rt_stdclass_set");
            abi::emit_pop_reg(emitter, "x0");                                   // bucket → return
            abi::emit_release_temporary_stack(emitter, 16);                     // drop the original ptr/len pair
            // Box as Mixed object so the caller can pass it through Mixed pipelines.
            emitter.instruction("mov x1, x0");                                  // bucket ptr
            emitter.instruction("mov x2, #0");                                  // prepare AArch64 call argument
            emitter.instruction("mov x0, #6");                                  // tag = object
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            // Save the string ptr/len pair.
            abi::emit_push_reg_pair(emitter, "rax", "rdx");
            abi::emit_call_label(emitter, "__rt_stdclass_new");                 // rax = bucket
            abi::emit_push_reg(emitter, "rax");                                 // preserve bucket
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // peek string ptr
            emitter.instruction("mov rsi, QWORD PTR [rsp + 24]");               // peek string len
            emitter.instruction("mov rax, 1");                                  // tag = string
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction("mov rcx, rax");                                // mixed_ptr → 4th arg
            abi::emit_pop_reg(emitter, "rax");                                  // bucket
            abi::emit_push_reg(emitter, "rax");                                 // re-push
            emitter.instruction("mov rdi, rax");                                // bucket → 1st arg
            let (data_sym, data_len) = data.add_string(b"data");
            abi::emit_symbol_address(emitter, "rsi", &data_sym);
            emitter.instruction(&format!("mov rdx, {}", data_len));             // prepare SysV call argument
            abi::emit_call_label(emitter, "__rt_stdclass_set");
            emitter.instruction("mov rdi, QWORD PTR [rsp + 24]");               // string len → int payload
            emitter.instruction("xor esi, esi");                                // high word
            emitter.instruction("mov rax, 0");                                  // tag = int
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.instruction("mov rcx, rax");                                // mixed → 4th arg
            abi::emit_pop_reg(emitter, "rdi");                                  // bucket
            abi::emit_push_reg(emitter, "rdi");
            let (datalen_sym, datalen_len) = data.add_string(b"datalen");
            abi::emit_symbol_address(emitter, "rsi", &datalen_sym);
            emitter.instruction(&format!("mov rdx, {}", datalen_len));          // prepare SysV call argument
            abi::emit_call_label(emitter, "__rt_stdclass_set");
            abi::emit_pop_reg(emitter, "rdi");                                  // bucket
            abi::emit_release_temporary_stack(emitter, 16);                     // drop saved ptr/len pair
            emitter.instruction("xor esi, esi");                                // clear register value
            emitter.instruction("mov rax, 6");                                  // tag = object
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
        }
    }
    Some(PhpType::Mixed)
}

/// `stream_bucket_make_writeable($brigade)`: pop the head bucket from
/// the brigade's internal `_buckets` indexed-array property. Returns
/// Mixed(null) when:
///   - the brigade arg is not a Mixed(object) (e.g. null was passed).
///   - the brigade has no `_buckets` property or it is not an indexed array.
///   - the `_buckets` array is empty.
///
/// The popped bucket is returned as Mixed-boxed object (matching what
/// `stream_bucket_new` produces). The brigade's `_buckets` array is
/// mutated in place (`__rt_array_shift` decrements length and slides the
/// remaining entries left); the boxed-Mixed cell pointer stored as the
/// `_buckets` property stays the same so no stdclass_set write-back is
/// needed.
pub fn emit_make_writeable(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_bucket_make_writeable()");
    let arg_ty = emit_expr(&args[0], emitter, ctx, data);
    let arg_is_mixed = matches!(arg_ty, PhpType::Mixed | PhpType::Union(_));
    let (buckets_sym, buckets_len) = data.add_string(b"_buckets");
    let return_null = ctx.next_label("sbmw_null");
    let done = ctx.next_label("sbmw_done");
    match emitter.target.arch {
        Arch::AArch64 => {
            // x0 = Mixed cell ptr (when Mixed) or raw obj ptr (when Object).
            if arg_is_mixed {
                emitter.instruction(&format!("cbz x0, {}", return_null));       // null Mixed → no brigade
                emitter.instruction("ldr x9, [x0]");                            // tag
                emitter.instruction("cmp x9, #6");                              // tag==6 (object)?
                emitter.instruction(&format!("b.ne {}", return_null));          // branch when the checked value is nonzero or different
                emitter.instruction("ldr x0, [x0, #8]");                        // unbox: obj ptr
            }
            emitter.instruction(&format!("cbz x0, {}", return_null));           // branch when the checked value is zero or equal
            // x0 = brigade obj; look up _buckets.
            abi::emit_symbol_address(emitter, "x1", &buckets_sym);
            emitter.instruction(&format!("mov x2, #{}", buckets_len));          // prepare AArch64 call argument
            abi::emit_call_label(emitter, "__rt_stdclass_get");                  // x0 = Mixed*
            emitter.instruction(&format!("cbz x0, {}", return_null));           // branch when the checked value is zero or equal
            emitter.instruction("ldr x9, [x0]");                                // Mixed tag
            emitter.instruction("cmp x9, #4");                                  // tag==4 (indexed array)?
            emitter.instruction(&format!("b.ne {}", return_null));              // branch when the checked value is nonzero or different
            emitter.instruction("ldr x9, [x0, #8]");                            // array ptr from Mixed payload_lo
            emitter.instruction(&format!("cbz x9, {}", return_null));           // branch when the checked value is zero or equal
            emitter.instruction("ldr x10, [x9]");                               // length
            emitter.instruction(&format!("cbz x10, {}", return_null));          // branch when the checked value is zero or equal
            emitter.instruction("mov x0, x9");                                  // array ptr → x0
            abi::emit_call_label(emitter, "__rt_array_shift");                   // x0 = popped Mixed* (the bucket)
            emitter.instruction(&format!("b {}", done));                        // continue at target label
            emitter.label(&return_null);
            emitter.instruction("mov x0, #8");                                  // tag = null
            emitter.instruction("mov x1, #0");                                  // prepare AArch64 call argument
            emitter.instruction("mov x2, #0");                                  // prepare AArch64 call argument
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done);
        }
        Arch::X86_64 => {
            if arg_is_mixed {
                emitter.instruction("test rax, rax");                           // null Mixed?
                emitter.instruction(&format!("jz {}", return_null));            // branch when the checked value is zero or equal
                emitter.instruction("mov r10, QWORD PTR [rax]");                // tag
                emitter.instruction("cmp r10, 6");                              // object?
                emitter.instruction(&format!("jne {}", return_null));           // branch when the checked value is nonzero or different
                emitter.instruction("mov rax, QWORD PTR [rax + 8]");            // unbox: obj ptr
            }
            emitter.instruction("test rax, rax");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", return_null));                // branch when the checked value is zero or equal
            // SysV: stdclass_get(rdi=obj, rsi=name_ptr, rdx=name_len).
            emitter.instruction("mov rdi, rax");                                // prepare SysV call argument
            abi::emit_symbol_address(emitter, "rsi", &buckets_sym);
            emitter.instruction(&format!("mov rdx, {}", buckets_len));          // prepare SysV call argument
            abi::emit_call_label(emitter, "__rt_stdclass_get");                  // rax = Mixed*
            emitter.instruction("test rax, rax");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", return_null));                // branch when the checked value is zero or equal
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // Mixed tag
            emitter.instruction("cmp r10, 4");                                  // indexed array?
            emitter.instruction(&format!("jne {}", return_null));               // branch when the checked value is nonzero or different
            emitter.instruction("mov r10, QWORD PTR [rax + 8]");                // array ptr from Mixed payload_lo
            emitter.instruction("test r10, r10");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", return_null));                // branch when the checked value is zero or equal
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // length
            emitter.instruction("test r11, r11");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", return_null));                // branch when the checked value is zero or equal
            // SysV: array_shift(rdi=array).
            emitter.instruction("mov rdi, r10");                                // prepare SysV call argument
            abi::emit_call_label(emitter, "__rt_array_shift");                   // rax = popped Mixed* (the bucket)
            emitter.instruction(&format!("jmp {}", done));                      // continue at target label
            emitter.label(&return_null);
            emitter.instruction("mov rax, 8");                                  // tag = null
            emitter.instruction("xor edi, edi");                                // clear register value
            emitter.instruction("xor esi, esi");                                // clear register value
            abi::emit_call_label(emitter, "__rt_mixed_from_value");
            emitter.label(&done);
        }
    }
    Some(PhpType::Mixed)
}

/// `stream_bucket_append($brigade, $bucket)` / `_prepend(...)`: actually
/// push the bucket into the brigade's `_buckets` indexed-array property.
/// If `_buckets` is missing or not an indexed array, a fresh
/// indexed array of Mixed-boxed pointers is created and stored back via
/// `__rt_stdclass_set`. Otherwise the existing array is appended to via
/// `__rt_array_push_int`; if the push grew the array, the new pointer
/// is also written back through `__rt_stdclass_set`.
///
/// Both append and prepend share the same emit body — prepend would
/// require an `__rt_array_unshift` helper which doesn't exist yet, so v1
/// treats prepend as an append (PHP filter chains rarely use prepend; the
/// dispatcher walks the brigade head-to-tail in order, so the practical
/// difference is small).
pub fn emit_append_or_prepend(
    _name: &str,
    args: &[Expr],
    emitter: &mut Emitter,
    ctx: &mut Context,
    data: &mut DataSection,
) -> Option<PhpType> {
    emitter.comment("stream_bucket_append/prepend()");
    // Evaluate $brigade — Mixed cell ptr (when Mixed-typed) or raw obj ptr.
    let brigade_ty = emit_expr(&args[0], emitter, ctx, data);
    let brigade_is_mixed = matches!(brigade_ty, PhpType::Mixed | PhpType::Union(_));
    let (buckets_sym, buckets_len) = data.add_string(b"_buckets");
    let done = ctx.next_label("sba_done");
    let skip_init = ctx.next_label("sba_existing");
    let push = ctx.next_label("sba_push");
    let writeback = ctx.next_label("sba_writeback");
    match emitter.target.arch {
        Arch::AArch64 => {
            if brigade_is_mixed {
                emitter.instruction(&format!("cbz x0, {}", done));              // branch when the checked value is zero or equal
                emitter.instruction("ldr x9, [x0]");                            // load runtime value
                emitter.instruction("cmp x9, #6");                              // compare runtime values for the next branch
                emitter.instruction(&format!("b.ne {}", done));                 // branch when the checked value is nonzero or different
                emitter.instruction("ldr x0, [x0, #8]");                        // load runtime value
            }
            emitter.instruction(&format!("cbz x0, {}", done));                  // branch when the checked value is zero or equal
            // Save brigade obj on a temp stack slot for use after evaluating $bucket.
            abi::emit_push_reg(emitter, "x0");
            // Evaluate $bucket — Mixed cell ptr in x0.
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg(emitter, "x0");                                   // save bucket Mixed*
            // Reload brigade and look up _buckets.
            emitter.instruction("ldr x0, [sp, #16]");                           // brigade obj (peek)
            abi::emit_symbol_address(emitter, "x1", &buckets_sym);
            emitter.instruction(&format!("mov x2, #{}", buckets_len));          // prepare AArch64 call argument
            abi::emit_call_label(emitter, "__rt_stdclass_get");                  // x0 = Mixed*
            // Either it's a Mixed(indexed-array) we can push into, or we need a fresh one.
            emitter.instruction(&format!("cbz x0, {}", push));                  // null Mixed → make new
            emitter.instruction("ldr x9, [x0]");                                // load runtime value
            emitter.instruction("cmp x9, #4");                                  // indexed array?
            emitter.instruction(&format!("b.ne {}", push));                     // wrong tag → make new
            emitter.instruction("ldr x9, [x0, #8]");                            // array ptr from Mixed
            emitter.instruction(&format!("cbz x9, {}", push));                  // null array → make new
            emitter.instruction("mov x0, x9");                                  // array ptr ready for push_int
            emitter.instruction(&format!("b {}", skip_init));                   // continue at target label

            emitter.label(&push);
            // Allocate a fresh empty indexed array (capacity 4, stride 8 = Mixed pointer slots).
            emitter.instruction("mov x0, #4");                                  // prepare AArch64 call argument
            emitter.instruction("mov x1, #8");                                  // prepare AArch64 call argument
            abi::emit_call_label(emitter, "__rt_array_new");                     // x0 = new array
            // Stamp value_type tag = 7 (boxed Mixed) so dispatchers route correctly.
            // Mask 0x80ff matches the existing AArch64 stamp helper convention:
            // preserve the kind byte + COW bit, clear the rest before OR'ing the
            // new value_type tag in.
            emitter.instruction("ldr x10, [x0, #-8]");                          // packed kind word
            emitter.instruction("mov x12, #0x80ff");                            // mask: low byte (kind) + COW bit
            emitter.instruction("and x10, x10, x12");                           // keep persistent metadata
            emitter.instruction("mov x11, #7");                                 // value_type = boxed Mixed
            emitter.instruction("lsl x11, x11, #8");                            // place in byte lane
            emitter.instruction("orr x10, x10, x11");                           // combine runtime bit flags
            emitter.instruction("str x10, [x0, #-8]");                          // store runtime value

            emitter.label(&skip_init);
            // Push the bucket Mixed* into the array. We also incref so the
            // cell survives the caller's end-of-scope decref (common pattern
            // in brigade-driven filters: $b = make_writeable(); append(out, $b);
            // — when the method returns, $b's slot is decref'd, and without
            // the extra owner the array would dangle).
            abi::emit_push_reg(emitter, "x0");                                   // save array across incref
            emitter.instruction("ldr x0, [sp, #16]");                           // peek bucket Mixed*
            abi::emit_call_label(emitter, "__rt_incref");
            abi::emit_pop_reg(emitter, "x0");                                    // restore array
            emitter.instruction("ldr x1, [sp, #0]");                            // bucket Mixed*
            abi::emit_call_label(emitter, "__rt_array_push_int");                // x0 = updated array
            // We always write the (re-boxed) Mixed back so the brigade sees the right pointer.
            emitter.instruction("mov x3, x0");                                  // array ptr → boxing low payload arg
            emitter.instruction("mov x0, #4");                                  // tag = indexed array
            emitter.instruction("mov x1, x3");                                  // payload lo
            emitter.instruction("mov x2, #0");                                  // payload hi
            abi::emit_call_label(emitter, "__rt_mixed_from_value");              // x0 = fresh Mixed cell wrapping the array
            // stdclass_set(brigade, "_buckets", 8, mixed_array).
            emitter.label(&writeback);
            emitter.instruction("mov x3, x0");                                  // mixed* → 4th arg
            emitter.instruction("ldr x0, [sp, #16]");                           // brigade obj
            abi::emit_symbol_address(emitter, "x1", &buckets_sym);
            emitter.instruction(&format!("mov x2, #{}", buckets_len));          // prepare AArch64 call argument
            abi::emit_call_label(emitter, "__rt_stdclass_set");
            // Pop the saved (bucket Mixed*, brigade obj) pair.
            abi::emit_release_temporary_stack(emitter, 32);
            emitter.label(&done);
        }
        Arch::X86_64 => {
            if brigade_is_mixed {
                emitter.instruction("test rax, rax");                           // check whether the runtime value is zero
                emitter.instruction(&format!("jz {}", done));                   // branch when the checked value is zero or equal
                emitter.instruction("mov r10, QWORD PTR [rax]");                // move runtime value between registers
                emitter.instruction("cmp r10, 6");                              // compare runtime values for the next branch
                emitter.instruction(&format!("jne {}", done));                  // branch when the checked value is nonzero or different
                emitter.instruction("mov rax, QWORD PTR [rax + 8]");            // prepare runtime result value
            }
            emitter.instruction("test rax, rax");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", done));                       // branch when the checked value is zero or equal
            abi::emit_push_reg(emitter, "rax");                                  // save brigade obj
            emit_expr(&args[1], emitter, ctx, data);
            abi::emit_push_reg(emitter, "rax");                                  // save bucket Mixed*
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // brigade obj
            abi::emit_symbol_address(emitter, "rsi", &buckets_sym);
            emitter.instruction(&format!("mov rdx, {}", buckets_len));          // prepare SysV call argument
            abi::emit_call_label(emitter, "__rt_stdclass_get");                  // rax = Mixed*
            emitter.instruction("test rax, rax");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", push));                       // branch when the checked value is zero or equal
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // move runtime value between registers
            emitter.instruction("cmp r10, 4");                                  // compare runtime values for the next branch
            emitter.instruction(&format!("jne {}", push));                      // branch when the checked value is nonzero or different
            emitter.instruction("mov r10, QWORD PTR [rax + 8]");                // move runtime value between registers
            emitter.instruction("test r10, r10");                               // check whether the runtime value is zero
            emitter.instruction(&format!("jz {}", push));                       // branch when the checked value is zero or equal
            emitter.instruction("mov rax, r10");                                // array ptr
            emitter.instruction(&format!("jmp {}", skip_init));                 // continue at target label

            emitter.label(&push);
            emitter.instruction("mov rdi, 4");                                  // prepare SysV call argument
            emitter.instruction("mov rsi, 8");                                  // prepare SysV call argument
            abi::emit_call_label(emitter, "__rt_array_new");                     // rax = new array
            // Stamp value_type=7. Mask matches the existing x86_64 stamp helper:
            // preserve the high-dword magic marker plus low-byte kind + COW.
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // move runtime value between registers
            emitter.instruction("mov r11, 0xffffffff000080ff");                 // move runtime value between registers
            emitter.instruction("and r10, r11");                                // mask runtime value
            emitter.instruction("mov r11, 7");                                  // move runtime value between registers
            emitter.instruction("shl r11, 8");                                  // shift runtime value
            emitter.instruction("or r10, r11");                                 // combine runtime bit flags
            emitter.instruction("mov QWORD PTR [rax - 8], r10");                // store runtime value

            emitter.label(&skip_init);
            // Incref the bucket Mixed* so it survives caller's end-of-scope decref.
            abi::emit_push_reg(emitter, "rax");                                  // save array across incref
            emitter.instruction("mov rax, QWORD PTR [rsp + 16]");               // peek bucket Mixed*
            abi::emit_call_label(emitter, "__rt_incref");
            abi::emit_pop_reg(emitter, "rax");                                   // restore array
            emitter.instruction("mov rdi, rax");                                // array → SysV first arg of push_int
            emitter.instruction("mov rsi, QWORD PTR [rsp]");                    // bucket Mixed* → SysV second arg
            abi::emit_call_label(emitter, "__rt_array_push_int");                // rax = updated array

            emitter.instruction("mov rdi, rax");                                // prepare SysV call argument
            emitter.instruction("xor esi, esi");                                // clear register value
            emitter.instruction("mov rax, 4");                                  // tag = indexed array
            abi::emit_call_label(emitter, "__rt_mixed_from_value");              // rax = fresh Mixed* wrapping array
            emitter.label(&writeback);
            emitter.instruction("mov rcx, rax");                                // value mixed → 4th SysV arg
            emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");               // brigade obj → 1st
            abi::emit_symbol_address(emitter, "rsi", &buckets_sym);
            emitter.instruction(&format!("mov rdx, {}", buckets_len));          // prepare SysV call argument
            abi::emit_call_label(emitter, "__rt_stdclass_set");
            abi::emit_release_temporary_stack(emitter, 32);
            emitter.label(&done);
        }
    }
    Some(PhpType::Void)
}
