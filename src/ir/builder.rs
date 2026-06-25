//! Purpose:
//! Provides the checked construction API for hand-built and lowered EIR.
//!
//! Called from:
//! - Phase 02 tests and future Phase 03 AST-to-EIR lowering.
//!
//! Key details:
//! - The builder assigns value/instruction IDs, prevents emission after a
//!   terminator, and stores conservative effects at instruction creation time.

use crate::ir::block::{BasicBlock, BlockId, Terminator};
use crate::ir::effects::Effects;
use crate::ir::function::{Function, LocalKind, LocalSlotId};
use crate::ir::instr::{Immediate, InstId, Instruction, Op};
use crate::ir::types::IrType;
use crate::ir::value::{Ownership, Value, ValueDef, ValueId};
use crate::span::Span;
use crate::types::PhpType;

/// Mutator API for constructing one EIR function.
pub struct Builder<'f> {
    func: &'f mut Function,
    current: Option<BlockId>,
}

impl<'f> Builder<'f> {
    /// Creates a builder over the provided function.
    pub fn new(func: &'f mut Function) -> Self {
        Self {
            func,
            current: None,
        }
    }

    /// Sets the function entry block.
    pub fn set_entry(&mut self, block: BlockId) {
        self.assert_block_exists(block);
        self.func.entry = block;
    }

    /// Creates a block with an auto-generated `bbN` name and typed parameters.
    pub fn create_block_with_params(&mut self, params: Vec<(IrType, PhpType)>) -> BlockId {
        let name = format!("bb{}", self.func.blocks.len());
        self.create_named_block(name, params)
    }

    /// Creates a block with the requested display name and typed parameters.
    pub fn create_named_block(
        &mut self,
        name: impl Into<String>,
        params: Vec<(IrType, PhpType)>,
    ) -> BlockId {
        let block_id = BlockId::from_raw(self.func.blocks.len() as u32);
        let mut param_values = Vec::with_capacity(params.len());
        for (index, (ir_type, php_type)) in params.into_iter().enumerate() {
            let value_id = ValueId::from_raw(self.func.values.len() as u32);
            self.func.values.push(Value {
                ir_type,
                php_type: php_type.clone(),
                def: ValueDef::BlockParam {
                    block: block_id,
                    index: index as u16,
                },
                ownership: Ownership::for_php_type(&php_type),
            });
            param_values.push(value_id);
        }
        self.func
            .blocks
            .push(BasicBlock::new(block_id, name.into(), param_values));
        block_id
    }

    /// Moves the insertion cursor to the end of a block.
    pub fn position_at_end(&mut self, block: BlockId) {
        self.assert_block_exists(block);
        self.current = Some(block);
    }

    /// Returns one block parameter value by index.
    pub fn block_param(&self, block: BlockId, index: usize) -> ValueId {
        self.func.blocks[block.as_raw() as usize].params[index]
    }

    /// Adds a local slot to the function being built.
    pub fn add_local(
        &mut self,
        name: Option<String>,
        ir_type: IrType,
        php_type: PhpType,
        kind: LocalKind,
    ) -> LocalSlotId {
        self.func.add_local(name, ir_type, php_type, kind)
    }

    /// Widens an existing local slot so its frame storage can hold the incoming PHP type.
    pub fn widen_local_storage_type(&mut self, slot: LocalSlotId, php_type: PhpType) {
        let local = &mut self.func.locals[slot.as_raw() as usize];
        let storage_type = widened_local_storage_type(&local.php_type, &php_type);
        local.ir_type = local_storage_ir_type(&storage_type);
        local.php_type = storage_type;
    }

    /// Returns the current frame storage PHP type for a local slot.
    pub fn local_php_type(&self, slot: LocalSlotId) -> PhpType {
        self.func.locals[slot.as_raw() as usize].php_type.clone()
    }

    /// Returns the semantic role of a local slot.
    pub fn local_kind(&self, slot: LocalSlotId) -> LocalKind {
        self.func.locals[slot.as_raw() as usize].kind
    }

    /// Returns the storage type for a value already emitted in this function.
    pub fn value_type(&self, value: ValueId) -> IrType {
        self.func.values[value.as_raw() as usize].ir_type
    }

