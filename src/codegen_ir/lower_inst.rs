//! Purpose:
//! Lowers individual EIR instructions into target-aware assembly snippets.
//! Starts with scalar constants and output needed for the first executable smoke test.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`.
//!
//! Key details:
//! - Results are written to fixed value-placement slots immediately after definition.
//! - Unsupported opcodes fail explicitly instead of falling back to legacy AST codegen.

use crate::codegen::abi;
use crate::codegen::platform::Arch;
use crate::ir::{CmpPredicate, Immediate, InstId, Instruction, LocalSlotId, Op, ValueId};
use crate::names::{function_symbol, ir_global_symbol};
use crate::types::PhpType;

use super::context::FunctionContext;
use super::function_variants;
use super::{CodegenIrError, Result};

mod arithmetic;
mod arrays;
mod buffers;
mod builtins;
mod comparisons;
mod conversions;
mod externs;
mod floats;
mod hashes;
mod iterators;
mod ownership;
mod pointers;
mod predicates;
mod strings;

/// Lowers one EIR instruction by opcode.
pub(super) fn lower_instruction(ctx: &mut FunctionContext<'_>, inst_id: InstId) -> Result<()> {
    let inst = ctx
        .function
        .instruction(inst_id)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst_id.as_raw()))?;
    match inst.op {
        Op::ConstI64 => lower_const_i64(ctx, &inst),
        Op::ConstF64 => floats::lower_const_f64(ctx, &inst),
        Op::ConstBool => lower_const_bool(ctx, &inst),
        Op::ConstNull => lower_const_null(ctx, &inst),
        Op::ConstStr => strings::lower_const_str(ctx, &inst),
        Op::LoadLocal => lower_load_local(ctx, &inst),
        Op::StoreLocal => lower_store_local(ctx, &inst),
        Op::LoadGlobal => lower_load_global(ctx, &inst),
        Op::StoreGlobal => lower_store_global(ctx, &inst),
        Op::IAdd => arithmetic::lower_int_binop(ctx, &inst, "add", "add"),
        Op::ISub => arithmetic::lower_int_binop(ctx, &inst, "sub", "sub"),
        Op::IMul => arithmetic::lower_int_binop(ctx, &inst, "mul", "imul"),
        Op::IDiv => arithmetic::lower_int_div_to_float(ctx, &inst),
        Op::ISMod => arithmetic::lower_int_mod(ctx, &inst),
        Op::INeg => arithmetic::lower_int_unary(ctx, &inst, "neg", "neg"),
        Op::IBitAnd => arithmetic::lower_int_binop(ctx, &inst, "and", "and"),
        Op::IBitOr => arithmetic::lower_int_binop(ctx, &inst, "orr", "or"),
        Op::IBitXor => arithmetic::lower_int_binop(ctx, &inst, "eor", "xor"),
        Op::IBitNot => arithmetic::lower_int_unary(ctx, &inst, "mvn", "not"),
        Op::IShl => arithmetic::lower_int_shift(ctx, &inst, "lsl", "shl"),
        Op::IShrA => arithmetic::lower_int_shift(ctx, &inst, "asr", "sar"),
        Op::MixedNumericBinop => arithmetic::lower_mixed_numeric_binop(ctx, &inst),
        Op::FAdd => floats::lower_float_binop(ctx, &inst, "fadd", "addsd"),
        Op::FSub => floats::lower_float_binop(ctx, &inst, "fsub", "subsd"),
        Op::FMul => floats::lower_float_binop(ctx, &inst, "fmul", "mulsd"),
        Op::FDiv => floats::lower_float_binop(ctx, &inst, "fdiv", "divsd"),
        Op::FPow => floats::lower_float_pow(ctx, &inst),
        Op::FNeg => floats::lower_float_neg(ctx, &inst),
        Op::ICmp => lower_int_compare(ctx, &inst),
        Op::FCmp => floats::lower_float_compare(ctx, &inst),
        Op::Spaceship => comparisons::lower_spaceship(ctx, &inst),
        Op::StrictEq => comparisons::lower_strict_eq(ctx, &inst, true),
        Op::StrictNotEq => comparisons::lower_strict_eq(ctx, &inst, false),
        Op::LooseEq => comparisons::lower_loose_eq(ctx, &inst, true),
        Op::LooseNotEq => comparisons::lower_loose_eq(ctx, &inst, false),
        Op::IsNull => predicates::lower_is_null(ctx, &inst),
        Op::IsTruthy => predicates::lower_is_truthy(ctx, &inst),
        Op::IToF => floats::lower_int_to_float(ctx, &inst),
        Op::FToI => floats::lower_float_to_int(ctx, &inst),
        Op::IToStr => strings::lower_int_like_to_string(ctx, &inst),
        Op::FToStr => strings::lower_float_to_string(ctx, &inst),
        Op::BoolToStr => strings::lower_int_like_to_string(ctx, &inst),
        Op::StrToI => conversions::lower_str_to_int(ctx, &inst),
        Op::StrToF => conversions::lower_str_to_float(ctx, &inst),
        Op::Cast => conversions::lower_cast(ctx, &inst),
        Op::StrConcat => strings::lower_str_concat(ctx, &inst),
        Op::StrLen => strings::lower_str_len(ctx, &inst),
        Op::StrCharAt => strings::lower_str_char_at(ctx, &inst),
        Op::ArrayNew => arrays::lower_array_new(ctx, &inst),
        Op::ArrayLen => arrays::lower_array_len(ctx, &inst),
        Op::ArrayGet => arrays::lower_array_get(ctx, &inst),
        Op::ArraySet => arrays::lower_array_set(ctx, &inst),
        Op::ArrayPush => arrays::lower_array_push(ctx, &inst),
        Op::HashNew => hashes::lower_hash_new(ctx, &inst),
        Op::HashLen => hashes::lower_hash_len(ctx, &inst),
        Op::HashGet => hashes::lower_hash_get(ctx, &inst),
        Op::HashSet => hashes::lower_hash_set(ctx, &inst),
        Op::IterStart => iterators::lower_iter_start(ctx, &inst),
        Op::IterNext => iterators::lower_iter_next(ctx, &inst),
        Op::IterCurrentKey => iterators::lower_iter_current_key(ctx, &inst),
        Op::IterCurrentValue => iterators::lower_iter_current_value(ctx, &inst),
        Op::IterEnd => iterators::lower_iter_end(ctx, &inst),
        Op::PtrCast => pointers::lower_ptr_cast(ctx, &inst),
        Op::BufferNew => buffers::lower_buffer_new(ctx, &inst),
        Op::BufferGet => buffers::lower_buffer_get(ctx, &inst),
        Op::BufferSet => buffers::lower_buffer_set(ctx, &inst),
        Op::Call => lower_direct_call(ctx, &inst),
        Op::ExternCall => externs::lower_extern_call(ctx, &inst),
        Op::BuiltinCall => builtins::lower_builtin_call(ctx, &inst),
        Op::Acquire => ownership::lower_acquire(ctx, &inst),
        Op::Release => ownership::lower_release(ctx, &inst),
        Op::Move | Op::Borrow => ownership::lower_forward(ctx, &inst),
        Op::EchoValue => lower_echo_value(ctx, &inst),
        Op::PrintValue => lower_print_value(ctx, &inst),
        Op::ThrowException => lower_throw_exception(ctx, &inst),
        Op::ErrorSuppressBegin => lower_runtime_void_call(ctx, "__rt_diag_push_suppression"),
        Op::ErrorSuppressEnd => lower_runtime_void_call(ctx, "__rt_diag_pop_suppression"),
        Op::IncludeOnceMark => lower_include_once_mark(ctx, &inst),
        Op::IncludeOnceGuard => lower_include_once_guard(ctx, &inst),
        Op::FunctionVariantDispatch => Ok(()),
        Op::FunctionVariantMark => lower_function_variant_mark(ctx, &inst),
        Op::Nop => Ok(()),
        _ => Err(CodegenIrError::unsupported(format!("opcode {}", inst.op.name()))),
    }
}

