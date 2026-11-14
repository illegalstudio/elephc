//! Purpose:
//! Lowers the five internal resource helpers the injected proc prelude composes:
//! `__elephc_resource_payload`, `__elephc_resource_kind`, `__elephc_resource_close`,
//! `__elephc_proc_resource` and `__elephc_fd_stream`.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions::group_15`.
//!
//! Key details:
//! - A resource is a tag-9 Mixed cell: low word = native payload, high word = kind.
//!   The two readers answer -1 for anything that is not a resource, so the prelude can
//!   raise php-src's own TypeError texts in PHP.
//! - A process resource is kind 11 (`PROCESS_RESOURCE_KIND`), which has no scope-exit
//!   destructor and which `__rt_resource_type_name` reports as `process`.
//! - `__elephc_resource_close` stamps the same `-id` closed sentinel an explicit
//!   `fclose()` writes, without touching the native handle, and is a no-op for a value
//!   that is not an open resource.

use super::*;
use crate::codegen_support::runtime::PROCESS_RESOURCE_KIND;

/// Which word of a boxed resource a reader returns.
#[derive(Clone, Copy)]
enum ResourceWord {
    /// The low payload word (descriptor, process id, or `-id` once closed).
    Payload,
    /// The high kind word.
    Kind,
}

/// Lowers `__elephc_resource_payload($value)`.
pub(crate) fn lower_elephc_resource_payload(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_resource_word(ctx, inst, "__elephc_resource_payload", ResourceWord::Payload)
}

/// Lowers `__elephc_resource_kind($value)`.
pub(crate) fn lower_elephc_resource_kind(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_resource_word(ctx, inst, "__elephc_resource_kind", ResourceWord::Kind)
}

/// Reads one word of a boxed resource into the integer result, or -1 for a non-resource.
///
/// An unboxed `Resource`-typed operand is a bare native stream descriptor, so its payload
/// is the value itself and its kind is 1 (plain stream descriptor).
fn lower_resource_word(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    word: ResourceWord,
) -> Result<()> {
    super::super::ensure_arg_count(inst, name, 1)?;
    let value = expect_operand(inst, 0)?;
    let result = abi::int_result_reg(ctx.emitter);
    match ctx.raw_value_php_type(value)? {
        PhpType::Resource(_) => match word {
            ResourceWord::Payload => {
                ctx.load_value_to_result(value)?;
            }
            ResourceWord::Kind => abi::emit_load_int_immediate(ctx.emitter, result, 1),
        },
        PhpType::Mixed | PhpType::Union(_) => {
            ctx.load_value_to_result(value)?;
            emit_unboxed_resource_word(ctx, word);
        }
        _ => abi::emit_load_int_immediate(ctx.emitter, result, -1),
    }
    store_if_result(ctx, inst)
}

/// Unboxes the Mixed cell in the integer result register and keeps the selected word of
/// a tag-9 payload, or -1 for any other tag.
fn emit_unboxed_resource_word(ctx: &mut FunctionContext<'_>, word: ResourceWord) {
    let not_resource = ctx.next_label("resource_word_not_resource");
    let done = ctx.next_label("resource_word_done");
    // -- unbox the Mixed cell into tag + payload words --
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match (ctx.emitter.target.arch, word) {
        (Arch::AArch64, _) => {
            // -- keep the selected word of a resource cell, or answer -1 --
            ctx.emitter.instruction("cmp x0, #9");                              // is the boxed value a resource?
            ctx.emitter.instruction(&format!("b.ne {not_resource}"));           // anything else answers -1
            match word {
                ResourceWord::Payload => ctx.emitter.instruction("mov x0, x1"), // return the native payload low word
                ResourceWord::Kind => ctx.emitter.instruction("mov x0, x2"),    // return the resource kind high word
            }
            ctx.emitter.instruction(&format!("b {done}"));                      // skip the non-resource answer
            ctx.emitter.label(&not_resource);
            ctx.emitter.instruction("mov x0, #-1");                             // -1 marks a value that is not a resource
        }
        (Arch::X86_64, _) => {
            // -- keep the selected word of a resource cell, or answer -1 --
            ctx.emitter.instruction("cmp rax, 9");                              // is the boxed value a resource?
            ctx.emitter.instruction(&format!("jne {not_resource}"));            // anything else answers -1
            match word {
                ResourceWord::Payload => ctx.emitter.instruction("mov rax, rdi"), // return the native payload low word
                ResourceWord::Kind => ctx.emitter.instruction("mov rax, rdx"),  // return the resource kind high word
            }
            ctx.emitter.instruction(&format!("jmp {done}"));                    // skip the non-resource answer
            ctx.emitter.label(&not_resource);
            ctx.emitter.instruction("mov rax, -1");                             // -1 marks a value that is not a resource
        }
    }
    ctx.emitter.label(&done);
}

