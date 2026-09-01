//! Purpose:
//! Lowers date/time system builtins for the EIR backend.
//! Marshals already-evaluated EIR operands into the shared runtime helpers.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Time builtins are effectful and must reuse the target-aware runtime
//!   helpers rather than duplicating libc/syscall behavior in the EIR backend.

use crate::codegen::abi;
use crate::codegen::platform::{Arch, Platform};
use crate::codegen::{CodegenIrError, Result};
use crate::ir::{Instruction, ValueId};
use crate::types::PhpType;

use super::super::super::context::FunctionContext;
use super::{expect_operand, load_value_to_first_int_arg, store_if_result};

/// Lowers the internal suppression-aware diagnostic helper.
///
/// The first input is a PHP-style warning/deprecation line. An optional source
/// line appends the module's canonical source path and that line number. The
/// optional third input is the corresponding `E_*` level (default `E_WARNING`).
/// The active `error_reporting()` mask and `@` suppression both apply to every
/// fragment of the diagnostic.
pub(crate) fn lower_elephc_diag_warning(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "__elephc_diag_warning", 1, 3)?;
    let masked_label = ctx.next_label("diag_warning_masked");
    if let Some(level) = inst.operands.get(2).copied() {
        require_integer_like(
            ctx.load_value_to_reg(
                level,
                match ctx.emitter.target.arch {
                    Arch::AArch64 => "x10",
                    Arch::X86_64 => "r10",
                },
            )?,
            "__elephc_diag_warning error level",
        )?;
    } else {
        abi::emit_load_int_immediate(
            ctx.emitter,
            match ctx.emitter.target.arch {
                Arch::AArch64 => "x10",
                Arch::X86_64 => "r10",
            },
            2,
        );
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_rt_error_reporting", 0); // load the active PHP error-reporting mask
            ctx.emitter.instruction("tst x9, x10");                             // is this diagnostic level enabled?
            ctx.emitter.instruction(&format!("b.eq {}", masked_label));         // suppress levels excluded by error_reporting()
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r11", "_rt_error_reporting", 0); // load the active PHP error-reporting mask
            ctx.emitter.instruction("test r11, r10");                           // is this diagnostic level enabled?
            ctx.emitter.instruction(&format!("jz {}", masked_label));           // suppress levels excluded by error_reporting()
        }
    }
    let message = expect_operand(inst, 0)?;
    require_string(
        ctx.value_php_type(message)?,
        "__elephc_diag_warning message",
    )?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => ctx.load_string_value_to_regs(message, "x1", "x2")?,
        Arch::X86_64 => ctx.load_string_value_to_regs(message, "rdi", "rsi")?,
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_write");
    if let Some(line) = inst.operands.get(1).copied() {
        let no_location_label = ctx.next_label("diag_warning_no_location");
        ctx.load_value_to_result(line)?;
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction("cmp x0, #0");                          // does the caller have a real source line?
                ctx.emitter.instruction(&format!("b.le {}", no_location_label)); // omit location for the zero/default sentinel
            }
            Arch::X86_64 => {
                ctx.emitter.instruction("test rax, rax");                       // does the caller have a real source line?
                ctx.emitter.instruction(&format!("jle {}", no_location_label)); // omit location for the zero/default sentinel
            }
        }
        let source = ctx.module.source_path.as_deref().unwrap_or("Unknown");
        emit_diag_warning_fragment(ctx, format!(" in {source} on line ").as_bytes());
        ctx.load_value_to_result(line)?;
        abi::emit_call_label(ctx.emitter, "__rt_itoa");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {}
            Arch::X86_64 => {
                ctx.emitter.instruction("mov rdi, rax");                        // diagnostic pointer = formatted source line
                ctx.emitter.instruction("mov rsi, rdx");                        // diagnostic length = formatted source-line length
            }
        }
        abi::emit_call_label(ctx.emitter, "__rt_diag_write");
        emit_diag_warning_fragment(ctx, b"\n");
        ctx.emitter.label(&no_location_label);
    }
    ctx.emitter.label(&masked_label);
    store_if_result(ctx, inst)
}

/// Writes one static diagnostic fragment through the suppression-aware warning runtime.
fn emit_diag_warning_fragment(ctx: &mut FunctionContext<'_>, fragment: &[u8]) {
    let (label, len) = ctx.data.add_string(fragment);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(ctx.emitter, "x1", &label);
            abi::emit_load_int_immediate(ctx.emitter, "x2", len as i64);
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(ctx.emitter, "rdi", &label);
            abi::emit_load_int_immediate(ctx.emitter, "rsi", len as i64);
        }
    }
    abi::emit_call_label(ctx.emitter, "__rt_diag_write");
}

/// Lowers `error_reporting(?int $error_level = null)`, returning the previous
/// runtime mask and updating it only when an integer level is supplied.
pub(crate) fn lower_error_reporting(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count_between(inst, "error_reporting", 0, 1)?;
    abi::emit_load_symbol_to_reg(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_rt_error_reporting",
        0,
    );
    mask_error_reporting_result_for_suppression(ctx);
    let Some(level) = inst.operands.first().copied() else {
        return store_if_result(ctx, inst);
    };
    if matches!(ctx.value_php_type(level)?.codegen_repr(), PhpType::Void) {
        return store_if_result(ctx, inst);
    }

    emit_scratch_reserve(ctx, 16);
    emit_store_result_to_scratch(ctx, 0);
    resolve_integer_arg_to_result(ctx, level, "error_reporting error level")?;
    abi::emit_store_reg_to_symbol(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        "_rt_error_reporting",
        0,
    );
    emit_load_scratch_to_reg(ctx, abi::int_result_reg(ctx.emitter), 0);
    emit_scratch_release(ctx, 16);
    store_if_result(ctx, inst)
}

