//! Purpose:
//! Emits the module's one copy of the codegen-raised throwable, so the allocate-fill-throw
//! sequence behind a `TypeError`, a `ValueError` or an `Error` exists once per program instead
//! of once per raise site.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` before the module's own functions.
//! - `crate::codegen::lower_inst::exceptions::emit_static_exception_at()` at every raise.
//!
//! Key details:
//! - Measured on the compiled Symfony `--web` app: **165 115 inlined raises, 53.5 lines each,
//!   8 834 931 lines — 14.4% of a 61 455 536-line module.** Everything in those lines is
//!   identical except a class-id symbol, a message pointer and length, a creation line, and the
//!   uncaught report's text; all five become one interned data record, and the site becomes a
//!   symbol address plus a call.
//! - The sequence is MOVED, not replaced: the same instructions in the same order, reading the
//!   five values from the record instead of from immediates. What can go wrong is structural —
//!   a bad register, a missing symbol — never a differently shaped throwable.
//! - The helper never returns. It either writes PHP's uncaught report and exits, or jumps to
//!   `__rt_throw_current`, which unwinds through the helper frame to the caller's handler.
//!   `crate::codegen::shared_helper` documents that this is sound and pins it with
//!   `test_a_throw_inside_the_shared_string_ladder_is_still_catchable`.

use crate::codegen::context::FunctionContext;
use crate::codegen::data_section::{DataSection, DataWord};
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::codegen::shared_state::SharedCodegenState;
use crate::codegen::abi;
use crate::codegen_support::runtime::UNCAUGHT_EXIT_STATUS;
use crate::codegen_support::throwable_layout;
use crate::ir::Module;
use crate::types::PhpType;

use super::shared_helper::emit_shared_helper;
use super::Result;

/// Label of the helper that raises one built-in throwable described by a data record.
pub(super) const STATIC_THROW_LABEL: &str = "_eir_shared_static_throw";

/// Byte offsets of the record the helper reads, in `add_words()` order.
mod field {
    /// Address of the class-id global (`_spl_type_error_class_id`, …), not the id itself.
    pub(super) const CLASS_ID_SYMBOL: usize = 0;
    /// Pointer to the exception message bytes.
    pub(super) const MESSAGE: usize = 8;
    /// Length of the exception message.
    pub(super) const MESSAGE_LEN: usize = 16;
    /// One-based source line of the `new` behind the raise, or zero when unknown.
    pub(super) const CREATION_LINE: usize = 24;
    /// Pointer to the complete uncaught-report text, newlines included.
    pub(super) const FATAL: usize = 32;
    /// Length of the uncaught-report text.
    pub(super) const FATAL_LEN: usize = 40;
    /// Which report the no-handler path writes; see `REPORT_UNCAUGHT`/`REPORT_TYPED_PROPERTY`.
    pub(super) const REPORT: usize = 48;
}

/// Creation line of a raise inside a shared helper: the helper has no PHP line of its own, so the
/// throwable takes the line of the statement that called it, published in `_php_site`.
pub(in crate::codegen) const LINE_OF_CALLING_SITE: u32 = u32::MAX;

/// Returns whether `ctx` is a shared helper body, whose raises belong to the calling statement.
pub(in crate::codegen) fn raises_on_calling_site(ctx: &FunctionContext<'_>) -> bool {
    ctx.function.flags.is_synthetic && ctx.function.name.starts_with("_eir_shared_")
}

/// php's uncaught-throwable report: flush the output buffer, write to fd 1, exit 255.
pub(in crate::codegen) const REPORT_UNCAUGHT: u64 = 0;

/// The uninitialised typed-property fatal: no flush, write to fd 2, exit 1.
///
/// Those three differences from the uncaught report are not improvements waiting to happen —
/// they are what the inlined sequence did, and this is a move.
pub(in crate::codegen) const REPORT_TYPED_PROPERTY: u64 = 1;