/// Lowers a concrete include-loaded function variant activation marker.
fn lower_function_variant_mark(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_data(inst)?;
    let label = ctx
        .module
        .data
        .strings
        .get(data.as_raw() as usize)
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))?;
    let parsed = function_variants::parse_variant_label(label)
        .ok_or_else(|| CodegenIrError::invalid_module(format!("invalid function variant label '{}'", label)))?;
    function_variants::emit_variant_mark(ctx.emitter, ctx.data, &parsed)
}

/// Lowers an include-once marker by setting its module-global guard symbol.
fn lower_include_once_mark(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let label = include_once_label(ctx, inst)?;
    ctx.data.add_comm(label.clone(), 8);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), &label, 0);
    Ok(())
}

/// Lowers an include-once guard to a boolean branch condition and marks first entry.
fn lower_include_once_guard(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let label = include_once_label(ctx, inst)?;
    ctx.data.add_comm(label.clone(), 8);
    let already_label = ctx.next_label("include_once_already");
    let done_label = ctx.next_label("include_once_done");
    let result_reg = abi::int_result_reg(ctx.emitter);
    abi::emit_load_symbol_to_reg(ctx.emitter, result_reg, &label, 0);
    abi::emit_branch_if_int_result_nonzero(ctx.emitter, &already_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 1);
    abi::emit_store_reg_to_symbol(ctx.emitter, result_reg, &label, 0);
    abi::emit_jump(ctx.emitter, &done_label);
    ctx.emitter.label(&already_label);
    abi::emit_load_int_immediate(ctx.emitter, result_reg, 0);
    ctx.emitter.label(&done_label);
    store_if_result(ctx, inst)
}