/// Applies php-src's fatal-only mask to `error_reporting()` while `@` is active.
fn mask_error_reporting_result_for_suppression(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "x9", "_rt_diag_suppression", 0);
            abi::emit_load_int_immediate(ctx.emitter, "x10", 4437);
            ctx.emitter.instruction("and x10, x0, x10");                        // intersect the active mask with PHP's fatal-only @ mask
            ctx.emitter.instruction("cmp x9, #0");                              // is an error-suppression scope active?
            ctx.emitter.instruction("csel x0, x0, x10, eq");                    // expose the full mask outside @ and the fatal-only subset inside it
        }
        Arch::X86_64 => {
            abi::emit_load_symbol_to_reg(ctx.emitter, "r10", "_rt_diag_suppression", 0);
            abi::emit_load_int_immediate(ctx.emitter, "r11", 4437);
            ctx.emitter.instruction("and r11, rax");                            // intersect the active mask with PHP's fatal-only @ mask
            ctx.emitter.instruction("test r10, r10");                           // is an error-suppression scope active?
            ctx.emitter.instruction("cmovnz rax, r11");                         // reveal the fatal-only subset while suppressed
        }
    }
}

/// Lowers `date(format, timestamp?)` through the shared formatter runtime helper.
pub(crate) fn lower_date(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_date_like(ctx, inst, "date", "__rt_date")
}

/// Lowers `gmdate(format[, timestamp])`: the UTC counterpart of `date()`.
///
/// Identical argument marshalling to `date()`, but dispatches to `__rt_gmdate`, which formats
/// the instant in UTC regardless of the active default timezone.
pub(crate) fn lower_gmdate(
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
    abi::emit_call_label(ctx.emitter, "__rt_str_persist");
    store_if_result(ctx, inst)
}

/// Lowers `date_default_timezone_get()` through the shared runtime helper.
///
/// Takes no arguments; `__rt_date_default_timezone_get` returns the stored timezone
/// identifier (or the literal `"UTC"` when none was set) in the string-result registers.
pub(crate) fn lower_date_default_timezone_get(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "date_default_timezone_get", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_date_default_timezone_get");
    store_if_result(ctx, inst)
}

/// Lowers `date_default_timezone_set(timezoneId)` through the shared runtime helper.
///
/// Public calls first validate the identifier against the php-src timelib table,
/// emit PHP's suppression-aware notice and return `false` for invalid names, then
/// call `__rt_date_default_timezone_set` for valid names. Synthetic DateTime
/// methods bypass the public gate because their internal civil-time calculations
/// deliberately use POSIX fixed-offset strings that PHP does not accept publicly.
pub(crate) fn lower_date_default_timezone_set(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "date_default_timezone_set", 1)?;
    let identifier = expect_operand(inst, 0)?;
    require_string(
        ctx.value_php_type(identifier)?,
        "date_default_timezone_set timezone",
    )?;

    if !ctx.function.flags.is_synthetic {
        let valid_label = ctx.next_label("date_default_timezone_set_valid");
        let done_label = ctx.next_label("date_default_timezone_set_done");
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.load_string_value_to_regs(identifier, "x0", "x1")?;
                ctx.emitter.bl_c("elephc_tz_timezone_valid");
                ctx.emitter.instruction(&format!("cbnz x0, {valid_label}"));    // accept identifiers present in php-src's timelib timezone table
            }
            Arch::X86_64 => {
                ctx.load_string_value_to_regs(identifier, "rdi", "rsi")?;
                ctx.emitter.bl_c("elephc_tz_timezone_valid");
                ctx.emitter.instruction("test rax, rax");                       // inspect the php-src timezone-table membership result
                ctx.emitter.instruction(&format!("jnz {valid_label}"));         // accept identifiers present in php-src's timelib timezone table
            }
        }

        emit_diag_warning_fragment(
            ctx,
            b"\nNotice: date_default_timezone_set(): Timezone ID '",
        );
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.load_string_value_to_regs(identifier, "x1", "x2")?,
            Arch::X86_64 => ctx.load_string_value_to_regs(identifier, "rdi", "rsi")?,
        }
        abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
        let source = ctx.module.source_path.as_deref().unwrap_or("Unknown");
        let line = inst.span.map_or(0, |span| span.line);
        emit_diag_warning_fragment(
            ctx,
            format!("' is invalid in {source} on line {line}\n").as_bytes(),
        );
        abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
        abi::emit_jump(ctx.emitter, &done_label);

        ctx.emitter.label(&valid_label);
        match ctx.emitter.target.arch {
            Arch::AArch64 => ctx.load_string_value_to_regs(identifier, "x1", "x2")?,
            Arch::X86_64 => ctx.load_string_value_to_regs(identifier, "rax", "rdx")?,
        }
        abi::emit_call_label(ctx.emitter, "__rt_date_default_timezone_set");
        ctx.emitter.label(&done_label);
        return store_if_result(ctx, inst);
    }

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
pub(crate) fn lower_microtime(
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

/// Lowers `mktime(hour, minute, second, month, day, year)` through vendored timelib.
pub(crate) fn lower_mktime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_mktime_like(ctx, inst, "mktime", "elephc_tz_mktime")
}

/// Lowers `gmmktime(...)`: the UTC counterpart of `mktime()`.
///
/// Identical six-integer argument marshalling, but dispatches to timelib's UTC entry point.
pub(crate) fn lower_gmmktime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_mktime_like(ctx, inst, "gmmktime", "elephc_tz_gmmktime")
}

