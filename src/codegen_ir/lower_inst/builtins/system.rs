//! Purpose:
//! Lowers date/time system builtins for the EIR backend.
//! Marshals already-evaluated EIR operands into the shared runtime helpers.
//!
//! Called from:
//! - `crate::codegen_ir::lower_inst::builtins::lower_builtin_call()`.
//!
//! Key details:
//! - Time builtins are effectful and must reuse the target-aware runtime
//!   helpers rather than duplicating libc/syscall behavior in the EIR backend.

use crate::codegen::abi;
use crate::codegen::platform::{Arch, Platform};
use crate::codegen_ir::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

/// Lowers `date(format, timestamp?)` through the shared formatter runtime helper.
pub(super) fn lower_date(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_date_like(ctx, inst, "date", "__rt_date")
}

/// Lowers `gmdate(format[, timestamp])`: the UTC counterpart of `date()`.
///
/// Identical argument marshalling to `date()`, but dispatches to `__rt_gmdate`, which formats
/// the instant in UTC regardless of the active default timezone.
pub(super) fn lower_gmdate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_date_like(ctx, inst, "gmdate", "__rt_gmdate")
}

/// Shared lowering for `date`/`gmdate`: marshals the optional timestamp and format, then calls
/// `runtime_symbol` (`__rt_date` for local time, `__rt_gmdate` for UTC). `name` is used for the
/// argument-count diagnostic only.
fn lower_date_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_symbol: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 1, 2)?;
    let format = expect_operand(inst, 0)?;
    let timestamp = inst.operands.get(1).copied();

    // Materialize the format string first, then stage it across timestamp loading:
    // coercing a boxed Mixed timestamp calls a runtime helper that clobbers the
    // string registers, so the format pointer/length are parked on the stack and
    // restored immediately before the formatter call. Materializing the format
    // first also lets it be a boxed Mixed value (e.g. a foreach loop variable).
    load_date_string_arg(ctx, format, "date format")?;
    stage_date_string_regs(ctx);
    load_date_timestamp(ctx, timestamp)?;
    unstage_date_string_regs(ctx);
    abi::emit_call_label(ctx.emitter, runtime_symbol);
    store_if_result(ctx, inst)
}

/// Lowers `date_default_timezone_get()` through the shared runtime helper.
///
/// Takes no arguments; `__rt_date_default_timezone_get` returns the stored timezone
/// identifier (or the literal `"UTC"` when none was set) in the string-result registers.
pub(super) fn lower_date_default_timezone_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "date_default_timezone_get", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_date_default_timezone_get");
    store_if_result(ctx, inst)
}

/// Lowers `date_default_timezone_set(timezoneId)` through the shared runtime helper.
///
/// Materializes the identifier string into the registers the helper reads (ptr/len in
/// `x1`/`x2` on ARM64, `rax`/`rdx` on x86_64), then `__rt_date_default_timezone_set`
/// applies it via libc `putenv`+`tzset` and returns PHP `true` in the integer-result register.
pub(super) fn lower_date_default_timezone_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "date_default_timezone_set", 1)?;
    let identifier = expect_operand(inst, 0)?;
    require_string(
        ctx.value_php_type(identifier)?,
        "date_default_timezone_set timezone",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(identifier, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(identifier, "rax", "rdx")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_date_default_timezone_set");
    store_if_result(ctx, inst)
}

/// Lowers `microtime()` / `microtime(true)` / `microtime(false)` / `microtime($flag)`.
///
/// Dispatch is driven by the arg-aware result type set in `ir_lower` (see
/// `call_return_type_for_args` and the `microtime` fallback in `call_return_type`):
/// `Float` (literal `true`) calls the existing `__rt_microtime` float helper; `Str`
/// (omitted / literal `false`) calls `__rt_microtime_str`, which builds the
/// "0.NNNNNNNN sec" string on the stack and persists it; `Mixed` (non-literal flag)
/// marshals the flag and calls `__rt_microtime_mixed`, which branches at runtime and
/// boxes either the string or the float.
pub(super) fn lower_microtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "microtime", 0, 1)?;
    match inst.result_php_type.codegen_repr() {
        PhpType::Float => {
            // microtime(true): existing float helper, result in d0/xmm0.
            abi::emit_call_label(ctx.emitter, "__rt_microtime");
        }
        PhpType::Str => {
            // microtime() / microtime(false): the "0.NNNNNNNN sec" string form.
            abi::emit_call_label(ctx.emitter, "__rt_microtime_str");
        }
        _ => {
            // microtime($flag): the flag is a runtime value, so box string|float as Mixed.
            if let Some(as_float) = inst.operands.first().copied() {
                match ctx.emitter.target.arch {
                    Arch::AArch64 => materialize_integer_arg(ctx, as_float, "x0", "microtime as_float")?,
                    Arch::X86_64 => materialize_integer_arg(ctx, as_float, "rdi", "microtime as_float")?,
                }
            }
            abi::emit_call_label(ctx.emitter, "__rt_microtime_mixed");
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers `mktime(hour, minute, second, month, day, year)` through the runtime helper.
pub(super) fn lower_mktime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_mktime_like(ctx, inst, "mktime", "__rt_mktime")
}

/// Lowers `gmmktime(...)`: the UTC counterpart of `mktime()`.
///
/// Identical six-integer argument marshalling, but dispatches to `__rt_gmmktime`, which
/// interprets the broken-down date/time as UTC (`timegm`) instead of local time.
pub(super) fn lower_gmmktime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_mktime_like(ctx, inst, "gmmktime", "__rt_gmmktime")
}

