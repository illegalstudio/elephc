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
//! - `__elephc_eval_opcache_rt_script_path` writes a BORROWED `(ptr, len)` into an explicit
//!   output record backed by a thread-local bridge buffer. The bytes are copied into an owned PHP string with
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
            // `rax`, NOT `rdi`: `__rt_str_persist` documents its input as the x86_64 string
            // RESULT pair (`rax`/`rdx`) rather than the SysV argument registers, and its first
            // instruction is `cmp rax, r10`. Writing `rdi` left `rax` holding whatever the
            // previous instruction had put there, so the "empty string" was whatever that was.
            ctx.emitter.instruction("mov rax, 0");                              // empty source pointer (length 0 is never dereferenced)
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
    let arg = abi::runtime_helper_int_arg_reg(ctx.emitter, 0).to_string();
    let result = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_reg_move(ctx.emitter, &arg, &result);
    ctx.emitter.emit_native_bridge_symbol_call("__elephc_eval_opcache_rt_stat", 1);
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
    ctx.emitter.emit_native_bridge_symbol_call("__elephc_eval_opcache_schedule_restart", 0);
    store_if_result(ctx, inst)
}

/// Lowers the three path-taking runtime-tier helpers to their bridge symbols.
///
/// `__elephc_opcache_rt_is_cached`, `_rt_discard` and `_rt_compile` differ only in which
/// symbol they call, so they share one lowering rather than three copies of the same
/// argument staging. Each takes the path as a PHP string and answers an integer boolean.
///
/// THE FIRST STRING ARGUMENT IN THIS FAMILY. Every other `rt_*` helper passes integers, so
/// the staging is spelled out here: the operand is materialized into the string RESULT pair
/// and then moved into the first two integer argument registers. The move order matters on
/// AArch64 and only there — the result pair is `(x1, x2)` and the argument registers are
/// `x0, x1`, so the pointer has to leave `x1` before the length is written into it. On
/// x86_64 the pair is `(rax, rdx)` against `rdi, rsi` and nothing overlaps; writing the move
/// in one order for both arches is what keeps that asymmetry from becoming a latent bug the
/// next reader has to rediscover.
///
/// PAY-FOR-USE, the same rule as every sibling: a binary with no eval bridge has no dynamic
/// tier to ask about, so the call folds to `0` and the interpreter archive is never
/// referenced. The prelude's manifest answer still stands there, which is the whole answer
/// for a binary that cannot have dynamically cached scripts in the first place.
pub(crate) fn lower_opcache_rt_path_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    ctx.emitter.blank();
    ctx.emitter.comment(&format!("{name}()"));
    if !links_the_eval_bridge(ctx) {
        emit_zero_result(ctx);
        return store_if_result(ctx, inst);
    }
    ctx.load_value_to_result(expect_operand(inst, 0)?)?;
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    let ptr_arg = abi::runtime_helper_int_arg_reg(ctx.emitter, 0);
    let len_arg = abi::runtime_helper_int_arg_reg(ctx.emitter, 1);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter
                .instruction(&format!("mov {ptr_arg}, {ptr_reg}"));                // path pointer out of x1 BEFORE the length overwrites it
            ctx.emitter
                .instruction(&format!("mov {len_arg}, {len_reg}"));                // path length → second bridge argument
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {ptr_arg}, {ptr_reg}"));                // path pointer → SysV first argument register
            ctx.emitter
                .instruction(&format!("mov {len_arg}, {len_reg}"));                // path length → SysV second argument register
        }
    }
    ctx.emitter.emit_native_bridge_symbol_call(
        &format!("__elephc_eval_opcache_rt_{}", &name["__elephc_opcache_rt_".len()..]), 2);
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
    abi::emit_load_int_immediate(ctx.emitter, abi::runtime_helper_int_arg_reg(ctx.emitter, 2), 1);
    ctx.emitter.emit_native_bridge_symbol_call("__elephc_eval_opcache_swap_directive", 3);
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
    ctx.emitter.emit_native_bridge_symbol_call("__elephc_eval_opcache_rt_script_field", 2);
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
    emit_borrowed_string_call(ctx, "__elephc_eval_opcache_rt_blacklist_entry");
    store_if_result(ctx, inst)
}

/// Lowers `__elephc_opcache_rt_script_path(index)` to the bridge's path reader.
///
/// The bridge writes a borrowed `(ptr, len)` pair into an explicit output record; a
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
    emit_borrowed_string_call(ctx, "__elephc_eval_opcache_rt_script_path");
    store_if_result(ctx, inst)
}

/// Copies one borrowed bridge string through an explicit, ABI-stable output record.
fn emit_borrowed_string_call(ctx: &mut FunctionContext<'_>, symbol: &str) {
    abi::emit_reserve_temporary_stack(ctx.emitter, 16);
    let index_arg = abi::runtime_helper_int_arg_reg(ctx.emitter, 0).to_string();
    let index_result = abi::int_result_reg(ctx.emitter).to_string();
    abi::emit_reg_move(ctx.emitter, &index_arg, &index_result);
    let out_arg = abi::runtime_helper_int_arg_reg(ctx.emitter, 1).to_string();
    abi::emit_temporary_stack_address(ctx.emitter, &out_arg, 0);
    ctx.emitter.emit_native_bridge_symbol_call(symbol, 2);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldp x1, x2, [sp]");                        // load the borrowed pointer and length before retiring output storage
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rax, QWORD PTR [rsp]");                // load the borrowed pointer into the string-result register
            ctx.emitter.instruction("mov rdx, QWORD PTR [rsp + 8]");            // load the borrowed byte length into the string-result register
        }
    }
    abi::emit_release_temporary_stack(ctx.emitter, 16);
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");                      // copy the borrowed bytes into an owned PHP string
}
