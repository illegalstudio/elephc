//! Purpose:
//! Holds per-function state while the EIR backend lowers SSA instructions to assembly.
//! Provides table lookups, value-slot loads/stores, data-pool access, and label creation.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit`, `crate::codegen_ir::lower_inst`, and
//!   `crate::codegen_ir::lower_term`.
//!
//! Key details:
//! - Phase 04 stores every SSA value in a stack slot and reloads result registers at use sites.
//! - The context delegates target-specific movement to `crate::codegen::abi`.

use std::collections::HashMap;

use crate::codegen::abi;
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::ir::{BlockId, DataId, Function, LocalSlotId, Module, ValueId};
use crate::types::PhpType;

use super::frame::FrameLayout;
use super::value_placement::ValuePlacement;
use super::{CodegenIrError, Result};

/// Mutable backend state for one EIR function.
pub(super) struct FunctionContext<'a> {
    pub(super) module: &'a Module,
    pub(super) function: &'a Function,
    pub(super) emitter: &'a mut Emitter,
    pub(super) data: &'a mut DataSection,
    pub(super) placement: ValuePlacement,
    local_offsets: HashMap<LocalSlotId, usize>,
    pub(super) frame_size: usize,
    pub(super) epilogue_emitted: bool,
    label_counter: usize,
}

impl<'a> FunctionContext<'a> {
    /// Creates a lowering context with finalized frame and value-placement metadata.
    pub(super) fn new(
        module: &'a Module,
        function: &'a Function,
        emitter: &'a mut Emitter,
        data: &'a mut DataSection,
        layout: FrameLayout,
    ) -> Self {
        Self {
            module,
            function,
            emitter,
            data,
            placement: layout.value_placement,
            local_offsets: layout.local_offsets,
            frame_size: layout.frame_size,
            epilogue_emitted: false,
            label_counter: 0,
        }
    }

    /// Returns a unique local label with a readable prefix.
    pub(super) fn next_label(&mut self, prefix: &str) -> String {
        let label = format!("_eir_{}_{}", prefix, self.label_counter);
        self.label_counter += 1;
        label
    }

    /// Returns the assembly label for a non-entry EIR block.
    pub(super) fn block_label(&self, block_name: &str, raw: u32) -> String {
        format!("_eir_{}_{}_{}", label_fragment(&self.function.name), label_fragment(block_name), raw)
    }

    /// Returns the assembly label for a block id.
    pub(super) fn block_label_for_id(&self, block: BlockId) -> Result<String> {
        let block = self
            .function
            .block(block)
            .ok_or_else(|| CodegenIrError::missing_entry("block", block.as_raw()))?;
        Ok(self.block_label(&block.name, block.id.as_raw()))
    }

    /// Returns a function value or a structured backend error.
    pub(super) fn value_php_type(&self, value: ValueId) -> Result<PhpType> {
        self.function
            .value(value)
            .map(|metadata| metadata.php_type.codegen_repr())
            .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))
    }

    /// Returns the runtime PHP type stored in a local slot.
    pub(super) fn local_php_type(&self, slot: LocalSlotId) -> Result<PhpType> {
        self.function
            .locals
            .get(slot.as_raw() as usize)
            .map(|metadata| metadata.php_type.codegen_repr())
            .ok_or_else(|| CodegenIrError::missing_entry("local slot", slot.as_raw()))
    }

    /// Loads a stored SSA value into the target's canonical result register(s).
    pub(super) fn load_value_to_result(&mut self, value: ValueId) -> Result<PhpType> {
        let ty = self.value_php_type(value)?;
        let offset = self.value_offset(value)?;
        abi::emit_load(self.emitter, &ty, offset);
        Ok(ty)
    }

    /// Loads a single-register SSA value into a caller-selected register.
    pub(super) fn load_value_to_reg(&mut self, value: ValueId, reg: &str) -> Result<PhpType> {
        let ty = self.value_php_type(value)?;
        let offset = self.value_offset(value)?;
        abi::load_at_offset(self.emitter, reg, offset);
        Ok(ty)
    }

    /// Loads a local slot into the target's canonical result register(s).
    pub(super) fn load_local_to_result(&mut self, slot: LocalSlotId) -> Result<PhpType> {
        let ty = self.local_php_type(slot)?;
        let offset = self.local_offset(slot)?;
        abi::emit_load(self.emitter, &ty, offset);
        Ok(ty)
    }

    /// Stores the current result register(s) into the SSA value's fixed stack slot.
    pub(super) fn store_result_value(&mut self, value: ValueId) -> Result<()> {
        let ty = self.value_php_type(value)?;
        let offset = self.value_offset(value)?;
        self.store_current_result_at_offset(&ty, offset);
        Ok(())
    }

    /// Stores an SSA value into an addressable local slot.
    pub(super) fn store_value_to_local(&mut self, slot: LocalSlotId, value: ValueId) -> Result<()> {
        let ty = self.load_value_to_result(value)?;
        let offset = self.local_offset(slot)?;
        self.store_current_result_at_offset(&ty, offset);
        Ok(())
    }

    /// Stores the current result register(s) into a frame offset.
    fn store_current_result_at_offset(&mut self, ty: &PhpType, offset: usize) {
        match &ty {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(self.emitter);
                abi::store_at_offset(self.emitter, ptr_reg, offset);
                abi::store_at_offset(self.emitter, len_reg, offset - 8);
            }
            PhpType::Float => {
                abi::store_at_offset(self.emitter, abi::float_result_reg(self.emitter), offset);
            }
            PhpType::Void | PhpType::Never => {}
            _ => {
                abi::store_at_offset(self.emitter, abi::int_result_reg(self.emitter), offset);
            }
        }
    }

    /// Interns a module data-pool string into the assembly data section.
    pub(super) fn intern_string_data(&mut self, data_id: DataId) -> Result<(String, usize)> {
        let value = self
            .module
            .data
            .strings
            .get(data_id.as_raw() as usize)
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data_id.as_raw()))?;
        Ok(self.data.add_string(value.as_bytes()))
    }

    /// Returns the frame offset assigned to a value by Phase 04 placement.
    fn value_offset(&self, value: ValueId) -> Result<usize> {
        self.placement
            .slot(value)
            .ok_or_else(|| CodegenIrError::missing_entry("value slot", value.as_raw()))
    }

    /// Returns the frame offset assigned to an addressable EIR local.
    pub(super) fn local_offset(&self, slot: LocalSlotId) -> Result<usize> {
        self.local_offsets
            .get(&slot)
            .copied()
            .ok_or_else(|| CodegenIrError::missing_entry("local slot offset", slot.as_raw()))
    }
}

/// Converts arbitrary names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
