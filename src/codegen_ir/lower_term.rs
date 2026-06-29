//! Purpose:
//! Lowers EIR block terminators into jumps, returns, exits, and fatal termination paths.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`.
//!
//! Key details:
//! - Fatal terminators write their data-pool diagnostic to stderr and exit.
//! - Throw terminators publish `_exc_value` and enter the shared exception unwinder.
//! - Unreachable terminators emit target-native trap instructions.
//! - Generator suspension remains an explicit unsupported Phase 04 path.

use crate::codegen::platform::Arch;
use crate::ir::{BlockId, DataId, IrType, SwitchCase, Terminator, ValueId};
use crate::types::PhpType;

use crate::codegen::abi;

use super::context::FunctionContext;
use super::frame;
use super::{CodegenIrError, Result};

/// Lowers one EIR terminator.
pub(super) fn lower_terminator(ctx: &mut FunctionContext<'_>, term: &Terminator) -> Result<()> {
    match term {
        Terminator::Return { value: None } => {
            if ctx.is_main {
                if ctx.web {
                    frame::emit_web_handler_epilogue(ctx);
                } else {
                    frame::emit_main_epilogue(ctx);
                }
            } else {
                jump_to_function_epilogue(ctx)?;
            }
            Ok(())
        }
        Terminator::Return { value: Some(value) } => {
            if ctx.function.flags.by_ref_return {
                // A by-reference-returning function hands back the reference-cell pointer
                // (`$x = &f()` aliases it). The pointer is a single machine word regardless of
                // the aliased element type, so place it in the integer result register rather
                // than splitting a `Str`/`Float` declared return across the string/float regs.
                let int_reg = abi::int_result_reg(ctx.emitter);
                ctx.load_value_to_reg(*value, int_reg)?;
                jump_to_function_epilogue(ctx)?;
                return Ok(());
            }
            let source_ty = ctx.load_value_to_result(*value)?;
            if ctx.is_main {
                // The top-level script's return value is discarded (PHP only uses
                // a top-level `return <expr>;` as an include's return value); the
                // expression's side effects already ran, so just run the entry
                // epilogue exactly like a bare `return;`.
                if ctx.web {
                    frame::emit_web_handler_epilogue(ctx);
                } else {
                    frame::emit_main_epilogue(ctx);
                }
                return Ok(());
            }
            if ctx.function.return_php_type.codegen_repr() == PhpType::TaggedScalar {
                super::lower_inst::coerce_loaded_value_to_tagged_scalar(ctx, &source_ty)?;
            }
            jump_to_function_epilogue(ctx)?;
            Ok(())
        }
        Terminator::Unreachable => {
            lower_unreachable(ctx);
            Ok(())
        }
        Terminator::Br { target, args } => {
            lower_branch(ctx, *target, args, "br")?;
            Ok(())
        }
        Terminator::CondBr {
            cond,
            then_target,
            then_args,
            else_target,
            else_args,
        } => {
            ctx.load_value_to_result(*cond)?;
            validate_block_args(ctx, *then_target, then_args, "cond_br then")?;
            validate_block_args(ctx, *else_target, else_args, "cond_br else")?;
            let then_label = ctx.block_label_for_id(*then_target)?;
            let else_label = ctx.block_label_for_id(*else_target)?;
            let then_edge = edge_label(ctx, then_args, &then_label, "cond_then_args");
            let else_edge = edge_label(ctx, else_args, &else_label, "cond_else_args");
            abi::emit_branch_if_int_result_nonzero(ctx.emitter, &then_edge);
            abi::emit_jump(ctx.emitter, &else_edge);
            emit_edge_args(ctx, &then_edge, *then_target, then_args, "cond_br then")?;
            emit_edge_args(ctx, &else_edge, *else_target, else_args, "cond_br else")?;
            Ok(())
        }
        Terminator::Switch {
            scrutinee,
            cases,
            default,
            default_args,
        } => lower_switch(ctx, *scrutinee, cases, *default, default_args),
        Terminator::Throw { value } => lower_throw_value(ctx, *value),
        Terminator::Fatal { message } => lower_fatal(ctx, *message),
        Terminator::GeneratorSuspend { .. } => {
            Err(CodegenIrError::unsupported("generator_suspend terminator"))
        }
    }
}