/// Lowers `__elephc_resource_close($resource)`: stamps the closed sentinel into an open
/// resource's Mixed cell so every alias reads as closed (`Unknown`), and does nothing for
/// an unboxed handle, a non-resource, or an already closed resource.
pub(crate) fn lower_elephc_resource_close(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_resource_close", 1)?;
    let value = expect_operand(inst, 0)?;
    if !capture_resource_box_for_release(ctx, value)? {
        return store_if_result(ctx, inst);
    }
    let skip = ctx.next_label("resource_close_skip");
    let done = ctx.next_label("resource_close_done");
    // -- unbox the cell and skip anything that is not an open resource --
    ctx.load_value_to_result(value)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x0, #9");                              // only a resource cell can be closed
            ctx.emitter.instruction(&format!("b.ne {skip}"));                   // leave every other value untouched
            ctx.emitter.instruction(&format!("tbnz x1, #63, {skip}"));          // a negative payload is already closed
            ctx.emitter.instruction("mov x0, x1");                              // the sentinel helper keys the id on the payload
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("cmp rax, 9");                              // only a resource cell can be closed
            ctx.emitter.instruction(&format!("jne {skip}"));                    // leave every other value untouched
            ctx.emitter.instruction("test rdi, rdi");                           // inspect the payload sign
            ctx.emitter.instruction(&format!("js {skip}"));                     // a negative payload is already closed
            ctx.emitter.instruction("mov rax, rdi");                            // the sentinel helper keys the id on the payload
        }
    }
    // -- stamp the closed sentinel into the resource cell --
    emit_resource_release_sentinel(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- join the paths, dropping the stash slot on the skip path --
            ctx.emitter.instruction(&format!("b {done}"));                      // the sentinel already released the stash slot
            ctx.emitter.label(&skip);
            ctx.emitter.instruction("add sp, sp, #16");                         // release the unused stash slot
        }
        Arch::X86_64 => {
            // -- join the paths, dropping the stash slot on the skip path --
            ctx.emitter.instruction(&format!("jmp {done}"));                    // the sentinel already released the stash slot
            ctx.emitter.label(&skip);
            ctx.emitter.instruction("add rsp, 16");                             // release the unused stash slot
        }
    }
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_proc_resource($process)`: boxes a bridge process id as a kind-11
/// resource with a fresh PHP resource id, or `false` for a negative id.
pub(crate) fn lower_elephc_proc_resource(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_proc_resource", 1)?;
    let process = expect_operand(inst, 0)?;
    ctx.load_value_to_result(process)?;
    box_resource_payload_or_false_result(ctx, "proc_resource", PROCESS_RESOURCE_KIND);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_fd_stream($fd)`: boxes a native descriptor as an ordinary kind-1
/// stream resource (closed at scope exit like any `fopen()` stream), or `false` for -1.
pub(crate) fn lower_elephc_fd_stream(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::super::ensure_arg_count(inst, "__elephc_fd_stream", 1)?;
    let fd = expect_operand(inst, 0)?;
    ctx.load_value_to_result(fd)?;
    box_stream_fd_or_false_result(ctx, "fd_stream");
    store_if_result(ctx, inst)
}
