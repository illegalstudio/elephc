//! Purpose:
//! Stream, socket, pathinfo, and stat result boxing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

/// Boxes a raw stream string slice as Mixed, choosing false from a separate consumed
/// flag rather than from the byte count.
///
/// `stream_get_line` can legitimately return an empty string — a delimiter sitting at
/// the read position strips the segment to nothing — so an emptiness test would report
/// EOF for a real, empty segment. The helper reports whether it consumed any byte at
/// all (aarch64 `x0`, x86_64 `rcx`) and only that answers PHP's `string|false`.
pub(super) fn box_stream_string_or_false_on_unconsumed_result(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // nothing consumed at all: PHP reports false
            ctx.emitter.instruction("mov x0, #1");                              // select runtime tag 1 for the stream string
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for stream EOF
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rcx, rcx");                           // nothing consumed at all?
            ctx.emitter.instruction(&format!("jz {}", false_label));            // PHP reports false for an exhausted stream
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream string pointer as the Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the stream string length as the Mixed high payload word
            ctx.emitter.instruction("mov eax, 1");                              // select runtime tag 1 for the stream string
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for stream EOF
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a raw stream string slice or EOF result into Mixed string-or-false form.
pub(super) fn box_stream_string_or_false_on_empty_result(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x2, #0");                              // test whether the stream read produced bytes
            ctx.emitter.instruction(&format!("b.le {}", false_label));          // box false when the stream hit EOF or read failure
            ctx.emitter.instruction("mov x0, #1");                              // select runtime tag 1 for the stream string
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for stream EOF
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rdx, 0");                              // test whether the stream read produced bytes
            ctx.emitter.instruction(&format!("jle {}", false_label));           // box false when the stream hit EOF or read failure
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stream string pointer as the Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the stream string length as the Mixed high payload word
            ctx.emitter.instruction("mov eax, 1");                              // select runtime tag 1 for the stream string
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for stream EOF
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a non-negative stream descriptor as a PHP resource or false on failure.
///
/// The resource is tagged with scope-cleanup kind 1 (native stream fd, closed via
/// `close()` at scope exit). Callers whose handle needs a different destructor use
/// `box_stream_fd_or_false_result_kind` instead.
pub(super) fn box_stream_fd_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    box_stream_fd_or_false_result_kind(ctx, label_prefix, 1, false, false);
}

/// Adopts a non-negative backend descriptor into the resource registry and boxes its handle.
///
/// The legacy cleanup `kind` becomes the backend kind in stable StreamState.
/// The Mixed payload contains only the opaque generation handle; backend
/// descriptors and destructor selection never escape the registry entry.
pub(super) fn box_stream_fd_or_false_result_kind(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
    kind: u64,
    aux_from_result: bool,
    kind_from_result: bool,
) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the stream helper returned a negative descriptor
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false when stream creation failed
            if aux_from_result {
                ctx.emitter.instruction("mov x3, x1");                          // preserve the returned backend owner for StreamState adoption
            } else {
                ctx.emitter.instruction("mov x3, #0");                          // ordinary descriptors have no auxiliary backend owner
            }
            if kind_from_result {
                ctx.emitter.instruction("mov x1, x2");                          // adopt the backend discriminator returned by the open helper
            } else {
                emit_stream_backend_kind_for_descriptor(ctx, kind, label_prefix);
            }
            ctx.emitter.instruction("mov x2, #1");                              // the registry owns and must eventually close the backend descriptor
            abi::emit_call_label(ctx.emitter, "__rt_stream_adopt_fd");
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // adoption closes the descriptor and returns false on allocation failure
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x1, x0");                              // box only the opaque generation handle as the Mixed low payload
            ctx.emitter.instruction(&format!("mov x2, #{}", kind));             // transitional registry-ownership marker; backend state stays in StreamState
            ctx.emitter.instruction("mov x0, #9");                              // select runtime tag 9 for a stream resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the creator-owned handle after the box retained it
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the resource result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for fopen failure
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the stream helper returned a negative descriptor
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false when stream creation failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the backend descriptor to the registry adopter
            if kind_from_result {
                ctx.emitter.instruction("mov rsi, rcx");                        // adopt the backend discriminator returned by the open helper
                ctx.emitter.instruction("mov rcx, rdx");                        // preserve the returned backend owner for StreamState adoption
            } else if aux_from_result {
                ctx.emitter.instruction("mov rcx, rdx");                        // preserve the returned backend owner for StreamState adoption
            } else {
                ctx.emitter.instruction("xor ecx, ecx");                        // ordinary descriptors have no auxiliary backend owner
            }
            if !kind_from_result {
                emit_stream_backend_kind_for_descriptor(ctx, kind, label_prefix);
            }
            ctx.emitter.instruction("mov edx, 1");                              // the registry owns and must eventually close the backend descriptor
            abi::emit_call_label(ctx.emitter, "__rt_stream_adopt_fd");
            ctx.emitter.instruction("test rax, rax");                           // did registry allocation produce an opaque handle?
            ctx.emitter.instruction(&format!("jz {}", false_label));            // adoption closes the descriptor and returns false on failure
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, rax");                            // box only the opaque generation handle as the Mixed low payload
            ctx.emitter.instruction(&format!("mov esi, {}", kind));             // transitional registry-ownership marker; backend state stays in StreamState
            ctx.emitter.instruction("mov eax, 9");                              // select runtime tag 9 for a stream resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 16]");           // reload the creator-owned handle after the box retained it
            abi::emit_call_label(ctx.emitter, "__rt_resource_release");
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the resource result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for fopen failure
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Selects a registry backend kind from the requested destructor and synthetic descriptor range.
fn emit_stream_backend_kind_for_descriptor(
    ctx: &mut FunctionContext<'_>,
    requested_kind: u64,
    label_prefix: &str,
) {
    let done_label = ctx.next_label(&format!("{}_backend_kind_done", label_prefix));
    let user_label = ctx.next_label(&format!("{}_backend_kind_user", label_prefix));
    let phar_label = ctx.next_label(&format!("{}_backend_kind_phar", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x1, #{}", requested_kind));   // default to the caller-selected native backend destructor
            if requested_kind == 1 {
                ctx.emitter.instruction("mov w9, #0x4000");                     // materialize the userspace-wrapper synthetic range base
                ctx.emitter.instruction("lsl w9, w9, #16");                     // form USER_WRAPPER_FD_BASE as 0x40000000
                ctx.emitter.instruction("cmp x0, x9");                          // is the backend below all synthetic wrapper handles?
                ctx.emitter.instruction(&format!("b.lt {}", done_label));       // native descriptors keep the direct-fd backend kind
                ctx.emitter.instruction("mov w10, #0x5000");                    // materialize the buffered-Phar synthetic range base
                ctx.emitter.instruction("lsl w10, w10, #16");                   // form the Phar write descriptor base as 0x50000000
                ctx.emitter.instruction("cmp x0, x10");                         // is the synthetic descriptor below the Phar range?
                ctx.emitter.instruction(&format!("b.lt {}", user_label));       // lower synthetic handles belong to userspace wrappers
                ctx.emitter.instruction("add x9, x10, #32");                    // compute the exclusive end of the buffered-Phar range
                ctx.emitter.instruction("cmp x0, x9");                          // does this descriptor select a buffered Phar stream?
                ctx.emitter.instruction(&format!("b.lt {}", phar_label));       // buffered Phar streams use their finalizer backend
                ctx.emitter.label(&user_label);
                ctx.emitter.instruction("mov x1, #2");                          // backend kind 2 dispatches userspace stream_close
                ctx.emitter.instruction(&format!("b {}", done_label));          // skip the buffered-Phar backend selection
                ctx.emitter.label(&phar_label);
                ctx.emitter.instruction("mov x1, #5");                          // backend kind 5 dispatches buffered Phar finalization
            } else if requested_kind == 4 {
                ctx.emitter.instruction("mov w9, #0x4000");                     // materialize the userspace-wrapper synthetic range base
                ctx.emitter.instruction("lsl w9, w9, #16");                     // form USER_WRAPPER_FD_BASE as 0x40000000
                ctx.emitter.instruction("cmp x0, x9");                          // is this a synthetic userspace directory handle?
                ctx.emitter.instruction(&format!("b.lt {}", done_label));       // native and glob directories retain backend kind 4
                ctx.emitter.instruction("mov x1, #6");                          // backend kind 6 dispatches userspace dir_closedir
            }
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov esi, {}", requested_kind));   // default to the caller-selected native backend destructor
            if requested_kind == 1 {
                ctx.emitter.instruction("cmp rax, 0x40000000");                 // is the backend below all synthetic wrapper handles?
                ctx.emitter.instruction(&format!("jl {}", done_label));         // native descriptors keep the direct-fd backend kind
                ctx.emitter.instruction("cmp rax, 0x50000000");                 // is the synthetic descriptor below the Phar range?
                ctx.emitter.instruction(&format!("jl {}", user_label));         // lower synthetic handles belong to userspace wrappers
                ctx.emitter.instruction("cmp rax, 0x50000020");                 // does this descriptor select a buffered Phar stream?
                ctx.emitter.instruction(&format!("jl {}", phar_label));         // buffered Phar streams use their finalizer backend
                ctx.emitter.label(&user_label);
                ctx.emitter.instruction("mov esi, 2");                          // backend kind 2 dispatches userspace stream_close
                ctx.emitter.instruction(&format!("jmp {}", done_label));        // skip the buffered-Phar backend selection
                ctx.emitter.label(&phar_label);
                ctx.emitter.instruction("mov esi, 5");                          // backend kind 5 dispatches buffered Phar finalization
            } else if requested_kind == 4 {
                ctx.emitter.instruction("cmp rax, 0x40000000");                 // is this a synthetic userspace directory handle?
                ctx.emitter.instruction(&format!("jl {}", done_label));         // native and glob directories retain backend kind 4
                ctx.emitter.instruction("mov esi, 6");                          // backend kind 6 dispatches userspace dir_closedir
            }
        }
    }
    ctx.emitter.label(&done_label);
}

/// Boxes a socket-pair array result or PHP false as `Mixed`.
pub(super) fn box_stream_socket_pair_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stream_socket_pair_false");
    let done_label = ctx.next_label("stream_socket_pair_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // null pointer means socketpair failed
            ctx.emitter.instruction("mov x1, #9");                              // resource tag: each fd becomes Mixed(resource)
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            abi::emit_push_reg(ctx.emitter, "x0");                              // the creator-owned array outlives the box
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
            );
            // The box retained the array, so the reference the helper created it with has to go:
            // without this the array and both element cells outlived the released box.
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [sp, #16]");                       // reload the creator-owned array
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the false boxing path after success
            ctx.emitter.label(&false_label);
            emit_bool_result(ctx, false);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // null pointer means socketpair failed
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when socketpair failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the descriptor array to array_to_mixed
            ctx.emitter.instruction("mov esi, 9");                              // resource tag: each fd becomes Mixed(resource)
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            abi::emit_push_reg(ctx.emitter, "rax");                             // the creator-owned array outlives the box
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
            );
            // See the AArch64 counterpart: the box retained the array, so the creator's reference
            // has to go or the array and both element cells outlive the released box.
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp + 16]");           // reload the creator-owned array
            abi::emit_call_label(ctx.emitter, "__rt_decref_any");
            abi::emit_pop_reg(ctx.emitter, "rax");
            abi::emit_release_temporary_stack(ctx.emitter, 16);
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the false boxing path after success
            ctx.emitter.label(&false_label);
            emit_bool_result(ctx, false);
            emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes an owned runtime string result into PHP `string|false` Mixed form.
