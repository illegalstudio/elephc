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

mod string_append;

use std::collections::{HashMap, HashSet};

use crate::ir::block::{BlockId, SwitchCase, Terminator};
use crate::ir::effects::Effects;
use crate::ir::function::{Function, LocalKind, LocalSlotId};
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
    BlockIdMismatch {
        expected: BlockId,
        actual: BlockId,
    },
    BlockMissingTerminator(BlockId),
    UnknownBlock(BlockId),
    UnknownInstruction(InstId),
    UnknownSource(crate::ir::SourceId),
    InvalidDeclarationName(crate::ir::DataId),
    UnresolvedSource(std::path::PathBuf),
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
        /// The instruction's opcode and source position, so a backend gap names the PHP line it
        /// came from instead of only an `InstId` the reader cannot look up.
        ///
        /// `LoweringError` renders this enum with `{:?}`, so anything a reader needs has to be
        /// IN the value. Finding the construct behind one of these used to mean re-running the
        /// compiler under a temporary `eprintln!`.
        at: Option<String>,
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
    InvalidStringAppendTransfer(InstId),
    UnknownRuntimeCallSignature(InstId),
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
        for inst in &function.instructions {
            if let Some(Immediate::Source(id)) = &inst.immediate {
                if module.source_catalog().and_then(|catalog| catalog.get(*id)).is_none() {
                    return Err(ValidationError::UnknownSource(*id));
                }
            }
            if let Some(Immediate::ClassLikeActivation(activation)) = &inst.immediate {
                let (source, name) = (&activation.source, &activation.name);
                if module.source_catalog().and_then(|catalog| catalog.get(*source)).is_none() {
                    return Err(ValidationError::UnknownSource(*source));
                }
                if module.data.strings.get(name.as_raw() as usize).is_none_or(|name| name.is_empty()) {
                    return Err(ValidationError::InvalidDeclarationName(*name));
                }
            }
        }
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
    string_append::validate_transfers(function)?;
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
    if !function.blocks[function.entry.as_raw() as usize]
        .params
        .is_empty()
    {
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
        // A buffer element's storage width (`buffer<i32>`) is never a value type: element reads
        // and writes carry the `int` or `float` the width reads as.
        if matches!(value.php_type, PhpType::Narrow(_)) {
            return Err(ValidationError::PhpTypeMismatch(value_id));
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

/// Verifies a resolved buffer access takes its descriptor straight from a `BufferResolve`
/// result: codegen reads the element's storage width (`buffer<i32>`, `buffer<f32>`) from that
/// instruction's handle operand, so a descriptor that reaches the access through a block
/// parameter or another instruction would lose the width.
fn check_descriptor_from_buffer_resolve(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    let descriptor = inst.operands[0];
    let value = function
        .value(descriptor)
        .ok_or(ValidationError::UnknownValue(descriptor))?;
    let mismatch = || ValidationError::OperandTypeMismatch {
        inst: inst_id,
        operand: descriptor,
        expected: "buffer_resolve descriptor",
        actual: value.ir_type,
    };
    let ValueDef::Instruction { inst: def, .. } = value.def else {
        return Err(mismatch());
    };
    let def_inst = function
        .instruction(def)
        .ok_or(ValidationError::UnknownInstruction(def))?;
    if def_inst.op != Op::BufferResolve {
        return Err(mismatch());
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
    dominators: &Dominators,
) -> Result<(), ValidationError> {
    for block in &function.blocks {
        for (index, inst_id) in block.instructions.iter().enumerate() {
            let inst = function
                .instruction(*inst_id)
                .ok_or(ValidationError::UnknownInstruction(*inst_id))?;
            validate_instruction_result(function, block.id, index as u32, *inst_id, inst)?;
            validate_instruction_effects(*inst_id, inst)?;
            validate_instruction_immediate(*inst_id, inst)?;
            validate_instruction_operands(
                function,
                block.id,
                index as u32,
                *inst_id,
                inst,
                dominators,
            )?;
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
            if value.def
                != (ValueDef::Instruction {
                    block,
                    index,
                    inst: inst_id,
                })
            {
                return Err(ValidationError::ValueDefMismatch(value_id));
            }
            Ok(())
        }
    }
}

/// Validates that non-refinable opcodes carry their canonical effect set.
/// `load_local` additionally admits exactly `PURE`, which is attached only after
/// the immutable-local pass proves that the named scalar slot cannot change.
/// Physical property initialization additionally allocates owned reference cells.
/// A named `prop_set` additionally admits exactly the packed field store effects.
fn validate_instruction_effects(
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    let expected = if inst.op == Op::MixedUnbox {
        Op::mixed_unbox_effects(&inst.result_php_type)
    } else if inst.op == Op::FToI
        && matches!(
            inst.immediate,
            Some(Immediate::Bool(true) | Immediate::FloatKeyDiagnostic)
        )
    {
        Op::FToI.default_effects() | Effects::MAY_WARN
    } else if matches!(inst.op, Op::PropSet | Op::PropUnset)
        && matches!(inst.immediate, Some(Immediate::PropertyRef { .. }))
    {
        inst.op.default_effects() | Effects::ALLOC_HEAP
    } else {
        inst.op.default_effects()
    };
    let immutable_local_refinement = inst.op == Op::LoadLocal && inst.effects.is_pure();
    // A gradual boundary cast (mixed to a declared scalar) warns through the user error
    // handler, which may throw: lowering marks exactly those casts as throw points.
    let throwing_boundary_cast = inst.op == Op::Cast
        && inst.effects == Op::Cast.default_effects() | Effects::MAY_THROW;
    let packed_field_store_refinement = inst.op == Op::PropSet
        && matches!(inst.immediate, Some(Immediate::Data(_)))
        && inst.effects == Op::PropSet.packed_field_access_effects();
    if !inst.op.allows_effect_refinement()
        && !immutable_local_refinement
        && !packed_field_store_refinement
        && !throwing_boundary_cast
        && inst.effects != expected
    {
        return Err(ValidationError::EffectMismatch {
            inst: inst_id,
            expected,
            actual: inst.effects,
        });
    }
    Ok(())
}

#[cfg(test)]
mod instruction_effect_tests {
    use super::*;
    use crate::ir::Ownership;

    /// Unresolved calls accept their default effects and reject unproven purity claims.
    #[test]
    fn call_effect_batch_validator_rejects_unresolved_refinement() {
        for op in [Op::FunctionVariantCall, Op::ClosureCall, Op::ExprCall,
            Op::CallableDescriptorInvoke, Op::PipeCall, Op::EvalLiteralCall,
            Op::EvalFunctionCall, Op::EvalFunctionCallArray, Op::EvalObjectNew,
            Op::EvalStaticMethodCall]
        {
            let mut instruction = Instruction::new(
                op, Vec::new(), None, None, IrType::Void, PhpType::Void,
                Ownership::NonHeap, op.default_effects(), None,
            );
            let id = InstId::from_raw(0);
            assert_eq!(validate_instruction_effects(id, &instruction), Ok(()), "{op:?}");
            instruction.effects = Effects::PURE;
            assert!(matches!(validate_instruction_effects(id, &instruction),
                Err(ValidationError::EffectMismatch { .. })), "{op:?}");
        }
    }

    /// Unboxing a descriptor or object must retain its payload, unlike scalar extraction.
    #[test]
    fn mixed_unbox_effects_require_exact_payload_ownership_barriers() {
        for ty in [PhpType::Callable, PhpType::Object("Owner".to_owned()), PhpType::Int] {
            let effects = Op::mixed_unbox_effects(&ty);
            let tracked = ty != PhpType::Int;
            assert_eq!(effects.contains(Effects::REFCOUNT_OP | Effects::WRITES_HEAP), tracked);
            let mut instruction = Instruction::new(
                Op::MixedUnbox, Vec::new(), None, None, IrType::I64, ty,
                if tracked { Ownership::Owned } else { Ownership::NonHeap }, effects, None,
            );
            let id = InstId::from_raw(0);
            assert_eq!(validate_instruction_effects(id, &instruction), Ok(()));
            instruction.effects ^= Effects::REFCOUNT_OP;
            assert!(matches!(validate_instruction_effects(id, &instruction),
                Err(ValidationError::EffectMismatch { .. })));
            instruction.effects = effects | Effects::OUTPUT;
            assert!(validate_instruction_effects(id, &instruction).is_err());
        }
    }

    /// Physical initializer writes require allocation effects without weakening ordinary writes.
    #[test]
    fn physical_property_initializers_have_exact_allocation_effects() {
        for op in [Op::PropSet, Op::PropUnset] {
            for physical in [false, true] {
                let immediate = physical.then_some(Immediate::PropertyRef { class: 1, property: 0 });
                let expected = op.default_effects()
                    | if physical { Effects::ALLOC_HEAP } else { Effects::PURE };
                let mut instruction = Instruction::new(
                    op, Vec::new(), immediate, None, IrType::Void, PhpType::Void,
                    Ownership::NonHeap, expected, None,
                );
                let id = InstId::from_raw(0);
                assert_eq!(validate_instruction_effects(id, &instruction), Ok(()));
                instruction.effects = expected ^ Effects::ALLOC_HEAP;
                assert!(matches!(
                    validate_instruction_effects(id, &instruction),
                    Err(ValidationError::EffectMismatch { .. })
                ));
                instruction.effects = expected | Effects::OUTPUT;
                assert!(validate_instruction_effects(id, &instruction).is_err());
            }
        }
    }

    /// A named `prop_set` admits exactly one refinement, the packed field store effects, and
    /// keeps rejecting every other deviation from its canonical contract.
    #[test]
    fn named_prop_set_admits_only_the_packed_field_store_refinement() {
        let packed = Op::PropSet.packed_field_access_effects();
        let mut instruction = Instruction::new(
            Op::PropSet,
            Vec::new(),
            Some(Immediate::Data(crate::ir::DataId::from_raw(0))),
            None,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            packed,
            None,
        );
        let id = InstId::from_raw(0);
        assert_eq!(validate_instruction_effects(id, &instruction), Ok(()));
        instruction.effects = Op::PropSet.default_effects();
        assert_eq!(validate_instruction_effects(id, &instruction), Ok(()));
        for effects in [packed | Effects::OUTPUT, Effects::WRITES_HEAP] {
            instruction.effects = effects;
            assert!(matches!(
                validate_instruction_effects(id, &instruction),
                Err(ValidationError::EffectMismatch { .. })
            ));
        }
    }

    /// Reflected physical access has its own immediate and does not claim initializer allocation.
    #[test]
    fn reflected_property_access_has_distinct_canonical_effects() {
        for op in [Op::PropGet, Op::PropSet] {
            let immediate = Some(Immediate::ReflectionPropertyRef {
                class: 1,
                property: 0,
            });
            let mut instruction = Instruction::new(
                op,
                Vec::new(),
                immediate,
                None,
                IrType::Void,
                PhpType::Void,
                Ownership::NonHeap,
                op.default_effects(),
                None,
            );
            let id = InstId::from_raw(0);
            assert_eq!(validate_instruction_immediate(id, &instruction), Ok(()));
            assert_eq!(validate_instruction_effects(id, &instruction), Ok(()));
            instruction.effects |= Effects::OUTPUT;
            if op == Op::PropSet {
                assert!(matches!(
                    validate_instruction_effects(id, &instruction),
                    Err(ValidationError::EffectMismatch { .. })
                ));
            }
        }
    }

    /// Unsetting a last-owner element can allocate COW storage and execute a throwing destructor.
    #[test]
    fn unset_effects_preserve_cow_and_destructor_boundaries() {
        let required = Effects::READS_HEAP | Effects::WRITES_HEAP | Effects::ALLOC_HEAP
            | Effects::REFCOUNT_OP | Effects::MAY_THROW | Effects::MAY_FATAL;
        for op in [
            Op::HashUnset,
            Op::PropUnset,
            Op::DynamicPropUnset,
            Op::OffsetUnset,
        ] {
            assert!(op.default_effects().contains(required), "{op:?}");
        }
    }

    /// A property STORE can create a dynamic property, which diagnoses and can unwind.
    ///
    /// Both forms carry the same contract because phase B2 gave them the same consequences: the
    /// per-instance hash is read and can be reallocated, the boxed value is refcounted, php 8.5's
    /// `Creation of dynamic property C::$p is deprecated` goes to the diagnostic stream, and a
    /// name this scope may not write, or a value a declared slot refuses, raises a catchable
    /// `Error`. Advertising less let the optimizer reorder or drop such a store.
    #[test]
    fn property_store_effects_cover_dynamic_creation_diagnostics_and_unwind() {
        let required = Effects::READS_GLOBAL
            | Effects::WRITES_GLOBAL
            | Effects::READS_HEAP
            | Effects::WRITES_HEAP
            | Effects::ALLOC_HEAP
            | Effects::MAY_THROW
            | Effects::MAY_WARN
            | Effects::MAY_FATAL
            | Effects::REFCOUNT_OP;
        for op in [Op::PropSet, Op::DynamicPropSet] {
            assert!(op.default_effects().contains(required), "{op:?}");
        }
    }

    /// A reference-cell load can now refuse, so it carries the checked variant's contract.
    ///
    /// The plain form used to advertise a bare `READS_HEAP`. Phase B2 made it raise php's
    /// catchable access `Error` for a name this scope may not reach, and made the `Mixed` form
    /// raise instead of publishing a zero pointer as a live cell. That throw unwinds through
    /// frame cleanup which can run PHP destructors, so the two forms must not differ.
    #[test]
    fn reference_cell_loads_share_the_unwinding_contract() {
        let required = Effects::READS_HEAP
            | Effects::WRITES_HEAP
            | Effects::ALLOC_HEAP
            | Effects::REFCOUNT_OP
            | Effects::MAY_THROW
            | Effects::MAY_FATAL;
        for op in [Op::LoadPropRefCell, Op::LoadPropRefCellChecked] {
            assert!(op.default_effects().contains(required), "{op:?}");
        }
    }

    /// Nested element reference loads can normalize and replace their parent container.
    #[test]
    fn array_element_reference_load_models_mutation_and_ownership_effects() {
        let required = Effects::READS_HEAP
            | Effects::WRITES_HEAP
            | Effects::ALLOC_HEAP
            | Effects::REFCOUNT_OP
            | Effects::MAY_FATAL;
        assert!(
            Op::LoadArrayElemRefCell.default_effects().contains(required),
            "LoadArrayElemRefCell must remain an ownership-sensitive heap mutation"
        );
    }
}

/// Validates immediate shape for opcodes whose immediate is structurally required.
fn validate_instruction_immediate(
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    use Immediate as Imm;
    use Op::*;
    if let Some(Imm::UnresolvedSource(path)) = &inst.immediate {
        return Err(ValidationError::UnresolvedSource((**path).clone()));
    }
    match inst.op {
        ConstI64 => require_immediate(inst_id, inst, "i64", |imm| matches!(imm, Imm::I64(_))),
        ClassLikeActivate => require_immediate(inst_id, inst, "declaration site", |imm| {
            matches!(imm, Imm::ClassLikeActivation(_))
        }),
        ConstF64 => require_immediate(inst_id, inst, "f64", |imm| matches!(imm, Imm::F64(_))),
        ConstBool => require_immediate(inst_id, inst, "bool", |imm| matches!(imm, Imm::Bool(_))),
        IncludeOnceMark | IncludeOnceGuard => {
            require_immediate(inst_id, inst, "source id", |imm| matches!(imm, Imm::Source(_)))
        }
        IncludeFrameEnter | IncludeFrameExit => require_immediate(
            inst_id,
            inst,
            "include frame",
            |imm| matches!(imm, Imm::IncludeFrame(_)),
        ),
        ConstStr | ConstClassName | ClosureScopeClassName | DataAddr | Warn
        | FunctionVariantMark | FunctionVariantDispatch | LoadPropRefCell | BindPropRefCell
        | LoadPropRefCellChecked
        | LoadStaticPropertyRefCell
        | EvalFunctionCallArray | EvalFunctionExists | EvalClassExists | EvalConstantExists
        | EvalConstantFetch
        | EvalStaticMethodCall
        | EnumBackingStringToInt
        | ArgumentStringToInt
        | EnumBackingMixedToInt
        | PackedFieldMixedToInt
        | PackedFieldMixedToFloat
        | PackedFieldMixedToBool
        | ReturnBoundaryMixedToInt
        | PropInitialized
        | StaticPropInitialized
        | ReflectionStaticPropertyInitialized => {
            require_immediate(inst_id, inst, "data id", |imm| matches!(imm, Imm::Data(_)))
        }
        EvalLiteralCall => require_immediate(inst_id, inst, "profiled data id", |imm| {
            matches!(imm, Imm::Data(_) | Imm::ProfiledData { .. })
        }),
        PropGet => require_immediate(inst_id, inst, "property name or reflected physical slot", |imm| {
            matches!(imm, Imm::Data(_) | Imm::ReflectionPropertyRef { .. })
        }),
        PropSet => require_immediate(inst_id, inst, "property name or trusted physical slot", |imm| {
            matches!(
                imm,
                Imm::Data(_)
                    | Imm::PropertyRef { .. }
                    | Imm::ReflectionPropertyRef { .. }
            )
        }),
        PropUnset => require_immediate(inst_id, inst, "property name or initializer physical slot", |imm| {
            matches!(imm, Imm::Data(_) | Imm::PropertyRef { .. })
        }),
        StrAppend | LoadLocal | StoreLocal | UnsetLocal | ZeroLocalSlot | LoadRefCell | StoreRefCell
        | ReleaseLocalRefCell | AcquireRefCell
        | ReleaseLocalSlot | PushCallOperandOwner | PopCallOperandOwner
        | LoadStaticLocal | StoreStaticLocal | InitStaticLocal | StaticLocalUninitialized => require_immediate(inst_id, inst, "local slot", |imm| {
            matches!(imm, Imm::LocalSlot(_))
        }),
        InvokerRefArg => require_immediate(inst_id, inst, "local slot or global name", |imm| {
            matches!(imm, Imm::LocalSlot(_) | Imm::GlobalName(_))
        }),
        ArrayLocalRefCell => require_immediate(inst_id, inst, "local slot", |imm| {
            matches!(imm, Imm::LocalSlot(_))
        }),
        BindRefCellPtr => require_immediate(inst_id, inst, "reference alias slot or alias/owner pair", |imm| {
            matches!(imm, Imm::LocalSlot(_) | Imm::LocalSlotPair { .. })
        }),
        PromoteLocalRefCell | AliasLocalRefCell | RetainLocalRefCell | AdoptRefCellPtr => require_immediate(inst_id, inst, "local slot pair", |imm| {
            matches!(imm, Imm::LocalSlotPair { .. })
        }),
        EvalScopeGet | EvalScopeSet | GlobalRefCell | UnsetGlobal => require_immediate(inst_id, inst, "global name", |imm| {
            matches!(imm, Imm::GlobalName(_))
        }),
        ICmp | FCmp | PhpRelCmp => require_immediate(inst_id, inst, "comparison predicate", |imm| {
            matches!(imm, Imm::CmpPredicate(_))
        }),
        MixedNumericBinop => require_immediate(inst_id, inst, "mixed numeric op", |imm| {
            matches!(imm, Imm::MixedNumericOp(_))
        }),
        ICheckedNumericChainToInt => {
            require_immediate(inst_id, inst, "checked numeric chain", |imm| {
                matches!(imm, Imm::CheckedNumericChain(chain) if !chain.operations().is_empty()
                    && chain.operations().iter().copied()
                        .all(crate::ir_passes::checked_numeric_chain::is_checked_chain_operation))
            })
        }
        StrIncDec => require_immediate(inst_id, inst, "increment delta", |imm| {
            matches!(imm, Imm::I64(1) | Imm::I64(-1))
        }),
        Cast => require_immediate(inst_id, inst, "cast target", |imm| {
            matches!(imm, Imm::CastTarget(_) | Imm::StringOffsetCast | Imm::ImplicitIntCast)
        }),
        TypePredicate => require_immediate(inst_id, inst, "type predicate", |imm| {
            matches!(imm, Imm::TypePredicate(_))
        }),
        GcControl => require_immediate(inst_id, inst, "GC control selector", |imm| {
            matches!(imm, Imm::I64(value) if crate::ir::GcControlOp::from_i64(*value).is_some())
        }),
        CoreBuiltin => require_immediate(inst_id, inst, "Core builtin selector", |imm| {
            matches!(imm, Imm::I64(value) if crate::ir::CoreBuiltinOp::from_i64(*value).is_some())
        }),
        IterStart => require_immediate(inst_id, inst, "iter_start metadata", |imm| {
            matches!(imm, Imm::IterStart(_))
        }),
        IterEnd => require_immediate(inst_id, inst, "iterator-state local slot", |imm| {
            matches!(imm, Imm::LocalSlot(_))
        }),
        StrCharAt => {
            if matches!(inst.immediate, None | Some(Imm::Bool(_))) {
                Ok(())
            } else {
                Err(ValidationError::UnexpectedImmediate(inst_id))
            }
        }
        FToI => {
            if matches!(inst.immediate, None | Some(Imm::Bool(true) | Imm::FloatKeyDiagnostic)) {
                Ok(())
            } else {
                Err(ValidationError::UnexpectedImmediate(inst_id))
            }
        }
        HashSet => {
            if matches!(inst.immediate, None | Some(Imm::Bool(true) | Imm::InPlacePropertyWrite)) {
                Ok(())
            } else {
                Err(ValidationError::UnexpectedImmediate(inst_id))
            }
        }
        HashGetForWrite | ArraySetMixedKey => {
            if matches!(inst.immediate, None | Some(Imm::Bool(true))) {
                Ok(())
            } else {
                Err(ValidationError::UnexpectedImmediate(inst_id))
            }
        }
        BufferGetResolved | BufferSetResolved => {
            if matches!(inst.immediate, None | Some(Imm::Bool(true))) {
                Ok(())
            } else {
                Err(ValidationError::UnexpectedImmediate(inst_id))
            }
        }
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
        return Err(ValidationError::MissingImmediate {
            inst: inst_id,
            expected,
        });
    };
    if matches_expected(imm) {
        Ok(())
    } else {
        Err(ValidationError::MissingImmediate {
            inst: inst_id,
            expected,
        })
    }
}

/// Validates operand IDs and dominance for one instruction.
fn validate_instruction_operands(
    function: &Function,
    block: BlockId,
    inst_index: u32,
    _inst_id: InstId,
    inst: &Instruction,
    dominators: &Dominators,
) -> Result<(), ValidationError> {
    for operand in &inst.operands {
        validate_use(function, *operand, block, Some(inst_index), dominators)?;
    }
    Ok(())
}

/// Validates core opcode operand/result type rules.
fn validate_opcode_rules(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    use Op::*;
    match inst.op {
        ConstI64 | ConstBool | ConstNull => check_count(inst_id, inst, 0, "0"),
        ConstF64 | ConstStr | ConstClassName | ClosureScopeClassName | ConstEnumCase | LoadCalledClassId | DataAddr | ArrayNew | HashNew
        | CallableArrayNew | GeneratorNew | InvokerRefArg | ArrayLocalRefCell
        | ErrorSuppressBegin | ErrorSuppressEnd | TryPushHandler | TryPopHandler
        | CatchCurrent | CatchBind | FinallyEnter | FinallyExit | IncludeOnceMark
        | IncludeOnceGuard | IncludeFrameEnter | IncludeFrameExit | ClassLikeActivate
        | FunctionVariantMark | FunctionVariantDispatch | EvalFunctionExists
        | EvalConstantExists | EvalConstantFetch | ConcatReset | GcCollect | Nop => {
            check_count(inst_id, inst, 0, "0")
        }
        // Memory-usage selectors carry the optional `$real_usage` flag as their one operand.
        GcControl => check_count_at_most(inst_id, inst, 1, "0 or 1"),
        EvalClassExists => check_count_at_most(inst_id, inst, 1, "0 or 1"),
        CoreBuiltin => {
            let Some(Immediate::I64(selector)) = inst.immediate.as_ref() else {
                return Ok(());
            };
            let Some(operation) = crate::ir::CoreBuiltinOp::from_i64(*selector) else {
                return Ok(());
            };
            let count = operation.operand_count();
            if matches!(
                operation,
                crate::ir::CoreBuiltinOp::DebugBacktrace
                    | crate::ir::CoreBuiltinOp::DebugPrintBacktrace
            ) {
                check_count_at_least(
                    inst_id,
                    inst,
                    count,
                    "the Core backtrace options plus current frame arguments",
                )
            } else {
                check_count(inst_id, inst, count, "the selector-specific Core builtin arity")
            }
        }
        EvalLiteralCall | EvalFunctionCallArray | EvalScopeGet => {
            check_count(inst_id, inst, 1, "1")
        }
        EvalScopeSet => check_count(inst_id, inst, 2, "2"),
        ClosureNew => Ok(()),
        ClosureBind => {
            match inst.immediate {
                None | Some(Immediate::ClosureBindScope(_)) => check_count(inst_id, inst, 2, "2"),
                Some(Immediate::ClosureBindObjectScope(_)) => check_count(inst_id, inst, 3, "3"),
                _ => Err(ValidationError::UnexpectedImmediate(inst_id)),
            }
        }
        FirstClassCallableNew => check_count_at_most(inst_id, inst, 1, "0 or 1"),
        ObjectNew => Ok(()),
        EvalStaticMethodCall => Ok(()),
        IAdd | ISub | IMul | IDiv | ISDiv | ISMod | IPow | IBitAnd | IBitOr | IBitXor
        | IShl | IShrA => check_binary(function, inst_id, inst, IrType::I64, "I64"),
        ICheckedAdd | ICheckedSub | ICheckedMul | ICheckedAddToInt | ICheckedSubToInt
        | ICheckedMulToInt | ICheckedPow | ICheckedDiv => {
            check_binary(function, inst_id, inst, IrType::I64, "I64")
        }
        ICheckedNumericChainToInt => check_checked_numeric_chain(function, inst_id, inst),
        FAdd | FSub | FMul | FDiv | FPow => check_binary(function, inst_id, inst, IrType::F64, "F64"),
        MixedNumericBinop => check_count(inst_id, inst, 2, "2"),
        // The operand is either a concrete `Str` payload or a boxed Mixed cell, so only
        // the arity is pinned here; the backend dispatches on the operand's EIR type.
        StrIncDec => check_count(inst_id, inst, 1, "1"),
        ObjectClassId => check_heap_unary(
            function,
            inst_id,
            inst,
            IrHeapKind::Object,
            "Heap(Object)",
        ),
        // Runtime class names can arrive as a concrete string or a boxed Mixed string. The
        // backend validates the storage representation before invoking its lookup helper.
        ClassNameToId => check_count(inst_id, inst, 1, "1"),
        INeg | IBitNot => check_unary(function, inst_id, inst, IrType::I64, "I64"),
        FNeg => check_unary(function, inst_id, inst, IrType::F64, "F64"),
        ICmp => check_binary(function, inst_id, inst, IrType::I64, "I64"),
        FCmp => check_binary(function, inst_id, inst, IrType::F64, "F64"),
        PhpRelCmp => check_count(inst_id, inst, 2, "2"),
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
        StrAppend | StrConcat | StrBitAnd | StrBitOr | StrBitXor | StrEq | StrCmp | StrLooseEq => {
            check_binary(function, inst_id, inst, IrType::Str, "Str")
        }
        StrSetOffset => {
            check_count(inst_id, inst, 3, "3")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Str, "Str")?;
            check_operand_type(function, inst_id, inst, 1, IrType::I64, "I64")?;
            check_operand_type(function, inst_id, inst, 2, IrType::Str, "Str")
        }
        StrCharAt => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Str, "Str")?;
            check_operand_type(function, inst_id, inst, 1, IrType::I64, "I64")
        }
        BufferNew => check_unary(function, inst_id, inst, IrType::I64, "I64"),
        LoadLocal
        | LoadRefCell
        | LoadGlobal
        | GlobalRefCell
        | UnsetGlobal
        | LoadStaticLocal
        | StaticLocalUninitialized
        | LoadStaticProperty
        | LoadStaticPropertyRefCell
        | LoadReflectionStaticProperty
        | ReflectionStaticPropertyInitialized
        | ExternGlobalLoad => check_count(inst_id, inst, 0, "0"),
        ThrowError => check_count(inst_id, inst, 0, "0"),
        ThrowErrorValue => check_unary(function, inst_id, inst, IrType::Str, "Str"),
        UnsetLocal | ZeroLocalSlot | PromoteLocalRefCell | AliasLocalRefCell | RetainLocalRefCell
        | ReleaseLocalRefCell
        | ReleaseLocalSlot | PushCallOperandOwner | PopCallOperandOwner => {
            check_count(inst_id, inst, 0, "0")
        }
        AcquireRefCell => {
            check_count(inst_id, inst, 1, "1")?;
            let result = inst.result.ok_or(ValidationError::InstructionResultMissing(inst_id))?;
            if inst.result_type != IrType::I64
                || !matches!(inst.result_php_type, PhpType::Pointer(_))
            {
                return Err(ValidationError::ResultTypeMismatch(result));
            }
            Ok(())
        }
        StoreLocal | StoreGlobal | StoreStaticLocal | InitStaticLocal | StoreStaticProperty
        | StoreReflectionStaticProperty | ExternGlobalStore | StoreRefCell | BindRefCellPtr | AdoptRefCellPtr
        | Acquire | Release | Move | Borrow | EnsureOwned | EchoValue | PrintValue | WriteStdout
        | WriteStrStdout | VarDump | PrintR | ThrowException | GeneratorReturn
        | ThrowNamedParameterOverwrite | PtrCheckNonnull => {
            check_count(inst_id, inst, 1, "1")
        }
        LoadDynamicStaticProperty => {
            check_count(inst_id, inst, 1, "1")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Str, "Str")
        }
        StoreDynamicStaticProperty => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Str, "Str")
        }
        ReleaseUnlessAliases => {
            check_count(inst_id, inst, 2, "2")
        }
        MixedTagOf | MixedUnbox | MixedCastBool | MixedCastInt | MixedCastFloat
        | MixedCastString | MixedToHash => {
            check_heap_unary(function, inst_id, inst, IrHeapKind::Mixed, "Heap(Mixed)")
        }
        ArrayUnion => check_binary(
            function,
            inst_id,
            inst,
            IrType::Heap(IrHeapKind::Array),
            "Heap(Array)",
        ),
        HashUnion => check_binary(
            function,
            inst_id,
            inst,
            IrType::Heap(IrHeapKind::Hash),
            "Heap(Hash)",
        ),
        ArrayHashUnion => check_array_hash_union(function, inst_id, inst),
        HashArrayUnion => check_hash_array_union(function, inst_id, inst),
        HashSpread => check_binary(function, inst_id, inst, IrType::Heap(IrHeapKind::Hash), "Heap(Hash)"),
        ArrayLen | ArrayGet | ArrayGetSilent | ArrayIsset | ArrayElemAddr | ArraySet | ArrayPush | ArrayEnsureUnique
        | ArrayToPointerWords
        | ArrayCloneShallow | ArraySetMixedKey | ArrayGetMixedKey
        | ArrayGetMixedKeySilent | ArrayGetMixedKeyForWrite => {
            check_first_heap(function, inst_id, inst, IrHeapKind::Array, "Heap(Array)")
        }
        // ArrayToHash normalizes either supported PHP array storage kind. An in-place PHP array
        // operation can change packed/hash storage without changing the source-level array type.
        ArrayToHash => check_array_to_hash(function, inst_id, inst),
        // Typed fetch-for-write reads require an already integer-coerced key. Mixed-result
        // nested parents instead use the storage-aware runtime reader, which accepts string
        // and boxed keys on an indexed or runtime-promoted receiver.
        ArrayGetForWrite => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Heap(IrHeapKind::Array), "Heap(Array)")?;
            if inst.result_php_type.codegen_repr() == crate::types::PhpType::Mixed {
                let operand = inst.operands[1];
                let actual = function.value(operand)
                    .ok_or(ValidationError::UnknownValue(operand))?.ir_type;
                if matches!(actual, IrType::I64 | IrType::Str
                    | IrType::Heap(IrHeapKind::Mixed | IrHeapKind::Union)) {
                    Ok(())
                } else {
                    Err(ValidationError::OperandTypeMismatch {
                        inst: inst_id, operand, expected: "I64, Str, or boxed key", actual,
                    })
                }
            } else {
                check_operand_type(function, inst_id, inst, 1, IrType::I64, "I64")
            }
        }
        // The hash counterpart of the fetch-for-write read. A nested write can also emit it
        // after ArrayFetchForWrite, with a true immediate to suppress a repeated float-key
        // diagnostic. The key stays in whatever form `hash_get` accepts because lookup
        // normalizes it itself.
        HashGetForWrite => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(function, inst_id, inst, 0, IrType::Heap(IrHeapKind::Hash), "Heap(Hash)")
        }
        LoadArrayElemRefCell | LoadArrayElemRefCellExisting => {
            check_count(inst_id, inst, 2, "2")?;
            let operand = inst.operands[0];
            let actual = function
                .value(operand)
                .ok_or(ValidationError::UnknownValue(operand))?
                .ir_type;
            if matches!(
                actual,
                IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash | IrHeapKind::Mixed)
            ) {
                Ok(())
            } else {
                Err(ValidationError::OperandTypeMismatch {
                    inst: inst_id,
                    operand,
                    expected: "Heap(Array), Heap(Hash), or Heap(Mixed)",
                    actual,
                })
            }
        }
        MixedArrayAppend | OffsetUnset => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(
                function,
                inst_id,
                inst,
                0,
                IrType::Heap(IrHeapKind::Mixed),
                "Heap(Mixed)",
            )
        }
        HashLen | HashGet | HashGetSilent | HashIsset | HashSet | HashAppend | HashEnsureUnique
        | HashCloneShallow | DescriptorArgSet | DescriptorArgKeyExists | HashRefElement => {
            check_first_heap(function, inst_id, inst, IrHeapKind::Hash, "Heap(Hash)")
        }
        ElemRefCell => check_count(inst_id, inst, 2, "2"),
        HashBindRefElement => {
            check_count(inst_id, inst, 3, "3")?;
            check_first_heap(function, inst_id, inst, IrHeapKind::Hash, "Heap(Hash)")
        }
        // `SlotDetach` is the one array op that accepts either storage: it nulls `container[key]`
        // on an indexed array (via `__rt_array_set_refcounted`) or on a hash (via `__rt_hash_set`).
        // This rule is not optional — the match ends in `_ => Ok(())`, so an unlisted op would be
        // validated by *accepting anything*, including a malformed operand list.
        SlotDetach => {
            check_count(inst_id, inst, 2, "2")?;
            let container = inst.operands[0];
            let actual = function
                .value(container)
                .ok_or(ValidationError::UnknownValue(container))?
                .ir_type;
            match actual {
                IrType::Heap(IrHeapKind::Array) | IrType::Heap(IrHeapKind::Hash) => Ok(()),
                _ => Err(ValidationError::OperandTypeMismatch {
                    inst: inst_id,
                    operand: container,
                    expected: "Heap(Array) or Heap(Hash)",
                    actual,
                }),
            }
        }
        IterCurrentValueRef => check_count(inst_id, inst, 1, "1"),
        IterEnd => validate_iter_end(function, inst_id, inst),
        ArrayKeyExists | OffsetExists => check_count_at_least(inst_id, inst, 1, "at least 1"),
        // `DynamicPropUnset` takes the receiver plus the runtime name: `PropUnset` names its
        // property in an immediate, so it takes one operand, and the count is what pins that.
        DynamicPropUnset | DynamicPropRefCell | BindPropRefCell => check_count(inst_id, inst, 2, "2"),
        BufferLen | BufferGet | BufferSet | BufferFree => {
            check_first_heap(function, inst_id, inst, IrHeapKind::Buffer, "Heap(Buffer)")
        }
        BufferResolve => check_heap_unary(function, inst_id, inst, IrHeapKind::Buffer, "Heap(Buffer)"),
        BufferGetResolved => {
            check_count(inst_id, inst, 2, "2")?;
            check_operand_type(function, inst_id, inst, 0, IrType::I64, "I64 buffer descriptor")?;
            check_descriptor_from_buffer_resolve(function, inst_id, inst)?;
            check_operand_type(function, inst_id, inst, 1, IrType::I64, "I64 buffer index")
        }
        BufferSetResolved => {
            check_count(inst_id, inst, 3, "3")?;
            check_operand_type(function, inst_id, inst, 0, IrType::I64, "I64 buffer descriptor")?;
            check_descriptor_from_buffer_resolve(function, inst_id, inst)?;
            check_operand_type(function, inst_id, inst, 1, IrType::I64, "I64 buffer index")
        }
        DynamicObjectNewWithoutConstructorMixed
        | PropGet
        | PropGetForWrite
        | PropInitialized
        | LoadPropRefCell
        | LoadPropRefCellChecked
        | DynamicPropGet
        | DynamicPropSet
        | NullsafePropGet
        | NullsafeMethodCall
        | MethodLookup
        | MethodCall
        | InstanceOf
        | InstanceOfDynamic => {
            check_count_at_least(inst_id, inst, 1, "at least 1")
        }
        PropSet => check_count(inst_id, inst, 2, "2"),
        PropUnset => check_count(inst_id, inst, 1, "1"),
        CallablePtr
        | NormalizeCallable
        | ReflectionMaterialize
        | ReflRowLookup
        | PdoAdapterAddr
        | DynamicClassHasConstructor
        | DynamicClassIsAbstract
        | DynamicPdoStatementClassStatus
        | DynamicPdoCalledClassStatus => check_count(inst_id, inst, 1, "1"),
        DynamicPdoStatementConstructorCall => check_count(inst_id, inst, 3, "3"),
        ReflectionFillMembers | ReflWord | ReflPoolStr | ReflAt => check_count(inst_id, inst, 2, "2"),
        ReflCallableName => check_count(inst_id, inst, 1, "1"),
        ObjectHandleReserve | ObjectHandleUnpend => check_count(inst_id, inst, 0, "0"),
        ObjectHandlePend => check_count(inst_id, inst, 1, "1"),
        DynamicPdoStatementInitialize => check_count(inst_id, inst, 5, "5"),
        RuntimeCall => validate_typed_runtime_call(function, inst_id, inst),
        IterStart => validate_iter_start(function, inst_id, inst),
        _ => Ok(()),
    }
}

