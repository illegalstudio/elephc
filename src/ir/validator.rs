//! Purpose:
//! Validates EIR modules and functions for structural, type, dominance,
//! ownership, and effect invariants.
//!
//! Called from:
//! - Phase 02 tests, future `--emit-ir`, and future IR pass/codegen gates.
//!
//! Key details:
//! - The validator is conservative: uncertain or malformed IR fails early so
//!   later passes can assume table IDs, branch args, and value types are sound.

use std::collections::{HashMap, HashSet};

use crate::ir::block::{BlockId, SwitchCase, Terminator};
use crate::ir::effects::Effects;
use crate::ir::function::Function;
use crate::ir::instr::{Immediate, InstId, Instruction, Op};
use crate::ir::module::Module;
use crate::ir::types::{IrHeapKind, IrType};
use crate::ir::value::{Ownership, ValueDef, ValueId};
use crate::types::PhpType;

/// Validation error reported for malformed EIR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationError {
    NoBlocks,
    NoEntryBlock,
    EntryBlockHasParams(BlockId),
    BlockIdMismatch { expected: BlockId, actual: BlockId },
    BlockMissingTerminator(BlockId),
    UnknownBlock(BlockId),
    UnknownInstruction(InstId),
    UnknownValue(ValueId),
    DuplicateInstructionInBlocks(InstId),
    ValueDefMismatch(ValueId),
    VoidValueUsed(ValueId),
    ResultTypeMismatch(ValueId),
    PhpTypeMismatch(ValueId),
    OwnershipTypeMismatch(ValueId),
    InstructionResultMissing(InstId),
    InstructionUnexpectedResult(InstId),
    OperandCountMismatch {
        inst: InstId,
        expected: &'static str,
        actual: usize,
    },
    OperandTypeMismatch {
        inst: InstId,
        operand: ValueId,
        expected: &'static str,
        actual: IrType,
    },
    MissingImmediate {
        inst: InstId,
        expected: &'static str,
    },
    UnexpectedImmediate(InstId),
    EffectMismatch {
        inst: InstId,
        expected: Effects,
        actual: Effects,
    },
    UseNotDominated {
        value: ValueId,
        used_in: BlockId,
    },
    BranchArgCountMismatch {
        target: BlockId,
        expected: usize,
        actual: usize,
    },
    BranchArgTypeMismatch {
        target: BlockId,
        index: usize,
        expected: IrType,
        actual: IrType,
    },
    ConditionTypeMismatch {
        value: ValueId,
        actual: IrType,
    },
    SwitchScrutineeTypeMismatch {
        value: ValueId,
        actual: IrType,
    },
    ReturnTypeMismatch {
        expected: IrType,
        actual: Option<IrType>,
    },
    NeverFunctionReturns,
}

/// Validates every function stored in a module.
pub fn validate_module(module: &Module) -> Result<(), ValidationError> {
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        validate_function(function)?;
    }
    Ok(())
}

/// Validates one function and all of its function-local tables.
pub fn validate_function(function: &Function) -> Result<(), ValidationError> {
    validate_function_shape(function)?;
    validate_value_table(function)?;
    validate_instruction_placement(function)?;
    let dominators = compute_dominators(function);
    validate_instructions(function, &dominators)?;
    validate_terminators(function, &dominators)?;
    Ok(())
}

/// Validates block existence, IDs, entry placement, and terminators.
fn validate_function_shape(function: &Function) -> Result<(), ValidationError> {
    if function.blocks.is_empty() {
        return Err(ValidationError::NoBlocks);
    }
    if function.block(function.entry).is_none() {
        return Err(ValidationError::NoEntryBlock);
    }
    if !function.blocks[function.entry.as_raw() as usize].params.is_empty() {
        return Err(ValidationError::EntryBlockHasParams(function.entry));
    }
    for (index, block) in function.blocks.iter().enumerate() {
        let expected = BlockId::from_raw(index as u32);
        if block.id != expected {
            return Err(ValidationError::BlockIdMismatch {
                expected,
                actual: block.id,
            });
        }
        if block.terminator.is_none() {
            return Err(ValidationError::BlockMissingTerminator(block.id));
        }
    }
    Ok(())
}