/// Lowers `checkdate(month, day, year)` through the shared Gregorian-validation runtime helper.
///
/// Marshals the three integers into the leading ABI argument registers (unboxing any boxed
/// `Mixed`/`Union` argument), then calls `__rt_checkdate`, which returns PHP `true`/`false` in the
/// integer result register for a valid/invalid date.
pub(crate) fn lower_checkdate(
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
pub(crate) fn lower_getdate(
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
pub(crate) fn lower_localtime(
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
pub(crate) fn lower_hrtime(
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
pub(crate) fn lower_http_response_code(
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
pub(crate) fn lower_header(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
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
/// argument registers, then calls the supplied local-time or UTC timelib bridge symbol.
/// `name` is used for the argument-count diagnostic only.
fn lower_mktime_like(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
    name: &str,
    runtime_symbol: &str,
) -> Result<()> {
    let given = inst.operands.len();
    if given == 0 || given > 6 {
        let message = if given == 0 {
            format!("{}() expects at least 1 argument, 0 given", name)
        } else {
            format!("{}() expects at most 6 arguments, {} given", name, given)
        };
        let location = ctx
            .module
            .source_path
            .clone()
            .map(|file| (file, inst.span.map_or(0, |span| span.line)));
        super::super::exceptions::emit_argument_count_error(ctx, &message, location);
        return Ok(());
    }
    super::ensure_arg_count(inst, name, 6)?;
    marshal_integer_args(ctx, inst, &MKTIME_ARG_LABELS)?;
    ctx.emitter.bl_c(runtime_symbol);
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
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(value)?;
            crate::codegen::sentinels::emit_tagged_scalar_to_int_null_as_zero(ctx.emitter);
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
            ctx.emitter.instruction(                                            // stage the resolved integer in scratch
                &format!("str {}, [sp, #{}]", result, offset)
            );
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
            ctx.emitter.instruction(&format!("ldr {}, [sp, #{}]", reg, offset));// load the staged integer into the target register
        }
        Arch::X86_64 => {
            ctx.emitter
                .instruction(&format!("mov {}, QWORD PTR [rsp + {}]", reg, offset)); // load the staged integer into the target register
        }
    }
}

/// Lowers `sleep(seconds)` through the target's C library symbol.
pub(crate) fn lower_sleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "sleep", "sleep seconds")
}

/// Lowers `strtotime(datetime[, baseTimestamp])` through the shared parser runtime helper.
///
/// Returns PHP's `int|false`: the `__rt_strtotime` success flag selects boxed `false` on failure,
/// while every successful value (including `i64::MIN` and `-1`) is boxed as a `Mixed` integer, so
/// `=== false`, `=== -1`, and `echo` all observe the distinct results.
/// Supports PHP's optional `$baseTimestamp`. (The `__elephc_strtotime_raw` alias keeps the plain
/// `-1` integer shape for the synthetic `DateTime` internals.)
pub(crate) fn lower_strtotime(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    emit_strtotime_marshal(ctx, inst, "strtotime")?;
    emit_box_strtotime_int_or_false(ctx);
    store_if_result(ctx, inst)
}

/// Boxes the `__rt_strtotime` result into a `Mixed` `int|false` cell using its success flag.
fn emit_box_strtotime_int_or_false(ctx: &mut FunctionContext<'_>) {
    let false_label = ctx.next_label("strtotime_box_false");
    let done_label = ctx.next_label("strtotime_box_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x1, #0");                              // did timelib report parse failure?
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
            ctx.emitter.instruction("test rdx, rdx");                           // did timelib report parse failure?
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
/// as `strtotime`, but maps a failed status to `-1` so callers retain their legacy integer-only
/// internal contract without sacrificing the valid `i64::MIN` timestamp.
pub(crate) fn lower_elephc_strtotime_raw(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    emit_strtotime_marshal(ctx, inst, "__elephc_strtotime_raw")?;
    emit_strtotime_failure_to_minus_one(ctx);
    store_if_result(ctx, inst)
}

/// Tests whether a dynamically named AOT class exposes an inherited or declared constructor.
pub(crate) fn lower_elephc_class_has_constructor(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_class_has_constructor", 1)?;
    super::super::objects::lower_dynamic_class_has_constructor(ctx, inst)
}

/// Classifies a dynamically named class for PDO's custom statement construction rules.
pub(crate) fn lower_elephc_pdo_statement_class_status(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_pdo_statement_class_status", 1)?;
    super::super::objects::lower_dynamic_pdo_statement_class_status(ctx, inst)
}

/// Classifies the late-static called class for `PDO::connect()` driver validation.
pub(crate) fn lower_elephc_pdo_called_class_status(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_pdo_called_class_status", 1)?;
    super::super::objects::lower_dynamic_pdo_called_class_status(ctx, inst)
}

/// Invokes a selected PDOStatement subclass constructor after its native state is initialized.
pub(crate) fn lower_elephc_invoke_pdo_statement_constructor(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_invoke_pdo_statement_constructor", 3)?;
    super::super::objects::lower_dynamic_pdo_statement_constructor_call(ctx, inst)
}

/// Initializes the private PDOStatement base fields on a dynamically allocated subclass.
pub(crate) fn lower_elephc_initialize_pdo_statement(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "__elephc_initialize_pdo_statement", 5)?;
    super::super::objects::lower_dynamic_pdo_statement_initialize(ctx, inst)
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
    materialize_optional_strtotime_base(ctx, base, name)?;
    abi::emit_call_label(ctx.emitter, "__rt_strtotime");
    Ok(())
}

/// Materializes `strtotime()`'s nullable base timestamp and its runtime presence flag.
///
/// Nullable unions use the inline tagged-scalar representation, while wider `mixed` values
/// use a boxed cell. Both forms must preserve PHP's distinction between a real timestamp zero
/// and `null`, which asks timelib to use the current time.
fn materialize_optional_strtotime_base(
    ctx: &mut FunctionContext<'_>,
    base: Option<ValueId>,
    name: &str,
) -> Result<()> {
    let Some(base) = base else {
        emit_strtotime_base_absent(ctx);
        return Ok(());
    };
    match ctx.value_php_type(base)?.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            emit_strtotime_base_absent(ctx);
        }
        PhpType::Int | PhpType::Bool => {
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.load_value_to_reg(base, "x0")?;
                    ctx.emitter.instruction("mov x3, #1");                      // a concrete base timestamp was provided
                }
                Arch::X86_64 => {
                    ctx.load_value_to_reg(base, "rdx")?;
                    ctx.emitter.instruction("mov rcx, 1");                      // a concrete base timestamp was provided
                }
            }
        }
        PhpType::TaggedScalar => {
            stage_date_string_regs(ctx);
            ctx.load_value_to_result(base)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("cmp x1, #8");                      // tagged-scalar tag 8 means the optional base is null
                    ctx.emitter.instruction("cset x3, ne");                     // only a non-null payload supplies the base timestamp
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("cmp rdx, 8");                      // tagged-scalar tag 8 means the optional base is null
                    ctx.emitter.instruction("setne cl");                        // record whether the tagged payload is a concrete integer
                    ctx.emitter.instruction("movzx rcx, cl");                   // widen the has-base flag for the runtime ABI
                    ctx.emitter.instruction("mov rdx, rax");                    // move the tagged payload into the base-timestamp register
                }
            }
            unstage_date_string_regs(ctx);
        }
        PhpType::Mixed | PhpType::Union(_) => {
            materialize_boxed_nullable_strtotime_base(ctx, base)?;
        }
        PhpType::Object(class_name) => {
            super::super::exceptions::emit_type_error(
                ctx,
                &format!(
                    "{}(): Argument #2 ($baseTimestamp) must be of type ?int, {} given",
                    name, class_name
                ),
            );
        }
        ty => {
            return Err(CodegenIrError::unsupported(format!(
                "strtotime base for PHP type {:?}",
                ty
            )));
        }
    }
    Ok(())
}

