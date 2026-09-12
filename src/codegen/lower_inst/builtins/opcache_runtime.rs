//! Purpose:
//! Lowers the three internal `__elephc_opcache_rt_*` builtins the injected
//! `opcache_get_status()` body uses to read the runtime script cache: one figure by key,
//! one cached script's numeric field, and one cached script's path.
//!
//! Called from:
//! - `crate::codegen::lower_inst::runtime_functions` dispatch, through the
//!   `RuntimeFnId::ElephcOpcacheRt*` targets the builtin registry declares.
//!
//! Key details:
//! - THE PAY-FOR-USE GATE LIVES HERE. The runtime script cache exists only in a binary
//!   that links the eval bridge, because a runtime-dynamic include is a compile error at
//!   AOT top level and is reachable only from `eval()`. When `eval_bridge` is false there
//!   is no cache, the correct answer is the empty one, and folding it here is what keeps
//!   `opcache_get_status()` from pulling the interpreter archive into a program that
//!   merely asked for a status array. `RuntimeFeatures` is final before per-instruction
//!   lowering runs (`ir_lower::program` recomputes it twice, last before `validate_module`),
//!   so this reads a settled fact rather than a guess.
//! - `__elephc_eval_opcache_rt_script_path` returns a BORROWED `(ptr, len)` into a
//!   thread-local buffer in the bridge. The bytes are copied into an owned PHP string with
//!   `__rt_str_persist` immediately, before anything can call the bridge again.
//! - The gated-off path still produces a well-typed result: `0` for the integer forms and
//!   an owned empty string for the path form, both of which are what an empty cache
//!   reports anyway.
//! - Bridge calls go through `Target::extern_symbol`, never a bare string. These are C
//!   symbols exported by another crate, and macOS prefixes those with an underscore; a
//!   bare `"__elephc_eval_…"` links on Linux and fails on macOS with "symbol(s) not
//!   found". `__rt_str_persist` is different: it is a label this compiler emits into the
//!   runtime object itself, so it is named literally.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::codegen::Result;
use crate::ir::Instruction;

use super::super::super::context::FunctionContext;
use super::super::resolve_int_operand_to_result;
use super::{expect_operand, store_if_result};

/// Returns whether this binary can have a runtime script cache at all.
///
/// The cache lives in the eval bridge, so a binary without one has no dynamic tier and
/// nothing to report. See the module docblock for why this decides the lowering.
fn links_the_eval_bridge(ctx: &FunctionContext<'_>) -> bool {
    ctx.module.required_runtime_features.eval_bridge
}

/// Leaves an integer `0` in the result register, the empty cache's answer.
fn emit_zero_result(ctx: &mut FunctionContext<'_>) {
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
}

/// Leaves an owned empty PHP string in the string result registers.
///
/// Goes through `__rt_str_persist` with a null source and zero length — the same shape
/// the PHAR bridges use for their "nothing to return" branch — so the result is an owned
/// string like every other one, not a borrowed null.
fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, #0");                              // empty source pointer (length 0 is never dereferenced)
            ctx.emitter.instruction("mov x2, #0");                              // empty string length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, 0");                              // empty source pointer (length 0 is never dereferenced)
            ctx.emitter.instruction("mov rdx, 0");                              // empty string length
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
}

/// Lowers `__elephc_opcache_rt_stat(key)` to the eval bridge's figure reader.
pub(crate) fn lower_opcache_rt_stat(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_opcache_rt_stat", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_opcache_rt_stat()");
    if !links_the_eval_bridge(ctx) {
        emit_zero_result(ctx);
        return store_if_result(ctx, inst);
    }
    resolve_int_operand_to_result(ctx, expect_operand(inst, 0)?, "opcache rt stat key")?;
    super::super::call_operands::move_int_result_to_first_arg(ctx);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_opcache_rt_stat");
    abi::emit_call_label(ctx.emitter, &symbol);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_opcache_rt_reset()` to the bridge's restart scheduler.
///
/// Answers `1` for the call that scheduled the restart and `0` for any later one in the
/// same request, which is the once-then-false shape `opcache_reset()` reports.
///
/// PAY-FOR-USE: a binary with no eval bridge has no dynamic tier to restart, so the call
/// folds to `0` and the interpreter archive is never referenced. The prelude's own latch
/// still answers there, so what such a binary REPORTS does not change.
pub(crate) fn lower_opcache_rt_reset(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_opcache_rt_reset", 0)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_opcache_rt_reset()");
    if !links_the_eval_bridge(ctx) {
        emit_zero_result(ctx);
        return store_if_result(ctx, inst);
    }
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_opcache_schedule_restart");
    abi::emit_call_label(ctx.emitter, &symbol);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_opcache_rt_swap(id, value)` to the bridge's directive setter.