/// Validates value IDs, definition metadata, PHP type compatibility, and ownership shape.
fn validate_value_table(function: &Function) -> Result<(), ValidationError> {
    for (index, value) in function.values.iter().enumerate() {
        let value_id = ValueId::from_raw(index as u32);
        match value.def {
            ValueDef::BlockParam { block, index } => {
                let Some(block_ref) = function.block(block) else {
                    return Err(ValidationError::UnknownBlock(block));
                };
                if block_ref.params.get(index as usize) != Some(&value_id) {
                    return Err(ValidationError::ValueDefMismatch(value_id));
                }
            }
            ValueDef::Instruction { block, index, inst } => {
                let Some(block_ref) = function.block(block) else {
                    return Err(ValidationError::UnknownBlock(block));
                };
                if block_ref.instructions.get(index as usize) != Some(&inst) {
                    return Err(ValidationError::ValueDefMismatch(value_id));
                }
                let Some(inst_ref) = function.instruction(inst) else {
                    return Err(ValidationError::UnknownInstruction(inst));
                };
                if inst_ref.result != Some(value_id) {
                    return Err(ValidationError::ValueDefMismatch(value_id));
                }
            }
        }
        if !php_type_compatible(value.ir_type, &value.php_type) {
            return Err(ValidationError::PhpTypeMismatch(value_id));
        }
        if !ownership_compatible(value.ir_type, &value.php_type, value.ownership) {
            return Err(ValidationError::OwnershipTypeMismatch(value_id));
        }
    }
    Ok(())
}

/// Validates instruction IDs are placed once and point into the instruction table.
fn validate_instruction_placement(function: &Function) -> Result<(), ValidationError> {
    let mut seen = HashSet::new();
    for block in &function.blocks {
        for inst_id in &block.instructions {
            if function.instruction(*inst_id).is_none() {
                return Err(ValidationError::UnknownInstruction(*inst_id));
            }
            if !seen.insert(*inst_id) {
                return Err(ValidationError::DuplicateInstructionInBlocks(*inst_id));
            }
        }
    }
    Ok(())
}