/// Materializes a boxed nullable `strtotime()` base while preserving the staged datetime string.
fn materialize_boxed_nullable_strtotime_base(
    ctx: &mut FunctionContext<'_>,
    base: ValueId,
) -> Result<()> {
    let null_label = ctx.next_label("strtotime_base_null");
    let done_label = ctx.next_label("strtotime_base_ready");
    stage_date_string_regs(ctx);
    load_value_to_first_int_arg(ctx, base)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp x0, #8");                              // runtime tag 8 means the boxed base is null
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // null asks timelib to choose the current timestamp
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.emitter.instruction("mov x3, #1");                              // a concrete boxed base timestamp was provided
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the null cleanup path
            ctx.emitter.label(&null_label);
            abi::emit_pop_reg(ctx.emitter, "x9");
            ctx.emitter.instruction("mov x3, #0");                              // boxed null leaves the base timestamp absent
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp rax, 8");                              // runtime tag 8 means the boxed base is null
            ctx.emitter.instruction(&format!("je {}", null_label));             // null asks timelib to choose the current timestamp
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.emitter.instruction("mov rdx, rax");                            // move the concrete payload into the base-timestamp register
            ctx.emitter.instruction("mov rcx, 1");                              // a concrete boxed base timestamp was provided
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the null cleanup path
            ctx.emitter.label(&null_label);
            abi::emit_pop_reg(ctx.emitter, "r11");
            ctx.emitter.instruction("xor ecx, ecx");                            // boxed null leaves the base timestamp absent
        }
    }
    ctx.emitter.label(&done_label);
    unstage_date_string_regs(ctx);
    Ok(())
}

/// Marks `strtotime()`'s optional base as absent in the target runtime ABI.
fn emit_strtotime_base_absent(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x3, #0");                              // no base means the runtime uses the current time
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("xor ecx, ecx");                            // no base means the runtime uses the current time
        }
    }
}

/// Maps a failed `__rt_strtotime` status to the legacy internal integer value `-1`.
fn emit_strtotime_failure_to_minus_one(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("cmp x1, #0");                              // did timelib report parse failure?
            ctx.emitter.instruction("mov x13, #-1");                            // legacy in-object failure value
            ctx.emitter.instruction("csel x0, x13, x0, eq");                    // failure → -1, otherwise keep the timestamp
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("test rdx, rdx");                           // did timelib report parse failure?
            ctx.emitter.instruction("mov r10, -1");                             // legacy in-object failure value
            ctx.emitter.instruction("cmove rax, r10");                          // failure → -1, otherwise keep the timestamp
        }
    }
}

/// Lowers `time()` through the shared wall-clock runtime helper.
pub(crate) fn lower_time(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "time", 0)?;
    abi::emit_call_label(ctx.emitter, "__rt_time");
    store_if_result(ctx, inst)
}

/// Lowers `usleep(microseconds)` through the target's C library symbol.
pub(crate) fn lower_usleep(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_unary_blocking_c_call(ctx, inst, "usleep", "usleep microseconds")
}

/// Lowers `exit(status?)` and `die(status?)` by terminating the current process.
pub(super) fn lower_exit(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count_between(inst, "exit", 0, 1)?;
    let Some(status) = inst.operands.first().copied() else {
        if ctx.shared.instrument.is_on() {
            // Shutdown output handlers are PHP calls and must finish inside the
            // still-open exact stack before the termination hook closes it.
            abi::emit_call_label(ctx.emitter, "__rt_ob_flush_all");
            crate::codegen::frame::emit_instr_terminate(ctx);
        }
        abi::emit_exit(ctx.emitter, 0);
        return Ok(());
    };
    require_integer_like(ctx.load_value_to_result(status)?, "exit status")?;
    emit_dynamic_exit(ctx);
    Ok(())
}

/// Lowers `getenv(name)` through the target-aware environment lookup helper and
/// boxes its string-or-false result.
///
/// The boxing is what makes the two answers distinguishable. Without it the
/// result was a plain string, so a variable that is NOT SET came back as `""` —
/// indistinguishable from one set to the empty string, and `getenv($x) !== false`,
/// which is the idiom for "is this set", was true for every name.
pub(crate) fn lower_getenv(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    super::ensure_arg_count(inst, "getenv", 1)?;
    let name = expect_operand(inst, 0)?;
    require_string(ctx.load_value_to_result(name)?.codegen_repr(), "getenv name")?;
    abi::emit_call_label(ctx.emitter, "__rt_getenv");
    super::io::box_owned_string_or_false_result(ctx, "getenv");
    store_if_result(ctx, inst)
}