///
/// The one `rt_*` lowering whose bridge call WRITES. It installs a directive on the live
/// runtime-cache configuration and answers the value it replaced, which is what
/// `ini_set()` reports.
///
/// PAY-FOR-USE, same rule as its sibling readers: a binary with no eval bridge has no
/// runtime cache to configure, so the call folds to `0` and the interpreter archive is
/// never referenced. That is not a silent loss — the prelude keeps the REPORTED value in
/// its own override store, so `ini_get()` still moves in such a binary; only the cache
/// that does not exist goes unconfigured.
pub(crate) fn lower_opcache_rt_swap(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_opcache_rt_swap", 2)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_opcache_rt_swap()");
    if !links_the_eval_bridge(ctx) {
        emit_zero_result(ctx);
        return store_if_result(ctx, inst);
    }
    // The VALUE is materialized first and spilled, for the same reason the field reader
    // stages its second operand first: resolving the id can clobber the argument
    // registers, which would lose a value staged before it.
    let value_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(expect_operand(inst, 1)?, value_reg)?;
    abi::emit_push_reg(ctx.emitter, value_reg);
    resolve_int_operand_to_result(ctx, expect_operand(inst, 0)?, "opcache rt swap id")?;
    abi::emit_pop_reg(ctx.emitter, value_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x1, {}", value_reg));                // new value → second bridge argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // directive id → SysV first argument register
            ctx.emitter
                .instruction(&format!("mov rsi, {}", value_reg));               // new value → SysV second argument register
        }
    }
    // `as_override = 1`: this write comes from `ini_set()`, so it lands in the override
    // table and outranks the compiled install that the first eval performs afterwards.
    abi::emit_load_int_immediate(ctx.emitter, abi::int_arg_reg_name(ctx.emitter.target, 2), 1);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_opcache_swap_directive");
    abi::emit_call_label(ctx.emitter, &symbol);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_opcache_rt_script_field(index, field)` to the bridge's field reader.
pub(crate) fn lower_opcache_rt_script_field(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_opcache_rt_script_field", 2)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_opcache_rt_script_field()");
    if !links_the_eval_bridge(ctx) {
        emit_zero_result(ctx);
        return store_if_result(ctx, inst);
    }
    // The FIELD is materialized first and spilled: resolving the index can clobber the
    // argument registers, so staging it second would lose the field.
    let field_reg = abi::secondary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(expect_operand(inst, 1)?, field_reg)?;
    abi::emit_push_reg(ctx.emitter, field_reg);
    resolve_int_operand_to_result(ctx, expect_operand(inst, 0)?, "opcache rt script index")?;
    abi::emit_pop_reg(ctx.emitter, field_reg);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov x1, {}", field_reg));                // field selector → second bridge argument
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // script index → SysV first argument register
            ctx.emitter
                .instruction(&format!("mov rsi, {}", field_reg));               // field selector → SysV second argument register
        }
    }
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_opcache_rt_script_field");
    abi::emit_call_label(ctx.emitter, &symbol);
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_opcache_rt_blacklist_entry(index)` to the bridge's pattern reader.
///
/// Byte-for-byte the same shape as `lower_opcache_rt_script_path` below, including the
/// register dance before `__rt_str_persist`; only the bridge symbol differs.
pub(crate) fn lower_opcache_rt_blacklist_entry(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_opcache_rt_blacklist_entry", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_opcache_rt_blacklist_entry()");
    if !links_the_eval_bridge(ctx) {
        emit_empty_string_result(ctx);
        return store_if_result(ctx, inst);
    }
    resolve_int_operand_to_result(ctx, expect_operand(inst, 0)?, "opcache blacklist index")?;
    super::super::call_operands::move_int_result_to_first_arg(ctx);
    let symbol = ctx
        .emitter
        .target
        .extern_symbol("__elephc_eval_opcache_rt_blacklist_entry");
    abi::emit_call_label(ctx.emitter, &symbol);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The pair lands in x0/x1; str_persist reads ptr from x1 and len from x2, so
            // the length moves FIRST or the pointer write would destroy it.
            ctx.emitter.instruction("mov x2, x1");                              // borrowed length → str_persist length register
            ctx.emitter.instruction("mov x1, x0");                              // borrowed pointer → str_persist source register
        }
        Arch::X86_64 => {
            // The pair lands in rax/rdx, and str_persist already reads the length from rdx.
            ctx.emitter.instruction("mov rdi, rax");                            // borrowed pointer → str_persist source register
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");                      // copy the borrowed bytes into an owned PHP string
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_opcache_rt_script_path(index)` to the bridge's path reader.
///
/// The bridge returns a borrowed `(ptr, len)` pair in the first two result registers; a
/// null pointer means "no such script" and is persisted as the empty string, which the
/// prelude treats as the end of the cached-script list.
pub(crate) fn lower_opcache_rt_script_path(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_opcache_rt_script_path", 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment("__elephc_opcache_rt_script_path()");
    if !links_the_eval_bridge(ctx) {
        emit_empty_string_result(ctx);
        return store_if_result(ctx, inst);
    }
    resolve_int_operand_to_result(ctx, expect_operand(inst, 0)?, "opcache rt script index")?;
    super::super::call_operands::move_int_result_to_first_arg(ctx);
    let symbol = ctx.emitter.target.extern_symbol("__elephc_eval_opcache_rt_script_path");
    abi::emit_call_label(ctx.emitter, &symbol);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // The pair lands in x0/x1; str_persist reads ptr from x1 and len from x2, so
            // the length moves FIRST or the pointer write would destroy it.
            ctx.emitter.instruction("mov x2, x1");                              // borrowed length → str_persist length register
            ctx.emitter.instruction("mov x1, x0");                              // borrowed pointer → str_persist source register
        }
        Arch::X86_64 => {
            // The pair lands in rax/rdx, and str_persist already reads the length from rdx.
            ctx.emitter.instruction("mov rdi, rax");                            // borrowed pointer → str_persist source register
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");                      // copy the borrowed bytes into an owned PHP string
    store_if_result(ctx, inst)
}
