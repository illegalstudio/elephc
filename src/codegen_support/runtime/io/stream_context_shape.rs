//! Purpose:
//! Emits `__rt_stream_context_options_shape_ok`, php's own shape rule for a stream-context
//! options array.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io::stream_context`, which raises php's
//!   `ValueError` when the answer is zero.
//!
//! Key details:
//! - php's `parse_context_options()` keeps only entries whose key is a STRING and whose value is
//!   an ARRAY, and raises `ValueError: Options should have the form
//!   ["wrappername"]["optionname"] = $value` for anything else. Measured on `php -n` 8.5.6:
//!   `['ssl' => "abc"]`, `['ssl' => 1]`, `['ssl' => null]` and `[0 => ['a' => 1]]` all raise it,
//!   while `[]`, an absent argument, and a non-string key INSIDE a wrapper array are all fine.
//! - elephc accepted every shape in silence and stored the malformed map, so a typo in a context
//!   array produced a context that simply carried nothing.
//! - The walk is `__rt_hash_iter_next`, so it sees exactly the entries php's own foreach does.
//! - A PACKED array is handled separately, because `[]` is one: an empty options array is
//!   valid php (`stream_context_create([])` is fine), while a NON-empty packed array can only
//!   have integer keys, which php refuses. Feeding a packed array to the hash iterator would
//!   read a header that is not there.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Runtime value tag for a packed indexed array.
const VALUE_TAG_ARRAY: i64 = 4;

/// Runtime value tag for an associative array.
const VALUE_TAG_HASH: i64 = 5;

/// Heap kind stored in the low byte of a container's kind word for a packed indexed array.
const HEAP_KIND_ARRAY: i64 = 2;