/// Returns the include-once guard symbol stored in the module data pool.
fn include_once_label(ctx: &FunctionContext<'_>, inst: &Instruction) -> Result<String> {
    let data = expect_data(inst)?;
    ctx.module
        .data
        .strings
        .get(data.as_raw() as usize)
        .cloned()
        .ok_or_else(|| CodegenIrError::missing_entry("data string", data.as_raw()))
}

/// Lowers a void EIR opcode that maps directly to one runtime helper call.
fn lower_runtime_void_call(ctx: &mut FunctionContext<'_>, label: &str) -> Result<()> {
    abi::emit_call_label(ctx.emitter, label);
    Ok(())
}

/// Lowers expression-form `throw` through the same runtime path as throw terminators.
fn lower_throw_exception(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    super::lower_term::lower_throw_value(ctx, value)
}

/// Lowers a direct call to a module-local user function.
fn lower_direct_call(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let function_name = ctx.function_name_data(expect_data(inst)?)?.to_string();
    let callee = ctx
        .callable_function_by_name(&function_name)
        .ok_or_else(|| CodegenIrError::unsupported(format!("call to unknown function {}", function_name)))?;
    if inst.operands.len() != callee.params.len() {
        return Err(CodegenIrError::unsupported(format!(
            "call to {} with {} args for {} params",
            function_name,
            inst.operands.len(),
            callee.params.len()
        )));
    }
    let overflow_bytes = materialize_direct_call_args(ctx, &inst.operands)?;
    let caller_stack_pad_bytes = direct_call_stack_pad_bytes(ctx, overflow_bytes);
    abi::emit_reserve_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_call_label(ctx.emitter, &function_symbol(&function_name));
    abi::emit_release_temporary_stack(ctx.emitter, caller_stack_pad_bytes);
    abi::emit_release_temporary_stack(ctx.emitter, overflow_bytes);
    if let Some(result) = inst.result {
        if ctx.value_php_type(result)? == PhpType::Void {
            abi::emit_load_int_immediate(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                0x7fff_ffff_ffff_fffe,
            );
        }
        ctx.store_result_value(result)?;
    }
    Ok(())
}

