//! Purpose:
//! Lowers the typed ext/posix operations that need more than a plain bridge call:
//! `posix_kill()`'s process-id `ValueError` and `posix_isatty()`'s int-or-stream operand.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::pcntl::lower()` for `PcntlRuntime::PosixKill`
//!   and `PcntlRuntime::PosixIsatty`.
//!
//! Key details:
//! - Both bridges return an `i64` status whose negative value selects PHP's diagnostic.
//! - A stream resource is reduced to its native descriptor before the terminal probe; an operand
//!   stored as a boxed `Mixed` cell is inspected at run time so `int|resource` values take the
//!   matching path, while inline `TaggedScalar` unions such as `?int` use the int conversion.
//! - A closed stream (negative release sentinel) throws PHP's `TypeError` before the probe.
//! - Integer operands pass `emit_int_param_guard()` first: values PHP's coercive `int` check
//!   refuses throw `TypeError` (or warn and return false for `posix_isatty()`) instead of
//!   reaching the weak cast, which would turn `[]` or `"abc"` into pid 0.

use crate::codegen::context::FunctionContext;
use crate::codegen::platform::Arch;
use crate::codegen::{abi, Result};
use crate::ir::Instruction;
use crate::types::PhpType;

use super::strings::load_as_int;
use super::{ensure_arg_count, expect_operand, store_if_result};

/// Runtime `Mixed` tag carried by boxed resource values.
const MIXED_TAG_RESOURCE: i64 = 9;
/// Runtime `Mixed` tag carried by boxed string values.
const MIXED_TAG_STRING: u8 = 1;

/// What a refused integer operand does once its diagnostic is built.
#[derive(Clone, Copy)]
pub(super) enum IntParamRejection<'a> {
    /// Throws the message as a catchable `TypeError`.
    Throw,
    /// Emits the message as a warning, loads `false` and jumps to the given label.
    WarnFalse(&'a str),
}

impl IntParamRejection<'_> {
    /// Whether the guarded parameter is `posix_isatty()`'s `int|resource`, which admits streams.
    fn admits_resources(self) -> bool {
        matches!(self, Self::WarnFalse(_))
    }
}

/// Lowers `posix_kill()` and raises PHP's `ValueError` for a process id outside `pid_t`.
pub(super) fn lower_kill(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "posix_kill", 2)?;
    // Both operands are validated before either is converted: a refused value must throw,
    // never coerce to pid 0 or -1 and signal a whole process group.
    emit_int_param_guard(
        ctx,
        expect_operand(inst, 0)?,
        "posix_kill(): Argument #1 ($process_id) must be of type int, ",
        IntParamRejection::Throw,
    )?;
    emit_int_param_guard(
        ctx,
        expect_operand(inst, 1)?,
        "posix_kill(): Argument #2 ($signal) must be of type int, ",
        IntParamRejection::Throw,
    )?;
    load_as_int(ctx, expect_operand(inst, 0)?, "posix_kill process_id")?;
    abi::emit_push_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
    load_as_int(ctx, expect_operand(inst, 1)?, "posix_kill signal")?;
    let invalid = ctx.next_label("posix_kill_invalid_pid");
    let done = ctx.next_label("posix_kill_done");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- deliver the signal and branch to the ValueError on a pid outside pid_t --
            ctx.emitter.instruction("mov x1, x0");                              // C arg1 = signal number to deliver
            abi::emit_pop_reg(ctx.emitter, "x0");                              // C arg0 = target process or group id
            ctx.emitter.bl_c("elephc_posix_kill");
            ctx.emitter.instruction("cmp x0, #0");                              // a negative status flags an out-of-range process id
            ctx.emitter.instruction(&format!("b.lt {invalid}"));                // raise PHP's ValueError instead of returning
        }
        Arch::X86_64 => {
            // -- deliver the signal and branch to the ValueError on a pid outside pid_t --
            ctx.emitter.instruction("mov rsi, rax");                            // C arg1 = signal number to deliver
            abi::emit_pop_reg(ctx.emitter, "rdi");                             // C arg0 = target process or group id
            ctx.emitter.bl_c("elephc_posix_kill");
            ctx.emitter.instruction("cmp rax, 0");                              // a negative status flags an out-of-range process id
            ctx.emitter.instruction(&format!("jl {invalid}"));                  // raise PHP's ValueError instead of returning
        }
    }
    abi::emit_jump(ctx.emitter, &done);
    ctx.emitter.label(&invalid);
    super::super::exceptions::emit_value_error(ctx, elephc_pcntl::POSIX_KILL_PID_RANGE_MESSAGE);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Lowers `posix_isatty()` for an integer descriptor, a stream resource, or a boxed value.