/// Validates instruction operands, result metadata, immediates, effects, and dominance.
fn validate_instructions(
    function: &Function,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> Result<(), ValidationError> {
    for block in &function.blocks {
        for (index, inst_id) in block.instructions.iter().enumerate() {
            let inst = function
                .instruction(*inst_id)
                .ok_or(ValidationError::UnknownInstruction(*inst_id))?;
            validate_instruction_result(function, block.id, index as u32, *inst_id, inst)?;
            validate_instruction_effects(*inst_id, inst)?;
            validate_instruction_immediate(*inst_id, inst)?;
            validate_instruction_operands(function, block.id, index as u32, *inst_id, inst, dominators)?;
            validate_opcode_rules(function, *inst_id, inst)?;
        }
    }
    Ok(())
}

/// Validates instruction result shape and result value metadata.
fn validate_instruction_result(
    function: &Function,
    block: BlockId,
    index: u32,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    match (inst.result_type.is_void(), inst.result) {
        (true, Some(_)) => Err(ValidationError::InstructionUnexpectedResult(inst_id)),
        (false, None) => Err(ValidationError::InstructionResultMissing(inst_id)),
        (true, None) => Ok(()),
        (false, Some(value_id)) => {
            let value = function
                .value(value_id)
                .ok_or(ValidationError::UnknownValue(value_id))?;
            if value.ir_type != inst.result_type {
                return Err(ValidationError::ResultTypeMismatch(value_id));
            }
            if value.php_type != inst.result_php_type {
                return Err(ValidationError::PhpTypeMismatch(value_id));
            }
            if value.ownership != inst.result_ownership {
                return Err(ValidationError::OwnershipTypeMismatch(value_id));
            }
            if value.def != (ValueDef::Instruction { block, index, inst: inst_id }) {
                return Err(ValidationError::ValueDefMismatch(value_id));
            }
            Ok(())
        }
    }
}

/// Validates that non-refinable opcodes carry their canonical effect set.
fn validate_instruction_effects(inst_id: InstId, inst: &Instruction) -> Result<(), ValidationError> {
    let expected = inst.op.default_effects();
    if !inst.op.allows_effect_refinement() && inst.effects != expected {
        return Err(ValidationError::EffectMismatch {
            inst: inst_id,
            expected,
            actual: inst.effects,
        });
    }
    Ok(())
}

/// Validates immediate shape for opcodes whose immediate is structurally required.
fn validate_instruction_immediate(inst_id: InstId, inst: &Instruction) -> Result<(), ValidationError> {
    use Immediate as Imm;
    use Op::*;
    match inst.op {
        ConstI64 => require_immediate(inst_id, inst, "i64", |imm| matches!(imm, Imm::I64(_))),
        ConstF64 => require_immediate(inst_id, inst, "f64", |imm| matches!(imm, Imm::F64(_))),
        ConstBool => require_immediate(inst_id, inst, "bool", |imm| matches!(imm, Imm::Bool(_))),
        ConstStr | ConstClassName | DataAddr | Warn | IncludeOnceMark | IncludeOnceGuard
        | FunctionVariantMark | FunctionVariantDispatch | LoadPropRefCell => {
            require_immediate(inst_id, inst, "data id", |imm| matches!(imm, Imm::Data(_)))
        }
        LoadLocal | StoreLocal | UnsetLocal | LoadRefCell | StoreRefCell | ReleaseLocalRefCell
        | BindRefCellPtr
        | LoadStaticLocal | StoreStaticLocal | InitStaticLocal | InvokerRefArg => require_immediate(inst_id, inst, "local slot", |imm| {
            matches!(imm, Imm::LocalSlot(_))
        }),
        PromoteLocalRefCell | AliasLocalRefCell => require_immediate(inst_id, inst, "local slot pair", |imm| {
            matches!(imm, Imm::LocalSlotPair { .. })
        }),
        ICmp | FCmp => require_immediate(inst_id, inst, "comparison predicate", |imm| {
            matches!(imm, Imm::CmpPredicate(_))
        }),
        MixedNumericBinop => require_immediate(inst_id, inst, "mixed numeric op", |imm| {
            matches!(imm, Imm::MixedNumericOp(_))
        }),
        Cast => require_immediate(inst_id, inst, "cast target", |imm| {
            matches!(imm, Imm::CastTarget(_))
        }),
        Nop => {
            if matches!(inst.immediate, None | Some(Imm::Data(_))) {
                Ok(())
            } else {
                Err(ValidationError::UnexpectedImmediate(inst_id))
            }
        }
        ConstNull => {
            if inst.immediate.is_some() {
                Err(ValidationError::UnexpectedImmediate(inst_id))
            } else {
                Ok(())
            }
        }
        _ => Ok(()),
    }
}

/// Validates that an instruction has an immediate matching the expected predicate.
fn require_immediate(
    inst_id: InstId,
    inst: &Instruction,
    expected: &'static str,
    matches_expected: impl FnOnce(&Immediate) -> bool,
) -> Result<(), ValidationError> {
    let Some(imm) = inst.immediate.as_ref() else {
        return Err(ValidationError::MissingImmediate { inst: inst_id, expected });
    };
    if matches_expected(imm) {
        Ok(())
    } else {
        Err(ValidationError::MissingImmediate { inst: inst_id, expected })
    }
}

/// Validates operand IDs and dominance for one instruction.
fn validate_instruction_operands(
    function: &Function,
    block: BlockId,
    inst_index: u32,
    _inst_id: InstId,
    inst: &Instruction,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> Result<(), ValidationError> {
    for operand in &inst.operands {
        validate_use(function, *operand, block, Some(inst_index), dominators)?;
    }
    Ok(())
}

/// Validates core opcode operand/result type rules.
fn validate_opcode_rules(function: &Function, inst_id: InstId, inst: &Instruction) -> Result<(), ValidationError> {
    use Op::*;
    match inst.op {
        ConstI64 | ConstBool | ConstNull => check_count(inst_id, inst, 0, "0"),
        ConstF64 | ConstStr | ConstClassName | ConstEnumCase | DataAddr | ArrayNew | HashNew
        | CallableArrayNew | GeneratorNew | InvokerRefArg
        | ErrorSuppressBegin | ErrorSuppressEnd | TryPushHandler | TryPopHandler
        | CatchCurrent | CatchBind | FinallyEnter | FinallyExit | IncludeOnceMark
        | IncludeOnceGuard | FunctionVariantMark | FunctionVariantDispatch | ConcatReset
        | GcCollect | Nop => {
            check_count(inst_id, inst, 0, "0")
        }
        ClosureNew => Ok(()),
        FirstClassCallableNew => check_count_at_most(inst_id, inst, 1, "0 or 1"),
        ObjectNew => Ok(()),
        IAdd | ISub | IMul | IDiv | ISDiv | ISMod | IPow | IBitAnd | IBitOr | IBitXor
        | IShl | IShrA => check_binary(function, inst_id, inst, IrType::I64, "I64"),
        FAdd | FSub | FMul | FDiv | FPow => check_binary(function, inst_id, inst, IrType::F64, "F64"),
        MixedNumericBinop => check_count(inst_id, inst, 2, "2"),
        INeg | IBitNot => check_unary(function, inst_id, inst, IrType::I64, "I64"),
        FNeg => check_unary(function, inst_id, inst, IrType::F64, "F64"),
        ICmp => check_binary(function, inst_id, inst, IrType::I64, "I64"),
        FCmp => check_binary(function, inst_id, inst, IrType::F64, "F64"),
        IToF => check_unary(function, inst_id, inst, IrType::I64, "I64"),
        IToStr => check_unary_any(
            function,
            inst_id,
            inst,
            &[IrType::I64, IrType::TaggedScalar],
            "I64 or TaggedScalar",
        ),
        ResourceToStr => check_unary(function, inst_id, inst, IrType::I64, "I64 resource"),
        FToI | FToStr => check_unary(function, inst_id, inst, IrType::F64, "F64"),
        BoolToStr => check_unary(function, inst_id, inst, IrType::I64, "I64 bool"),
        StrToI | StrToF | StrToNumber | StrLen | StrPersist => {
            check_unary(function, inst_id, inst, IrType::Str, "Str")
        }
        StrConcat | StrEq | StrCmp | StrLooseEq => check_binary(function, inst_id, inst, IrType::Str, "Str"),
        StrCharAt => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Str, "Str")?;
            check_operand_type(function, inst_id, inst, 1, IrType::I64, "I64")
        }
        BufferNew => check_unary(function, inst_id, inst, IrType::I64, "I64"),
        LoadLocal | LoadRefCell | LoadGlobal | LoadStaticLocal | LoadStaticProperty | ExternGlobalLoad => {
            check_count(inst_id, inst, 0, "0")
        }
        UnsetLocal | PromoteLocalRefCell | AliasLocalRefCell | ReleaseLocalRefCell => {
            check_count(inst_id, inst, 0, "0")
        }
        StoreLocal | StoreGlobal | StoreStaticLocal | InitStaticLocal | StoreStaticProperty | ExternGlobalStore
        | StoreRefCell | BindRefCellPtr | Acquire | Release | Move | Borrow | EnsureOwned
        | EchoValue | PrintValue | WriteStdout | WriteStrStdout | VarDump | PrintR
        | ThrowException | GeneratorReturn | PtrCheckNonnull => {
            check_count(inst_id, inst, 1, "1")
        }
        MixedTagOf | MixedUnbox | MixedCastBool | MixedCastInt | MixedCastFloat
        | MixedCastString => check_heap_unary(function, inst_id, inst, IrHeapKind::Mixed, "Heap(Mixed)"),
        ArrayUnion => check_binary(function, inst_id, inst, IrType::Heap(IrHeapKind::Array), "Heap(Array)"),
        HashUnion => check_binary(function, inst_id, inst, IrType::Heap(IrHeapKind::Hash), "Heap(Hash)"),
        ArrayHashUnion => check_array_hash_union(function, inst_id, inst),
        HashArrayUnion => check_hash_array_union(function, inst_id, inst),
        ArrayLen | ArrayGet | ArrayIsset | ArraySet | ArrayPush | ArrayEnsureUnique
        | ArrayCloneShallow | ArrayToHash => {
            check_first_heap(function, inst_id, inst, IrHeapKind::Array, "Heap(Array)")
        }
        MixedArrayAppend => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Heap(IrHeapKind::Mixed), "Heap(Mixed)")
        }
        HashLen | HashGet | HashIsset | HashSet | HashAppend | HashEnsureUnique | HashCloneShallow => {
            check_first_heap(function, inst_id, inst, IrHeapKind::Hash, "Heap(Hash)")
        }
        IterCurrentValueRef => check_count(inst_id, inst, 1, "1"),
        ArrayKeyExists | OffsetExists => check_count_at_least(inst_id, inst, 1, "at least 1"),
        BufferLen | BufferGet | BufferSet | BufferFree => {
            check_first_heap(function, inst_id, inst, IrHeapKind::Buffer, "Heap(Buffer)")
        }
        PropGet | PropSet | LoadPropRefCell | DynamicPropGet | DynamicPropSet | NullsafePropGet
        | NullsafeMethodCall | MethodLookup | MethodCall | InstanceOf | InstanceOfDynamic => {
            check_count_at_least(inst_id, inst, 1, "at least 1")
        }
        _ => Ok(()),
    }
}