/// Lowers a throw value by publishing it to the runtime exception slot and unwinding.
pub(super) fn lower_throw_value(ctx: &mut FunctionContext<'_>, value: ValueId) -> Result<()> {
    let ty = ctx.load_value_to_result(value)?;
    if !matches!(ty.codegen_repr(), PhpType::Object(_)) {
        return Err(CodegenIrError::unsupported(format!(
            "throw for PHP type {:?}",
            ty
        )));
    }
    abi::emit_store_reg_to_symbol(ctx.emitter, abi::int_result_reg(ctx.emitter), "_exc_value", 0);
    abi::emit_call_label(ctx.emitter, "__rt_throw_current");
    Ok(())
}

/// Lowers an unconditional branch and copies any target block parameters.
fn lower_branch(
    ctx: &mut FunctionContext<'_>,
    target: BlockId,
    args: &[ValueId],
    context: &str,
) -> Result<()> {
    materialize_block_args(ctx, target, args, context)?;
    let label = ctx.block_label_for_id(target)?;
    abi::emit_jump(ctx.emitter, &label);
    Ok(())
}

/// Emits a target-native trap for a block that should never execute.
fn lower_unreachable(ctx: &mut FunctionContext<'_>) {
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("udf #0");                                  // trap if an unreachable EIR block is entered
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("ud2");                                     // trap if an unreachable EIR block is entered
        }
    }
}

/// Lowers an unrecoverable fatal diagnostic and process exit.
fn lower_fatal(ctx: &mut FunctionContext<'_>, message: DataId) -> Result<()> {
    let (message_label, message_len) = ctx.intern_string_data(message)?;
    ctx.emitter.blank();
    ctx.emitter.comment("fatal");
    match ctx.emitter.target.arch {
        Arch::AArch64 => {
            ctx.emitter.instruction("mov x0, #2");                              // fd = stderr for the EIR fatal diagnostic
            ctx.emitter.adrp("x1", &message_label);
            ctx.emitter.add_lo12("x1", "x1", &message_label);
            ctx.emitter.instruction(&format!("mov x2, #{}", message_len));      // pass the EIR fatal diagnostic byte length to write
            ctx.emitter.syscall(4);
        }
        Arch::X86_64 => {
            ctx.emitter.instruction("mov edi, 2");                              // fd = stderr for the EIR fatal diagnostic
            abi::emit_symbol_address(ctx.emitter, "rsi", &message_label);
            ctx.emitter.instruction(&format!("mov edx, {}", message_len));      // pass the EIR fatal diagnostic byte length to write
            ctx.emitter.instruction("mov eax, 1");                              // Linux x86_64 syscall 1 = write
            ctx.emitter.instruction("syscall");                                 // emit the EIR fatal diagnostic before exiting
        }
    }
    abi::emit_exit(ctx.emitter, 1);
    Ok(())
}

/// Lowers an integer switch by comparing the scrutinee against each case value in source order.
fn lower_switch(
    ctx: &mut FunctionContext<'_>,
    scrutinee: ValueId,
    cases: &[SwitchCase],
    default: BlockId,
    default_args: &[ValueId],
) -> Result<()> {
    validate_block_args(ctx, default, default_args, "switch default")?;
    for case in cases {
        validate_block_args(ctx, case.target, &case.args, "switch case")?;
    }
    ctx.load_value_to_result(scrutinee)?;
    let result_reg = abi::int_result_reg(ctx.emitter);
    let case_reg = abi::secondary_scratch_reg(ctx.emitter);
    let mut case_edges = Vec::new();
    for case in cases {
        let target_label = ctx.block_label_for_id(case.target)?;
        let branch_label = edge_label(ctx, &case.args, &target_label, "switch_case_args");
        abi::emit_load_int_immediate(ctx.emitter, case_reg, case.value);
        match ctx.emitter.target.arch {
            Arch::AArch64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, case_reg)); // compare switch scrutinee with the case value
                ctx.emitter.instruction(&format!("b.eq {}", branch_label));     // branch to the matching switch case
            }
            Arch::X86_64 => {
                ctx.emitter.instruction(&format!("cmp {}, {}", result_reg, case_reg)); // compare switch scrutinee with the case value
                ctx.emitter.instruction(&format!("je {}", branch_label));       // branch to the matching switch case
            }
        }
        if !case.args.is_empty() {
            case_edges.push((branch_label, case.target, case.args.clone()));
        }
    }
    let default_label = ctx.block_label_for_id(default)?;
    let default_edge = edge_label(ctx, default_args, &default_label, "switch_default_args");
    abi::emit_jump(ctx.emitter, &default_edge);
    emit_edge_args(ctx, &default_edge, default, default_args, "switch default")?;
    for (label, target, args) in case_edges {
        emit_edge_args(ctx, &label, target, &args, "switch case")?;
    }
    Ok(())
}