pub(super) fn lower_isatty(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    ensure_arg_count(inst, "posix_isatty", 1)?;
    let operand = expect_operand(inst, 0)?;
    let raw_type = ctx.raw_value_php_type(operand)?;
    let done = ctx.next_label("posix_isatty_done");
    if !matches!(raw_type, PhpType::Resource(_)) {
        // Resources pass the guard; PHP warns and returns false for any other refused value.
        emit_int_param_guard(
            ctx,
            operand,
            "posix_isatty(): Argument #1 ($file_descriptor) must be of type int|resource, ",
            IntParamRejection::WarnFalse(&done),
        )?;
    }
    if let PhpType::Resource(_) = raw_type {
        ctx.load_value_to_result(operand)?;
        emit_reject_closed_stream(ctx);
    } else if raw_type.codegen_repr() == PhpType::Mixed {
        // Only a boxed `Mixed` cell can hold a resource; `?int` keeps its union metadata but is
        // stored inline as a `TaggedScalar`, which the weak integer conversion handles.
        emit_boxed_descriptor(ctx, operand)?;
    } else {
        load_as_int(ctx, operand, "posix_isatty file_descriptor")?;
    }
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- probe the descriptor and skip the range warning on a boolean status --
            ctx.emitter.bl_c("elephc_posix_isatty");
            ctx.emitter.instruction("cmp x0, #0");                              // a negative status flags an out-of-range descriptor
            ctx.emitter.instruction(&format!("b.ge {done}"));                   // terminal and non-terminal results are already booleans
        }
        Arch::X86_64 => {
            // -- probe the descriptor and skip the range warning on a boolean status --
            ctx.emitter.instruction("mov rdi, rax");                            // C arg0 = descriptor to probe
            ctx.emitter.bl_c("elephc_posix_isatty");
            ctx.emitter.instruction("cmp rax, 0");                              // a negative status flags an out-of-range descriptor
            ctx.emitter.instruction(&format!("jge {done}"));                    // terminal and non-terminal results are already booleans
        }
    }
    emit_fd_range_warning(ctx);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    ctx.emitter.label(&done);
    store_if_result(ctx, inst)
}

/// Loads a boxed `int|resource` operand as a descriptor, unwrapping resources to their fd.
///
/// Non-resource payloads fall back to PHP's weak integer conversion of the same operand.
fn emit_boxed_descriptor(
    ctx: &mut FunctionContext<'_>,
    operand: crate::ir::ValueId,
) -> Result<()> {
    let scalar = ctx.next_label("posix_isatty_scalar");
    let loaded = ctx.next_label("posix_isatty_fd_loaded");
    ctx.load_value_to_result(operand)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- unwrap a boxed stream resource to its native descriptor --
            ctx.emitter.instruction(&format!("cmp x0, #{MIXED_TAG_RESOURCE}")); // does the boxed operand hold a stream resource?
            ctx.emitter.instruction(&format!("b.ne {scalar}"));                 // integers and other scalars use weak int conversion
            ctx.emitter.instruction("mov x0, x1");                              // the resource payload is its native descriptor
        }
        Arch::X86_64 => {
            // -- unwrap a boxed stream resource to its native descriptor --
            ctx.emitter.instruction(&format!("cmp rax, {MIXED_TAG_RESOURCE}")); // does the boxed operand hold a stream resource?
            ctx.emitter.instruction(&format!("jne {scalar}"));                  // integers and other scalars use weak int conversion
            ctx.emitter.instruction("mov rax, rdi");                            // the resource payload is its native descriptor
        }
    }
    emit_reject_closed_stream(ctx);
    abi::emit_jump(ctx.emitter, &loaded);
    ctx.emitter.label(&scalar);
    load_as_int(ctx, operand, "posix_isatty file_descriptor")?;
    ctx.emitter.label(&loaded);
    Ok(())
}