pub(in crate::codegen::lower_inst::builtins) fn box_owned_string_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // branch when the runtime returned a null string pointer for failure
            abi::emit_push_reg_pair(ctx.emitter, "x1", "x2");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov x9, #1");                              // select runtime tag 1 for a string Mixed payload
            ctx.emitter.instruction("str x9, [x0]");                            // store the string tag in the Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "x10", "x11");
            ctx.emitter.instruction("stp x10, x11, [x0, #8]");                  // store the owned string pointer and length in the Mixed cell
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned a null string pointer for failure
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box false when the runtime string helper failed
            abi::emit_push_reg_pair(ctx.emitter, "rax", "rdx");
            ctx.emitter.instruction("mov rax, 24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(
                &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))
            );                                                                  // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov r10, 1");                              // select runtime tag 1 for a string Mixed payload
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");                // store the string tag in the Mixed cell
            abi::emit_pop_reg_pair(ctx.emitter, "r10", "r11");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned string pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r11");           // store the owned string length in the Mixed cell
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a raw `readfile()` byte count into PHP `int|false` Mixed form.
pub(super) fn box_readfile_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("readfile_false");
    let done_label = ctx.next_label("readfile_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x9, #-2");                             // runtime sentinel -2 means the file could not be opened
            ctx.emitter.instruction("cmp x0, x9");                              // test whether readfile failed before streaming began
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // box PHP false for open failure
            ctx.emitter.instruction("mov x1, x0");                              // pass the streamed byte count as the Mixed integer payload
            ctx.emitter.instruction("mov x2, #0");                              // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for readfile failure
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, -2");                             // runtime sentinel -2 means the file could not be opened
            ctx.emitter.instruction(&format!("je {}", false_label));            // box PHP false for open failure
            ctx.emitter.instruction("mov rdi, rax");                            // pass the streamed byte count as the Mixed integer payload
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for readfile failure
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a non-negative integer result or PHP `false` for the `-1` sentinel.
pub(super) fn box_negative_int_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the runtime returned the not-found sentinel
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false when the lookup did not find an entry
            ctx.emitter.instruction("mov x1, x0");                              // pass the lookup integer as the Mixed low payload word
            ctx.emitter.instruction("mov x2, #0");                              // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned the not-found sentinel
            ctx.emitter.instruction(&format!("js {}", false_label));            // box PHP false when the lookup did not find an entry
            ctx.emitter.instruction("mov rdi, rax");                            // pass the lookup integer as the Mixed low payload word
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a freshly owned pathinfo hash as a PHP associative-array Mixed cell.
pub(super) fn box_owned_pathinfo_array_as_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov x9, #5");                              // select runtime tag 5 for an associative-array Mixed payload
            ctx.emitter.instruction("str x9, [x0]");                            // store the associative-array tag in the Mixed cell
            abi::emit_pop_reg(ctx.emitter, "x10");
            ctx.emitter.instruction("str x10, [x0, #8]");                       // store the owned pathinfo hash pointer in the Mixed cell
            ctx.emitter.instruction("str xzr, [x0, #16]");                      // associative-array Mixed payloads do not use a high word
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, 24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(
                &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))
            );                                                                  // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax], 5");                  // select runtime tag 5 for an associative-array Mixed payload
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned pathinfo hash pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], 0");             // associative-array Mixed payloads do not use a high word
        }
    }
}