/// Returns whether this module routes its codegen-raised throwables through the shared helper.
///
/// The emitter and the raise sites both ask THIS function, so they cannot disagree about whether
/// the helper exists — a disagreement would either leave an unresolved label or emit a body
/// nothing calls.
///
/// WHY THERE IS NO SIZE GATE, unlike the `count()` guard next door. How many raises a module
/// emits cannot be read off its IR: a raise comes from a guard the BACKEND decides to emit — a
/// boxed `count()`, a division, an argument check — not from an instruction that names it. Every
/// proxy measured is noise: `<?php echo "hi";` already carries 231 EIR instructions and 8 bodies
/// from the built-in Throwable hierarchy, and raises nothing, while the same program with one
/// `count($mixed)` in it raises eight times at 251.
///
/// Deciding it LATER, from whether a site actually called the helper, is the exact answer and is
/// unsafe here: the first body to reach the helper may be one a codegen worker defers, and its
/// text is then discarded while the "already emitted" flag is not — which is how a later body
/// comes to call a symbol nothing defines. Emitting it up front has no such window.
///
/// So the helper is emitted whenever it can be used at all. The only program that pays for it is
/// one that raises NOTHING: measured on `<?php echo "hi";`, 6 002 lines of assembly become
/// 6 066, **+64 lines, +1.1%**. Everything that does raise is ahead — the same probes measured
/// -0.4% (one raise) to -4.2% (eight), and the Symfony `--web` module has 165 115 of them.
pub(super) fn module_shares_static_throw(
    _module: &Module,
    shared: &mut SharedCodegenState,
) -> bool {
    if let Some(cached) = shared.static_throw_sharing() {
        return cached;
    }
    // `--counters` and `--instrument` weave per-body records into the exit path, and the exit
    // path is exactly what moves. Leave those builds byte-identical to what they measured.
    let shares = !shared.counters && !shared.instrument.is_on();
    shared.set_static_throw_sharing(shares);
    shares
}