/// Throws PHP's `TypeError` when the stream descriptor in the int result register is closed.
///
/// `fclose()` stamps a negative release sentinel into the resource payload, so a negative
/// descriptor here can only be a closed stream, never a caller-supplied integer.
fn emit_reject_closed_stream(ctx: &mut FunctionContext<'_>) {
    let open = ctx.next_label("posix_isatty_stream_open");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            // -- let open streams through; closed ones fall into the TypeError --
            ctx.emitter.instruction("cmp x0, #0");                              // a negative payload is fclose()'s release sentinel
            ctx.emitter.instruction(&format!("b.ge {open}"));                   // open streams keep their native descriptor
        }
        Arch::X86_64 => {
            // -- let open streams through; closed ones fall into the TypeError --
            ctx.emitter.instruction("cmp rax, 0");                              // a negative payload is fclose()'s release sentinel
            ctx.emitter.instruction(&format!("jge {open}"));                    // open streams keep their native descriptor
        }
    }
    super::super::exceptions::emit_type_error(ctx, elephc_pcntl::POSIX_ISATTY_CLOSED_STREAM_MESSAGE);
    ctx.emitter.label(&open);
}

/// Emits PHP's out-of-range descriptor warning through the shared diagnostic channel.
fn emit_fd_range_warning(ctx: &mut FunctionContext<'_>) {
    let (label, len) = ctx.data.add_string(elephc_pcntl::POSIX_ISATTY_FD_RANGE_WARNING.as_bytes());
    let (pointer_reg, length_reg) = match ctx.emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    abi::emit_symbol_address(ctx.emitter, pointer_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, length_reg, len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_diag_warning");
}

/// Refuses an operand that PHP's coercive `int` parameter check rejects, before any conversion.
///
/// `prefix` is the message up to the runtime type word (`"f(): Argument #1 ($x) must be of type
/// int, "`). Ints, bools, floats, nulls and `TaggedScalar` unions pass untouched: `load_as_int()`
/// applies their conversions and diagnostics afterwards. A string passes only when it is fully
/// numeric and fits PHP's `int` range; a boxed operand is unboxed and refused when it holds anything other than those
/// scalars, so arrays, objects, resources, closures and non-numeric strings never reach the
/// weak cast that would turn them into 0 or 1. Boxed resources pass only for the
/// `int|resource` warning mode of `posix_isatty()`, which resolves them to descriptors itself.
pub(super) fn emit_int_param_guard(
    ctx: &mut FunctionContext<'_>,
    operand: crate::ir::ValueId,
    prefix: &str,
    rejection: IntParamRejection<'_>,
) -> Result<()> {
    match ctx.raw_value_php_type(operand)?.codegen_repr() {
        PhpType::Str => {
            let accepted = ctx.next_label("int_param_string_ok");
            let refused = ctx.next_label("int_param_string_refused");
            ctx.load_value_to_result(operand)?;
            emit_string_int_param_check(ctx);
            abi::emit_branch_if_int_result_zero(ctx.emitter, &refused);
            abi::emit_jump(ctx.emitter, &accepted);
            ctx.emitter.label(&refused);
            emit_int_param_rejection(ctx, prefix, "string", rejection);
            ctx.emitter.label(&accepted);
        }
        PhpType::Mixed | PhpType::Union(_) => emit_boxed_int_param_guard(ctx, operand, prefix, rejection)?,
        _ => {}
    }
    Ok(())
}

/// Unboxes a `Mixed` operand and refuses every runtime tag PHP cannot coerce to `int`.
fn emit_boxed_int_param_guard(
    ctx: &mut FunctionContext<'_>,
    operand: crate::ir::ValueId,
    prefix: &str,
    rejection: IntParamRejection<'_>,
) -> Result<()> {
    let accepted = ctx.next_label("int_param_boxed_ok");
    let string_arm = ctx.next_label("int_param_boxed_string");
    crate::codegen::lower_inst::call_operands::load_value_to_first_int_arg(ctx, operand)?;
    abi::emit_call_label(ctx.emitter, "__rt_mixed_unbox");
    let tag_reg = abi::int_result_reg(ctx.emitter);
    let mut refused = Vec::new();
    for (tag, name) in crate::codegen::lower_inst::objects::RUNTIME_TAGS {
        let target = match (tag, name) {
            (MIXED_TAG_STRING, _) => string_arm.clone(),
            (_, "int" | "float" | "bool" | "null") => accepted.clone(),
            (_, "resource") if rejection.admits_resources() => accepted.clone(),
            _ => {
                let label = ctx.next_label(&format!("int_param_refuse_{name}"));
                refused.push((tag, name, label.clone()));
                label
            }
        };
        crate::codegen::lower_inst::enums::emit_mixed_tag_branch(ctx, tag_reg, i64::from(tag), &target);
    }
    // `__rt_mixed_unbox` peels nested boxes, so no other tag reaches this fall-through.
    emit_int_param_rejection(ctx, prefix, "mixed", rejection);
    for (tag, name, label) in refused {
        ctx.emitter.label(&label);
        if tag == crate::codegen::lower_inst::objects::OBJECT_TAG {
            emit_object_int_param_rejection(ctx, prefix, rejection);
        } else {
            emit_int_param_rejection(ctx, prefix, name, rejection);
        }
    }
    ctx.emitter.label(&string_arm);
    let (string_ptr, string_len) = abi::string_result_regs(ctx.emitter);
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let payload_len = match ctx.emitter.target.arch {
        Arch::AArch64 => "x2",
        Arch::X86_64 => "rdx",
    };
    abi::emit_reg_move(ctx.emitter, string_len, payload_len);
    abi::emit_reg_move(ctx.emitter, string_ptr, payload);
    emit_string_int_param_check(ctx);
    let string_refused = ctx.next_label("int_param_refuse_string");
    abi::emit_branch_if_int_result_zero(ctx.emitter, &string_refused);
    abi::emit_jump(ctx.emitter, &accepted);
    ctx.emitter.label(&string_refused);
    emit_int_param_rejection(ctx, prefix, "string", rejection);
    ctx.emitter.label(&accepted);
    Ok(())
}

/// Asks the bridge whether the string in the string-result registers passes PHP's `int` check.
///
/// Leaves 1 in the integer result register for an accepted string and 0 for a refused one.
/// The bridge checks the value range as well as the grammar, so `"1e999"` or
/// `"9223372036854775808"` is refused instead of saturating or collapsing to pid 0.
fn emit_string_int_param_check(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, x1");                              // C arg0 = PHP string bytes
            ctx.emitter.instruction("mov x1, x2");                              // C arg1 = PHP string byte length
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov rdi, rax");                            // C arg0 = PHP string bytes
            ctx.emitter.instruction("mov rsi, rdx");                            // C arg1 = PHP string byte length
        }
    }
    ctx.emitter.bl_c("elephc_posix_int_param_accepts");
}