/// Lowers `checkdate(month, day, year)` through the shared Gregorian-validation runtime helper.
///
/// Marshals the three integers into the leading ABI argument registers (unboxing any boxed
/// `Mixed`/`Union` argument), then calls `__rt_checkdate`, which returns PHP `true`/`false` in the
/// integer result register for a valid/invalid date.
pub(super) fn lower_checkdate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "checkdate", 3)?;
    marshal_integer_args(
        ctx,
        inst,
        &["checkdate month", "checkdate day", "checkdate year"],
    )?;
    abi::emit_call_label(ctx.emitter, "__rt_checkdate");
    store_if_result(ctx, inst)
}

/// Lowers `getdate([$timestamp])` through the shared decomposition runtime helper.
///
/// Marshals the optional timestamp (the `-1` current-time sentinel when omitted; a boxed
/// `Mixed`/`Union` argument is unboxed) into the integer result register where `__rt_getdate`
/// reads it, then boxes the returned associative-array hash pointer into a `Mixed` cell — the same
/// representation `stat`/`getdate` use, so the checker types the result `Mixed`.
pub(super) fn lower_getdate(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "getdate", 0, 1)?;
    load_date_timestamp(ctx, inst.operands.first().copied())?;
    abi::emit_call_label(ctx.emitter, "__rt_getdate");
    emit_box_hash_pointer_as_assoc_mixed(ctx);
    store_if_result(ctx, inst)
}

/// Boxes the raw associative-array hash pointer in the integer result register into a `Mixed` cell
/// (runtime tag 5), the representation `getdate`/`localtime` results use — mirroring `stat`.
fn emit_box_hash_pointer_as_assoc_mixed(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // Mixed payload low word = hash pointer
            ctx.emitter.instruction("mov x2, #0");                              // associative-array payloads do not use the high word
            ctx.emitter.instruction("mov x0, #5");                              // runtime tag 5 = associative array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // Mixed payload low word = hash pointer
            ctx.emitter.instruction("xor esi, esi");                            // associative-array payloads do not use the high word
            ctx.emitter.instruction("mov rax, 5");                              // runtime tag 5 = associative array
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
        }
    }
}

/// Lowers `localtime([$timestamp[, $associative]])` through the shared decomposition runtime helper.
///
/// `__rt_localtime` reads the timestamp from the integer result register (`x0`/`rax`) and the
/// associative-keys flag from the second argument register (`x1`/`rsi`) — an irregular ABI, so the
/// two values are staged in scratch (the flag may unbox a `Mixed`, clobbering the timestamp) and
/// reloaded into their distinct registers with no intervening call, then the returned hash pointer
/// is boxed into a `Mixed` associative-array cell like `getdate`.
pub(super) fn lower_localtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "localtime", 0, 2)?;
    emit_scratch_reserve(ctx, 16);
    load_date_timestamp(ctx, inst.operands.first().copied())?;
    emit_store_result_to_scratch(ctx, 0);
    match inst.operands.get(1).copied() {
        Some(flag) => resolve_integer_arg_to_result(ctx, flag, "localtime associative flag")?,
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0),
    }
    emit_store_result_to_scratch(ctx, 8);
    emit_load_scratch_to_reg(ctx, abi::int_result_reg(ctx.emitter), 0);
    emit_load_scratch_to_reg(ctx, abi::int_arg_reg_name(ctx.emitter.target, 1), 8);
    emit_scratch_release(ctx, 16);
    abi::emit_call_label(ctx.emitter, "__rt_localtime");
    emit_box_hash_pointer_as_assoc_mixed(ctx);
    store_if_result(ctx, inst)
}

/// Lowers `hrtime([$as_number])` through the monotonic-clock runtime helper.
///
/// `__rt_hrtime` reads the as-number flag from the integer result register (`x0`/`rax`) and returns
/// an already-boxed `Mixed` result — a boxed `[sec, nsec]` array when the flag is `0`/false, or a
/// boxed nanosecond integer when truthy — so no post-call boxing is needed. Unlike the timestamp
/// builtins the omitted-argument default is `0` (array form), not the `-1` current-time sentinel.
pub(super) fn lower_hrtime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "hrtime", 0, 1)?;
    match inst.operands.first().copied() {
        Some(flag) => resolve_integer_arg_to_result(ctx, flag, "hrtime as_number flag")?,
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0),
    }
    abi::emit_call_label(ctx.emitter, "__rt_hrtime");
    store_if_result(ctx, inst)
}