/// Validates the operand shape for indexed+associative array union.
fn check_array_hash_union(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 2, "2")?;
    check_operand_type(function, inst_id, inst, 0, IrType::Heap(IrHeapKind::Array), "Heap(Array)")?;
    check_operand_type(function, inst_id, inst, 1, IrType::Heap(IrHeapKind::Hash), "Heap(Hash)")
}

/// Validates the operand shape for associative+indexed array union.
fn check_hash_array_union(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 2, "2")?;
    check_operand_type(function, inst_id, inst, 0, IrType::Heap(IrHeapKind::Hash), "Heap(Hash)")?;
    check_operand_type(function, inst_id, inst, 1, IrType::Heap(IrHeapKind::Array), "Heap(Array)")
}

/// Validates one exact operand count.
fn check_count(inst_id: InstId, inst: &Instruction, expected: usize, expected_label: &'static str) -> Result<(), ValidationError> {
    if inst.operands.len() == expected {
        Ok(())
    } else {
        Err(ValidationError::OperandCountMismatch {
            inst: inst_id,
            expected: expected_label,
            actual: inst.operands.len(),
        })
    }
}

/// Validates a minimum operand count.
fn check_count_at_least(inst_id: InstId, inst: &Instruction, min: usize, expected_label: &'static str) -> Result<(), ValidationError> {
    if inst.operands.len() >= min {
        Ok(())
    } else {
        Err(ValidationError::OperandCountMismatch {
            inst: inst_id,
            expected: expected_label,
            actual: inst.operands.len(),
        })
    }
}