/// Returns the helper label a raise in `ctx` should call, if any.
///
/// `None` inside the helper itself, which is what stops the body from calling the label it is
/// defining.
pub(in crate::codegen) fn shared_throw_label(ctx: &mut FunctionContext<'_>) -> Option<&'static str> {
    if ctx.function.name == STATIC_THROW_LABEL {
        return None;
    }
    let module = ctx.module;
    module_shares_static_throw(module, ctx.shared).then_some(STATIC_THROW_LABEL)
}

/// Interns the record describing one raise and returns its data label.
///
/// `add_words()` deduplicates by content, so two sites that raise the same class with the same
/// message and line share one record — which is most of them, because the message is a fixed
/// string the guard chose.
#[allow(clippy::too_many_arguments)]
pub(in crate::codegen) fn throw_descriptor(
    data: &mut DataSection,
    class_id_symbol: &str,
    message_label: &str,
    message_len: usize,
    creation_line: u32,
    fatal_label: &str,
    fatal_len: usize,
    report: u64,
) -> String {
    data.add_words(vec![
        DataWord::Symbol(class_id_symbol.to_string()),
        DataWord::Symbol(message_label.to_string()),
        DataWord::U64(message_len as u64),
        DataWord::U64(u64::from(creation_line)),
        DataWord::Symbol(fatal_label.to_string()),
        DataWord::U64(fatal_len as u64),
        DataWord::U64(report),
    ])
}

/// Emits the shared raise when the module uses it.
pub(super) fn emit_shared_static_throw(
    module: &Module,
    emitter: &mut Emitter,
    data: &mut DataSection,
    shared: &mut SharedCodegenState,
    regalloc_linear: bool,
) -> Result<()> {
    if !module_shares_static_throw(module, shared) {
        return Ok(());
    }
    emit_shared_helper(
        module,
        emitter,
        data,
        shared,
        regalloc_linear,
        STATIC_THROW_LABEL,
        PhpType::Void,
        &format!(
            "--- shared codegen-raised throwable: {} (record address in the int result register) ---",
            STATIC_THROW_LABEL
        ),
        emit_static_throw_from_result,
    )
}

/// Raises the throwable the record in the int result register describes.
///
/// The record survives the two calls below in the reserved nested-call register, which is
/// callee-saved and which `shared_helper`'s entry sequence has already preserved for the caller.
fn emit_static_throw_from_result(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let record = abi::nested_call_reg(ctx.emitter);
    let result = abi::int_result_reg(ctx.emitter);
    let throw_label = ctx.next_label("static_exception_throw");
    let typed_property_label = ctx.next_label("typed_property_throw");
    let line_known = ctx.next_label("static_exception_line_known");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", record, result)); // keep the record across the allocating calls below
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_exc_handler_top", 0);
            ctx.emitter
                .instruction(&format!("cbnz x9, {}", throw_label));          // use the standard unwinder when a catch handler is active
            abi::emit_load_from_address(ctx.emitter, "x9", record, field::REPORT);
            ctx.emitter
                .instruction(&format!("cbnz x9, {}", typed_property_label)); // the typed-property fatal writes a different report
            if ctx.backtrace_enabled {
                // Frame readers are live: the standard unwinder reaches the full uncaught report,
                // which prints PHP's `Stack trace:` through the frames the error escaped.
                abi::emit_jump(ctx.emitter, &throw_label);
            }
            // Drain buffered output first: PHP emits it before the report.
            ctx.emitter.instruction("bl __rt_ob_flush_all");                 // drain buffered output before the fatal report
            abi::emit_load_from_address(ctx.emitter, "x1", record, field::FATAL);
            abi::emit_load_from_address(ctx.emitter, "x2", record, field::FATAL_LEN);
            ctx.emitter.instruction("mov x0, #1");                           // fd = stdout, where PHP writes this report
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, UNCAUGHT_EXIT_STATUS);
            ctx.emitter.label(&typed_property_label);
            abi::emit_load_from_address(ctx.emitter, "x1", record, field::FATAL);
            abi::emit_load_from_address(ctx.emitter, "x2", record, field::FATAL_LEN);
            ctx.emitter.instruction("mov x0, #2");                           // select stderr for the uninitialized typed-property fatal
            ctx.emitter.syscall(4);
            abi::emit_exit(ctx.emitter, 1);
            ctx.emitter.label(&throw_label);
            throwable_layout::emit_allocate(ctx.emitter, throwable_layout::PAYLOAD_SIZE);
            ctx.emitter.instruction("mov x9, #6");                           // heap kind 6 = throwable object instance
            ctx.emitter.instruction("str x9, [x0, #-8]");                    // stamp the allocation as a runtime object
            ctx.emitter.instruction("bl __rt_object_handle_acquire");        // bind the new object to its PHP object handle
            abi::emit_load_from_address(ctx.emitter, "x9", record, field::CLASS_ID_SYMBOL);
            ctx.emitter.instruction("ldr x9, [x9]");                         // read the built-in throwable class id through its global
            ctx.emitter.instruction("str x9, [x0]");                         // store the built-in throwable class id
            abi::emit_load_from_address(ctx.emitter, "x9", record, field::MESSAGE);
            ctx.emitter.instruction("str x9, [x0, #8]");                     // store the static exception message pointer
            abi::emit_load_from_address(ctx.emitter, "x9", record, field::MESSAGE_LEN);
            ctx.emitter.instruction("str x9, [x0, #16]");                    // store the exception message length
            ctx.emitter.instruction("str xzr, [x0, #24]");                   // exception code defaults to zero
            abi::emit_load_from_address(ctx.emitter, "x9", record, field::CREATION_LINE);
            ctx.emitter.instruction("mov w10, #-1");                         // the calling-site line sentinel, u32::MAX
            ctx.emitter.instruction("cmp x9, x10");                          // does the line come from the calling site?
            ctx.emitter.instruction(&format!("b.ne {}", line_known));        // no: the raise baked its own line
            // A shared helper's raise names the line of the PHP statement that called it, which
            // that statement published in `_php_site` ({file descriptor, line}).
            let site = abi::php_site_symbol(&ctx.emitter.target);
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", &site, 0);
            ctx.emitter.instruction("ldr x9, [x9, #8]");                     // the calling statement's line
            ctx.emitter.label(&line_known);
            abi::emit_store_to_address(ctx.emitter, "x9", "x0", throwable_layout::LINE_OFFSET);
            ctx.emitter.instruction(&format!(
                "str xzr, [x0, #{}]",
                throwable_layout::PREVIOUS_OFFSET
            ));                                                              // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "x0", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov {}, {}", record, result)); // keep the record across the allocating calls below
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_exc_handler_top", 0);
            ctx.emitter.instruction("test r10, r10");                        // check whether a catch handler is active
            ctx.emitter
                .instruction(&format!("jnz {}", throw_label));               // use the standard unwinder when a handler can receive the error
            abi::emit_load_from_address(ctx.emitter, "r10", record, field::REPORT);
            ctx.emitter.instruction("test r10, r10");                        // classify which report this raise writes
            ctx.emitter
                .instruction(&format!("jnz {}", typed_property_label));      // the typed-property fatal writes a different report
            if ctx.backtrace_enabled {
                // Frame readers are live: the standard unwinder reaches the full uncaught report.
                abi::emit_jump(ctx.emitter, &throw_label);
            }
            // The helper frame is already 16-byte aligned, and the report text lives in the
            // record rather than in stack temporaries, so no rsp dance is needed here.
            ctx.emitter.instruction("call __rt_ob_flush_all");               // drain buffered output before the fatal report
            abi::emit_load_from_address(ctx.emitter, "rsi", record, field::FATAL);
            abi::emit_load_from_address(ctx.emitter, "rdx", record, field::FATAL_LEN);
            ctx.emitter.instruction("mov edi, 1");                           // fd = stdout, where PHP writes this report
            ctx.emitter.instruction("mov eax, 1");                           // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                              // emit the specific fatal message
            abi::emit_exit(ctx.emitter, UNCAUGHT_EXIT_STATUS);
            ctx.emitter.label(&typed_property_label);
            abi::emit_load_from_address(ctx.emitter, "rsi", record, field::FATAL);
            abi::emit_load_from_address(ctx.emitter, "rdx", record, field::FATAL_LEN);
            ctx.emitter.instruction("mov edi, 2");                           // select stderr for the uninitialized typed-property fatal
            ctx.emitter.instruction("mov eax, 1");                           // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                              // emit the typed-property fatal diagnostic
            abi::emit_exit(ctx.emitter, 1);
            ctx.emitter.label(&throw_label);
            throwable_layout::emit_allocate(ctx.emitter, throwable_layout::PAYLOAD_SIZE);
            ctx.emitter.instruction(&format!(
                "mov r10, 0x{:x}",
                crate::codegen_support::sentinels::x86_64_heap_kind_word(6)
            ));                                                              // stamp the canonical x86_64 heap-kind word (magic + kind 6 throwable)
            ctx.emitter.instruction("mov QWORD PTR [rax - 8], r10");         // stamp the allocation as a runtime object
            ctx.emitter.instruction("call __rt_object_handle_acquire");      // bind the new object to its PHP object handle
            abi::emit_load_from_address(ctx.emitter, "r10", record, field::CLASS_ID_SYMBOL);
            ctx.emitter.instruction("mov r10, QWORD PTR [r10]");             // read the built-in throwable class id through its global
            ctx.emitter.instruction("mov QWORD PTR [rax], r10");             // store the built-in throwable class id
            abi::emit_load_from_address(ctx.emitter, "r10", record, field::MESSAGE);
            ctx.emitter.instruction("mov QWORD PTR [rax + 8], r10");         // store the static exception message pointer
            abi::emit_load_from_address(ctx.emitter, "r10", record, field::MESSAGE_LEN);
            ctx.emitter.instruction("mov QWORD PTR [rax + 16], r10");        // store the exception message length
            ctx.emitter.instruction("mov QWORD PTR [rax + 24], 0");          // exception code defaults to zero
            abi::emit_load_from_address(ctx.emitter, "r10", record, field::CREATION_LINE);
            ctx.emitter.instruction("mov r11d, 0xffffffff");                 // the calling-site line sentinel, u32::MAX
            ctx.emitter.instruction("cmp r10, r11");                         // does the line come from the calling site?
            ctx.emitter.instruction(&format!("jne {}", line_known));         // no: the raise baked its own line
            // A shared helper's raise names the line of the PHP statement that called it, which
            // that statement published in `_php_site` ({file descriptor, line}).
            let site = abi::php_site_symbol(&ctx.emitter.target);
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", &site, 0);
            ctx.emitter.instruction("mov r10, QWORD PTR [r10 + 8]");         // the calling statement's line
            ctx.emitter.label(&line_known);
            abi::emit_store_to_address(ctx.emitter, "r10", "rax", throwable_layout::LINE_OFFSET);
            ctx.emitter.instruction(&format!(
                "mov QWORD PTR [rax + {}], 0",
                throwable_layout::PREVIOUS_OFFSET
            ));                                                              // previous defaults to null
            abi::emit_store_reg_to_symbol(ctx.emitter, "rax", "_exc_value", 0);
            abi::emit_jump(ctx.emitter, "__rt_throw_current");
        }
    }
    Ok(())
}