/// Lowers `http_response_code([$code])` to `__rt_http_response_code`. The code (or
/// 0 = "read current" when omitted) goes into the first integer argument register;
/// the routine returns the resulting status as an int. PHP semantics (read vs set,
/// return-previous) live in the bridge's `elephc_web_set_status`.
pub(super) fn lower_http_response_code(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "http_response_code", 0, 1)?;
    match inst.operands.first().copied() {
        Some(code) => {
            load_value_to_first_int_arg(ctx, code)?;
        }
        None => abi::emit_load_int_immediate(
            ctx.emitter,
            abi::int_arg_reg_name(ctx.emitter.target, 0),
            0,
        ),
    }
    abi::emit_call_label(ctx.emitter, "__rt_http_response_code");
    store_if_result(ctx, inst)
}

/// Lowers `header($line[, $replace[, $code]])` to `__rt_header`, materializing the
/// four C-ABI integer arguments: arg0=line ptr, arg1=line len, arg2=`$replace`
/// (default true), arg3=`$response_code` (default 0). `$replace`/`$code` are staged
/// to scratch first (their evaluation may call helpers that clobber the string
/// registers), then the line string is loaded and the staged ints reloaded into
/// arg2/arg3. All PHP `header()` behavior lives in the bridge (`elephc_web_header`).
pub(super) fn lower_header(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "header", 1, 3)?;
    let line = expect_operand(inst, 0)?;
    emit_scratch_reserve(ctx, 16);
    // $replace (default true = 1) → scratch[0]
    match inst.operands.get(1).copied() {
        Some(value) => resolve_integer_arg_to_result(ctx, value, "header replace flag")?,
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1),
    }
    emit_store_result_to_scratch(ctx, 0);
    // $response_code (default 0) → scratch[8]
    match inst.operands.get(2).copied() {
        Some(value) => resolve_integer_arg_to_result(ctx, value, "header response_code")?,
        None => abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0),
    }
    emit_store_result_to_scratch(ctx, 8);
    // line string → string-result regs, then move ptr/len into arg0/arg1
    super::io::load_string_to_result(ctx, line, "header line")?;
    emit_move_string_result_to_first_two_args(ctx);
    // staged ints → arg2 ($replace) / arg3 ($response_code)
    emit_load_scratch_to_arg_reg(ctx, 2, 0);
    emit_load_scratch_to_arg_reg(ctx, 3, 8);
    emit_scratch_release(ctx, 16);
    abi::emit_call_label(ctx.emitter, "__rt_header");
    store_if_result(ctx, inst)
}

/// Moves the string-result registers (AArch64 `x1`=ptr/`x2`=len, x86_64 `rax`=ptr/
/// `rdx`=len) into the first two C-ABI integer argument registers (ptr→arg0, len→arg1).
fn emit_move_string_result_to_first_two_args(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // line pointer → first argument register
            ctx.emitter.instruction("mov x1, x2");                              // line length → second argument register
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // line pointer → first argument register
            ctx.emitter.instruction("mov rsi, rdx");                            // line length → second argument register
        }
    }
}

/// Shared lowering for `mktime`/`gmmktime`: marshals the six date/time integers into the ABI
/// argument registers, then calls `runtime_symbol` (`__rt_mktime` for local time, `__rt_gmmktime`
/// for UTC). `name` is used for the argument-count diagnostic only.
fn lower_mktime_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_symbol: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 6)?;
    marshal_integer_args(ctx, inst, &MKTIME_ARG_LABELS)?;
    abi::emit_call_label(ctx.emitter, runtime_symbol);
    store_if_result(ctx, inst)
}

/// Diagnostic labels for the six `mktime`/`gmmktime` integer arguments, in ABI order.
const MKTIME_ARG_LABELS: [&str; 6] = [
    "mktime hour",
    "mktime minute",
    "mktime second",
    "mktime month",
    "mktime day",
    "mktime year",
];