/// Boxes the raw stat integer payload into PHP `int|false` Mixed form.
pub(super) fn box_stat_int_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_int_false");
    let done_label = ctx.next_label("stat_int_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box PHP false when the runtime success flag is unset
            ctx.emitter.instruction("mov x2, xzr");                             // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x1, x0");                              // pass the stat integer as the Mixed low payload word
            ctx.emitter.instruction("mov x0, #0");                              // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // test whether the runtime success flag is set
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when the stat helper failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the stat integer as the Mixed low payload word
            ctx.emitter.instruction("xor esi, esi");                            // integer Mixed payloads do not use a high word
            ctx.emitter.instruction("xor eax, eax");                            // select runtime tag 0 for an integer Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the integer Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a raw float payload into PHP `float|false` Mixed form.
///
/// Sibling of `box_stat_int_or_false_result`, sharing its success-flag register. The
/// difference is only that the payload arrives in the FLOAT result register and moves across
/// as its IEEE-754 bit pattern, which is what a tag-2 Mixed cell stores.
pub(super) fn box_float_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // box PHP false when the runtime success flag is unset
            ctx.emitter.instruction("fmov x1, d0");                             // the IEEE-754 bits are the Mixed low payload word
            ctx.emitter.instruction("mov x2, xzr");                             // float Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #2");                              // select runtime tag 2 for a float Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload
            ctx.emitter.instruction("mov x2, #0");
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // is the runtime success flag set?
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when it is not
            ctx.emitter.instruction("movq rdi, xmm0");                          // the IEEE-754 bits are the Mixed low payload word
            ctx.emitter.instruction("xor esi, esi");                            // float Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 2");                              // select runtime tag 2 for a float Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload
            ctx.emitter.instruction("xor esi, esi");
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a raw INDEXED array pointer into PHP `array|false` Mixed form.
///
/// A helper that answers with a pointer has only the null pointer left to signal failure,
/// and storing that raw leaves the caller reading `null` — which is not `false`, so the
/// idiomatic `while (($row = fgetcsv($h)) !== false)` never ends. Sibling of
/// `box_stat_array_or_false_result`, which boxes a HASH payload (tag 5) rather than the
/// indexed payload (tag 4) these helpers build.
pub(super) fn box_indexed_array_or_false_result(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
) {
    let false_label = ctx.next_label(&format!("{}_arr_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_arr_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // a null array pointer is the helper's only failure signal
            ctx.emitter.instruction("mov x1, x0");                              // pass the array as the Mixed payload
            ctx.emitter.instruction("mov x2, #0");                              // indexed-array Mixed payloads do not use the high word
            ctx.emitter.instruction("mov x0, #4");                              // runtime tag 4 = indexed array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the array result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload
            ctx.emitter.instruction("mov x2, #0");                              // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // a null array pointer is the helper's only failure signal
            ctx.emitter.instruction(&format!("jz {}", false_label));
            ctx.emitter.instruction("mov rdi, rax");                            // pass the array as the Mixed payload
            ctx.emitter.instruction("xor esi, esi");                            // indexed-array Mixed payloads do not use the high word
            ctx.emitter.instruction("mov eax, 4");                              // runtime tag 4 = indexed array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the array result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload
            ctx.emitter.instruction("xor esi, esi");                            // bool Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 3");                              // runtime tag 3 = boolean false
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes a raw listing pointer as `array|false` and makes the box the listing's SOLE owner.
///
/// The plain boxer leaves the creation reference alive: the tag-4 box RETAINS its payload, so
/// the listing sat at refcount 2 and `sort($d)`'s copy-on-write split sorted a COPY while the
/// box kept pointing at the unsorted original. Callers whose runtime helper hands back a
/// freshly created array (scandir, glob, file) release that creation reference here, right
/// after the box takes its own.
pub(super) fn box_listing_or_false_result(ctx: &mut FunctionContext<'_>, label_prefix: &str) {
    let no_release = ctx.next_label(&format!("{}_no_release", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");                              // the creator's raw listing (or null)
            box_indexed_array_or_false_result(ctx, label_prefix);
            abi::emit_pop_reg(ctx.emitter, "x9");
            ctx.emitter.instruction(&format!("cbz x9, {}", no_release));        // a failed listing has nothing to release
            abi::emit_push_reg(ctx.emitter, "x0");                              // hold the boxed result across the release
            ctx.emitter.instruction("mov x0, x9");
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.label(&no_release);
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rax");                             // the creator's raw listing (or null)
            box_indexed_array_or_false_result(ctx, label_prefix);
            abi::emit_pop_reg(ctx.emitter, "r9");
            ctx.emitter.instruction("test r9, r9");
            ctx.emitter.instruction(&format!("jz {}", no_release));             // a failed listing has nothing to release
            abi::emit_push_reg(ctx.emitter, "rax");                             // hold the boxed result across the release
            ctx.emitter.instruction("mov rax, r9");                             // the decref family reads rax
            abi::emit_call_label(ctx.emitter, "__rt_decref_array");
            abi::emit_pop_reg(ctx.emitter, "rax");
            ctx.emitter.label(&no_release);
        }
    }
}

/// Runtime Mixed tag for a HASH payload, as `__rt_mixed_from_value` reads it.
///
/// Spelled here rather than inline so the shared boxer below cannot be handed a tag that
/// disagrees with the heap kind it stores — the drift that helper exists to prevent.
const MIXED_TAG_ASSOC_ARRAY: u64 = 5;

/// Boxes the raw stat hash payload into PHP `array|false` Mixed form.
pub(super) fn box_stat_array_or_false_result(ctx: &mut FunctionContext<'_>) {
    box_array_or_false_result(ctx, MIXED_TAG_ASSOC_ARRAY, "stat_array");
}

/// Boxes a runtime array pointer as a Mixed of `tag`, or PHP false when it is null.
fn box_array_or_false_result(ctx: &mut FunctionContext<'_>, tag: u64, label_prefix: &str) {
    let false_label = ctx.next_label(&format!("{label_prefix}_false"));
    let done_label = ctx.next_label(&format!("{label_prefix}_done"));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // branch when the stat runtime returned a null hash pointer
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction(&format!("mov x9, #{}", tag));              // select the runtime tag matching this array's payload shape
            ctx.emitter.instruction("str x9, [x0]");                            // store the array tag in the Mixed cell
            abi::emit_pop_reg(ctx.emitter, "x10");
            ctx.emitter.instruction("str x10, [x0, #8]");                       // store the owned array pointer in the Mixed cell
            ctx.emitter.instruction("str xzr, [x0, #16]");                      // array Mixed payloads do not use a high word
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the array Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the stat runtime returned a null hash pointer
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box false when the runtime stat-array helper failed
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rax, 24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction(
                &format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))
            );                                                                  // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction(&format!("mov QWORD PTR [rax], {}", tag));  // select the runtime tag matching this array's payload shape
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned array pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], 0");             // array Mixed payloads do not use a high word
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the array Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Boxes the raw stat string slice into PHP `string|false` Mixed form.
pub(super) fn box_stat_string_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_string_false");
    let done_label = ctx.next_label("stat_string_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x1, {}", false_label));       // box PHP false when the runtime returned a null string pointer
            ctx.emitter.instruction("mov x0, #1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime returned a null string pointer
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when filetype failed
            ctx.emitter.instruction("mov rdi, rax");                            // pass the filetype string pointer as the Mixed low payload word
            ctx.emitter.instruction("mov rsi, rdx");                            // pass the filetype string length as the Mixed high payload word
            ctx.emitter.instruction("mov eax, 1");                              // select runtime tag 1 for a string Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the string Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}


/// Records literal wrapper metadata on the opaque handle held by a boxed stream result.
pub(super) fn emit_record_stream_meta_after_boxed_literal(
    ctx: &mut FunctionContext<'_>,
    wrapper_id: i64,
    uri: &str,
) {
    let (label, len) = ctx.data.add_string(uri.as_bytes());
    let uri_len = len as i64;
    let done_label = ctx.next_label("stream_meta_literal_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed fopen result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // failed opens have no handle metadata to publish
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction(&format!("mov x1, #{}", wrapper_id));
            abi::emit_symbol_address(ctx.emitter, "x2", &label);
            ctx.emitter.instruction(&format!("mov x3, #{}", uri_len));
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_meta");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // failed opens have no handle metadata to publish
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction(&format!("mov esi, {}", wrapper_id));
            abi::emit_symbol_address(ctx.emitter, "rdx", &label);
            ctx.emitter.instruction(&format!("mov rcx, {}", uri_len));
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_meta");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
}

/// Records metadata using a URI pair already preserved in the caller's stack slot.
pub(super) fn emit_record_stream_meta_after_boxed_stashed(
    ctx: &mut FunctionContext<'_>,
    wrapper_id: i64,
) {
    let done_label = ctx.next_label("stream_meta_stashed_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed fopen result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // failed opens leave the stashed URI untouched
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction(&format!("mov x1, #{}", wrapper_id));
            ctx.emitter.instruction("ldr x2, [sp, #16]");                       // reload the caller-stashed URI pointer
            ctx.emitter.instruction("ldr x3, [sp, #24]");                       // reload the caller-stashed URI byte length
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_meta");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // failed opens leave the stashed URI untouched
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction(&format!("mov esi, {}", wrapper_id));
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 16]");           // reload the caller-stashed URI pointer
            ctx.emitter.instruction("mov rcx, QWORD PTR [rsp + 24]");           // reload the caller-stashed URI byte length
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_meta");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
}

/// Records the mode `stream_get_meta_data()` must report, on the boxed stream's opaque handle.
///
/// Runs after the URI has been recorded, because the runtime helper reads it to tell an echoing
/// wrapper from one of the memory wrappers, which report a mode of their own choosing.
pub(super) fn emit_record_stream_mode_after_boxed(
    ctx: &mut FunctionContext<'_>,
    mode: ValueId,
) -> Result<()> {
    let done_label = ctx.next_label("stream_mode_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed fopen result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // failed opens have no mode to report
            abi::emit_push_reg(ctx.emitter, "x0");                              // the boxed result outlives the mode load and the call
            load_string_to_result(ctx, mode, "fopen mode")?;                    // re-materialize the caller's mode string
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the boxed result without dropping the slot
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle from the Mixed payload
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_mode");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // failed opens have no mode to report
            abi::emit_push_reg(ctx.emitter, "rax");                             // the boxed result outlives the mode load and the call
            load_string_to_result(ctx, mode, "fopen mode")?;                    // re-materialize the caller's mode string
            ctx.emitter.instruction("mov rsi, rax");                            // pass the mode pointer
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the boxed result without dropping the slot
            ctx.emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");            // pass the opaque stream handle from the Mixed payload
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_mode");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Records a URI held in a runtime buffer on the boxed stream's opaque handle.
///
/// For a stream whose path the program never wrote: `tmpfile()` creates its own file, and PHP
/// reports that file as the URI where elephc reported nothing at all.
pub(super) fn emit_record_stream_meta_after_boxed_symbol(
    ctx: &mut FunctionContext<'_>,
    wrapper_id: i64,
    symbol: &str,
    len: i64,
) {
    let done_label = ctx.next_label("stream_meta_symbol_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // a failed open has no metadata to publish
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction(&format!("mov x1, #{}", wrapper_id));
            abi::emit_symbol_address(ctx.emitter, "x2", symbol);                // the buffer holding the URI
            ctx.emitter.instruction(&format!("mov x3, #{}", len));              // and its byte length
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_meta");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // a failed open has no metadata to publish
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction(&format!("mov esi, {}", wrapper_id));
            abi::emit_symbol_address(ctx.emitter, "rdx", symbol);               // the buffer holding the URI
            ctx.emitter.instruction(&format!("mov rcx, {}", len));              // and its byte length
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_meta");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
}

/// Records a fixed mode spelling on the boxed stream's opaque handle.
///
/// For a stream whose mode is not a caller argument at all: `tmpfile()` takes none and PHP
/// reports `r+b` for the handle it hands back, where the descriptor's access bits can only say
/// `r+`.
pub(super) fn emit_record_stream_mode_literal_after_boxed(
    ctx: &mut FunctionContext<'_>,
    mode: &str,
) {
    let (label, len) = ctx.data.add_string(mode.as_bytes());
    let mode_len = len as i64;
    let done_label = ctx.next_label("stream_mode_literal_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // failed opens have no mode to report
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_symbol_address(ctx.emitter, "x1", &label);                // the fixed mode spelling
            ctx.emitter.instruction(&format!("mov x2, #{}", mode_len));         // and its byte length
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the boxed result
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_mode");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // failed opens have no mode to report
            abi::emit_push_reg(ctx.emitter, "rax");
            abi::emit_symbol_address(ctx.emitter, "rsi", &label);               // the fixed mode spelling
            ctx.emitter.instruction(&format!("mov rdx, {}", mode_len));         // and its byte length
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the boxed result
            ctx.emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");            // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_mode");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
}

/// Gives a boxed `fsockopen()` descriptor the transport and `uri` php records for it.
///
/// `fsockopen()` has no address OPERAND to re-materialize — it composes one from a hostname and a
/// port — so the address travels through `_fsockopen_uri_ptr`/`_len` instead of through
/// `load_string_to_result`, and the helper reads it there. The tag guard is the same: a failed
/// open boxed `false`, which owns no stream to describe.
pub(super) fn emit_record_fsockopen_meta_after_boxed(ctx: &mut FunctionContext<'_>) {
    let done_label = ctx.next_label("fsockopen_meta_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // a failed open has nothing to describe
            abi::emit_push_reg(ctx.emitter, "x0");                              // the boxed result outlives the call
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_fsockopen_meta");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // a failed open has nothing to describe
            abi::emit_push_reg(ctx.emitter, "rax");                             // the boxed result outlives the call
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_fsockopen_meta");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
}

/// Records which transport a boxed socket was opened on, for `stream_type` metadata.
///
/// `address` is the operand the caller wrote, re-materialized here so the run-time scheme decides
/// the name exactly as it decides what gets opened. `None` records `fallback` instead, which is how
/// a socket pair and an accepted connection — neither of which names an address — get their name.
pub(super) fn emit_record_stream_transport_after_boxed(
    ctx: &mut FunctionContext<'_>,
    address: Option<ValueId>,
    fallback: u64,
) -> Result<()> {
    let done_label = ctx.next_label("stream_transport_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // a failed open has no transport
            abi::emit_push_reg(ctx.emitter, "x0");                              // the boxed result outlives the call
            match address {
                Some(address) => {
                    load_string_to_result(ctx, address, "socket address")?;     // x1/x2 = the address bytes
                }
                None => {
                    ctx.emitter.instruction("mov x1, #0");                      // no address of its own
                    ctx.emitter.instruction("mov x2, #0");
                }
            }
            ctx.emitter.instruction(&format!("mov x3, #{}", fallback));         // the name to use without one
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the boxed result
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_transport");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // a failed open has no transport
            abi::emit_push_reg(ctx.emitter, "rax");                             // the boxed result outlives the call
            match address {
                Some(address) => {
                    load_string_to_result(ctx, address, "socket address")?;     // rax/rdx = the address bytes
                    ctx.emitter.instruction("mov rsi, rax");                    // pass the address pointer
                }
                None => {
                    ctx.emitter.instruction("xor esi, esi");                    // no address of its own
                    ctx.emitter.instruction("xor edx, edx");
                }
            }
            ctx.emitter.instruction(&format!("mov rcx, {}", fallback));         // the name to use without one
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the boxed result
            ctx.emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");            // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_transport");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Records on a boxed socket the transport its server handle was opened on.
///
/// An accepted connection has no address of its own, and php-src names it after the listener: an
/// accept on a `unix://` server reports `unix_socket`, not `tcp_socket/ssl`.
pub(super) fn emit_inherit_stream_transport_after_boxed(
    ctx: &mut FunctionContext<'_>,
    server: ValueId,
) -> Result<()> {
    let done_label = ctx.next_label("stream_transport_inherit_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // a failed accept has no transport
            abi::emit_push_reg(ctx.emitter, "x0");                              // the boxed result outlives the calls
            load_stream_handle_to_result(ctx, server, "stream_socket_accept")?; // the listener's handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_transport");         // x0 = the listener's transport
            ctx.emitter.instruction("mov x3, x0");                              // record it on the accepted socket
            ctx.emitter.instruction("mov x1, #0");                              // which has no address of its own
            ctx.emitter.instruction("mov x2, #0");
            ctx.emitter.instruction("ldr x0, [sp]");                            // reload the boxed result
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_transport");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // a failed accept has no transport
            abi::emit_push_reg(ctx.emitter, "rax");                             // the boxed result outlives the calls
            load_stream_handle_to_result(ctx, server, "stream_socket_accept")?; // the listener's handle
            ctx.emitter.instruction("mov rdi, rax");                            // pass it to the transport lookup
            abi::emit_call_label(ctx.emitter, "__rt_stream_transport");         // rax = the listener's transport
            ctx.emitter.instruction("mov rcx, rax");                            // record it on the accepted socket
            ctx.emitter.instruction("xor esi, esi");                            // which has no address of its own
            ctx.emitter.instruction("xor edx, edx");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rsp]");                // reload the boxed result
            ctx.emitter.instruction("mov rdi, QWORD PTR [rdi + 8]");            // pass the opaque stream handle
            abi::emit_call_label(ctx.emitter, "__rt_stream_record_transport");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Persists a caller-stashed socket address host on the boxed stream's opaque handle.
pub(super) fn emit_stash_connect_host_after_boxed_stashed(ctx: &mut FunctionContext<'_>) {
    let done_label = ctx.next_label("stream_connect_host_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldr x9, [x0]");                            // inspect the boxed socket result tag
            ctx.emitter.instruction("cmp x9, #9");                              // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("b.ne {}", done_label));           // failed connects have no handle on which to persist a host
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("ldr x0, [x0, #8]");                        // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction("ldr x1, [sp, #16]");                       // reload the caller-stashed socket address pointer
            ctx.emitter.instruction("ldr x2, [sp, #24]");                       // reload the caller-stashed socket address byte length
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
            abi::emit_pop_reg(ctx.emitter, "x0");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp QWORD PTR [rax], 9");                  // runtime tag 9 identifies a stream resource
            ctx.emitter.instruction(&format!("jne {}", done_label));            // failed connects have no handle on which to persist a host
            abi::emit_push_reg(ctx.emitter, "rax");
            ctx.emitter.instruction("mov rdi, QWORD PTR [rax + 8]");            // pass the opaque stream handle from the Mixed payload
            ctx.emitter.instruction("mov rsi, QWORD PTR [rsp + 16]");           // reload the caller-stashed socket address pointer
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 24]");           // reload the caller-stashed socket address byte length
            abi::emit_call_label(ctx.emitter, "__rt_stash_connect_host");
            abi::emit_pop_reg(ctx.emitter, "rax");
        }
    }
    ctx.emitter.label(&done_label);
}