/// Lowers `putenv(assignment)` by copying the environment string into persistent heap storage.
pub(crate) fn lower_putenv(
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

/// Lowers `setlocale(category, locales, ...rest)` through libc and boxes `string|false`.
pub(crate) fn lower_setlocale(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    if inst.operands.len() < 2 {
        return Err(CodegenIrError::invalid_module(format!(
            "setlocale expected at least 2 args, got {}",
            inst.operands.len()
        )));
    }
    if inst.result.is_some() && inst.result_php_type.codegen_repr() != PhpType::Mixed {
        return Err(CodegenIrError::invalid_module(format!(
            "setlocale result must be Mixed (string|false), got {:?}",
            inst.result_php_type
        )));
    }

    let category = expect_operand(inst, 0)?;
    require_integer_like(
        ctx.raw_value_php_type(category)?.codegen_repr(),
        "setlocale category",
    )?;
    let success_label = ctx.next_label("setlocale_success");
    let done_label = ctx.next_label("setlocale_done");

    for candidate in inst.operands.iter().skip(1).copied() {
        let candidate_ty = ctx.raw_value_php_type(candidate)?.clone();
        match candidate_ty {
            PhpType::Array(element) if element.codegen_repr() == PhpType::Str => {
                emit_setlocale_string_array_candidate(
                    ctx,
                    category,
                    candidate,
                    &success_label,
                )?;
            }
            other
                if matches!(
                    other.codegen_repr(),
                    PhpType::Str
                        | PhpType::Int
                        | PhpType::Float
                        | PhpType::Bool
                        | PhpType::Void
                        | PhpType::Never
                        | PhpType::TaggedScalar
                        | PhpType::Mixed
                        | PhpType::Union(_)
                ) =>
            {
                emit_setlocale_string_candidate(ctx, category, candidate, &success_label)?;
            }
            other => {
                return Err(CodegenIrError::unsupported(format!(
                    "setlocale locale candidate for PHP type {:?}",
                    other
                )));
            }
        }
    }

    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Bool);
    abi::emit_jump(ctx.emitter, &done_label);

    ctx.emitter.label(&success_label);
    emit_setlocale_success_string(ctx);
    crate::codegen::emit_box_current_value_as_mixed(ctx.emitter, &PhpType::Str);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Calls libc `setlocale` for one already-evaluated string candidate.
fn emit_setlocale_string_candidate(
    ctx: &mut FunctionContext<'_>,
    category: ValueId,
    candidate: ValueId,
    success_label: &str,
) -> Result<()> {
    let convert_label = ctx.next_label("setlocale_candidate_convert");
    let call_label = ctx.next_label("setlocale_candidate_call");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            require_integer_like(
                ctx.load_value_to_result(category)?.codegen_repr(),
                "setlocale category",
            )?;
            abi::emit_push_reg(ctx.emitter, "x0");
            super::strings::load_value_as_string_to_regs(
                ctx,
                candidate,
                "setlocale",
                "x1",
                "x2",
            )?;
            ctx.emitter.instruction("cmp x2, #1");                              // only the single byte "0" requests the current locale
            ctx.emitter.instruction(&format!("b.ne {}", convert_label));        // ordinary locale strings must be null-terminated
            ctx.emitter.instruction("ldrb w9, [x1]");                           // inspect the sole locale-candidate byte
            ctx.emitter.instruction("cmp w9, #48");                             // is this PHP's special "0" query candidate?
            ctx.emitter.instruction(&format!("b.ne {}", convert_label));        // nonzero one-byte locale names remain ordinary candidates
            ctx.emitter.instruction("mov x1, #0");                              // libc uses a null locale pointer to query current state
            ctx.emitter.instruction(&format!("b {}", call_label));              // skip scratch C-string conversion for the query
            ctx.emitter.label(&convert_label);
            abi::emit_call_label(ctx.emitter, "__rt_cstr");
            ctx.emitter.instruction("mov x1, x0");                              // pass the null-terminated locale candidate as libc's second argument
            ctx.emitter.label(&call_label);
            abi::emit_pop_reg(ctx.emitter, "x0");
            ctx.emitter.bl_c("setlocale");
            ctx.emitter.instruction(&format!("cbnz x0, {}", success_label));    // stop at the first locale candidate accepted by libc
        }
        Arch::X86_64 => {
            require_integer_like(
                ctx.load_value_to_result(category)?.codegen_repr(),
                "setlocale category",
            )?;
            abi::emit_push_reg(ctx.emitter, "rax");
            super::strings::load_value_as_string_to_regs(
                ctx,
                candidate,
                "setlocale",
                "rax",
                "rdx",
            )?;
            ctx.emitter.instruction("cmp rdx, 1");                              // only the single byte "0" requests the current locale
            ctx.emitter.instruction(&format!("jne {}", convert_label));         // ordinary locale strings must be null-terminated
            ctx.emitter.instruction("cmp BYTE PTR [rax], 48");                  // is this PHP's special "0" query candidate?
            ctx.emitter.instruction(&format!("jne {}", convert_label));         // nonzero one-byte locale names remain ordinary candidates
            ctx.emitter.instruction("xor esi, esi");                            // libc uses a null locale pointer to query current state
            ctx.emitter.instruction(&format!("jmp {}", call_label));            // skip scratch C-string conversion for the query
            ctx.emitter.label(&convert_label);
            abi::emit_call_label(ctx.emitter, "__rt_cstr");
            ctx.emitter.instruction("mov rsi, rax");                            // pass the null-terminated locale candidate as libc's second argument
            ctx.emitter.label(&call_label);
            abi::emit_pop_reg(ctx.emitter, "rdi");
            ctx.emitter.bl_c("setlocale");
            ctx.emitter.instruction("test rax, rax");                           // did libc accept the requested locale candidate?
            ctx.emitter.instruction(&format!("jnz {}", success_label));         // stop at the first accepted locale candidate
        }
    }
    Ok(())
}

/// Tries every value of one indexed string-array candidate in PHP iteration order.
fn emit_setlocale_string_array_candidate(
    ctx: &mut FunctionContext<'_>,
    category: ValueId,
    candidate: ValueId,
    success_label: &str,
) -> Result<()> {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            emit_setlocale_string_array_candidate_aarch64(
                ctx,
                category,
                candidate,
                success_label,
            )
        }
        Arch::X86_64 => {
            emit_setlocale_string_array_candidate_x86_64(
                ctx,
                category,
                candidate,
                success_label,
            )
        }
    }
}