/// Emits a jump to the current user function's shared epilogue.
fn jump_to_function_epilogue(ctx: &mut FunctionContext<'_>) -> Result<()> {
    let Some(label) = ctx.epilogue_label.clone() else {
        return Err(CodegenIrError::unsupported(
            "return values on the EIR backend entry function",
        ));
    };
    abi::emit_jump(ctx.emitter, &label);
    Ok(())
}

/// Returns the target label directly when an edge has no arguments, otherwise a copy-stub label.
fn edge_label(
    ctx: &mut FunctionContext<'_>,
    args: &[ValueId],
    target_label: &str,
    prefix: &str,
) -> String {
    if args.is_empty() {
        return target_label.to_string();
    }
    ctx.next_label(prefix)
}

/// Emits an edge copy stub for branch arguments and jumps to the real target block.
fn emit_edge_args(
    ctx: &mut FunctionContext<'_>,
    label: &str,
    target: BlockId,
    args: &[ValueId],
    context: &str,
) -> Result<()> {
    if args.is_empty() {
        return Ok(());
    }
    ctx.emitter.label(label);
    materialize_block_args(ctx, target, args, context)?;
    let target_label = ctx.block_label_for_id(target)?;
    abi::emit_jump(ctx.emitter, &target_label);
    Ok(())
}

/// Copies branch arguments into the target block parameter slots using parallel-move semantics.
fn materialize_block_args(
    ctx: &mut FunctionContext<'_>,
    target: BlockId,
    args: &[ValueId],
    context: &str,
) -> Result<()> {
    let params = validate_block_args(ctx, target, args, context)?;
    if args.is_empty() {
        return Ok(());
    }
    let mut arg_types = Vec::with_capacity(args.len());
    for arg in args {
        let ty = ctx.load_value_to_result(*arg)?;
        abi::emit_push_result_value(ctx.emitter, &ty);
        arg_types.push(ty);
    }
    for (param, ty) in params.iter().zip(arg_types.iter()).rev() {
        pop_result_value(ctx, ty);
        ctx.store_result_value(*param)?;
    }
    Ok(())
}

/// Validates that an edge supplies one storage-compatible value for each target block parameter.
fn validate_block_args(
    ctx: &FunctionContext<'_>,
    target: BlockId,
    args: &[ValueId],
    context: &str,
) -> Result<Vec<ValueId>> {
    let block = ctx
        .function
        .block(target)
        .ok_or_else(|| CodegenIrError::missing_entry("block", target.as_raw()))?;
    if block.params.len() != args.len() {
        return Err(CodegenIrError::invalid_module(format!(
            "{} supplies {} block arguments for target '{}' with {} parameters",
            context,
            args.len(),
            block.name,
            block.params.len()
        )));
    }
    for (index, (param, arg)) in block.params.iter().zip(args.iter()).enumerate() {
        let expected = value_ir_type(ctx, *param)?;
        let actual = value_ir_type(ctx, *arg)?;
        if expected != actual {
            return Err(CodegenIrError::invalid_module(format!(
                "{} argument {} has EIR type {:?}, expected {:?}",
                context, index, actual, expected
            )));
        }
    }
    Ok(block.params.clone())
}

/// Returns one value's EIR storage type or a structured backend error.
fn value_ir_type(ctx: &FunctionContext<'_>, value: ValueId) -> Result<IrType> {
    ctx.function
        .value(value)
        .map(|metadata| metadata.ir_type)
        .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))
}