/// Loads SSA operands into ABI argument registers and caller-stack slots for a direct call.
fn materialize_direct_call_args(ctx: &mut FunctionContext<'_>, args: &[ValueId]) -> Result<usize> {
    let arg_types = args
        .iter()
        .map(|value| ctx.value_php_type(*value))
        .collect::<Result<Vec<_>>>()?;
    let assignments =
        abi::build_outgoing_arg_assignments_for_target(ctx.emitter.target, &arg_types, 0);
    for (value, ty) in args.iter().zip(arg_types.iter()) {
        ctx.load_value_to_result(*value)?;
        abi::emit_push_result_value(ctx.emitter, ty);
    }
    Ok(abi::materialize_outgoing_args(ctx.emitter, &assignments))
}

/// Returns the temporary caller-stack pad needed to match incoming stack-arg offsets.
fn direct_call_stack_pad_bytes(ctx: &FunctionContext<'_>, overflow_bytes: usize) -> usize {
    match ctx.emitter.target.arch {
        Arch::AArch64 if overflow_bytes > 0 => 16,
        _ => 0,
    }
}

/// Lowers a signed integer comparison into a boolean result value.
fn lower_int_compare(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let lhs = expect_operand(inst, 0)?;
    let rhs = expect_operand(inst, 1)?;
    let predicate = expect_cmp_predicate(inst)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let rhs_reg = abi::secondary_scratch_reg(ctx.emitter);
    require_integer_like(ctx.load_value_to_reg(lhs, result_reg)?, inst)?;
    require_integer_like(ctx.load_value_to_reg(rhs, rhs_reg)?, inst)?;
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, rhs_reg)); // compare signed integer operands for the EIR predicate
            ctx.emitter.instruction(&format!("cset {}, {}", result_reg, aarch64_condition(predicate)?)); // materialize the predicate result as 0 or 1
        }
        Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, rhs_reg)); // compare signed integer operands for the EIR predicate
            ctx.emitter.instruction(&format!("set{} al", x86_64_condition(predicate)?)); // materialize the predicate result in the low byte
            ctx.emitter.instruction(&format!("movzx {}, al", result_reg));      // widen the predicate byte into the integer result register
        }
    }
    store_if_result(ctx, inst)
}

/// Lowers an addressable local load into the result register and SSA destination slot.
fn lower_load_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    ctx.load_local_to_result(slot)?;
    store_if_result(ctx, inst)
}

/// Lowers an addressable local store from one SSA operand.
fn lower_store_local(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let slot = expect_local_slot(inst)?;
    let value = expect_operand(inst, 0)?;
    ctx.store_value_to_local(slot, value)
}

/// Lowers a global storage load into the result register and SSA destination slot.
fn lower_load_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let symbol = ir_global_symbol(name);
    let result = inst.result.ok_or_else(|| {
        CodegenIrError::invalid_module("load_global missing result value")
    })?;
    let ty = ctx.value_php_type(result)?;
    ctx.data.add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
    abi::emit_load_symbol_to_result(ctx.emitter, &symbol, &ty);
    store_if_result(ctx, inst)
}

/// Lowers a global storage store from one SSA operand.
fn lower_store_global(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let data = expect_global_name(inst)?;
    let name = ctx.global_name_data(data)?;
    let symbol = ir_global_symbol(name);
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    ctx.data.add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
    abi::emit_store_result_to_symbol(ctx.emitter, &symbol, &ty, false);
    Ok(())
}