/// Requires a single source operand plus valid state, owner, and origin slots.
///
/// The origin slot is only meaningful for a by-reference start: it is the local that republishes
/// the container after growth or a copy-on-write split, so `IterNext` can reload the live table.
/// Rejecting it on a by-value start keeps the metadata from silently describing an iterator that
/// never reloads.
fn validate_iter_start(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 1, "1")?;
    let Some(Immediate::IterStart(metadata)) = inst.immediate.as_ref()
    else {
        return Ok(());
    };
    let by_ref = metadata.is_by_ref();
    let state = metadata.state();
    let owner = metadata.owner();
    let origin = metadata.origin();
    if !valid_iterator_state_slot(function, state) {
        return Err(ValidationError::MissingImmediate {
            inst: inst_id,
            expected: "valid iter_start iterator-state local slot",
        });
    }
    if let Some(slot) = origin {
        if !by_ref {
            return Err(ValidationError::MissingImmediate {
                inst: inst_id,
                expected: "iter_start origin slot only on a by-reference start",
            });
        }
        if !function
            .locals
            .get(slot.as_raw() as usize)
            .is_some_and(|local| local.id == slot)
        {
            return Err(ValidationError::MissingImmediate {
                inst: inst_id,
                expected: "valid iter_start origin local slot",
            });
        }
    }
    let Some(slot) = owner else {
        return Ok(());
    };
    if function.locals.get(slot.as_raw() as usize).is_some_and(|local| {
        local.id == slot
            && local.kind == LocalKind::OwnedTemp
            && local.php_type.codegen_repr() == PhpType::Mixed
            && local.ir_type == IrType::Heap(IrHeapKind::Mixed)
    }) {
        Ok(())
    } else {
        Err(ValidationError::MissingImmediate {
            inst: inst_id,
            expected: "valid iter_start owner local slot",
        })
    }
}