/// Marshals `labels.len()` integer arguments into the leading ABI argument registers, unboxing any
/// `Mixed`/`Union` argument first.
///
/// Date/time runtimes such as `mktime`/`gmmktime`/`checkdate` pass their integers in argument
/// registers (`x0`-`x5`, or `rdi`/`rsi`/`rdx`/`rcx`/`r8`/`r9`). Unboxing a `Mixed` argument calls
/// `__rt_mixed_cast_int`, which clobbers the caller-saved argument registers, so loading the
/// integers straight into those registers would lose every argument resolved before a later boxed
/// one (the bug `test_mktime_unboxes_mixed_args` covers). Each argument is instead resolved to a
/// plain integer one at a time and staged in a 16-byte-aligned stack scratch area below the frame —
/// untouched by the unbox calls, whose own frames sit below it — then all are reloaded into the
/// argument registers with no intervening call.
fn marshal_integer_args(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    labels: &[&str],
) -> Result<()> {
    let count = labels.len();
    let scratch_bytes = (count * 8).div_ceil(16) * 16; // round the per-arg slots up to 16-byte alignment
    emit_scratch_reserve(ctx, scratch_bytes);
    for (index, label) in labels.iter().enumerate() {
        resolve_integer_arg_to_result(ctx, expect_operand(inst, index)?, label)?;
        emit_store_result_to_scratch(ctx, index * 8);
    }
    for index in 0..count {
        emit_load_scratch_to_arg_reg(ctx, index, index * 8);
    }
    emit_scratch_release(ctx, scratch_bytes);
    Ok(())
}

/// Resolves one date/time integer argument into the canonical integer result register, unboxing a
/// boxed `Mixed`/`Union` value through `__rt_mixed_cast_int`. Genuinely non-integer types (string,
/// float, array) still produce an `unsupported` diagnostic.
fn resolve_integer_arg_to_result(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(value)?;
        }
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
        }
        ty => {
            return Err(CodegenIrError::unsupported(format!(
                "{} for PHP type {:?}",
                context, ty
            )));
        }
    }
    Ok(())
}

/// Reserves `bytes` of 16-byte-aligned scratch space below the stack pointer for argument staging.
/// Calls made while resolving arguments push their own frames below this area, so the staged
/// integers are never overwritten.
fn emit_scratch_reserve(ctx: &mut FunctionContext<'_>, bytes: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("sub sp, sp, #{}", bytes));        // reserve 16-byte-aligned argument scratch below the frame
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("sub rsp, {}", bytes));            // reserve 16-byte-aligned argument scratch below the frame
        }
    }
}

/// Releases the scratch space reserved by `emit_scratch_reserve`, restoring the stack pointer.
fn emit_scratch_release(ctx: &mut FunctionContext<'_>, bytes: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("add sp, sp, #{}", bytes));        // release the argument scratch area
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("add rsp, {}", bytes));            // release the argument scratch area
        }
    }
}

/// Stages the canonical integer result register into the scratch slot at `offset` from the stack
/// pointer.
fn emit_store_result_to_scratch(ctx: &mut FunctionContext<'_>, offset: usize) {
    let result = abi::int_result_reg(ctx.emitter);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("str {}, [sp, #{}]", result, offset)); // stage the resolved integer in scratch
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov QWORD PTR [rsp + {}], {}", offset, result)); // stage the resolved integer in scratch
        }
    }
}

/// Loads the staged integer at scratch `offset` into the `index`-th integer argument register.
fn emit_load_scratch_to_arg_reg(ctx: &mut FunctionContext<'_>, index: usize, offset: usize) {
    let arg_reg = abi::int_arg_reg_name(ctx.emitter.target, index);
    emit_load_scratch_to_reg(ctx, arg_reg, offset);
}

/// Loads the staged integer at scratch `offset` into a caller-selected register.
fn emit_load_scratch_to_reg(ctx: &mut FunctionContext<'_>, reg: &str, offset: usize) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("ldr {}, [sp, #{}]", reg, offset)); // load the staged integer into the target register
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [rsp + {}]", reg, offset)); // load the staged integer into the target register
        }
    }
}

/// Lowers `sleep(seconds)` through the target's C library symbol.
pub(super) fn lower_sleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "sleep", "sleep seconds")
}

/// Lowers `strtotime(datetime[, baseTimestamp])` through the shared parser runtime helper.
///
/// Returns PHP's `int|false`: the `__rt_strtotime` `i64::MIN` parse-failure sentinel is boxed as
/// `Mixed` `false`, and every other value (including a real `-1` pre-epoch timestamp) is boxed as
/// a `Mixed` integer, so `=== false`, `=== -1`, and `echo` all observe the distinct results.
/// Supports PHP's optional `$baseTimestamp`. (The `__elephc_strtotime_raw` alias keeps the plain
/// `-1` integer shape for the synthetic `DateTime` internals.)
pub(super) fn lower_strtotime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    emit_strtotime_marshal(ctx, inst, "strtotime")?;
    emit_box_strtotime_int_or_false(ctx);
    store_if_result(ctx, inst)
}