/// Throws or warns `<prefix><given> given` for a statically named runtime type.
fn emit_int_param_rejection(
    ctx: &mut FunctionContext<'_>,
    prefix: &str,
    given: &str,
    rejection: IntParamRejection<'_>,
) {
    match rejection {
        IntParamRejection::Throw => {
            super::super::exceptions::emit_type_error(ctx, &format!("{prefix}{given} given"));
        }
        IntParamRejection::WarnFalse(done) => {
            super::super::runtime_class_messages::emit_static_diagnostic(
                ctx,
                &format!("Warning: {prefix}{given} given\n"),
                true,
            );
            emit_false_result(ctx, done);
        }
    }
}

/// Throws or warns `<prefix><Class> given` for the unboxed object in the payload register.
fn emit_object_int_param_rejection(
    ctx: &mut FunctionContext<'_>,
    prefix: &str,
    rejection: IntParamRejection<'_>,
) {
    let payload = crate::codegen_support::mixed_unbox_payload_reg(ctx.emitter.target);
    let result = abi::int_result_reg(ctx.emitter);
    abi::emit_reg_move(ctx.emitter, result, payload);
    super::super::runtime_class_messages::emit_runtime_class_name_to_string_result(ctx, result, "object");
    match rejection {
        IntParamRejection::Throw => {
            super::super::runtime_class_messages::emit_concat_static_prefix(ctx, prefix);
            super::super::runtime_class_messages::emit_concat_static_suffix(ctx, " given");
            abi::emit_call_label(ctx.emitter, "__rt_str_persist");
            super::super::exceptions::emit_type_error_from_string_result(ctx);
        }
        IntParamRejection::WarnFalse(done) => {
            let (name_ptr, name_len) = abi::string_result_regs(ctx.emitter);
            abi::emit_push_reg_pair(ctx.emitter, name_ptr, name_len);
            super::super::runtime_class_messages::emit_static_diagnostic(ctx, &format!("Warning: {prefix}"), false);
            let (fragment_ptr, fragment_len) =
                super::super::runtime_class_messages::diagnostic_fragment_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, fragment_ptr, fragment_len);
            abi::emit_call_label(ctx.emitter, "__rt_diag_warning_fragment");
            super::super::runtime_class_messages::emit_static_diagnostic(ctx, " given\n", true);
            emit_false_result(ctx, done);
        }
    }
}

/// Loads PHP `false` into the integer result register and jumps to the builtin's result store.
fn emit_false_result(ctx: &mut FunctionContext<'_>, done: &str) {
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 0);
    abi::emit_jump(ctx.emitter, done);
}