/// Emits the AArch64 indexed string-array locale candidate loop.
fn emit_setlocale_string_array_candidate_aarch64(
    ctx: &mut FunctionContext<'_>,
    category: ValueId,
    candidate: ValueId,
    success_label: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("setlocale_array_loop");
    let exhausted_label = ctx.next_label("setlocale_array_exhausted");
    let accepted_label = ctx.next_label("setlocale_array_accepted");
    let continue_label = ctx.next_label("setlocale_array_continue");
    let convert_label = ctx.next_label("setlocale_array_convert");
    let call_label = ctx.next_label("setlocale_array_call");

    require_integer_like(
        ctx.load_value_to_result(category)?.codegen_repr(),
        "setlocale category",
    )?;
    abi::emit_push_reg(ctx.emitter, "x0");
    ctx.load_value_to_reg(candidate, "x10")?;
    ctx.emitter.instruction("ldr x9, [x10]");                                   // load the indexed locale-candidate count
    ctx.emitter.instruction("add x10, x10, #24");                               // point at the first 16-byte string payload slot
    ctx.emitter.instruction("mov x12, #0");                                     // begin with the first locale candidate
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp x12, x9");                                     // have all array candidates been attempted?
    ctx.emitter.instruction(&format!("b.ge {}", exhausted_label));              // continue with later variadic candidates when exhausted
    ctx.emitter.instruction("lsl x13, x12, #4");                                // scale the array index by the string slot width
    ctx.emitter.instruction("ldr x1, [x10, x13]");                              // load the current locale string pointer
    ctx.emitter.instruction("add x14, x13, #8");                                // compute the current locale string length offset
    ctx.emitter.instruction("ldr x2, [x10, x14]");                              // load the current locale string byte length
    abi::emit_push_reg_pair(ctx.emitter, "x9", "x10");
    abi::emit_push_reg(ctx.emitter, "x12");
    ctx.emitter.instruction("cmp x2, #1");                                      // only the single byte "0" requests current locale state
    ctx.emitter.instruction(&format!("b.ne {}", convert_label));                // ordinary array candidates need C-string conversion
    ctx.emitter.instruction("ldrb w15, [x1]");                                  // inspect the sole locale-candidate byte
    ctx.emitter.instruction("cmp w15, #48");                                    // is this array entry PHP's special "0" query?
    ctx.emitter.instruction(&format!("b.ne {}", convert_label));                // nonzero one-byte locale names remain ordinary candidates
    ctx.emitter.instruction("mov x1, #0");                                      // query libc with a null locale pointer
    ctx.emitter.instruction(&format!("b {}", call_label));                      // skip scratch conversion for the query entry
    ctx.emitter.label(&convert_label);
    abi::emit_call_label(ctx.emitter, "__rt_cstr");
    ctx.emitter.instruction("mov x1, x0");                                      // pass the current null-terminated array candidate to libc
    ctx.emitter.label(&call_label);
    ctx.emitter.instruction("ldr x0, [sp, #32]");                               // reload the locale category parked below loop state
    ctx.emitter.bl_c("setlocale");
    ctx.emitter.instruction("mov x15, x0");                                     // preserve libc's result while restoring the locale loop
    abi::emit_pop_reg(ctx.emitter, "x12");
    abi::emit_pop_reg_pair(ctx.emitter, "x9", "x10");
    ctx.emitter.instruction("mov x0, x15");                                     // restore libc's selected-locale pointer
    ctx.emitter.instruction(&format!("cbnz x0, {}", accepted_label));           // stop when this array candidate succeeds
    ctx.emitter.instruction("add x12, x12, #1");                                // advance to the next array candidate
    ctx.emitter.instruction(&format!("b {}", loop_label));                      // keep trying candidates in PHP array order
    ctx.emitter.label(&exhausted_label);
    abi::emit_pop_reg(ctx.emitter, "x1");
    abi::emit_jump(ctx.emitter, &continue_label);
    ctx.emitter.label(&accepted_label);
    abi::emit_pop_reg(ctx.emitter, "x1");
    abi::emit_jump(ctx.emitter, success_label);
    ctx.emitter.label(&continue_label);
    Ok(())
}