/// Requires operand-free cleanup naming one addressable iterator-state local.
fn validate_iter_end(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 0, "0")?;
    let Some(Immediate::LocalSlot(state)) = inst.immediate else {
        return Ok(());
    };
    if valid_iterator_state_slot(function, state)
        && function.instructions.iter().any(|candidate| {
            candidate.op == Op::IterStart
                && matches!(
                    candidate.immediate.as_ref(),
                    Some(Immediate::IterStart(metadata)) if metadata.state() == state
                )
        })
    {
        Ok(())
    } else {
        Err(ValidationError::MissingImmediate {
            inst: inst_id,
            expected: "valid iter_end iterator-state local slot",
        })
    }
}

/// Returns whether a slot has the frame representation reserved for iterator state.
fn valid_iterator_state_slot(function: &Function, slot: LocalSlotId) -> bool {
    function
        .locals
        .get(slot.as_raw() as usize)
        .is_some_and(|local| {
            local.id == slot
                && local.kind == LocalKind::IteratorState
                && local.php_type.codegen_repr() == PhpType::Iterable
                && local.ir_type == IrType::Heap(IrHeapKind::Iterable)
        })
}

/// Validates the operand/operation correspondence of one fused checked numeric chain.
fn check_checked_numeric_chain(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    let Some(Immediate::CheckedNumericChain(chain)) = inst.immediate.as_ref() else {
        return Ok(());
    };
    check_count(
        inst_id,
        inst,
        chain.operations().len() + 1,
        "one more than its operation count",
    )?;
    for index in 0..inst.operands.len() {
        check_operand_type(function, inst_id, inst, index, IrType::I64, "I64")?;
    }
    Ok(())
}