/// Boxes the `__rt_strtotime` integer result into a `Mixed` `int|false` cell: the `i64::MIN`
/// parse-failure sentinel becomes boxed `false` (runtime tag 3), and any other value becomes a
/// boxed integer (runtime tag 0), preserving a genuine `-1` timestamp as a distinct integer.
fn emit_box_strtotime_int_or_false(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("strtotime_box_false");
    let done_label = ctx.next_label("strtotime_box_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("movz x13, #0x8000, lsl #48");              // load the i64::MIN parse-failure sentinel
            ctx.emitter.instruction("cmp x0, x13");                             // did the parse fail?
            ctx.emitter.instruction(&format!("b.eq {}", false_label));          // failure → box PHP false instead of an integer
            ctx.emitter.instruction("mov x1, x0");                              // Mixed payload low word = the parsed timestamp
            ctx.emitter.instruction("mov x2, #0");                              // integer payloads do not use the high word
            ctx.emitter.instruction("mov x0, #0");                              // runtime tag 0 = int
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the false-boxing path after boxing the integer
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("mov x1, #0");                              // boolean payload for false is zero
            ctx.emitter.instruction("mov x2, #0");                              // boolean payloads do not use the high word
            ctx.emitter.instruction("mov x0, #3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movabs r10, -9223372036854775808");        // load the i64::MIN parse-failure sentinel
            ctx.emitter.instruction("cmp rax, r10");                            // did the parse fail?
            ctx.emitter.instruction(&format!("je {}", false_label));            // failure → box PHP false instead of an integer
            ctx.emitter.instruction("mov rdi, rax");                            // Mixed payload low word = the parsed timestamp
            ctx.emitter.instruction("xor esi, esi");                            // integer payloads do not use the high word
            ctx.emitter.instruction("mov rax, 0");                              // runtime tag 0 = int
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the false-boxing path after boxing the integer
            ctx.emitter.label(&false_label);
            ctx.emitter.instruction("xor edi, edi");                            // boolean payload for false is zero
            ctx.emitter.instruction("xor esi, esi");                            // boolean payloads do not use the high word
            ctx.emitter.instruction("mov rax, 3");                              // runtime tag 3 = bool
            abi::emit_call_label(ctx.emitter, "__rt_mixed_from_value");
            ctx.emitter.label(&done_label);
        }
    }
}

/// Lowers the internal `__elephc_strtotime_raw(datetime[, baseTimestamp])` alias.
///
/// Backs the synthetic `DateTime` constructor and `modify()`. Marshals the same runtime ABI
/// as `strtotime`, but maps the `i64::MIN` parse-failure sentinel to `-1` so callers store the
/// timestamp directly as the legacy `-1` in-object failure value.
pub(super) fn lower_elephc_strtotime_raw(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    emit_strtotime_marshal(ctx, inst, "__elephc_strtotime_raw")?;
    emit_strtotime_sentinel_to_minus_one(ctx);
    store_if_result(ctx, inst)
}

/// Marshals the shared `__rt_strtotime` ABI for `strtotime` / `__elephc_strtotime_raw`.
///
/// Loads the datetime string (`x1`/`x2` on ARM64, `rdi`/`rsi` on x86_64), the optional base
/// timestamp (`x0`/`rdx`), and the has-base flag (`x3`/`rcx`: `1` when a base was supplied, `0`
/// so the runtime uses the current time otherwise), then calls `__rt_strtotime`. The datetime
/// string is materialized first: a boxed-`Mixed` argument (e.g. a `foreach` loop variable over a
/// string array) is coerced through a runtime helper that clobbers the integer-argument/result
/// registers, so it must precede the integer-only base (a simple load that cannot clobber the
/// string registers). `name` drives the argument-count and type diagnostics only.
fn emit_strtotime_marshal(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    ensure_arg_count_between(inst, name, 1, 2)?;
    let datetime = expect_operand(inst, 0)?;
    let base = inst.operands.get(1).copied();
    load_date_string_arg(ctx, datetime, name)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            if let Some(base) = base {
                materialize_integer_arg(ctx, base, "x0", "strtotime base")?;
                ctx.emitter.instruction("mov x3, #1");                          // a base timestamp was provided
            } else {
                ctx.emitter.instruction("mov x3, #0");                          // no base → runtime uses the current time
            }
        }
        Arch::X86_64 => {
            if let Some(base) = base {
                materialize_integer_arg(ctx, base, "rdx", "strtotime base")?;
                ctx.emitter.instruction("mov rcx, 1");                          // a base timestamp was provided
            } else {
                ctx.emitter.instruction("xor ecx, ecx");                        // no base → runtime uses the current time
            }
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_strtotime");
    Ok(())
}

/// Maps the `__rt_strtotime` `i64::MIN` parse-failure sentinel to `-1` in the integer result.
///
/// Keeps `-1` itself usable as a real pre-epoch timestamp: only the sentinel is rewritten, so
/// the synthetic `DateTime` callers that store the raw integer observe PHP's `false`-on-failure
/// contract as the legacy `-1` in-object value.
fn emit_strtotime_sentinel_to_minus_one(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("movz x13, #0x8000, lsl #48");              // load the i64::MIN parse-failure sentinel
            ctx.emitter.instruction("cmp x0, x13");                             // did the parse fail?
            ctx.emitter.instruction("mov x13, #-1");                            // legacy in-object failure value
            ctx.emitter.instruction("csel x0, x13, x0, eq");                    // sentinel → -1, otherwise keep the timestamp
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("movabs r10, -9223372036854775808");        // load the i64::MIN parse-failure sentinel
            ctx.emitter.instruction("cmp rax, r10");                            // did the parse fail?
            ctx.emitter.instruction("mov r10, -1");                             // legacy in-object failure value
            ctx.emitter.instruction("cmove rax, r10");                          // sentinel → -1, otherwise keep the timestamp
        }
    }
}