/// Emits the Linux x86_64 indexed string-array locale candidate loop.
fn emit_setlocale_string_array_candidate_x86_64(
    ctx: &mut FunctionContext<'_>,
    category: ValueId,
    candidate: ValueId,
    success_label: &str,
) -> Result<()> {
    let loop_label = ctx.next_label("setlocale_array_loop");
    let exhausted_label = ctx.next_label("setlocale_array_exhausted");
    let accepted_label = ctx.next_label("setlocale_array_accepted");
    let continue_label = ctx.next_label("setlocale_array_continue");
    let convert_label = ctx.next_label("setlocale_array_convert");
    let call_label = ctx.next_label("setlocale_array_call");

    require_integer_like(
        ctx.load_value_to_result(category)?.codegen_repr(),
        "setlocale category",
    )?;
    abi::emit_push_reg(ctx.emitter, "rax");
    ctx.load_value_to_reg(candidate, "r11")?;
    ctx.emitter.instruction("mov r12, QWORD PTR [r11]");                        // load the indexed locale-candidate count
    ctx.emitter.instruction("lea r11, [r11 + 24]");                             // point at the first 16-byte string payload slot
    ctx.emitter.instruction("xor r13d, r13d");                                  // begin with the first locale candidate
    ctx.emitter.label(&loop_label);
    ctx.emitter.instruction("cmp r13, r12");                                    // have all array candidates been attempted?
    ctx.emitter.instruction(&format!("jge {}", exhausted_label));               // continue with later variadic candidates when exhausted
    ctx.emitter.instruction("mov rcx, r13");                                    // copy the array index before scaling it
    ctx.emitter.instruction("shl rcx, 4");                                      // scale the array index by the string slot width
    ctx.emitter.instruction("mov rax, QWORD PTR [r11 + rcx]");                  // load the current locale string pointer
    ctx.emitter.instruction("mov rdx, QWORD PTR [r11 + rcx + 8]");              // load the current locale string byte length
    abi::emit_push_reg_pair(ctx.emitter, "r11", "r12");
    abi::emit_push_reg(ctx.emitter, "r13");
    ctx.emitter.instruction("cmp rdx, 1");                                      // only the single byte "0" requests current locale state
    ctx.emitter.instruction(&format!("jne {}", convert_label));                 // ordinary array candidates need C-string conversion
    ctx.emitter.instruction("cmp BYTE PTR [rax], 48");                          // is this array entry PHP's special "0" query?
    ctx.emitter.instruction(&format!("jne {}", convert_label));                 // nonzero one-byte locale names remain ordinary candidates
    ctx.emitter.instruction("xor esi, esi");                                    // query libc with a null locale pointer
    ctx.emitter.instruction(&format!("jmp {}", call_label));                    // skip scratch conversion for the query entry
    ctx.emitter.label(&convert_label);
    abi::emit_call_label(ctx.emitter, "__rt_cstr");
    ctx.emitter.instruction("mov rsi, rax");                                    // pass the current null-terminated array candidate to libc
    ctx.emitter.label(&call_label);
    ctx.emitter.instruction("mov rdi, QWORD PTR [rsp + 32]");                   // reload the locale category parked below loop state
    ctx.emitter.bl_c("setlocale");
    ctx.emitter.instruction("mov r10, rax");                                    // preserve libc's result while restoring the locale loop
    abi::emit_pop_reg(ctx.emitter, "r13");
    abi::emit_pop_reg_pair(ctx.emitter, "r11", "r12");
    ctx.emitter.instruction("mov rax, r10");                                    // restore libc's selected-locale pointer
    ctx.emitter.instruction("test rax, rax");                                   // did the current array candidate succeed?
    ctx.emitter.instruction(&format!("jnz {}", accepted_label));                // stop when this array candidate succeeds
    ctx.emitter.instruction("add r13, 1");                                      // advance to the next array candidate
    ctx.emitter.instruction(&format!("jmp {}", loop_label));                    // keep trying candidates in PHP array order
    ctx.emitter.label(&exhausted_label);
    abi::emit_pop_reg(ctx.emitter, "rdi");
    abi::emit_jump(ctx.emitter, &continue_label);
    ctx.emitter.label(&accepted_label);
    abi::emit_pop_reg(ctx.emitter, "rdi");
    abi::emit_jump(ctx.emitter, success_label);
    ctx.emitter.label(&continue_label);
    Ok(())
}

/// Converts libc's accepted locale pointer into elephc string result registers.
fn emit_setlocale_success_string(ctx: &mut FunctionContext<'_>) {
    let scan_label = ctx.next_label("setlocale_result_scan");
    let scanned_label = ctx.next_label("setlocale_result_scanned");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x1, x0");                              // keep the selected locale pointer as the PHP string result
            ctx.emitter.instruction("mov x2, #0");                              // initialize the selected locale byte length
            ctx.emitter.label(&scan_label);
            ctx.emitter.instruction("ldrb w9, [x1, x2]");                       // read the next selected-locale byte
            ctx.emitter.instruction(&format!("cbz w9, {}", scanned_label));     // stop at libc's trailing null byte
            ctx.emitter.instruction("add x2, x2, #1");                          // count one more selected-locale byte
            ctx.emitter.instruction(&format!("b {}", scan_label));              // continue measuring the selected locale
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov r8, rax");                             // keep the selected locale pointer while measuring it
            ctx.emitter.instruction("xor edx, edx");                            // initialize the selected locale byte length
            ctx.emitter.label(&scan_label);
            ctx.emitter.instruction("cmp BYTE PTR [r8 + rdx], 0");              // test for libc's trailing null byte
            ctx.emitter.instruction(&format!("je {}", scanned_label));          // stop after measuring the full selected locale
            ctx.emitter.instruction("add rdx, 1");                              // count one more selected-locale byte
            ctx.emitter.instruction(&format!("jmp {}", scan_label));            // continue measuring the selected locale
            ctx.emitter.label(&scanned_label);
            ctx.emitter.instruction("mov rax, r8");                             // restore the selected locale pointer as the PHP string result
            return;
        }
    }
    ctx.emitter.label(&scanned_label);
}

/// Lowers `php_uname(mode?)` through the target-aware uname runtime helper.
pub(crate) fn lower_php_uname(
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
pub(crate) fn lower_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "exec")
}

/// Lowers `shell_exec(command)` by capturing shell stdout through the shared runtime helper.
pub(crate) fn lower_shell_exec(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_shell_exec_like(ctx, inst, "shell_exec")
}

/// Lowers `system(command)` through libc `system()` and returns the compiler's empty string result.
pub(crate) fn lower_system(
    ctx: &mut FunctionContext<'_>,
    inst: &Instruction,
) -> Result<()> {
    lower_direct_system_call(ctx, inst, "system", true)
}