/// Validates operand and result storage types for typed runtime calls.
fn validate_typed_runtime_call(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    let Some(Immediate::RuntimeCall(target)) = inst.immediate else {
        return Ok(());
    };
    if target == crate::ir::RuntimeCallTarget::ExceptionGuardOwned {
        check_count(inst_id, inst, 2, "owned heap value and insertion token")?;
        check_operand_type(function, inst_id, inst, 1, IrType::I64, "I64")?;
        let operand = inst.operands[0];
        let source = function.value(operand).ok_or(ValidationError::UnknownValue(operand))?;
        if !Ownership::php_type_needs_lifetime_tracking(&source.php_type) {
            return Err(ValidationError::OperandTypeMismatch {
                inst: inst_id, operand, expected: "owned heap value", actual: source.ir_type,
            });
        }
        let token = inst.result.ok_or(ValidationError::InstructionResultMissing(inst_id))?;
        if inst.result_type != IrType::I64 { return Err(ValidationError::ResultTypeMismatch(token)); }
        if function.value(token).ok_or(ValidationError::UnknownValue(token))?.php_type != PhpType::Int {
            return Err(ValidationError::PhpTypeMismatch(token));
        }
        return Ok(());
    }
    if matches!(target, crate::ir::RuntimeCallTarget::ProfiledFunction {
        arguments: crate::ir::RuntimeArgumentLayout::IndexedArray, ..
    }) {
        check_count(inst_id, inst, 1, "one packed argument array")?;
        check_operand_type(function, inst_id, inst, 0, IrType::Heap(IrHeapKind::Array), "Heap(Array)")?;
        let operand = inst.operands[0];
        if function.value(operand).ok_or(ValidationError::UnknownValue(operand))?.php_type
            != PhpType::Array(Box::new(PhpType::Mixed)) {
            return Err(ValidationError::PhpTypeMismatch(operand));
        }
    }
    let signature = target
        .signature()
        .ok_or(ValidationError::UnknownRuntimeCallSignature(inst_id))?;
    match signature {
        crate::ir::RuntimeCallSignature::Fixed { parameters, result } => {
            if inst.operands.len() != parameters.len() {
                return Err(ValidationError::OperandCountMismatch {
                    inst: inst_id,
                    expected: "typed runtime signature",
                    actual: inst.operands.len(),
                    at: runtime_call_location(function, inst, target),
                });
            }
            for (index, expected) in parameters.iter().copied().enumerate() {
                check_operand_type(
                    function,
                    inst_id,
                    inst,
                    index,
                    expected,
                    ir_type_label(expected),
                )?;
            }
            if inst.result_type != result {
                return Err(ValidationError::ResultTypeMismatch(
                    inst.result.expect("typed runtime call must have a result"),
                ));
            }
        }
        crate::ir::RuntimeCallSignature::Polymorphic {
            min_operands,
            max_operands,
        } => {
            let actual = inst.operands.len();
            if actual < min_operands || max_operands.is_some_and(|maximum| actual > maximum) {
                return Err(ValidationError::OperandCountMismatch {
                    inst: inst_id,
                    expected: "registry runtime signature",
                    actual,
                    at: runtime_call_location(function, inst, target),
                });
            }
        }
    }
    Ok(())
}