/// Restores one temporary stack value into the canonical result register(s).
fn pop_result_value(ctx: &mut FunctionContext<'_>, ty: &PhpType) {
    match ty.codegen_repr() {
        PhpType::Bool
        | PhpType::Int
        | PhpType::Resource(_)
        | PhpType::Iterable
        | PhpType::Mixed
        | PhpType::Union(_)
        | PhpType::Array(_)
        | PhpType::AssocArray { .. }
        | PhpType::Buffer(_)
        | PhpType::Callable
        | PhpType::Object(_)
        | PhpType::Packed(_)
        | PhpType::Pointer(_) => {
            abi::emit_pop_reg(ctx.emitter, abi::int_result_reg(ctx.emitter));
        }
        PhpType::Float => {
            abi::emit_pop_float_reg(ctx.emitter, abi::float_result_reg(ctx.emitter));
        }
        PhpType::Str => {
            let (ptr_reg, len_reg) = abi::string_result_regs(ctx.emitter);
            abi::emit_pop_reg_pair(ctx.emitter, ptr_reg, len_reg);
        }
        PhpType::TaggedScalar => {
            abi::emit_pop_reg_pair(
                ctx.emitter,
                abi::int_result_reg(ctx.emitter),
                crate::codegen::sentinels::tagged_scalar_tag_reg(ctx.emitter),
            );
        }
        PhpType::Void | PhpType::Never => {}
    }
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for EIR terminator assembly lowering.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The tests construct tiny EIR modules directly so terminator opcodes can be isolated.

    use crate::codegen::platform::{Arch, Platform, Target};
    use crate::codegen_ir::generate_user_asm_from_ir;
    use crate::ir::{
        Builder, Function, IrHeapKind, IrType, Module, Op, Ownership, SwitchCase, Terminator,
    };
    use crate::types::PhpType;

    /// Verifies ARM64 unreachable terminators lower to the Phase 04 trap opcode.
    #[test]
    fn unreachable_terminator_emits_aarch64_trap() {
        let asm = generate_unreachable_main_asm(Target::new(Platform::Linux, Arch::AArch64));

        assert!(asm.contains("udf #0"), "{asm}");
    }

    /// Verifies x86_64 unreachable terminators lower to the Phase 04 trap opcode.
    #[test]
    fn unreachable_terminator_emits_x86_64_trap() {
        let asm = generate_unreachable_main_asm(Target::new(Platform::Linux, Arch::X86_64));

        assert!(asm.contains("ud2"), "{asm}");
    }

    /// Verifies unconditional branch arguments are copied into target block parameter slots.
    #[test]
    fn br_arguments_are_copied_to_target_params() {
        let asm = generate_branch_arg_main_asm(Target::new(Platform::Linux, Arch::AArch64));

        assert!(asm.contains("ldur x0, [x29, #-16]"), "{asm}");
        assert!(asm.contains("stur x0, [x29, #-8]"), "{asm}");
    }

    /// Verifies conditional branch arguments use per-edge copy stubs.
    #[test]
    fn cond_br_arguments_emit_edge_copy_stubs() {
        let asm = generate_cond_branch_arg_main_asm(Target::new(Platform::Linux, Arch::X86_64));

        assert!(asm.contains("_eir_main_cond_then_args_0:"), "{asm}");
        assert!(asm.contains("_eir_main_cond_else_args_1:"), "{asm}");
    }

    /// Verifies switch case and default arguments use per-edge copy stubs.
    #[test]
    fn switch_arguments_emit_edge_copy_stubs() {
        let asm = generate_switch_arg_main_asm(Target::new(Platform::Linux, Arch::AArch64));

        assert!(asm.contains("_eir_main_switch_case_args_0:"), "{asm}");
        assert!(asm.contains("_eir_main_switch_default_args_1:"), "{asm}");
    }

    /// Verifies throw terminators publish `_exc_value` and call the exception unwinder.
    #[test]
    fn throw_terminator_enters_runtime_unwinder() {
        let asm = generate_throw_terminator_main_asm(Target::new(Platform::Linux, Arch::X86_64));

        assert!(asm.contains("mov QWORD PTR [rip + _exc_value], rax"), "{asm}");
        assert!(asm.contains("call __rt_throw_current"), "{asm}");
    }

    /// Verifies expression-form throw opcodes share the throw terminator runtime path.
    #[test]
    fn throw_exception_opcode_enters_runtime_unwinder() {
        let asm = generate_throw_exception_opcode_main_asm(Target::new(Platform::Linux, Arch::AArch64));

        assert!(asm.contains("_exc_value"), "{asm}");
        assert!(asm.contains("bl __rt_throw_current"), "{asm}");
    }

    /// Builds a minimal EIR main function ending in `Unreachable` and returns its ASM.
    fn generate_unreachable_main_asm(target: Target) -> String {
        let mut module = Module::new(target);
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            builder.set_entry(entry);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        generate_user_asm_from_ir(&module, false, false).expect("unreachable module should lower")
    }

    /// Builds a minimal `br` fixture with one integer block argument.
    fn generate_branch_arg_main_asm(target: Target) -> String {
        let mut module = Module::new(target);
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let body = builder.create_named_block("body", vec![(IrType::I64, PhpType::Int)]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let value = builder.emit_const_i64(7);
            builder.terminate(Terminator::Br {
                target: body,
                args: vec![value],
            });
            builder.position_at_end(body);
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        generate_user_asm_from_ir(&module, false, false).expect("branch-arg module should lower")
    }

    /// Builds a minimal `cond_br` fixture with one integer argument on each edge.
    fn generate_cond_branch_arg_main_asm(target: Target) -> String {
        let mut module = Module::new(target);
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let then_block = builder.create_named_block("then", vec![(IrType::I64, PhpType::Int)]);
            let else_block = builder.create_named_block("else", vec![(IrType::I64, PhpType::Int)]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let cond = builder.emit_const_bool(true);
            let then_value = builder.emit_const_i64(7);
            let else_value = builder.emit_const_i64(8);
            builder.terminate(Terminator::CondBr {
                cond,
                then_target: then_block,
                then_args: vec![then_value],
                else_target: else_block,
                else_args: vec![else_value],
            });
            builder.position_at_end(then_block);
            builder.terminate(Terminator::Unreachable);
            builder.position_at_end(else_block);
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        generate_user_asm_from_ir(&module, false, false).expect("cond-branch-arg module should lower")
    }

    /// Builds a minimal `switch` fixture with case/default block arguments.
    fn generate_switch_arg_main_asm(target: Target) -> String {
        let mut module = Module::new(target);
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", Vec::new());
            let case_block = builder.create_named_block("case", vec![(IrType::I64, PhpType::Int)]);
            let default_block = builder.create_named_block("default", vec![(IrType::I64, PhpType::Int)]);
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let scrutinee = builder.emit_const_i64(1);
            let case_value = builder.emit_const_i64(7);
            let default_value = builder.emit_const_i64(8);
            builder.terminate(Terminator::Switch {
                scrutinee,
                cases: vec![SwitchCase {
                    value: 1,
                    target: case_block,
                    args: vec![case_value],
                }],
                default: default_block,
                default_args: vec![default_value],
            });
            builder.position_at_end(case_block);
            builder.terminate(Terminator::Unreachable);
            builder.position_at_end(default_block);
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        generate_user_asm_from_ir(&module, false, false).expect("switch-arg module should lower")
    }

    /// Builds a minimal throw-terminator fixture with an object block parameter.
    fn generate_throw_terminator_main_asm(target: Target) -> String {
        let mut module = Module::new(target);
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block(
                "entry",
                vec![(
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Exception".to_string()),
                )],
            );
            builder.set_entry(entry);
            let thrown = builder.block_param(entry, 0);
            builder.position_at_end(entry);
            builder.terminate(Terminator::Throw { value: thrown });
        }
        module.add_function(function);

        generate_user_asm_from_ir(&module, false, false).expect("throw terminator module should lower")
    }

    /// Builds a minimal throw-exception-opcode fixture with an object block parameter.
    fn generate_throw_exception_opcode_main_asm(target: Target) -> String {
        let mut module = Module::new(target);
        let mut function = Function::new("main".to_string(), IrType::Void, PhpType::Void);
        function.flags.is_main = true;
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block(
                "entry",
                vec![(
                    IrType::Heap(IrHeapKind::Object),
                    PhpType::Object("Exception".to_string()),
                )],
            );
            builder.set_entry(entry);
            let thrown = builder.block_param(entry, 0);
            builder.position_at_end(entry);
            let _ = builder.emit(
                Op::ThrowException,
                vec![thrown],
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
            );
            builder.terminate(Terminator::Unreachable);
        }
        module.add_function(function);

        generate_user_asm_from_ir(&module, false, false).expect("throw opcode module should lower")
    }
}