    /// Returns the PHP type metadata for a value already emitted in this function.
    pub fn value_php_type(&self, value: ValueId) -> PhpType {
        self.func.values[value.as_raw() as usize].php_type.clone()
    }

    /// Returns the ownership state for a value already emitted in this function.
    pub fn value_ownership(&self, value: ValueId) -> Ownership {
        self.func.values[value.as_raw() as usize].ownership
    }

    /// Returns the opcode that produced an instruction-defined value, if available.
    pub fn value_defining_op(&self, value: ValueId) -> Option<Op> {
        self.value_defining_instruction(value).map(|inst| inst.op)
    }

    /// Returns the instruction that produced an instruction-defined value, if available.
    pub fn value_defining_instruction(&self, value: ValueId) -> Option<&Instruction> {
        let value = self.func.values.get(value.as_raw() as usize)?;
        let ValueDef::Instruction { inst, .. } = value.def else {
            return None;
        };
        self.func.instructions.get(inst.as_raw() as usize)
    }

    /// Returns the current insertion block when one is selected.
    pub fn insertion_block(&self) -> Option<BlockId> {
        self.current
    }

    /// Returns true when the selected block already has a terminator.
    pub fn insertion_block_is_terminated(&self) -> bool {
        self.current
            .map(|block| self.func.blocks[block.as_raw() as usize].terminator.is_some())
            .unwrap_or(false)
    }

    /// Writes the terminator for the current block.
    pub fn terminate(&mut self, term: Terminator) {
        let block = self.current_block();
        let block = self.func.block_mut(block).expect("current block exists");
        assert!(
            block.terminator.is_none(),
            "attempted to replace an existing EIR terminator"
        );
        block.terminator = Some(term);
    }

    /// Emits an instruction using the opcode's default effect set.
    pub fn emit(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        result_type: IrType,
        result_php_type: PhpType,
        result_ownership: Ownership,
    ) -> Option<ValueId> {
        self.emit_with_effects(
            op,
            operands,
            immediate,
            result_type,
            result_php_type,
            result_ownership,
            op.default_effects(),
            None,
        )
    }

    /// Emits an instruction using explicitly supplied effects and optional source span.
    #[allow(clippy::too_many_arguments)]
    pub fn emit_with_effects(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        result_type: IrType,
        result_php_type: PhpType,
        result_ownership: Ownership,
        effects: Effects,
        span: Option<Span>,
    ) -> Option<ValueId> {
        let block_id = self.current_block();
        self.assert_can_append(block_id);
        for operand in &operands {
            self.assert_value_exists(*operand);
        }

        let inst_id = InstId::from_raw(self.func.instructions.len() as u32);
        let block_index = block_id.as_raw() as usize;
        let inst_index_in_block = self.func.blocks[block_index].instructions.len() as u32;
        let result = if result_type.is_void() {
            None
        } else {
            let value_id = ValueId::from_raw(self.func.values.len() as u32);
            self.func.values.push(Value {
                ir_type: result_type,
                php_type: result_php_type.clone(),
                def: ValueDef::Instruction {
                    block: block_id,
                    index: inst_index_in_block,
                    inst: inst_id,
                },
                ownership: result_ownership,
            });
            Some(value_id)
        };

        self.func.instructions.push(Instruction::new(
            op,
            operands,
            immediate,
            result,
            result_type,
            result_php_type,
            result_ownership,
            effects,
            span,
        ));
        self.func.blocks[block_index].instructions.push(inst_id);
        result
    }