/// Validates a maximum operand count.
fn check_count_at_most(inst_id: InstId, inst: &Instruction, max: usize, expected_label: &'static str) -> Result<(), ValidationError> {
    if inst.operands.len() <= max {
        Ok(())
    } else {
        Err(ValidationError::OperandCountMismatch {
            inst: inst_id,
            expected: expected_label,
            actual: inst.operands.len(),
        })
    }
}

/// Validates a unary operand type.
fn check_unary(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
    expected: IrType,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 1, "1")?;
    check_operand_type(function, inst_id, inst, 0, expected, expected_label)
}

/// Validates a unary operand type against a small set of accepted storage types.
fn check_unary_any(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
    expected: &[IrType],
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 1, "1")?;
    let operand = inst.operands[0];
    let actual = function
        .value(operand)
        .ok_or(ValidationError::UnknownValue(operand))?
        .ir_type;
    if expected.contains(&actual) {
        Ok(())
    } else {
        Err(ValidationError::OperandTypeMismatch {
            inst: inst_id,
            operand,
            expected: expected_label,
            actual,
        })
    }
}

/// Validates both operands of a binary operation share one type.
fn check_binary(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
    expected: IrType,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 2, "2")?;
    check_operand_type(function, inst_id, inst, 0, expected, expected_label)?;
    check_operand_type(function, inst_id, inst, 1, expected, expected_label)
}

/// Validates a single operand type by index.
fn check_operand_type(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
    index: usize,
    expected: IrType,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    let operand = inst.operands[index];
    let actual = function
        .value(operand)
        .ok_or(ValidationError::UnknownValue(operand))?
        .ir_type;
    if actual == expected {
        Ok(())
    } else {
        Err(ValidationError::OperandTypeMismatch {
            inst: inst_id,
            operand,
            expected: expected_label,
            actual,
        })
    }
}

/// Validates a unary heap operand with the expected heap subkind.
fn check_heap_unary(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
    kind: IrHeapKind,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 1, "1")?;
    check_first_heap(function, inst_id, inst, kind, expected_label)
}

/// Validates the first operand has the expected heap subkind.
fn check_first_heap(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
    kind: IrHeapKind,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    let Some(first) = inst.operands.first().copied() else {
        return Err(ValidationError::OperandCountMismatch {
            inst: inst_id,
            expected: "at least 1",
            actual: 0,
        });
    };
    let actual = function
        .value(first)
        .ok_or(ValidationError::UnknownValue(first))?
        .ir_type;
    if actual == IrType::Heap(kind) {
        Ok(())
    } else {
        Err(ValidationError::OperandTypeMismatch {
            inst: inst_id,
            operand: first,
            expected: expected_label,
            actual,
        })
    }
}

