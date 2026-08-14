//! Purpose:
//! Stream, socket, pathinfo, and stat result boxing.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`.
//!
//! Key details:
//! - Preserves target-aware ABI handling, runtime calls, and result ownership.

use super::*;

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
    box_stream_fd_or_false_result_kind(ctx, label_prefix, 1);
}

/// Boxes a non-negative descriptor as a PHP resource (or false on failure) and
/// records the scope-cleanup `kind` in the Mixed high payload word so
/// `__rt_mixed_free_deep` dispatches the right destructor: 1 = native stream fd
/// (`close`), 3 = `popen` pipe (`__rt_pclose`), 4 = `opendir` stream
/// (`__rt_closedir`).
pub(super) fn box_stream_fd_or_false_result_kind(
    ctx: &mut FunctionContext<'_>,
    label_prefix: &str,
    kind: u64,
) {
    let false_label = ctx.next_label(&format!("{}_false", label_prefix));
    let done_label = ctx.next_label(&format!("{}_done", label_prefix));
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #0");                              // test whether the stream helper returned a negative descriptor
            ctx.emitter.instruction(&format!("b.lt {}", false_label));          // box PHP false when stream creation failed
            abi::emit_call_label(ctx.emitter, "__rt_resource_id_mint");         // mint a FRESH resource id: the kernel reuses descriptor numbers, PHP never reuses ids
            ctx.emitter.instruction("mov x1, x0");                              // pass the native stream fd as the Mixed low payload word
            ctx.emitter.instruction(&format!("mov x2, #{}", kind));             // resource-kind subtype in the Mixed high word (1=fd,3=popen,4=dir)
            ctx.emitter.instruction("mov x0, #9");                              // select runtime tag 9 for a stream resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
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
            abi::emit_call_label(ctx.emitter, "__rt_resource_id_mint");         // mint a FRESH resource id: the kernel reuses descriptor numbers, PHP never reuses ids
            ctx.emitter.instruction("mov rdi, rax");                            // pass the native stream fd as the Mixed low payload word
            ctx.emitter.instruction(&format!("mov esi, {}", kind));             // resource-kind subtype in the Mixed high word (1=fd,3=popen,4=dir)
            ctx.emitter.instruction("mov eax, 9");                              // select runtime tag 9 for a stream resource
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
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

/// Boxes a socket-pair array result or PHP false as `Mixed`.
pub(super) fn box_stream_socket_pair_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stream_socket_pair_false");
    let done_label = ctx.next_label("stream_socket_pair_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // null pointer means socketpair failed
            ctx.emitter.instruction("mov x1, #9");                              // resource tag: each fd becomes Mixed(resource)
            abi::emit_call_label(ctx.emitter, "__rt_array_to_mixed");
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
            );
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
            emit_box_current_value_as_mixed(
                ctx.emitter,
                &PhpType::Array(Box::new(PhpType::Mixed)),
            );
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
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the x86_64 Mixed heap kind word
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
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax], 5");                  // select runtime tag 5 for an associative-array Mixed payload
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned pathinfo hash pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], 0");             // associative-array Mixed payloads do not use a high word
        }
    }
}

/// Boxes a raw floating-point payload into PHP `float|false` Mixed form.
///
/// The float counterpart of `box_stat_int_or_false_result`. The payload arrives in the FLOAT
/// result register (`d0`/`xmm0`) and the success flag in the INT one (`x0`/`rax`), which is why
/// this reads a different register pair than its integer sibling — the two never contend.
///
/// `0.0` is a legitimate byte count (a full filesystem reports zero bytes available), so a
/// helper that signalled failure by returning `0.0` could not be told apart from success. The
/// flag is what makes PHP's `false` reachable at all.
pub(super) fn box_float_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("float_false");
    let done_label = ctx.next_label("float_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // box PHP false when the runtime success flag is unset
            ctx.emitter.instruction("fmov x1, d0");                             // pass the double's bit pattern as the Mixed low payload word
            ctx.emitter.instruction("mov x2, xzr");                             // float Mixed payloads do not use a high word
            ctx.emitter.instruction("mov x0, #2");                              // select runtime tag 2 for a floating-point Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip false boxing after building the float Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("mov x2, #0");                              // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov x0, #3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rax, rax");                           // test whether the runtime success flag is set
            ctx.emitter.instruction(&format!("jz {}", false_label));            // box PHP false when the disk-space helper failed
            ctx.emitter.instruction("movq rdi, xmm0");                          // pass the double's bit pattern as the Mixed low payload word
            ctx.emitter.instruction("xor esi, esi");                            // float Mixed payloads do not use a high word
            ctx.emitter.instruction("mov eax, 2");                              // select runtime tag 2 for a floating-point Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip false boxing after building the float Mixed result
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // use zero as the false payload for the Mixed bool box
            ctx.emitter.instruction("xor esi, esi");                            // clear the unused high payload word for bool Mixed boxes
            ctx.emitter.instruction("mov eax, 3");                              // select runtime tag 3 for a boolean false Mixed value
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
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

/// Boxes the raw stat hash payload into PHP `array|false` Mixed form.
pub(super) fn box_stat_array_or_false_result(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("stat_array_false");
    let done_label = ctx.next_label("stat_array_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cbz x0, {}", false_label));       // branch when the stat runtime returned a null hash pointer
            abi::emit_push_reg(ctx.emitter, "x0");
            ctx.emitter.instruction("mov x0, #24");                             // request a mixed cell payload with tag and two value words
            abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
            ctx.emitter.instruction("mov x9, #5");                              // select heap kind 5 for a boxed Mixed cell
            ctx.emitter.instruction("str x9, [x0, #-8]");                       // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov x9, #5");                              // select runtime tag 5 for an associative-array Mixed payload
            ctx.emitter.instruction("str x9, [x0]");                            // store the associative-array tag in the Mixed cell
            abi::emit_pop_reg(ctx.emitter, "x10");
            ctx.emitter.instruction("str x10, [x0, #8]");                       // store the owned stat hash pointer in the Mixed cell
            ctx.emitter.instruction("str xzr, [x0, #16]");                      // associative-array Mixed payloads do not use a high word
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
            ctx.emitter.instruction(&format!("mov r10, 0x{:x}", crate::codegen_support::sentinels::x86_64_heap_kind_word(5))); // materialize the x86_64 Mixed heap kind word
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");            // stamp the allocation header as a Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax], 5");                  // select runtime tag 5 for an associative-array Mixed payload
            abi::emit_pop_reg(ctx.emitter, "r10");
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");            // store the owned stat hash pointer in the Mixed cell
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], 0");             // associative-array Mixed payloads do not use a high word
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