/// Returns the static diagnostic spelling for one EIR storage type.
fn ir_type_label(ir_type: IrType) -> &'static str {
    match ir_type {
        IrType::I64 => "I64",
        IrType::F64 => "F64",
        IrType::Str => "Str",
        IrType::TaggedScalar => "TaggedScalar",
        IrType::Heap(IrHeapKind::Array) => "Heap(Array)",
        IrType::Heap(IrHeapKind::Hash) => "Heap(Hash)",
        IrType::Heap(IrHeapKind::Object) => "Heap(Object)",
        IrType::Heap(IrHeapKind::Mixed) => "Heap(Mixed)",
        IrType::Heap(IrHeapKind::Iterable) => "Heap(Iterable)",
        IrType::Heap(IrHeapKind::Union) => "Heap(Union)",
        IrType::Heap(IrHeapKind::Buffer) => "Heap(Buffer)",
        IrType::Void => "Void",
    }
}

/// Validates the operand shape for indexed+associative array union.
fn check_array_hash_union(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 2, "2")?;
    check_operand_type(
        function,
        inst_id,
        inst,
        0,
        IrType::Heap(IrHeapKind::Array),
        "Heap(Array)",
    )?;
    check_operand_type(
        function,
        inst_id,
        inst,
        1,
        IrType::Heap(IrHeapKind::Hash),
        "Heap(Hash)",
    )
}

/// Validates the operand shape for associative+indexed array union.
fn check_hash_array_union(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 2, "2")?;
    check_operand_type(
        function,
        inst_id,
        inst,
        0,
        IrType::Heap(IrHeapKind::Hash),
        "Heap(Hash)",
    )?;
    check_operand_type(
        function,
        inst_id,
        inst,
        1,
        IrType::Heap(IrHeapKind::Array),
        "Heap(Array)",
    )
}

/// Validates the single PHP array operand accepted by `ArrayToHash`.
fn check_array_to_hash(
    function: &Function,
    inst_id: InstId,
    inst: &Instruction,
) -> Result<(), ValidationError> {
    check_count(inst_id, inst, 1, "1")?;
    let operand = inst.operands[0];
    let actual = function
        .value(operand)
        .ok_or(ValidationError::UnknownValue(operand))?
        .ir_type;
    if matches!(actual, IrType::Heap(IrHeapKind::Array | IrHeapKind::Hash)) {
        Ok(())
    } else {
        Err(ValidationError::OperandTypeMismatch {
            inst: inst_id,
            operand,
            expected: "Heap(Array)|Heap(Hash)",
            actual,
        })
    }
}

/// Validates one exact operand count.
fn check_count(
    inst_id: InstId,
    inst: &Instruction,
    expected: usize,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    if inst.operands.len() == expected {
        Ok(())
    } else {
        Err(ValidationError::OperandCountMismatch {
            inst: inst_id,
            expected: expected_label,
            actual: inst.operands.len(),
            at: instruction_location(inst),
        })
    }
}

/// Validates a minimum operand count.
fn check_count_at_least(
    inst_id: InstId,
    inst: &Instruction,
    min: usize,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    if inst.operands.len() >= min {
        Ok(())
    } else {
        Err(ValidationError::OperandCountMismatch {
            inst: inst_id,
            expected: expected_label,
            actual: inst.operands.len(),
            at: instruction_location(inst),
        })
    }
}