/// Lowers an integer constant into the canonical integer result register and slot.
fn lower_const_i64(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_i64(inst)?;
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a boolean constant into the canonical integer result register and slot.
fn lower_const_bool(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = i64::from(expect_bool(inst)?);
    abi::emit_load_int_immediate(ctx.emitter, abi::int_result_reg(ctx.emitter), value);
    store_if_result(ctx, inst)
}

/// Lowers a null constant to the runtime null sentinel and stores it in the result slot.
fn lower_const_null(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    abi::emit_load_int_immediate(
        ctx.emitter,
        abi::int_result_reg(ctx.emitter),
        0x7fff_ffff_ffff_fffe,
    );
    store_if_result(ctx, inst)
}

/// Lowers PHP echo output for a previously computed SSA value.
fn lower_echo_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    let value = expect_operand(inst, 0)?;
    let ty = ctx.load_value_to_result(value)?;
    emit_loaded_value_to_stdout(ctx, &ty)
}

/// Lowers PHP `print` output for a previously computed SSA value.
fn lower_print_value(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    lower_echo_value(ctx, inst)
}

/// Emits stdout output for the value currently loaded into result register(s).
fn emit_loaded_value_to_stdout(ctx: &mut FunctionContext<'_>, ty: &PhpType) -> Result<()> {
    ctx.emitter.blank();
    ctx.emitter.comment("echo");
    match ty {
        PhpType::Void | PhpType::Never => Ok(()),
        PhpType::Bool => {
            let skip_label = ctx.next_label("echo_skip_false");
            abi::emit_branch_if_int_result_zero(ctx.emitter, &skip_label);
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Int => {
            let skip_label = ctx.next_label("echo_skip_null");
            let sentinel_reg = abi::symbol_scratch_reg(ctx.emitter);
            abi::emit_load_int_immediate(ctx.emitter, sentinel_reg, 0x7fff_ffff_ffff_fffe);
            match ctx.emitter.target.arch {
                Arch::AArch64 => {
                    ctx.emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(ctx.emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    ctx.emitter.instruction(&format!("b.eq {}", skip_label));   // skip integer echo when the value represents null
                }
                Arch::X86_64 => {
                    ctx.emitter.instruction(&format!("cmp {}, {}", abi::int_result_reg(ctx.emitter), sentinel_reg)); // compare integer value against the runtime null sentinel
                    ctx.emitter.instruction(&format!("je {}", skip_label));     // skip integer echo when the value represents null
                }
            }
            abi::emit_write_stdout(ctx.emitter, ty);
            ctx.emitter.label(&skip_label);
            Ok(())
        }
        PhpType::Float
        | PhpType::Str
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Iterable
        | PhpType::Pointer(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. } => {
            abi::emit_write_stdout(ctx.emitter, ty);
            Ok(())
        }
        _ => Err(CodegenIrError::unsupported(format!("echo for PHP type {:?}", ty))),
    }
}

/// Returns the AArch64 condition-code suffix for an EIR comparison predicate.
fn aarch64_condition(predicate: CmpPredicate) -> Result<&'static str> {
    match predicate {
        CmpPredicate::Eq => Ok("eq"),
        CmpPredicate::Ne => Ok("ne"),
        CmpPredicate::Slt => Ok("lt"),
        CmpPredicate::Sle => Ok("le"),
        CmpPredicate::Sgt => Ok("gt"),
        CmpPredicate::Sge => Ok("ge"),
        other => Err(CodegenIrError::unsupported(format!(
            "integer comparison predicate {:?}",
            other
        ))),
    }
}

/// Returns the x86_64 setcc suffix for an EIR comparison predicate.
fn x86_64_condition(predicate: CmpPredicate) -> Result<&'static str> {
    match predicate {
        CmpPredicate::Eq => Ok("e"),
        CmpPredicate::Ne => Ok("ne"),
        CmpPredicate::Slt => Ok("l"),
        CmpPredicate::Sle => Ok("le"),
        CmpPredicate::Sgt => Ok("g"),
        CmpPredicate::Sge => Ok("ge"),
        other => Err(CodegenIrError::unsupported(format!(
            "integer comparison predicate {:?}",
            other
        ))),
    }
}