/// Lowers `time()` through the shared wall-clock runtime helper.
pub(super) fn lower_time(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "time", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_time");
    store_if_result(ctx, inst)
}

/// Lowers `usleep(microseconds)` through the target's C library symbol.
pub(super) fn lower_usleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "usleep", "usleep microseconds")
}

/// Lowers `exit(status?)` and `die(status?)` by terminating the current process.
pub(super) fn lower_exit(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "exit", 0, 1)?;
    let Some(status) = inst.operands.first().copied() else {
        abi::emit_exit(ctx.emitter, 0);
        return Ok(());
    };
    require_integer_like(ctx.load_value_to_result(status)?, "exit status")?;
    emit_dynamic_exit(ctx);
    Ok(())
}

/// Lowers `getenv(name)` through the target-aware environment lookup helper.
pub(super) fn lower_getenv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getenv", 1)?;
    let name = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(name)?.codegen_repr(), "getenv name")?;
    abi::emit_call_label(ctx.emitter, "__rt_getenv");
    store_if_result(ctx, inst)
}

/// Lowers `putenv(assignment)` by copying the environment string into persistent heap storage.
pub(super) fn lower_putenv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "putenv", 1)?;
    let assignment = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(assignment)?.codegen_repr(), "putenv assignment")?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => lower_putenv_aarch64(ctx),
        Arch::X86_64 => lower_putenv_x86_64(ctx),
    }
    store_if_result(ctx, inst)
}

/// Lowers `php_uname(mode?)` through the target-aware uname runtime helper.
pub(super) fn lower_php_uname(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    ensure_arg_count_between(inst, "php_uname", 0, 1)?;
    if let Some(mode) = inst.operands.first().copied() {
        require_string(ctx.load_value_to_result(mode)?.codegen_repr(), "php_uname mode")?;
    } else {
        let (label, len) = ctx.data.add_string(b"a");
        let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
        abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
        abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    }
    abi::emit_call_label(ctx.emitter, "__rt_php_uname");
    store_if_result(ctx, inst)
}

/// Lowers `exec(command)` by capturing shell stdout through the shared runtime helper.
pub(super) fn lower_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "exec")
}

/// Lowers `shell_exec(command)` by capturing shell stdout through the shared runtime helper.
pub(super) fn lower_shell_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "shell_exec")
}

/// Lowers `system(command)` through libc `system()` and returns the legacy empty string result.
pub(super) fn lower_system(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_direct_system_call(ctx, inst, "system", true)
}

/// Lowers `passthru(command)` through libc `system()` for direct stdout passthrough.
pub(super) fn lower_passthru(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_direct_system_call(ctx, inst, "passthru", false)
}

/// Lowers shell-capturing process builtins that return a PHP string.
fn lower_shell_exec_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let command = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(command)?.codegen_repr(), "shell command")?;
    abi::emit_call_label(ctx.emitter, "__rt_shell_exec");
    store_if_result(ctx, inst)
}

/// Lowers stdout-passthrough process builtins that execute a command via libc `system()`.
fn lower_direct_system_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    returns_empty_string: bool,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let command = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(command)?.codegen_repr(), "system command")?;
    abi::emit_call_label(ctx.emitter, "__rt_cstr");
    if ctx.emitter.target.arch == Arch::X86_64 {
        ctx.emitter.instruction("mov rdi, rax");                                // pass the null-terminated shell command to libc system()
    }
    ctx.emitter.bl_c("system");
    if returns_empty_string {
        emit_empty_string_result(ctx);
    }
    store_if_result(ctx, inst)
}

/// Materializes the legacy empty-string return value used after `system()`.
fn emit_empty_string_result(ctx: &mut FunctionContext<'_>) {
    let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
    abi::emit_load_int_immediate(ctx.emitter, ptr_reg, 0);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, 0);
}

/// Emits a process-exit sequence using the already-loaded integer result register.
fn emit_dynamic_exit(ctx: &mut FunctionContext<'_>) {
    match (ctx.emitter.target.platform, ctx.emitter.target.arch) {
        (Platform::MacOS, Arch::AArch64) | (Platform::Linux, Arch::AArch64) => {
            ctx.emitter.syscall(1);
        }
        (Platform::Linux, Arch::X86_64) => {
            ctx.emitter.instruction("mov rdi, rax");                            // move the computed exit code into the SysV first-argument register
            ctx.emitter.instruction("mov eax, 60");                             // Linux x86_64 syscall 60 = exit
            ctx.emitter.instruction("syscall");                                 // terminate the process through the Linux x86_64 syscall ABI
        }
        (Platform::MacOS, Arch::X86_64) => {
            panic!("exit() is not implemented yet for target macos-x86_64");
        }
    }
}