/// Validates a maximum operand count.
fn check_count_at_most(
    inst_id: InstId,
    inst: &Instruction,
    max: usize,
    expected_label: &'static str,
) -> Result<(), ValidationError> {
    if inst.operands.len() <= max {
        Ok(())
    } else {
        Err(ValidationError::OperandCountMismatch {
            inst: inst_id,
            expected: expected_label,
            actual: inst.operands.len(),
            at: instruction_location(inst),
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
            at: instruction_location(inst),
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
    dominators: &Dominators,
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
            Terminator::Return { value } => {
                validate_return(function, block.id, *value, dominators)?
            }
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
    dominators: &Dominators,
) -> Result<(), ValidationError> {
    validate_branch_args(function, case.target, &case.args)?;
    validate_terminator_uses(function, source_block, &case.args, dominators)
}

/// Validates return terminator type and normal-return compatibility.
///
/// A by-reference-returning function transports a raw reference-cell ADDRESS, not a value of
/// its declared result type, so `PhpType::Pointer` results are accepted there regardless of the
/// declared shape. Lowering obtains that address from `Op::AcquireRefCell`; this check rejects
/// payload-shaped returns even when their type matches the PHP declaration. Payload layout
/// compatibility and address provenance are separate lowering/runtime obligations.
fn validate_return(
    function: &Function,
    block: BlockId,
    value: Option<ValueId>,
    dominators: &Dominators,
) -> Result<(), ValidationError> {
    if matches!(function.return_php_type, PhpType::Never) {
        return Err(ValidationError::NeverFunctionReturns);
    }
    match value {
        Some(value_id) => {
            validate_use(function, value_id, block, None, dominators)?;
            let returned = function
                .value(value_id)
                .ok_or(ValidationError::UnknownValue(value_id))?;
            let actual = returned.ir_type;
            if function.flags.by_ref_return {
                return if actual == IrType::I64 && matches!(returned.php_type, PhpType::Pointer(_)) {
                    Ok(())
                } else {
                    Err(ValidationError::ReturnTypeMismatch {
                        expected: IrType::I64,
                        actual: Some(actual),
                    })
                };
            }
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
    dominators: &Dominators,
) -> Result<(), ValidationError> {
    for value in values {
        validate_use(function, *value, block, None, dominators)?;
    }
    Ok(())
}

/// Validates destination block argument count and type compatibility.
fn validate_branch_args(
    function: &Function,
    target: BlockId,
    args: &[ValueId],
) -> Result<(), ValidationError> {
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
    dominators: &Dominators,
) -> Result<(), ValidationError> {
    let Some(value_ref) = function.value(value) else {
        return Err(ValidationError::UnknownValue(value));
    };
    if value_ref.ir_type == IrType::Void {
        return Err(ValidationError::VoidValueUsed(value));
    }
    // A structured lowering can leave a fully formed continuation unreachable after its only
    // prospective predecessor terminates. Cross-block dominance has no executable meaning there,
    // including when the definition also belongs to a dead continuation. Keep checking IDs, types,
    // and same-block ordering so malformed instructions still fail validation.
    // (An unreachable block carries only itself as a dominator, so the entry does not dominate it.)
    let use_is_unreachable =
        use_block != function.entry && !dominators.dominates(function.entry, use_block);
    let definition_block = match value_ref.def {
        ValueDef::BlockParam { block, .. } | ValueDef::Instruction { block, .. } => block,
    };
    if use_is_unreachable && definition_block != use_block {
        return Ok(());
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
    dominators: &Dominators,
) -> bool {
    match def {
        ValueDef::BlockParam { block, .. } => {
            block == use_block || dominators.dominates(block, use_block)
        }
        ValueDef::Instruction { block, index, .. } if block == use_block => use_inst_index
            .map(|use_index| index < use_index)
            .unwrap_or(true),
        ValueDef::Instruction { block, .. } => dominators.dominates(block, use_block),
    }
}

/// Computes iterative dominator sets in reachable CFG reverse postorder.
///
/// Only predecessors reachable from the entry are intersected. An unreachable
/// block carries no real control flow from the entry, so including it as a
/// predecessor would wrongly shrink a reachable block's dominator set. For example, a
/// loop whose `for.update` is skipped by an unconditional `break` leaves that
/// update block unreachable yet still branching back to the loop header, which
/// would otherwise strip the entry block out of the header's dominators and
/// produce spurious `UseNotDominated` errors for any value the entry defines and
/// a later pass forwards into the loop. Unreachable blocks themselves resolve
/// to `{self}`. Validation skips cross-block dominance for their uses while
/// still rejecting same-block use-before-definition.
fn compute_dominators(function: &Function) -> Dominators {
    let predecessors = compute_predecessors(function);
    let reverse_postorder = reachable_reverse_postorder(function, &predecessors);
    let reachable: HashSet<BlockId> = reverse_postorder
        .iter()
        .map(|index| BlockId::from_raw(*index as u32))
        .collect();
    let block_count = function.blocks.len();
    let all_blocks = full_block_bitset(block_count);
    let entry_index = function.entry.as_raw() as usize;
    let mut dominators: Vec<Vec<u64>> = function
        .blocks
        .iter()
        .enumerate()
        .map(|(index, block)| {
            if index == entry_index || !reachable.contains(&block.id) {
                let mut singleton = empty_block_bitset(block_count);
                set_block_bit(&mut singleton, index);
                singleton
            } else {
                all_blocks.clone()
            }
        })
        .collect();
    let predecessor_indices = reachable_predecessor_indices(function, &predecessors, &reachable);

    #[cfg(test)]
    let mut sweeps = 0;
    let mut changed = true;
    while changed {
        #[cfg(test)]
        {
            sweeps += 1;
        }
        changed = false;
        // Block ids need not follow control flow. Visiting them numerically required one full
        // sweep per backward-numbered edge on a linear CFG; RPO propagates that chain at once.
        for &block_index in reverse_postorder.iter().skip(1) {
            let predecessors = &predecessor_indices[block_index];
            let mut next = predecessors
                .first()
                .map(|index| dominators[*index].clone())
                .unwrap_or_else(|| empty_block_bitset(block_count));
            for pred_index in predecessors.iter().skip(1) {
                bitset_and_assign(&mut next, &dominators[*pred_index]);
            }
            set_block_bit(&mut next, block_index);
            if dominators[block_index] != next {
                dominators[block_index] = next;
                changed = true;
            }
        }
    }
    Dominators {
        bitsets: dominators,
        #[cfg(test)]
        sweeps,
    }
}

/// Converts reachable predecessor block ids into dense block indices for bitset dominator scans.
fn reachable_predecessor_indices(
    function: &Function,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
    reachable: &HashSet<BlockId>,
) -> Vec<Vec<usize>> {
    function
        .blocks
        .iter()
        .map(|block| {
            predecessors
                .get(&block.id)
                .map(|preds| {
                    preds
                        .iter()
                        .filter(|pred| reachable.contains(pred))
                        .map(|pred| pred.as_raw() as usize)
                        .collect()
                })
                .unwrap_or_default()
        })
        .collect()
}

/// Builds a bitset with every block bit set, masking unused bits in the last word.
fn full_block_bitset(block_count: usize) -> Vec<u64> {
    let mut bits = vec![u64::MAX; block_bitset_word_count(block_count)];
    let trailing_bits = block_count % u64::BITS as usize;
    if trailing_bits != 0 {
        if let Some(last) = bits.last_mut() {
            *last = (1_u64 << trailing_bits) - 1;
        }
    }
    bits
}

/// Builds an empty block bitset with enough words for the function block count.
fn empty_block_bitset(block_count: usize) -> Vec<u64> {
    vec![0; block_bitset_word_count(block_count)]
}

/// Returns the number of machine words needed to represent one block bitset.
fn block_bitset_word_count(block_count: usize) -> usize {
    block_count.div_ceil(u64::BITS as usize)
}

/// Marks one block index as present in a dominator bitset.
fn set_block_bit(bits: &mut [u64], block_index: usize) {
    let word = block_index / u64::BITS as usize;
    let bit = block_index % u64::BITS as usize;
    bits[word] |= 1_u64 << bit;
}

/// Intersects a dominator bitset in place with another predecessor bitset.
fn bitset_and_assign(left: &mut [u64], right: &[u64]) {
    for (left_word, right_word) in left.iter_mut().zip(right.iter()) {
        *left_word &= *right_word;
    }
}

/// Dominator sets for one function, kept as the dense per-block bitsets the fixed point
/// already computes.
///
/// The fixed point below produces one `Vec<u64>` bitset per block, indexed by block index;
/// `validate_function_shape` has already proven `blocks[i].id == BlockId::from_raw(i)` before
/// `compute_dominators` runs, so a block id IS its bitset index. The validator only ever asks
/// one question of this structure — "does A dominate B?" — which the bitset answers with one
/// bounds-checked load and a shift.
///
/// This used to be materialized into a `HashMap<BlockId, HashSet<BlockId>>` by walking every
/// (block, candidate) pair, i.e. O(blocks^2) SipHash insertions per validated function, on a
/// path `validate_lowered_module` takes unconditionally in release. The map was never mutated
/// and never iterated: only `definition_dominates_use` read it, one membership test at a time.
struct Dominators {
    /// One dominator bitset per block, addressed by block index.
    bitsets: Vec<Vec<u64>>,
    /// Fixed-point sweeps, retained only for deterministic scheduling regressions.
    #[cfg(test)]
    sweeps: usize,
}

impl Dominators {
    /// Returns whether `candidate` is in `block`'s dominator set.
    ///
    /// Out-of-range ids answer `false` on both sides, which is exactly what the
    /// `HashMap::get(..).map(|set| set.contains(..)).unwrap_or(false)` chain this replaces did:
    /// the map only ever held keys for real blocks, and a set only ever held real block ids
    /// (`set_block_bit` is called only with in-range indices, and the trailing bits of the last
    /// word are masked out of `full_block_bitset`, so a padding bit is never set).
    fn dominates(&self, candidate: BlockId, block: BlockId) -> bool {
        self.bitsets
            .get(block.as_raw() as usize)
            .is_some_and(|bits| block_bit_is_set(bits, candidate.as_raw() as usize))
    }

    /// Materializes the block-id map this analysis used to return.
    ///
    /// Test-only: it exists so the bitset accessor can be differentially compared against the
    /// shape it replaced, on the same function, for every (block, candidate) pair.
    #[cfg(test)]
    fn to_block_id_map(&self, function: &Function) -> HashMap<BlockId, HashSet<BlockId>> {
        let mut map = HashMap::with_capacity(function.blocks.len());
        for (block_index, block) in function.blocks.iter().enumerate() {
            let mut set = HashSet::new();
            for (candidate_index, candidate) in function.blocks.iter().enumerate() {
                if block_bit_is_set(&self.bitsets[block_index], candidate_index) {
                    set.insert(candidate.id);
                }
            }
            map.insert(block.id, set);
        }
        map
    }
}

/// Returns whether a block index is present in a dominator bitset.
fn block_bit_is_set(bits: &[u64], block_index: usize) -> bool {
    let word = block_index / u64::BITS as usize;
    let bit = block_index % u64::BITS as usize;
    bits.get(word)
        .is_some_and(|word_bits| (word_bits & (1_u64 << bit)) != 0)
}

/// Traverses the validator's graph, including its implicit exception-handler edges.
///
/// The optimizer's terminator-only CFG omits these handler edges, so its RPO is not interchangeable
/// with this traversal. Deriving successors from the same predecessor table keeps reachability
/// and dominance on exactly one edge set. The explicit DFS stack also handles deeply nested CFGs
/// without consuming the Rust call stack. Invalid destination ids remain for terminator validation
/// to diagnose, as in the former reachability fixed point.
fn reachable_reverse_postorder(
    function: &Function,
    predecessors: &HashMap<BlockId, Vec<BlockId>>,
) -> Vec<usize> {
    let block_count = function.blocks.len();
    let mut successors = vec![Vec::new(); block_count];
    for (index, block) in function.blocks.iter().enumerate() {
        if let Some(preds) = predecessors.get(&block.id) {
            for pred in preds {
                if let Some(targets) = successors.get_mut(pred.as_raw() as usize) {
                    targets.push(index);
                }
            }
        }
    }
    let entry = function.entry.as_raw() as usize;
    let mut visited = vec![false; block_count];
    visited[entry] = true;
    let mut stack = vec![(entry, 0)];
    let mut postorder = Vec::with_capacity(block_count);
    while let Some((block, next)) = stack.last_mut() {
        if let Some(&target) = successors[*block].get(*next) {
            *next += 1;
            if !visited[target] {
                visited[target] = true;
                stack.push((target, 0));
            }
        } else {
            postorder.push(*block);
            stack.pop();
        }
    }
    postorder.reverse();
    postorder
}

/// Computes the set of blocks reachable from the entry over the same edge set as
/// `compute_predecessors` (terminator successors plus implicit exception-handler
/// edges): a block is reachable when it is the entry or any of its predecessors
/// is reachable. Iterates to a fixed point.
#[cfg(test)]
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

/// Returns true when PHP type metadata can use the given EIR storage type.
fn php_type_compatible(ir_type: IrType, php_type: &PhpType) -> bool {
    let php_type = php_type.codegen_repr();
    IrType::from_php(&php_type) == ir_type
        || matches!((ir_type, php_type), (IrType::I64, PhpType::Void))
}

/// Returns true when ownership is coherent with storage and PHP type metadata.
fn ownership_compatible(ir_type: IrType, php_type: &PhpType, ownership: Ownership) -> bool {
    let php_type = php_type.codegen_repr();
    let tracks_lifetime =
        ir_type.is_refcounted_storage() || Ownership::php_type_needs_lifetime_tracking(&php_type);
    if tracks_lifetime {
        !matches!(ownership, Ownership::NonHeap)
    } else {
        matches!(ownership, Ownership::NonHeap)
    }
}

/// Describes an instruction for a diagnostic: its opcode plus the source position it came from.
///
/// `LoweringError` renders `ValidationError` with `{:?}`, so a validation failure shows only what
/// the value itself carries. An `InstId` alone identifies nothing a reader can act on; the opcode
/// and the PHP `line:col` are what turn a backend gap into a construct they can go and look at.
fn instruction_location(inst: &Instruction) -> Option<String> {
    let span = inst.span?;
    Some(format!("{} at {}:{}", inst.op.name(), span.line, span.col))
}

/// Like [`instruction_location`], and also names the runtime call's target.
///
/// `runtime_call` says nothing about WHICH runtime function disagreed about its operand count,
/// and that is the whole question when one of them fails.
fn runtime_call_location(
    function: &Function,
    inst: &Instruction,
    target: crate::ir::RuntimeCallTarget,
) -> Option<String> {
    let base = instruction_location(inst).unwrap_or_else(|| inst.op.name().to_string());
    // The enclosing function is named for the same reason the checker records a declaration: an
    // `Instruction` carries a `Span` with no file, and after include/autoload expansion a bare
    // `line:col` is shared by every spliced file. The function name is what a reader can find.
    Some(format!("{base} -> {target:?} in {}", function.name))
}

#[cfg(test)]
mod dominator_bitset_tests {
    use super::*;
    use crate::ir::Builder;

    /// Builds a linear CFG whose block ids run against execution order after its entry.
    fn reverse_linear(block_count: usize) -> Function {
        assert!(block_count >= 2);
        let mut function = Function::new("reverse_linear".to_string(), IrType::I64, PhpType::Int);
        {
            let mut builder = Builder::new(&mut function);
            let blocks: Vec<_> = (0..block_count)
                .map(|index| builder.create_named_block(&format!("b{index}"), vec![]))
                .collect();
            builder.set_entry(blocks[0]);
            for index in 0..block_count {
                builder.position_at_end(blocks[index]);
                if index == 1 {
                    let result = builder.emit_const_i64(0);
                    builder.terminate(Terminator::Return { value: Some(result) });
                } else {
                    let next = if index == 0 { block_count - 1 } else { index - 1 };
                    builder.terminate(Terminator::Br { target: blocks[next], args: vec![] });
                }
            }
        }
        function
    }

    /// Independent set-based fixed point, deliberately kept in block-id order as an oracle.
    fn reference_dominator_sets(function: &Function) -> Vec<HashSet<BlockId>> {
        let predecessors = compute_predecessors(function);
        let reachable = reachable_from_entry(function, &predecessors);
        let all: HashSet<_> = function.blocks.iter().map(|block| block.id).collect();
        let mut sets = vec![all; function.blocks.len()];
        sets[function.entry.as_raw() as usize] = HashSet::from([function.entry]);
        loop {
            let mut changed = false;
            for block in &function.blocks {
                if block.id == function.entry {
                    continue;
                }
                let mut preds = predecessors
                    .get(&block.id)
                    .into_iter()
                    .flatten()
                    .filter(|pred| reachable.contains(pred));
                let mut next = preds
                    .next()
                    .map(|pred| sets[pred.as_raw() as usize].clone())
                    .unwrap_or_default();
                for pred in preds {
                    next.retain(|candidate| sets[pred.as_raw() as usize].contains(candidate));
                }
                next.insert(block.id);
                let index = block.id.as_raw() as usize;
                if sets[index] != next {
                    sets[index] = next;
                    changed = true;
                }
            }
            if !changed {
                return sets;
            }
        }
    }

    /// Compares every dominance query, including unknown ids, against the independent oracle.
    fn assert_reference_agrees(function: &Function) {
        let reference = reference_dominator_sets(function);
        let actual = compute_dominators(function);
        for block in &function.blocks {
            for candidate in &function.blocks {
                assert_eq!(
                    actual.dominates(candidate.id, block.id),
                    reference[block.id.as_raw() as usize].contains(&candidate.id),
                    "{}: {:?} dominates {:?}",
                    function.name,
                    candidate.id,
                    block.id
                );
            }
        }
        let absent = BlockId::from_raw(function.blocks.len() as u32);
        assert!(!actual.dominates(absent, function.entry));
        assert!(!actual.dominates(function.entry, absent));
        assert!(!actual.dominates(absent, absent));
    }

    /// Builds small deterministic CFGs, with cycles, dead islands and nonzero entry ids.
    fn generated_cfg(seed: u32, handlers: bool) -> Function {
        let block_count = 1 + seed as usize % 24;
        let mut state = seed.wrapping_add(1);
        let mut next = || {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            state
        };
        let mut function = Function::new(format!("generated_{seed}_{handlers}"), IrType::Void, PhpType::Void);
        {
            let mut builder = Builder::new(&mut function);
            let blocks: Vec<_> = (0..block_count)
                .map(|index| builder.create_named_block(&format!("b{index}"), vec![]))
                .collect();
            let entry = blocks[next() as usize % block_count];
            builder.set_entry(entry);
            builder.position_at_end(entry);
            let cond = builder.emit_const_i64(1);
            for (index, block) in blocks.iter().enumerate() {
                builder.position_at_end(*block);
                if handlers && index % 3 == 0 {
                    let handler = blocks[next() as usize % block_count];
                    builder.emit(
                        Op::TryPushHandler,
                        vec![],
                        Some(Immediate::I64(i64::from(handler.as_raw()))),
                        IrType::Void,
                        PhpType::Void,
                        Ownership::NonHeap,
                    );
                }
                let choice = next();
                let target = blocks[next() as usize % block_count];
                let terminator = match choice % 3 {
                    0 => Terminator::Return { value: None },
                    1 => Terminator::Br { target, args: vec![] },
                    _ => Terminator::CondBr {
                        cond,
                        then_target: target,
                        then_args: vec![],
                        else_target: blocks[next() as usize % block_count],
                        else_args: vec![],
                    },
                };
                builder.terminate(terminator);
            }
        }
        function
    }

    /// Scheduling must not change dominance on reducible or irreducible graphs.
    #[test]
    fn reverse_postorder_matches_set_oracle_on_generated_cfgs() {
        for seed in 0..256 {
            let function = generated_cfg(seed, false);
            assert_reference_agrees(&function);
            validate_function(&function).unwrap();
        }
    }

    /// Implicit exception edges participate in both reachability and dominance, even in cycles.
    #[test]
    fn reverse_postorder_matches_set_oracle_with_exception_edges() {
        for seed in 0..256 {
            assert_reference_agrees(&generated_cfg(seed, true));
        }
    }

    /// Validator work must follow CFG order rather than require one sweep per block id.
    #[test]
    fn reverse_linear_cfg_converges_in_two_sweeps() {
        let function = reverse_linear(256);
        let dominators = compute_dominators(&function);
        assert!(dominators.dominates(BlockId::from_raw(255), BlockId::from_raw(1)));
        assert!(
            dominators.sweeps <= 2,
            "reverse linear CFG required {} sweeps",
            dominators.sweeps
        );
        validate_function(&function).unwrap();
    }

    /// Runs one bounded profiler workload; no wall-clock assertion depends on machine load.
    #[test]
    #[ignore = "manual paired validator scheduling measurement"]
    fn profile_reverse_linear_dominators() {
        let function = reverse_linear(2048);
        let start = std::time::Instant::now();
        let dominators = std::hint::black_box(compute_dominators(&function));
        let elapsed = start.elapsed();
        assert!(dominators.dominates(BlockId::from_raw(2047), BlockId::from_raw(1)));
        eprintln!(
            "validator_profile blocks=2048 sweeps={} elapsed_us={}",
            dominators.sweeps,
            elapsed.as_micros()
        );
    }

    /// Asserts the bitset accessor and the block-id map it replaced answer identically for
    /// every ordered pair of blocks in `function`.
    fn assert_old_and_new_agree(function: &Function) {
        let dominators = compute_dominators(function);
        let map = dominators.to_block_id_map(function);
        for block in &function.blocks {
            for candidate in &function.blocks {
                let old = map
                    .get(&block.id)
                    .map(|set| set.contains(&candidate.id))
                    .unwrap_or(false);
                let new = dominators.dominates(candidate.id, block.id);
                assert_eq!(
                    old, new,
                    "dominates({:?}, {:?}) disagrees with the block-id map",
                    candidate.id, block.id
                );
            }
        }
        // An id past the last block answers false on both sides, as the map lookup did.
        let past_end = BlockId::from_raw(function.blocks.len() as u32);
        assert!(!dominators.dominates(past_end, function.entry));
        assert!(!dominators.dominates(function.entry, past_end));
    }

    /// Builds a four-block diamond: `entry` branches to `then`/`els`, both branch to `merge`.
    fn diamond() -> Function {
        let mut function = Function::new("diamond".to_string(), IrType::I64, PhpType::Int);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            let then_block = builder.create_named_block("then", vec![]);
            let else_block = builder.create_named_block("els", vec![]);
            let merge = builder.create_named_block("merge", vec![]);
            builder.set_entry(entry);

            builder.position_at_end(entry);
            let cond = builder.emit_const_i64(1);
            builder.terminate(Terminator::CondBr {
                cond,
                then_target: then_block,
                then_args: vec![],
                else_target: else_block,
                else_args: vec![],
            });

            builder.position_at_end(then_block);
            builder.terminate(Terminator::Br {
                target: merge,
                args: vec![],
            });

            builder.position_at_end(else_block);
            builder.terminate(Terminator::Br {
                target: merge,
                args: vec![],
            });

            builder.position_at_end(merge);
            let result = builder.emit_const_i64(0);
            builder.terminate(Terminator::Return {
                value: Some(result),
            });
        }
        function
    }

    /// Builds a four-block loop: `entry -> header`, `header -> body | exit`, `body -> header`.
    fn loop_with_backedge() -> Function {
        let mut function = Function::new("looped".to_string(), IrType::I64, PhpType::Int);
        {
            let mut builder = Builder::new(&mut function);
            let entry = builder.create_named_block("entry", vec![]);
            let header = builder.create_named_block("header", vec![]);
            let body = builder.create_named_block("body", vec![]);
            let exit = builder.create_named_block("exit", vec![]);
            builder.set_entry(entry);

            builder.position_at_end(entry);
            builder.terminate(Terminator::Br {
                target: header,
                args: vec![],
            });

            builder.position_at_end(header);
            let cond = builder.emit_const_i64(1);
            builder.terminate(Terminator::CondBr {
                cond,
                then_target: body,
                then_args: vec![],
                else_target: exit,
                else_args: vec![],
            });

            builder.position_at_end(body);
            builder.terminate(Terminator::Br {
                target: header,
                args: vec![],
            });

            builder.position_at_end(exit);
            let result = builder.emit_const_i64(0);
            builder.terminate(Terminator::Return {
                value: Some(result),
            });
        }
        function
    }

    /// Pins the diamond's dominance facts so the differential comparison cannot pass by both
    /// sides being wrong in the same way.
    #[test]
    fn diamond_dominance_facts_are_pinned() {
        let function = diamond();
        let dominators = compute_dominators(&function);
        let entry = function.blocks[0].id;
        let then_block = function.blocks[1].id;
        let else_block = function.blocks[2].id;
        let merge = function.blocks[3].id;
        assert!(dominators.dominates(entry, merge), "entry dominates the merge");
        assert!(dominators.dominates(merge, merge), "dominance is reflexive");
        assert!(
            !dominators.dominates(then_block, merge),
            "one arm never dominates the merge"
        );
        assert!(
            !dominators.dominates(else_block, merge),
            "one arm never dominates the merge"
        );
        assert!(
            !dominators.dominates(merge, entry),
            "a successor never dominates the entry"
        );
    }

    /// The bitset accessor answers exactly what the materialized map answered, on a diamond.
    #[test]
    fn diamond_dominance_matches_the_block_id_map() {
        assert_old_and_new_agree(&diamond());
    }

    /// The bitset accessor answers exactly what the materialized map answered, on a loop whose
    /// back edge exercises the fixed point's intersection step.
    #[test]
    fn loop_dominance_matches_the_block_id_map() {
        let function = loop_with_backedge();
        assert_old_and_new_agree(&function);
        let dominators = compute_dominators(&function);
        let entry = function.blocks[0].id;
        let header = function.blocks[1].id;
        let body = function.blocks[2].id;
        let exit = function.blocks[3].id;
        assert!(
            dominators.dominates(entry, body),
            "the entry dominates the loop body"
        );
        assert!(
            dominators.dominates(header, exit),
            "the header dominates the loop exit"
        );
        assert!(
            !dominators.dominates(body, header),
            "the back edge does not dominate the header"
        );
    }
}

#[cfg(test)]
mod iter_start_metadata_tests {
    use super::*;
    use crate::ir::LocalSlotId;

    /// Builds a one-operand `iter_start` carrying the supplied metadata.
    fn iter_start(
        state: LocalSlotId,
        by_ref: bool,
        owner: Option<LocalSlotId>,
        origin: Option<LocalSlotId>,
    ) -> Instruction {
        Instruction::new(
            Op::IterStart,
            vec![ValueId::from_raw(0)],
            Some(Immediate::IterStart(crate::ir::IterStartMetadata::new(
                state, by_ref, owner, origin,
            ))),
            Some(ValueId::from_raw(1)),
            IrType::Heap(IrHeapKind::Iterable),
            PhpType::Iterable,
            Ownership::MaybeOwned,
            Op::IterStart.default_effects(),
            None,
        )
    }

    /// A function with one ordinary array local that an origin can legitimately name.
    fn function_with_array_local() -> (Function, LocalSlotId, LocalSlotId) {
        let mut function = Function::new("test".to_owned(), IrType::Void, PhpType::Void);
        let origin = function.add_local(
            Some("a".to_owned()),
            IrType::Heap(IrHeapKind::Hash),
            PhpType::Array(Box::new(PhpType::Mixed)),
            LocalKind::PhpLocal,
        );
        let state = function.add_local(
            Some("iter_state".to_owned()),
            IrType::Heap(IrHeapKind::Iterable),
            PhpType::Iterable,
            LocalKind::IteratorState,
        );
        (function, origin, state)
    }

    /// A by-reference start may name any live local as the container it reloads from.
    #[test]
    fn by_reference_origin_naming_a_live_local_is_accepted() {
        let (function, slot, state) = function_with_array_local();
        let inst = iter_start(state, true, None, Some(slot));
        assert_eq!(
            validate_iter_start(&function, InstId::from_raw(0), &inst),
            Ok(())
        );
    }

    /// A by-value start never reloads, so carrying an origin would describe behavior that
    /// the backend does not emit. Rejecting it keeps the metadata honest.
    #[test]
    fn by_value_start_rejects_an_origin_slot() {
        let (function, slot, state) = function_with_array_local();
        let inst = iter_start(state, false, None, Some(slot));
        assert!(matches!(
            validate_iter_start(&function, InstId::from_raw(0), &inst),
            Err(ValidationError::MissingImmediate { .. })
        ));
    }

    /// An origin slot outside the function's local table cannot be reloaded from.
    #[test]
    fn origin_slot_outside_the_local_table_is_rejected() {
        let (function, _, state) = function_with_array_local();
        let inst = iter_start(
            state,
            true,
            None,
            Some(LocalSlotId::from_raw(99)),
        );
        assert!(matches!(
            validate_iter_start(&function, InstId::from_raw(0), &inst),
            Err(ValidationError::MissingImmediate { .. })
        ));
    }

    /// Iterator state must name the dedicated addressable local shape used by codegen.
    #[test]
    fn non_iterator_local_cannot_be_used_as_state() {
        let (function, ordinary_local, _) = function_with_array_local();
        let inst = iter_start(ordinary_local, false, None, None);
        assert!(matches!(
            validate_iter_start(&function, InstId::from_raw(0), &inst),
            Err(ValidationError::MissingImmediate { .. })
        ));
    }

    /// Starts without an origin keep validating exactly as before, by reference or not.
    #[test]
    fn absent_origin_leaves_existing_validation_unchanged() {
        let (function, _, state) = function_with_array_local();
        for by_ref in [false, true] {
            let inst = iter_start(state, by_ref, None, None);
            assert_eq!(
                validate_iter_start(&function, InstId::from_raw(0), &inst),
                Ok(())
            );
        }
    }

    /// The owner rule still applies, and it applies independently of the origin.
    #[test]
    fn owner_slot_is_still_validated_beside_an_origin() {
        let (mut function, slot, state) = function_with_array_local();
        let owner = function.add_local(
            None,
            IrType::Heap(IrHeapKind::Mixed),
            PhpType::Mixed,
            LocalKind::OwnedTemp,
        );
        let accepted = iter_start(state, true, Some(owner), Some(slot));
        assert_eq!(
            validate_iter_start(&function, InstId::from_raw(0), &accepted),
            Ok(())
        );
        // The array local is not an OwnedTemp Mixed slot, so it cannot be a getIterator owner.
        let rejected = iter_start(state, true, Some(slot), Some(slot));
        assert!(matches!(
            validate_iter_start(&function, InstId::from_raw(0), &rejected),
            Err(ValidationError::MissingImmediate { .. })
        ));
    }
}