/// Returns the x86_64 floating-point setcc suffix for an EIR comparison predicate.
fn x86_64_float_condition(predicate: CmpPredicate) -> Result<&'static str> {
    match predicate {
        CmpPredicate::Eq => Ok("e"),
        CmpPredicate::Ne => Ok("ne"),
        CmpPredicate::Slt | CmpPredicate::Olt => Ok("b"),
        CmpPredicate::Sle | CmpPredicate::Ole => Ok("be"),
        CmpPredicate::Sgt | CmpPredicate::Ogt => Ok("a"),
        CmpPredicate::Sge | CmpPredicate::Oge => Ok("ae"),
    }
}

/// Returns the secondary floating-point scratch register for the target.
fn secondary_float_reg(arch: Arch) -> &'static str {
    match arch {
        Arch::AArch64 => "d1",
        Arch::X86_64 => "xmm1",
    }
}

/// Verifies that an arithmetic operand has a single-register integer-like representation.
fn require_integer_like(ty: PhpType, inst: &Instruction) -> Result<()> {
    if matches!(ty, PhpType::Int | PhpType::Bool) {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Verifies that an operand has the floating-point representation expected by the opcode.
fn require_float(ty: PhpType, inst: &Instruction) -> Result<()> {
    if ty == PhpType::Float {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Verifies that an operand has the string-pair representation expected by the opcode.
fn require_string(ty: PhpType, inst: &Instruction) -> Result<()> {
    if ty == PhpType::Str {
        return Ok(());
    }
    Err(CodegenIrError::unsupported(format!(
        "{} for PHP type {:?}",
        inst.op.name(),
        ty
    )))
}

/// Stores the current result registers when an instruction has an SSA result.
fn store_if_result(ctx: &mut FunctionContext<'_>, inst: &Instruction) -> Result<()> {
    if let Some(result) = inst.result {
        ctx.store_result_value(result)?;
    }
    Ok(())
}

/// Returns the integer immediate attached to a constant instruction.
fn expect_i64(inst: &Instruction) -> Result<i64> {
    match inst.immediate {
        Some(Immediate::I64(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing i64 immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the floating-point immediate attached to a constant instruction.
fn expect_f64(inst: &Instruction) -> Result<f64> {
    match inst.immediate {
        Some(Immediate::F64(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing f64 immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the boolean immediate attached to a constant instruction.
fn expect_bool(inst: &Instruction) -> Result<bool> {
    match inst.immediate {
        Some(Immediate::Bool(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing bool immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the data-pool immediate attached to a data-backed instruction.
fn expect_data(inst: &Instruction) -> Result<crate::ir::DataId> {
    match inst.immediate {
        Some(Immediate::Data(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing data immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the comparison predicate attached to a compare instruction.
fn expect_cmp_predicate(inst: &Instruction) -> Result<CmpPredicate> {
    match inst.immediate {
        Some(Immediate::CmpPredicate(predicate)) => Ok(predicate),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing comparison predicate immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the local-slot immediate attached to a local access instruction.
fn expect_local_slot(inst: &Instruction) -> Result<LocalSlotId> {
    match inst.immediate {
        Some(Immediate::LocalSlot(slot)) => Ok(slot),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing local slot immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the global-name immediate attached to a global access instruction.
fn expect_global_name(inst: &Instruction) -> Result<crate::ir::DataId> {
    match inst.immediate {
        Some(Immediate::GlobalName(value)) => Ok(value),
        _ => Err(CodegenIrError::invalid_module(format!(
            "{} missing global-name immediate",
            inst.op.name()
        ))),
    }
}

/// Returns the operand at `index` or reports a malformed instruction.
fn expect_operand(inst: &Instruction, index: usize) -> Result<ValueId> {
    inst.operands.get(index).copied().ok_or_else(|| {
        CodegenIrError::invalid_module(format!(
            "{} missing operand {}",
            inst.op.name(),
            index
        ))
    })
}