/// Emits the AArch64 persistent-copy path for `putenv()`.
fn lower_putenv_aarch64(ctx: &mut FunctionContext<'_>) {
    let copy_loop = ctx.next_label("putenv_copy");
    let copy_done = ctx.next_label("putenv_copy_done");
    ctx.emitter.instruction("add x0, x2, #1");                                  // allocate space for the environment string plus trailing null
    ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                         // preserve the source string pointer and length across heap allocation
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("ldp x1, x2, [sp], #16");                           // restore the source string pointer and length after heap allocation
    ctx.emitter.instruction("mov x3, x0");                                      // keep the persistent destination buffer for copying and putenv()
    ctx.emitter.instruction("mov x4, #0");                                      // start copying at byte offset zero
    ctx.emitter.label(&copy_loop);
    ctx.emitter.instruction("cmp x4, x2");                                      // compare the copied byte count with the source length
    ctx.emitter.instruction(&format!("b.ge {}", copy_done));                    // finish once every source byte has been persisted
    ctx.emitter.instruction("ldrb w5, [x1, x4]");                               // load one byte from the source environment assignment
    ctx.emitter.instruction("strb w5, [x3, x4]");                               // copy the byte into the persistent putenv buffer
    ctx.emitter.instruction("add x4, x4, #1");                                  // advance to the next source byte
    ctx.emitter.instruction(&format!("b {}", copy_loop));                       // continue copying the environment assignment
    ctx.emitter.label(&copy_done);
    ctx.emitter.instruction("strb wzr, [x3, x4]");                              // append the C null terminator required by putenv()
    ctx.emitter.instruction("mov x0, x3");                                      // pass the persistent environment buffer to putenv()
    ctx.emitter.bl_c("putenv");
    ctx.emitter.instruction("cmp x0, #0");                                      // compare libc putenv() status against success
    ctx.emitter.instruction("cset x0, eq");                                     // return true when putenv() accepted the assignment
}

/// Emits the x86_64 persistent-copy path for `putenv()`.
fn lower_putenv_x86_64(ctx: &mut FunctionContext<'_>) {
    let copy_loop = ctx.next_label("putenv_copy");
    let copy_done = ctx.next_label("putenv_copy_done");
    ctx.emitter.instruction("sub rsp, 16");                                     // reserve aligned spill space for the source string across heap allocation
    ctx.emitter.instruction("mov QWORD PTR [rsp], rax");                        // save the source environment string pointer
    ctx.emitter.instruction("mov QWORD PTR [rsp + 8], rdx");                    // save the source environment string length
    ctx.emitter.instruction("mov rax, rdx");                                    // seed the heap allocation size from the source length
    ctx.emitter.instruction("add rax, 1");                                      // allocate space for the environment string plus trailing null
    abi::emit_call_label(ctx.emitter, "__rt_heap_alloc");
    ctx.emitter.instruction("mov rcx, QWORD PTR [rsp]");                        // restore the source environment string pointer
    ctx.emitter.instruction("mov r8, QWORD PTR [rsp + 8]");                     // restore the source environment string length
    ctx.emitter.instruction("add rsp, 16");                                     // release the temporary source string spill space
    ctx.emitter.instruction("mov r9, rax");                                     // keep the persistent destination buffer for copying and putenv()
    ctx.emitter.instruction("mov r10, 0");                                      // start copying at byte offset zero
    ctx.emitter.label(&copy_loop);
    ctx.emitter.instruction("cmp r10, r8");                                     // compare the copied byte count with the source length
    ctx.emitter.instruction(&format!("jae {}", copy_done));                     // finish once every source byte has been persisted
    ctx.emitter.instruction("mov r11b, BYTE PTR [rcx + r10]");                  // load one byte from the source environment assignment
    ctx.emitter.instruction("mov BYTE PTR [r9 + r10], r11b");                   // copy the byte into the persistent putenv buffer
    ctx.emitter.instruction("add r10, 1");                                      // advance to the next source byte
    ctx.emitter.instruction(&format!("jmp {}", copy_loop));                     // continue copying the environment assignment
    ctx.emitter.label(&copy_done);
    ctx.emitter.instruction("mov BYTE PTR [r9 + r10], 0");                      // append the C null terminator required by putenv()
    ctx.emitter.instruction("mov rdi, r9");                                     // pass the persistent environment buffer to putenv()
    ctx.emitter.bl_c("putenv");
    ctx.emitter.instruction("cmp rax, 0");                                      // compare libc putenv() status against success
    ctx.emitter.instruction("sete al");                                         // return true when putenv() accepted the assignment
    ctx.emitter.instruction("movzx rax, al");                                   // widen the boolean byte into the integer result register
}