/// Validates all block terminators and their CFG edge argument contracts.
fn validate_terminators(
    function: &Function,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> Result<(), ValidationError> {
    for block in &function.blocks {
        let Some(term) = &block.terminator else {
            return Err(ValidationError::BlockMissingTerminator(block.id));
        };
        match term {
            Terminator::Br { target, args } => {
                validate_branch_args(function, *target, args)?;
                validate_terminator_uses(function, block.id, args, dominators)?;
            }
            Terminator::CondBr {
                cond,
                then_target,
                then_args,
                else_target,
                else_args,
            } => {
                validate_i64_condition(function, *cond)?;
                validate_use(function, *cond, block.id, None, dominators)?;
                validate_branch_args(function, *then_target, then_args)?;
                validate_branch_args(function, *else_target, else_args)?;
                validate_terminator_uses(function, block.id, then_args, dominators)?;
                validate_terminator_uses(function, block.id, else_args, dominators)?;
            }
            Terminator::Switch {
                scrutinee,
                cases,
                default,
                default_args,
            } => {
                validate_switch_scrutinee(function, *scrutinee)?;
                validate_use(function, *scrutinee, block.id, None, dominators)?;
                for case in cases {
                    validate_switch_case(function, block.id, case, dominators)?;
                }
                validate_branch_args(function, *default, default_args)?;
                validate_terminator_uses(function, block.id, default_args, dominators)?;
            }
            Terminator::Return { value } => validate_return(function, block.id, *value, dominators)?,
            Terminator::Throw { value } => {
                validate_use(function, *value, block.id, None, dominators)?;
            }
            Terminator::Fatal { .. } | Terminator::Unreachable => {}
            Terminator::GeneratorSuspend {
                key,
                value,
                resume,
                resume_args,
            } => {
                if let Some(key) = key {
                    validate_use(function, *key, block.id, None, dominators)?;
                }
                if let Some(value) = value {
                    validate_use(function, *value, block.id, None, dominators)?;
                }
                validate_branch_args(function, *resume, resume_args)?;
                validate_terminator_uses(function, block.id, resume_args, dominators)?;
            }
        }
    }
    Ok(())
}

/// Validates a switch case edge and its argument uses.
fn validate_switch_case(
    function: &Function,
    source_block: BlockId,
    case: &SwitchCase,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> Result<(), ValidationError> {
    validate_branch_args(function, case.target, &case.args)?;
    validate_terminator_uses(function, source_block, &case.args, dominators)
}

/// Validates return terminator type and normal-return compatibility.
fn validate_return(
    function: &Function,
    block: BlockId,
    value: Option<ValueId>,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> Result<(), ValidationError> {
    if matches!(function.return_php_type, PhpType::Never) {
        return Err(ValidationError::NeverFunctionReturns);
    }
    match value {
        Some(value_id) => {
            validate_use(function, value_id, block, None, dominators)?;
            let actual = function
                .value(value_id)
                .ok_or(ValidationError::UnknownValue(value_id))?
                .ir_type;
            if actual == function.return_type {
                Ok(())
            } else {
                Err(ValidationError::ReturnTypeMismatch {
                    expected: function.return_type,
                    actual: Some(actual),
                })
            }
        }
        None => {
            if function.return_type == IrType::Void {
                Ok(())
            } else {
                Err(ValidationError::ReturnTypeMismatch {
                    expected: function.return_type,
                    actual: None,
                })
            }
        }
    }
}

/// Validates that each terminator argument is a dominated use.
fn validate_terminator_uses(
    function: &Function,
    block: BlockId,
    values: &[ValueId],
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> Result<(), ValidationError> {
    for value in values {
        validate_use(function, *value, block, None, dominators)?;
    }
    Ok(())
}

/// Validates destination block argument count and type compatibility.
fn validate_branch_args(function: &Function, target: BlockId, args: &[ValueId]) -> Result<(), ValidationError> {
    let Some(target_block) = function.block(target) else {
        return Err(ValidationError::UnknownBlock(target));
    };
    if target_block.params.len() != args.len() {
        return Err(ValidationError::BranchArgCountMismatch {
            target,
            expected: target_block.params.len(),
            actual: args.len(),
        });
    }
    for (index, (param, arg)) in target_block.params.iter().zip(args.iter()).enumerate() {
        let param_type = function
            .value(*param)
            .ok_or(ValidationError::UnknownValue(*param))?
            .ir_type;
        let arg_type = function
            .value(*arg)
            .ok_or(ValidationError::UnknownValue(*arg))?
            .ir_type;
        if param_type != arg_type {
            return Err(ValidationError::BranchArgTypeMismatch {
                target,
                index,
                expected: param_type,
                actual: arg_type,
            });
        }
    }
    Ok(())
}

/// Validates that a branch condition is an `I64` truthiness value.
fn validate_i64_condition(function: &Function, value: ValueId) -> Result<(), ValidationError> {
    let actual = function
        .value(value)
        .ok_or(ValidationError::UnknownValue(value))?
        .ir_type;
    if actual == IrType::I64 {
        Ok(())
    } else {
        Err(ValidationError::ConditionTypeMismatch { value, actual })
    }
}

/// Validates that a switch scrutinee is represented as `I64`.
fn validate_switch_scrutinee(function: &Function, value: ValueId) -> Result<(), ValidationError> {
    let actual = function
        .value(value)
        .ok_or(ValidationError::UnknownValue(value))?
        .ir_type;
    if actual == IrType::I64 {
        Ok(())
    } else {
        Err(ValidationError::SwitchScrutineeTypeMismatch { value, actual })
    }
}

/// Validates a value use exists, is non-void, and is dominated by its definition.
fn validate_use(
    function: &Function,
    value: ValueId,
    use_block: BlockId,
    use_inst_index: Option<u32>,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> Result<(), ValidationError> {
    let Some(value_ref) = function.value(value) else {
        return Err(ValidationError::UnknownValue(value));
    };
    if value_ref.ir_type == IrType::Void {
        return Err(ValidationError::VoidValueUsed(value));
    }
    if definition_dominates_use(value_ref.def, use_block, use_inst_index, dominators) {
        Ok(())
    } else {
        Err(ValidationError::UseNotDominated {
            value,
            used_in: use_block,
        })
    }
}

/// Returns true when a value definition dominates an instruction or terminator use.
fn definition_dominates_use(
    def: ValueDef,
    use_block: BlockId,
    use_inst_index: Option<u32>,
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
) -> bool {
    match def {
        ValueDef::BlockParam { block, .. } => {
            block == use_block
                || dominators
                    .get(&use_block)
                    .map(|set| set.contains(&block))
                    .unwrap_or(false)
        }
        ValueDef::Instruction { block, index, .. } if block == use_block => {
            use_inst_index.map(|use_index| index < use_index).unwrap_or(true)
        }
        ValueDef::Instruction { block, .. } => dominators
            .get(&use_block)
            .map(|set| set.contains(&block))
            .unwrap_or(false),
    }
}

/// Computes simple iterative dominator sets for the function CFG.
///
/// Only predecessors reachable from the entry are intersected. An unreachable
/// block carries no real control flow from the entry, so including it as a
/// predecessor would wrongly shrink a reachable block's dominator set — e.g. a
/// loop whose `for.update` is skipped by an unconditional `break` leaves that
/// update block unreachable yet still branching back to the loop header, which
/// would otherwise strip the entry block out of the header's dominators and
/// produce spurious `UseNotDominated` errors for any value the entry defines and
/// a later pass forwards into the loop. Unreachable blocks themselves still
/// resolve to `{self}` (no reachable predecessor), so genuine uses inside dead
/// code remain flagged until they are neutralized.
fn compute_dominators(function: &Function) -> HashMap<BlockId, HashSet<BlockId>> {
    let all_blocks: HashSet<BlockId> = function.blocks.iter().map(|block| block.id).collect();
    let predecessors = compute_predecessors(function);
    let reachable = reachable_from_entry(function, &predecessors);
    let mut dominators = HashMap::new();
    for block in &function.blocks {
        if block.id == function.entry {
            dominators.insert(block.id, HashSet::from([block.id]));
        } else {
            dominators.insert(block.id, all_blocks.clone());
        }
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            if block.id == function.entry {
                continue;
            }
            let preds: Vec<BlockId> = predecessors
                .get(&block.id)
                .map(|preds| preds.iter().copied().filter(|p| reachable.contains(p)).collect())
                .unwrap_or_default();
            let mut next = if preds.is_empty() {
                HashSet::new()
            } else {
                intersection_of_predecessors(&preds, &dominators, &all_blocks)
            };
            next.insert(block.id);
            if dominators.get(&block.id) != Some(&next) {
                dominators.insert(block.id, next);
                changed = true;
            }
        }
    }
    dominators
}

/// Computes the set of blocks reachable from the entry over the same edge set as
/// `compute_predecessors` (terminator successors plus implicit exception-handler
/// edges): a block is reachable when it is the entry or any of its predecessors
/// is reachable. Iterates to a fixed point.
fn reachable_from_entry(
    function: &Function,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> HashSet<BlockId> {
    let mut reachable: HashSet<BlockId> = HashSet::from([function.entry]);
    let mut changed = true;
    while changed {
        changed = false;
        for block in &function.blocks {
            if reachable.contains(&block.id) {
                continue;
            }
            if let Some(preds) = predecessors.get(&block.id) {
                if preds.iter().any(|pred| reachable.contains(pred)) {
                    reachable.insert(block.id);
                    changed = true;
                }
            }
        }
    }
    reachable
}

/// Builds predecessor lists from every terminator edge plus implicit exception edges.
///
/// A `try_push_handler <token>` instruction installs an exception handler whose block id equals
/// `<token>` (the backend recovers it the same way, `BlockId::from_raw(token as u32)`). Control can
/// reach that handler from anywhere in the protected region, but the push block dominates the whole
/// region, so the push block is the handler's immediate dominator. The terminator graph alone never
/// names the handler as a successor, so without this edge the handler looks unreachable: its
/// dominator set collapses to itself and corrupts every block reached from the catch body — for a
/// `try`/`catch` inside a loop the back-edge then strips the entry block out of the loop header's
/// dominators, yielding spurious `UseNotDominated` errors for values defined in the entry block.
fn compute_predecessors(function: &Function) -> HashMap<BlockId, Vec<BlockId>> {
    let mut predecessors: HashMap<BlockId, Vec<BlockId>> = HashMap::new();
    for block in &function.blocks {
        if let Some(term) = &block.terminator {
            for successor in successors(term) {
                predecessors.entry(successor).or_default().push(block.id);
            }
        }
        for inst_id in &block.instructions {
            let Some(inst) = function.instruction(*inst_id) else {
                continue;
            };
            if inst.op == Op::TryPushHandler {
                if let Some(Immediate::I64(token)) = inst.immediate {
                    let handler = BlockId::from_raw(token as u32);
                    predecessors.entry(handler).or_default().push(block.id);
                }
            }
        }
    }
    predecessors
}

/// Returns all direct successor blocks for a terminator.
fn successors(term: &Terminator) -> Vec<BlockId> {
    match term {
        Terminator::Br { target, .. } => vec![*target],
        Terminator::CondBr {
            then_target,
            else_target,
            ..
        } => vec![*then_target, *else_target],
        Terminator::Switch { cases, default, .. } => {
            let mut out: Vec<BlockId> = cases.iter().map(|case| case.target).collect();
            out.push(*default);
            out
        }
        Terminator::GeneratorSuspend { resume, .. } => vec![*resume],
        Terminator::Return { .. }
        | Terminator::Throw { .. }
        | Terminator::Fatal { .. }
        | Terminator::Unreachable => Vec::new(),
    }
}

/// Intersects dominator sets for all predecessors.
fn intersection_of_predecessors(
    predecessors: &[BlockId],
    dominators: &HashMap<BlockId, HashSet<BlockId>>,
    fallback: &HashSet<BlockId>,
) -> HashSet<BlockId> {
    let mut iter = predecessors.iter();
    let Some(first) = iter.next() else {
        return HashSet::new();
    };
    let mut result = dominators.get(first).cloned().unwrap_or_else(|| fallback.clone());
    for pred in iter {
        if let Some(set) = dominators.get(pred) {
            result = result.intersection(set).copied().collect();
        }
    }
    result
}

/// Returns true when PHP type metadata can use the given EIR storage type.
fn php_type_compatible(ir_type: IrType, php_type: &PhpType) -> bool {
    let php_type = php_type.codegen_repr();
    IrType::from_php(&php_type) == ir_type
        || matches!((ir_type, php_type), (IrType::I64, PhpType::Void))
}

/// Returns true when ownership is coherent with storage and PHP type metadata.
fn ownership_compatible(ir_type: IrType, php_type: &PhpType, ownership: Ownership) -> bool {
    let php_type = php_type.codegen_repr();
    let tracks_lifetime = ir_type.is_refcounted_storage()
        || Ownership::php_type_needs_lifetime_tracking(&php_type);
    if tracks_lifetime {
        !matches!(ownership, Ownership::NonHeap)
    } else {
        matches!(ownership, Ownership::NonHeap)
    }
}