/// Lowers `passthru(command)` through libc `system()` for direct stdout passthrough.
pub(crate) fn lower_passthru(
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
    abi::emit_cdylib_exit_escape(ctx.emitter);
    match (ctx.emitter.target.platform, ctx.emitter.target.arch) {
        (Platform::MacOS, Arch::AArch64) | (Platform::Linux, Arch::AArch64) => {
            ctx.emitter.instruction("mov x19, x0");                             // stash the exit code in a callee-saved register (this path never returns)
            ctx.emitter.instruction("bl __rt_ob_flush_all");                    // drain still-active output buffers to stdout before terminating
            crate::codegen::frame::emit_instr_terminate(ctx);
            ctx.emitter.instruction("mov x0, x19");                             // restore the exit code into the syscall argument register
            ctx.emitter.syscall(1);
        }
        (Platform::Linux, Arch::X86_64) => {
            ctx.emitter.instruction("mov rbx, rax");                            // stash the exit code in a callee-saved register (this path never returns)
            ctx.emitter.instruction("and rsp, -16");                            // realign the stack for the flush call (this path never returns)
            ctx.emitter.instruction("call __rt_ob_flush_all");                  // drain still-active output buffers to stdout before terminating
            crate::codegen::frame::emit_instr_terminate(ctx);
            ctx.emitter.instruction("mov rdi, rbx");                            // move the computed exit code into the SysV first-argument register
            ctx.emitter.instruction("mov eax, 60");                             // Linux x86_64 syscall 60 = exit
            ctx.emitter.instruction("syscall");                                 // terminate the process through the Linux x86_64 syscall ABI
        }
        (Platform::MacOS, Arch::X86_64) => {
            panic!("exit() is not implemented yet for target macos-x86_64");
        }
        (Platform::Windows, _) => {
            panic!("Windows target is not yet supported (see issue #379)");
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

/// Loads an optional date timestamp and records whether PHP supplied a concrete value.
///
/// The integer result register still receives `-1` for an omitted/null argument so legacy
/// decomposition helpers remain compatible. The presence flag (`x4` on ARM64, `rcx` on x86_64)
/// lets `date()`/`gmdate()` distinguish that sentinel from a real pre-epoch timestamp of `-1`.
fn load_date_timestamp(
    ctx: &mut FunctionContext<'_>,
    timestamp: Option<ValueId>,
) -> Result<()> {
    let result_reg = abi::int_result_reg(ctx.emitter);
    let Some(timestamp) = timestamp else {
        abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
        emit_date_timestamp_presence(ctx, false);
        return Ok(());
    };
    match ctx.value_php_type(timestamp)?.codegen_repr() {
        PhpType::Void | PhpType::Never => {
            abi::emit_load_int_immediate(ctx.emitter, result_reg, -1);
            emit_date_timestamp_presence(ctx, false);
            Ok(())
        }
        PhpType::Int | PhpType::Bool => {
            ctx.load_value_to_result(timestamp)?;
            emit_date_timestamp_presence(ctx, true);
            Ok(())
        }
        PhpType::TaggedScalar => {
            ctx.load_value_to_result(timestamp)?;
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction("mov x9, #-1");                     // materialize the current-time sentinel
                    ctx.emitter.instruction("cmp x1, #8");                      // tagged-scalar tag 8 means the timestamp is null
                    ctx.emitter.instruction("cset x4, ne");                     // record whether PHP supplied a concrete timestamp
                    ctx.emitter.instruction("csel x0, x9, x0, eq");             // null selects current time; integers keep their payload
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction("mov r11, -1");                     // materialize the current-time sentinel
                    ctx.emitter.instruction("cmp rdx, 8");                      // tagged-scalar tag 8 means the timestamp is null
                    ctx.emitter.instruction("setne cl");                        // record whether PHP supplied a concrete timestamp
                    ctx.emitter.instruction("movzx rcx, cl");                   // widen the presence flag for the runtime ABI
                    ctx.emitter.instruction("cmove rax, r11");                  // null selects current time; integers keep their payload
                }
            }
            Ok(())
        }
        // A boxed Mixed/Union timestamp (for example read from an associative array or produced by
        // a Mixed-typed expression) is unboxed to its integer value before formatting, matching
        // PHP's implicit integer coercion of the timestamp argument. The unboxed result lands in
        // the integer result register, which is where the formatter helper reads the timestamp.
        PhpType::Mixed | PhpType::Union(_) => {
            load_boxed_nullable_date_timestamp(ctx, timestamp)?;
            Ok(())
        }
        ty => Err(CodegenIrError::unsupported(format!(
            "date timestamp for PHP type {:?}",
            ty
        ))),
    }
}

/// Loads a boxed date timestamp, selecting current time for PHP null and integer coercion otherwise.
fn load_boxed_nullable_date_timestamp(
    ctx: &mut FunctionContext<'_>,
    timestamp: ValueId,
) -> Result<()> {
    let null_label = ctx.next_label("date_timestamp_null");
    let done_label = ctx.next_label("date_timestamp_ready");
    load_value_to_first_int_arg(ctx, timestamp)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_push_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp x0, #8");                              // runtime tag 8 means the boxed timestamp is null
            ctx.emitter.instruction(&format!("b.eq {}", null_label));           // null selects the formatter's current-time sentinel
            abi::emit_pop_reg(ctx.emitter, "x0");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.emitter.instruction("mov x4, #1");                              // a concrete boxed timestamp was supplied
            ctx.emitter.instruction(&format!("b {}", done_label));              // skip the null cleanup path after coercion
            ctx.emitter.label(&null_label);
            abi::emit_pop_reg(ctx.emitter, "x9");
            ctx.emitter.instruction("mov x0, #-1");                             // pass the current-time sentinel for boxed null
            ctx.emitter.instruction("mov x4, #0");                              // boxed null means no concrete timestamp was supplied
        }
        Arch::X86_64 => {
            abi::emit_push_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
            ctx.emitter.instruction("cmp rax, 8");                              // runtime tag 8 means the boxed timestamp is null
            ctx.emitter.instruction(&format!("je {}", null_label));             // null selects the formatter's current-time sentinel
            abi::emit_pop_reg(ctx.emitter, "rdi");
            abi::emit_call_label(ctx.emitter, "__rt_mixed_cast_int");
            ctx.emitter.instruction("mov rcx, 1");                              // a concrete boxed timestamp was supplied
            ctx.emitter.instruction(&format!("jmp {}", done_label));            // skip the null cleanup path after coercion
            ctx.emitter.label(&null_label);
            abi::emit_pop_reg(ctx.emitter, "r11");
            ctx.emitter.instruction("mov rax, -1");                             // pass the current-time sentinel for boxed null
            ctx.emitter.instruction("xor ecx, ecx");                            // boxed null means no concrete timestamp was supplied
        }
    }
    ctx.emitter.label(&done_label);
    Ok(())
}

/// Emits the target-specific optional-timestamp presence flag used by date runtimes.
fn emit_date_timestamp_presence(ctx: &mut FunctionContext<'_>, present: bool) {
    let value = i64::from(present);
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("mov x4, #{}", value));            // publish whether PHP supplied a concrete timestamp
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("mov rcx, {}", value));            // publish whether PHP supplied a concrete timestamp
        }
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