    /// Emits an `i64` integer constant.
    pub fn emit_const_i64(&mut self, value: i64) -> ValueId {
        self.emit(
            Op::ConstI64,
            Vec::new(),
            Some(Immediate::I64(value)),
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .expect("const_i64 produces a value")
    }

    /// Emits a boolean constant as an `I64` PHP bool value.
    pub fn emit_const_bool(&mut self, value: bool) -> ValueId {
        self.emit(
            Op::ConstBool,
            Vec::new(),
            Some(Immediate::Bool(value)),
            IrType::I64,
            PhpType::Bool,
            Ownership::NonHeap,
        )
        .expect("const_bool produces a value")
    }

    /// Emits a floating-point constant.
    pub fn emit_const_f64(&mut self, value: f64) -> ValueId {
        self.emit(
            Op::ConstF64,
            Vec::new(),
            Some(Immediate::F64(value)),
            IrType::F64,
            PhpType::Float,
            Ownership::NonHeap,
        )
        .expect("const_f64 produces a value")
    }

    /// Emits a static string literal by data-pool identifier.
    pub fn emit_const_str(&mut self, data_id: crate::ir::module::DataId) -> ValueId {
        self.emit(
            Op::ConstStr,
            Vec::new(),
            Some(Immediate::Data(data_id)),
            IrType::Str,
            PhpType::Str,
            Ownership::Persistent,
        )
        .expect("const_str produces a value")
    }

    /// Emits a null sentinel as an integer storage value.
    pub fn emit_const_null(&mut self) -> ValueId {
        self.emit(
            Op::ConstNull,
            Vec::new(),
            None,
            IrType::I64,
            PhpType::Void,
            Ownership::NonHeap,
        )
        .expect("const_null produces a value")
    }

    /// Emits integer addition.
    pub fn emit_iadd(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        self.emit(
            Op::IAdd,
            vec![lhs, rhs],
            None,
            IrType::I64,
            PhpType::Int,
            Ownership::NonHeap,
        )
        .expect("iadd produces a value")
    }

    /// Emits a local slot load.
    pub fn emit_load_local(
        &mut self,
        slot: crate::ir::function::LocalSlotId,
        ir_type: IrType,
        php_type: PhpType,
    ) -> ValueId {
        let ownership = Ownership::for_php_type(&php_type);
        self.emit(
            Op::LoadLocal,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            ir_type,
            php_type,
            ownership,
        )
        .expect("load_local produces a value")
    }

    /// Emits a local slot store.
    pub fn emit_store_local(&mut self, slot: crate::ir::function::LocalSlotId, value: ValueId) {
        let _ = self.emit(
            Op::StoreLocal,
            vec![value],
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
        );
    }

    /// Returns the current block or panics if no insertion point is active.
    fn current_block(&self) -> BlockId {
        self.current.expect("no EIR block selected for insertion")
    }

    /// Panics if the block identifier does not belong to this function.
    fn assert_block_exists(&self, block: BlockId) {
        assert!(
            (block.as_raw() as usize) < self.func.blocks.len(),
            "unknown EIR block {}",
            block.as_raw()
        );
    }

    /// Panics if the value identifier does not belong to this function.
    fn assert_value_exists(&self, value: ValueId) {
        assert!(
            (value.as_raw() as usize) < self.func.values.len(),
            "unknown EIR value {}",
            value.as_raw()
        );
    }

    /// Panics if an instruction is appended after the current block terminator.
    fn assert_can_append(&self, block: BlockId) {
        let block = &self.func.blocks[block.as_raw() as usize];
        assert!(
            block.terminator.is_none(),
            "attempted to emit an EIR instruction after a terminator"
        );
    }
}

/// Returns the local frame PHP representation that can store both observed types.
fn widened_local_storage_type(current: &PhpType, incoming: &PhpType) -> PhpType {
    let current = current.codegen_repr();
    let incoming = incoming.codegen_repr();
    if current == incoming {
        return current;
    }
    match (&current, &incoming) {
        (current, PhpType::Void | PhpType::Never) if local_storage_can_hold_null(current) => {
            current.clone()
        }
        (PhpType::Array(_), PhpType::Array(_)) => incoming,
        (PhpType::AssocArray { .. }, PhpType::AssocArray { .. }) => incoming,
        (
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Never,
        ) => incoming,
        _ => PhpType::Mixed,
    }
}

/// Returns true when a local storage shape can represent PHP null as a zero pointer.
fn local_storage_can_hold_null(php_type: &PhpType) -> bool {
    matches!(
        php_type,
        PhpType::Array(_)
            | PhpType::AssocArray { .. }
            | PhpType::Object(_)
            | PhpType::Packed(_)
            | PhpType::Mixed
            | PhpType::Union(_)
            | PhpType::Iterable
            | PhpType::Buffer(_)
            | PhpType::Callable
    )
}

/// Returns the IR storage class used for a local slot's PHP representation.
fn local_storage_ir_type(php_type: &PhpType) -> IrType {
    match php_type {
        PhpType::Void | PhpType::Never => IrType::I64,
        other => IrType::from_php(other),
    }
}