/// Lowers a one-argument blocking libc call that receives an integer duration.
fn lower_unary_blocking_c_call(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    context: &str,
) -> Result<()> {
    super::ensure_arg_count(inst, name, 1)?;
    let duration = expect_operand(inst, 0)?;
    require_integer_like(load_value_to_first_int_arg(ctx, duration)?, context)?;
    ctx.emitter.bl_c(name);
    store_if_result(ctx, inst)
}

/// Loads a `date()` timestamp or the `-1` current-time sentinel into the integer result register.
fn load_date_timestamp(
    ctx: &mut FunctionContext<'_>,
    timestamp: Option<ValueId>,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let Some(timestamp) = timestamp else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
        return Ok(());
    };
    match ctx.value_php_type(timestamp)?.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
            Ok(())
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(timestamp)?;
            Ok(())
        }
        // A boxed Mixed/Union timestamp (for example read from an associative array or produced by
        // a Mixed-typed expression) is unboxed to its integer value before formatting, matching
        // PHP's implicit integer coercion of the timestamp argument. The unboxed result lands in
        // the integer result register, which is where the formatter helper reads the timestamp.
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, timestamp)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            Ok(())
        }
        ty => Err(CodegenIrError::unsupported(format!(
            "date timestamp for PHP type {:?}",
            ty
        ))),
    }
}

/// Loads a date/strtotime string argument into the runtime helper's string
/// registers (ARM64 `x1`/`x2`, x86_64 `rdi`/`rsi`).
///
/// A plain `Str` is loaded directly. A boxed `Mixed`/`Union` value — for example a
/// `foreach` loop variable over a string array — is coerced through
/// `__rt_mixed_cast_string` (boxed pointer in the first integer-argument register;
/// result pointer/length in `x1`/`x2` on ARM64 and `rax`/`rdx` on x86_64) and then
/// moved into the canonical string registers. Other types keep the strict
/// `unsupported` diagnostic.
fn load_date_string_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    context: &str,
) -> Result<()> {
    match ctx.value_php_type(value)?.codegen_repr() {
        PhpType::Str => match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.load_string_value_to_regs(value, "x1", "x2"),
            Arch::X86_64 => ctx.load_string_value_to_regs(value, "rdi", "rsi"),
        },
        PhpType::Mixed | PhpType::Union(_) => {
            load_value_to_first_int_arg(ctx, value)?;
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_string");
            if ctx.emitter.target.arch == Arch::X86_64 {
                ctx.emitter.instruction("mov rdi, rax");                        // cast string pointer → first ABI string register
                ctx.emitter.instruction("mov rsi, rdx");                        // cast string length → second ABI string register
            }
            Ok(())
        }
        ty => require_string(ty, context),
    }
}

/// Saves the materialized date/strtotime string registers across timestamp
/// loading. Coercing a boxed Mixed timestamp clobbers the string registers, so
/// they are parked on the stack (16-byte aligned) and restored before the call.
fn stage_date_string_regs(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("stp x1, x2, [sp, #-16]!");                 // stage date string pointer/length below the stack
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("push rsi");                                // stage date string length
            ctx.emitter.instruction("push rdi");                                // stage date string pointer (keeps the stack 16-byte aligned)
        }
    }
}

/// Restores the date/strtotime string registers staged by `stage_date_string_regs`.
fn unstage_date_string_regs(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("ldp x1, x2, [sp], #16");                   // restore date string pointer/length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("pop rdi");                                 // restore date string pointer
            ctx.emitter.instruction("pop rsi");                                 // restore date string length
        }
    }
}

/// Loads one integer-like runtime argument into a caller-selected register.
fn materialize_integer_arg(
    ctx: &mut FunctionContext<'_>,
    value: ValueId,
    reg: &str,
    context: &str,
) -> Result<()> {
    require_integer_like(ctx.load_value_to_reg(value, reg)?, context)
}

/// Verifies a value can be passed as a date/time integer option.
fn require_integer_like(ty: PhpType, context: &str) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}

/// Verifies a value can be passed as a date/time string argument.
fn require_string(ty: PhpType, context: &str) -> Result<()> {
    if ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        context,
        ty
    )))
}

/// Verifies that the builtin call has between the expected lowered operand counts.
fn ensure_arg_count_between(
    inst: &Instruction,
    name: &str,
    min: usize,
    max: usize,
) -> Result<()> {
    if (min..=max).contains(&inst.operands.len()) {
        return Ok(());
    }
    Err(CodegenIrError::invalid_module(format!(
        "{} expected {} to {} args, got {}",
        name,
        min,
        max,
        inst.operands.len()
    )))
}