/// Emits `__rt_stream_context_options_shape_ok(hash) -> 1 when php would accept the map`.
///
/// A null hash answers 1: an absent options argument is not a malformed one.
///
/// # Input / Output
/// - AArch64: `x0` the hash pointer, answer in `x0`.
/// - x86_64: `rax` the hash pointer, answer in `rax`.
pub fn emit_stream_context_options_shape_ok(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: stream_context_options_shape_ok ---");
    emitter.label_global("__rt_stream_context_options_shape_ok");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("sub sp, sp, #32");                             // frame for the walk
            emitter.instruction("stp x29, x30, [sp, #16]");                     // save frame pointer and return address
            emitter.instruction("add x29, sp, #16");                            // establish the helper frame
            emitter.instruction("cbz x0, __rt_scos_ok");                        // no options is not a bad shape
            // A PACKED array reaches here as `[]` (valid, empty) or as a list with INTEGER keys
            // (which php refuses). Its header is not a hash header, so it is decided here rather
            // than walked.
            emitter.instruction("ldr x9, [x0, #-8]");                           // the container kind word
            emitter.instruction("and x9, x9, #0xff");                           // low byte = heap kind
            emitter.instruction(&format!("cmp x9, #{HEAP_KIND_ARRAY}"));
            emitter.instruction("b.ne __rt_scos_hash");                         // a hash is walked entry by entry
            emitter.instruction("ldr x9, [x0]");                                // a packed array's element count
            emitter.instruction("cbz x9, __rt_scos_ok");                        // `[]` is valid php
            emitter.instruction("b __rt_scos_bad");                             // any element means an integer key
            emitter.label("__rt_scos_hash");
            emitter.instruction("str x0, [sp, #0]");                            // hold the hash across the iterator
            emitter.instruction("mov x1, #0");                                  // cursor 0 starts the walk
            emitter.label("__rt_scos_loop");
            emitter.instruction("ldr x0, [sp, #0]");                            // the hash
            abi::emit_call_label(emitter, "__rt_hash_iter_next");               // x0=cursor x1=key x2=klen x5=tag
            emitter.instruction("cmn x0, #1");                                  // cursor -1 ends the walk
            emitter.instruction("b.eq __rt_scos_ok");                           // every entry passed
            emitter.instruction("cbz x1, __rt_scos_bad");                       // an INTEGER key has no string pointer
            // A hash built at RUNTIME holds boxed Mixed values (tag 7), so the concrete kind is
            // one indirection away. Comparing the box's tag against the container tags read every
            // such entry as "not an array" — measured on `json_decode($json, true)`.
            emitter.instruction("cmp x5, #7");                                  // runtime tag 7 = boxed Mixed
            emitter.instruction("b.ne __rt_scos_concrete");
            emitter.instruction("str x0, [sp, #8]");                            // the cursor shares x0 with the call result
            emitter.instruction("mov x0, x3");                                  // the boxed value
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("mov x5, x0");                                  // the concrete tag
            emitter.instruction("ldr x0, [sp, #8]");                            // restore the cursor
            emitter.label("__rt_scos_concrete");
            emitter.instruction(&format!("cmp x5, #{VALUE_TAG_ARRAY}"));        // a packed array value is accepted
            emitter.instruction("b.eq __rt_scos_next");
            emitter.instruction(&format!("cmp x5, #{VALUE_TAG_HASH}"));         // as is an associative one
            emitter.instruction("b.ne __rt_scos_bad");                          // anything else is php's ValueError
            emitter.label("__rt_scos_next");
            emitter.instruction("mov x1, x0");                                  // carry the cursor into the next step
            emitter.instruction("b __rt_scos_loop");
            emitter.label("__rt_scos_bad");
            emitter.instruction("mov x0, #0");                                  // php refuses this map
            emitter.instruction("b __rt_scos_done");
            emitter.label("__rt_scos_ok");
            emitter.instruction("mov x0, #1");                                  // php accepts it
            emitter.label("__rt_scos_done");
            emitter.instruction("ldp x29, x30, [sp, #16]");                     // restore frame pointer and return address
            emitter.instruction("add sp, sp, #32");                             // release the frame
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            // `__rt_hash_iter_next` takes rdi=hash, rsi=cursor and answers rax=cursor,
            // rdi=key pointer, rdx=key length, r9=value tag.
            emitter.instruction("push rbp");
            emitter.instruction("mov rbp, rsp");
            emitter.instruction("sub rsp, 16");
            emitter.instruction("test rax, rax");
            emitter.instruction("jz __rt_scos_ok_x86");                         // no options is not a bad shape
            // See the AArch64 arm: `[]` is a PACKED array and valid php, while a packed array
            // with elements can only have integer keys, which php refuses.
            emitter.instruction("mov r10, QWORD PTR [rax - 8]");                // the container kind word
            emitter.instruction("and r10, 0xff");                               // low byte = heap kind
            emitter.instruction(&format!("cmp r10, {HEAP_KIND_ARRAY}"));
            emitter.instruction("jne __rt_scos_hash_x86");                      // a hash is walked entry by entry
            emitter.instruction("mov r10, QWORD PTR [rax]");                    // a packed array's element count
            emitter.instruction("test r10, r10");
            emitter.instruction("jz __rt_scos_ok_x86");                         // `[]` is valid php
            emitter.instruction("jmp __rt_scos_bad_x86");                       // any element means an integer key
            emitter.label("__rt_scos_hash_x86");
            emitter.instruction("mov QWORD PTR [rbp - 8], rax");                // hold the hash across the iterator
            emitter.instruction("xor esi, esi");                                // cursor 0 starts the walk
            emitter.label("__rt_scos_loop_x86");
            emitter.instruction("mov rdi, QWORD PTR [rbp - 8]");                // the hash
            abi::emit_call_label(emitter, "__rt_hash_iter_next");
            emitter.instruction("cmp rax, -1");                                 // cursor -1 ends the walk
            emitter.instruction("je __rt_scos_ok_x86");                         // every entry passed
            emitter.instruction("test rdi, rdi");                               // an INTEGER key has no string pointer
            emitter.instruction("jz __rt_scos_bad_x86");
            // See the AArch64 arm: a runtime-built hash holds boxed Mixed values.
            emitter.instruction("cmp r9, 7");                                   // runtime tag 7 = boxed Mixed
            emitter.instruction("jne __rt_scos_concrete_x86");
            emitter.instruction("mov QWORD PTR [rbp - 16], rax");               // the cursor shares rax with the call result
            emitter.instruction("mov rax, rcx");                                // the boxed value
            abi::emit_call_label(emitter, "__rt_mixed_unbox");
            emitter.instruction("mov r9, rax");                                 // the concrete tag
            emitter.instruction("mov rax, QWORD PTR [rbp - 16]");               // restore the cursor
            emitter.label("__rt_scos_concrete_x86");
            emitter.instruction(&format!("cmp r9, {VALUE_TAG_ARRAY}"));         // a packed array value is accepted
            emitter.instruction("je __rt_scos_next_x86");
            emitter.instruction(&format!("cmp r9, {VALUE_TAG_HASH}"));          // as is an associative one
            emitter.instruction("jne __rt_scos_bad_x86");                       // anything else is php's ValueError
            emitter.label("__rt_scos_next_x86");
            emitter.instruction("mov rsi, rax");                                // carry the cursor into the next step
            emitter.instruction("jmp __rt_scos_loop_x86");
            emitter.label("__rt_scos_bad_x86");
            emitter.instruction("leave");
            emitter.instruction("xor eax, eax");                                // php refuses this map
            emitter.instruction("ret");
            emitter.label("__rt_scos_ok_x86");
            emitter.instruction("leave");
            emitter.instruction("mov eax, 1");                                  // php accepts it
            emitter.instruction("ret");
        }
    }
}
