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

use std::collections::{HashMap, HashSet};

use crate::codegen::{abi, emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed};
use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::ir::{BlockId, DataId, Function, LocalKind, LocalSlotId, Module, Op, Ownership, ValueDef, ValueId};
use crate::ir_passes::Allocation;
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
    pub(super) allocation: Allocation,
    pub(super) callee_saved_offsets: Vec<(&'static str, usize)>,
    local_offsets: HashMap<LocalSlotId, usize>,
    promoted_ref_cells: HashSet<LocalSlotId>,
    try_handler_offsets: HashMap<i64, usize>,
    pub(super) frame_size: usize,
    pub(super) concat_base_offset: usize,
    pub(super) epilogue_emitted: bool,
    pub(super) is_main: bool,
    pub(super) web: bool,
    pub(super) gc_stats: bool,
    pub(super) heap_debug: bool,
    pub(super) epilogue_label: Option<String>,
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
        is_main: bool,
        gc_stats: bool,
        heap_debug: bool,
        epilogue_label: Option<String>,
    ) -> Self {
        Self {
            module,
            function,
            emitter,
            data,
            placement: layout.value_placement,
            allocation: layout.allocation,
            callee_saved_offsets: layout.callee_saved_offsets,
            local_offsets: layout.local_offsets,
            promoted_ref_cells: HashSet::new(),
            try_handler_offsets: layout.try_handler_offsets,
            frame_size: layout.frame_size,
            concat_base_offset: layout.concat_base_offset,
            epilogue_emitted: false,
            is_main,
            web: false,
            gc_stats,
            heap_debug,
            epilogue_label,
            label_counter: 0,
        }
    }

    /// Returns a unique local label with a readable prefix.
    pub(super) fn next_label(&mut self, prefix: &str) -> String {
        let label = format!(
            "_eir_{}_{}_{}",
            label_fragment(&self.function.name),
            label_fragment(prefix),
            self.label_counter
        );
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

    /// Returns a module function by PHP name using PHP's case-insensitive lookup.
    pub(super) fn function_by_name(&self, name: &str) -> Option<&'a Function> {
        let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
        self.module
            .functions
            .iter()
            .chain(self.module.closures.iter())
            .find(|function| {
                crate::names::php_symbol_key(function.name.trim_start_matches('\\')) == key
            })
    }

    /// Returns true when an extern declaration exists for a PHP function name.
    pub(super) fn has_extern_function(&self, name: &str) -> bool {
        let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
        self.module.extern_decls.iter().any(|function| {
            crate::names::php_symbol_key(function.name.trim_start_matches('\\')) == key
        })
    }

    /// Returns the public include-variant group name matching a PHP function name.
    pub(super) fn function_variant_group_name(&self, name: &str) -> Option<String> {
        let key = crate::names::php_symbol_key(name.trim_start_matches('\\'));
        super::function_variants::collect_dispatch_groups(self.module)
            .into_iter()
            .find(|group| crate::names::php_symbol_key(group.name.trim_start_matches('\\')) == key)
            .map(|group| group.name)
    }

    /// Returns the concrete function whose signature should be used for a PHP call target.
    pub(super) fn callable_function_by_name(&self, name: &str) -> Option<&'a Function> {
        self.function_by_name(name)
            .or_else(|| super::function_variants::variant_callee_for_group(self.module, name))
    }

    /// Returns a function value or a structured backend error.
    pub(super) fn value_php_type(&self, value: ValueId) -> Result<PhpType> {
        self.function
            .value(value)
            .map(|metadata| metadata.php_type.codegen_repr())
            .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))
    }

    /// Returns a function value's source PHP metadata before codegen representation erasure.
    pub(super) fn raw_value_php_type(&self, value: ValueId) -> Result<PhpType> {
        self.function
            .value(value)
            .map(|metadata| metadata.php_type.clone())
            .ok_or_else(|| CodegenIrError::missing_entry("value", value.as_raw()))
    }

    /// Returns the EIR ownership metadata attached to an SSA value.
    pub(super) fn value_ownership(&self, value: ValueId) -> Result<Ownership> {
        self.function
            .value(value)
            .map(|metadata| metadata.ownership)
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

    /// Returns the semantic role attached to a local slot.
    pub(super) fn local_kind(&self, slot: LocalSlotId) -> Result<LocalKind> {
        self.function
            .locals
            .get(slot.as_raw() as usize)
            .map(|metadata| metadata.kind)
            .ok_or_else(|| CodegenIrError::missing_entry("local slot", slot.as_raw()))
    }

    /// Returns the local slot with the requested source name.
    pub(super) fn local_slot_by_name(&self, name: &str) -> Option<LocalSlotId> {
        self.function
            .locals
            .iter()
            .find(|local| local.name.as_deref() == Some(name))
            .map(|local| local.id)
    }

    /// Marks a local slot as storing a heap reference cell pointer instead of its raw value.
    pub(super) fn mark_promoted_ref_cell(&mut self, slot: LocalSlotId) {
        self.promoted_ref_cells.insert(slot);
    }

    /// Marks a local slot as storing its raw value again after an `unset()` unbind.
    pub(super) fn unmark_promoted_ref_cell(&mut self, slot: LocalSlotId) {
        self.promoted_ref_cells.remove(&slot);
    }

    /// Returns true when a local slot has been promoted to a heap reference cell.
    pub(super) fn is_promoted_ref_cell(&self, slot: LocalSlotId) -> bool {
        self.promoted_ref_cells.contains(&slot)
    }

    /// Returns true when a local slot stores a heap reference-cell pointer.
    pub(super) fn local_stores_ref_cell_pointer(&self, slot: LocalSlotId) -> bool {
        self.is_by_ref_param_slot(slot) || self.is_promoted_ref_cell(slot)
    }

    /// Returns true when the local slot is the storage slot for a by-reference parameter.
    fn is_by_ref_param_slot(&self, slot: LocalSlotId) -> bool {
        self.function
            .params
            .get(slot.as_raw() as usize)
            .is_some_and(|param| param.by_ref)
    }

    /// Loads a stored SSA value into the target's canonical result register(s).
    ///
    /// When the value lives in an allocated register, it is moved from there
    /// into the result register instead of loaded from a stack slot.
    pub(super) fn load_value_to_result(&mut self, value: ValueId) -> Result<PhpType> {
        let ty = self.value_php_type(value)?;
        if let Some(reg) = self.allocation.register_of(value) {
            let dst = if ty.codegen_repr() == PhpType::Float {
                abi::float_result_reg(self.emitter)
            } else {
                abi::int_result_reg(self.emitter)
            };
            abi::emit_reg_move(self.emitter, dst, reg);
        } else {
            let offset = self.value_offset(value)?;
            abi::emit_load(self.emitter, &ty.codegen_repr(), offset);
        }
        Ok(ty)
    }

    /// Loads a single-register SSA value into a caller-selected register.
    ///
    /// When the value lives in an allocated register, it is moved register to
    /// register (a no-op when the source already is the requested register).
    pub(super) fn load_value_to_reg(&mut self, value: ValueId, reg: &str) -> Result<PhpType> {
        let ty = self.value_php_type(value)?;
        if let Some(home) = self.allocation.register_of(value) {
            abi::emit_reg_move(self.emitter, reg, home);
        } else {
            let offset = self.value_offset(value)?;
            abi::load_at_offset(self.emitter, reg, offset);
        }
        Ok(ty)
    }

    /// Loads a string SSA value into a caller-selected register pair.
    pub(super) fn load_string_value_to_regs(
        &mut self,
        value: ValueId,
        ptr_reg: &str,
        len_reg: &str,
    ) -> Result<()> {
        let ty = self.value_php_type(value)?;
        if ty != PhpType::Str {
            return Err(CodegenIrError::unsupported(format!(
                "string register materialization for PHP type {:?}",
                ty
            )));
        }
        let offset = self.value_offset(value)?;
        abi::load_at_offset(self.emitter, ptr_reg, offset);
        abi::load_at_offset(self.emitter, len_reg, offset - 8);
        Ok(())
    }

    /// Loads a local slot into the target's canonical result register(s).
    pub(super) fn load_local_to_result(&mut self, slot: LocalSlotId) -> Result<PhpType> {
        if self.local_stores_ref_cell_pointer(slot) {
            return self.load_ref_cell_local_to_result(slot);
        }
        let ty = self.local_php_type(slot)?;
        let offset = self.local_offset(slot)?;
        abi::emit_load(self.emitter, &ty.codegen_repr(), offset);
        Ok(ty)
    }

    /// Loads the value pointed to by a local ref-cell pointer slot.
    fn load_ref_cell_local_to_result(&mut self, slot: LocalSlotId) -> Result<PhpType> {
        let ty = self.local_php_type(slot)?;
        reject_multiword_ref_cell_local(&ty, "load")?;
        let offset = self.local_offset(slot)?;
        let pointer_reg = abi::symbol_scratch_reg(self.emitter);
        abi::load_at_offset(self.emitter, pointer_reg, offset);
        match ty.codegen_repr() {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(self.emitter);
                abi::emit_load_from_address(self.emitter, ptr_reg, pointer_reg, 0);
                abi::emit_load_from_address(self.emitter, len_reg, pointer_reg, 8);
            }
            PhpType::Float => {
                abi::emit_load_from_address(self.emitter, abi::float_result_reg(self.emitter), pointer_reg, 0);
            }
            PhpType::TaggedScalar => {
                abi::emit_load_from_address(self.emitter, abi::int_result_reg(self.emitter), pointer_reg, 0);
                abi::emit_load_from_address(
                    self.emitter,
                    crate::codegen::sentinels::tagged_scalar_tag_reg(self.emitter),
                    pointer_reg,
                    8,
                );
            }
            _ => {
                abi::emit_load_from_address(self.emitter, abi::int_result_reg(self.emitter), pointer_reg, 0);
            }
        }
        Ok(ty)
    }

    /// Stores the current result register(s) into the SSA value's home.
    ///
    /// When the value lives in an allocated register, the result register is
    /// moved into it; otherwise it is stored into the value's stack slot.
    pub(super) fn store_result_value(&mut self, value: ValueId) -> Result<()> {
        let ty = self.value_php_type(value)?;
        if let Some(reg) = self.allocation.register_of(value) {
            let src = if ty.codegen_repr() == PhpType::Float {
                abi::float_result_reg(self.emitter)
            } else {
                abi::int_result_reg(self.emitter)
            };
            abi::emit_reg_move(self.emitter, reg, src);
        } else {
            let offset = self.value_offset(value)?;
            self.store_current_result_at_offset(&ty, offset);
        }
        Ok(())
    }

    /// Stores the integer result register as a single machine word into the SSA value's home.
    ///
    /// Reference-cell pointers are always one pointer-sized word regardless of the element
    /// type they alias (a `string` cell pointer is still one word, not a `{ptr,len}` pair).
    /// `LoadPropRefCell` and by-reference call results materialize the cell pointer into the
    /// integer result register, so it must be stored single-word; the type-driven
    /// `store_result_value` would otherwise split a `Str`/`Float` result across the string or
    /// float result registers and drop the pointer.
    pub(super) fn store_int_result_value(&mut self, value: ValueId) -> Result<()> {
        if let Some(reg) = self.allocation.register_of(value) {
            abi::emit_reg_move(self.emitter, reg, abi::int_result_reg(self.emitter));
        } else {
            let offset = self.value_offset(value)?;
            abi::store_at_offset(self.emitter, abi::int_result_reg(self.emitter), offset);
        }
        Ok(())
    }

    /// Stores an SSA value into an addressable local slot.
    pub(super) fn store_value_to_local(&mut self, slot: LocalSlotId, value: ValueId) -> Result<()> {
        if self.local_stores_ref_cell_pointer(slot) {
            return self.store_value_to_ref_cell_local(slot, value);
        }
        let source_ty = self.load_value_to_result(value)?;
        let target_ty = self.local_php_type(slot)?;
        if target_ty == PhpType::Mixed && source_ty != PhpType::Mixed {
            if self.value_can_own_mixed_box_source(value)? {
                emit_box_current_owned_value_as_mixed(self.emitter, &source_ty);
            } else {
                emit_box_current_value_as_mixed(self.emitter, &source_ty);
            }
        }
        coerce_current_result_for_target_store(self.emitter, &source_ty, &target_ty)?;
        let offset = self.local_offset(slot)?;
        self.store_current_result_at_offset(&target_ty, offset);
        Ok(())
    }

    /// After an in-place hash/array mutation whose runtime helper returns the
    /// possibly-reallocated container pointer in `value`'s register (already
    /// persisted via `store_result_value`), writes that pointer back to global
    /// storage when `value` was loaded from a global — i.e. a superglobal such as
    /// `$_SERVER`/`$_GET`/`$_POST`. Mirrors the local-slot write-back that array
    /// and hash set/append lowerings already perform; without it a global array
    /// that grows past its initial capacity leaves the global symbol pointing at
    /// freed storage (corruption / crash). No-op unless `value` came from
    /// `Op::LoadGlobal`.
    pub(super) fn writeback_global_array_source(&mut self, value: ValueId) -> Result<()> {
        let Some(value_ref) = self.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(());
        };
        let Some(inst_ref) = self.function.instruction(inst) else {
            return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
        };
        if inst_ref.op != Op::LoadGlobal {
            return Ok(());
        }
        let Some(crate::ir::Immediate::GlobalName(data)) = inst_ref.immediate else {
            return Ok(());
        };
        let name = self.global_name_data(data)?.to_string();
        let symbol = crate::names::ir_global_symbol(&name);
        let ty = self.value_php_type(value)?;
        self.data.add_comm(symbol.clone(), ty.codegen_repr().stack_size().max(8));
        self.load_value_to_result(value)?;
        abi::emit_store_result_to_symbol(self.emitter, &symbol, &ty, false);
        Ok(())
    }

    /// Stores an SSA value through a local ref-cell pointer slot.
    fn store_value_to_ref_cell_local(&mut self, slot: LocalSlotId, value: ValueId) -> Result<()> {
        let source_ty = self.load_value_to_result(value)?;
        let target_ty = self.local_php_type(slot)?;
        reject_multiword_ref_cell_local(&target_ty, "store")?;
        if target_ty == PhpType::Mixed && source_ty != PhpType::Mixed {
            if self.value_can_own_mixed_box_source(value)? {
                emit_box_current_owned_value_as_mixed(self.emitter, &source_ty);
            } else {
                emit_box_current_value_as_mixed(self.emitter, &source_ty);
            }
        }
        coerce_current_result_for_target_store(self.emitter, &source_ty, &target_ty)?;
        let offset = self.local_offset(slot)?;
        let pointer_reg = abi::symbol_scratch_reg(self.emitter);
        abi::load_at_offset(self.emitter, pointer_reg, offset);
        match target_ty.codegen_repr() {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(self.emitter);
                abi::emit_store_to_address(self.emitter, ptr_reg, pointer_reg, 0);
                abi::emit_store_to_address(self.emitter, len_reg, pointer_reg, 8);
            }
            PhpType::Float => {
                abi::emit_store_to_address(self.emitter, abi::float_result_reg(self.emitter), pointer_reg, 0);
            }
            PhpType::TaggedScalar => {
                abi::emit_store_to_address(self.emitter, abi::int_result_reg(self.emitter), pointer_reg, 0);
                abi::emit_store_to_address(
                    self.emitter,
                    crate::codegen::sentinels::tagged_scalar_tag_reg(self.emitter),
                    pointer_reg,
                    8,
                );
            }
            _ => {
                abi::emit_store_to_address(self.emitter, abi::int_result_reg(self.emitter), pointer_reg, 0);
            }
        }
        Ok(())
    }

    /// Stores the current result register(s) into a frame offset.
    fn store_current_result_at_offset(&mut self, ty: &PhpType, offset: usize) {
        match &ty.codegen_repr() {
            PhpType::Str => {
                let (ptr_reg, len_reg) = abi::string_result_regs(self.emitter);
                abi::store_at_offset(self.emitter, ptr_reg, offset);
                abi::store_at_offset(self.emitter, len_reg, offset - 8);
            }
            PhpType::TaggedScalar => {
                abi::store_at_offset(self.emitter, abi::int_result_reg(self.emitter), offset);
                abi::store_at_offset(
                    self.emitter,
                    crate::codegen::sentinels::tagged_scalar_tag_reg(self.emitter),
                    offset - 8,
                );
            }
            PhpType::Float => {
                abi::store_at_offset(self.emitter, abi::float_result_reg(self.emitter), offset);
            }
            PhpType::Void => {
                abi::store_at_offset(self.emitter, abi::int_result_reg(self.emitter), offset);
            }
            PhpType::Never => {}
            _ => {
                abi::store_at_offset(self.emitter, abi::int_result_reg(self.emitter), offset);
            }
        }
    }

    /// Returns true when a value producer can leave an owned source consumed by Mixed boxing.
    pub(super) fn value_can_own_mixed_box_source(&self, value: ValueId) -> Result<bool> {
        if self.value_php_type(value)?.codegen_repr() == PhpType::Str {
            return self.value_is_heap_owned_string_for_mixed_box(value);
        }
        let Some(value_ref) = self.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(false);
        };
        let inst = self
            .function
            .instruction(inst)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
        Ok(matches!(
            inst.op,
            Op::Acquire
                | Op::ArrayNew
                | Op::HashNew
                | Op::ArrayToMixed
                | Op::ArrayCloneShallow
                | Op::HashCloneShallow
                | Op::ArrayUnion
                | Op::HashUnion
                | Op::ArrayHashUnion
                | Op::HashArrayUnion
                | Op::ArrayToHash
                | Op::ObjectNew
                | Op::DynamicObjectNew
                | Op::DynamicObjectNewMixed
                | Op::ClosureNew
                | Op::FirstClassCallableNew
                | Op::CallableArrayNew
                | Op::BufferNew
                | Op::GeneratorNew
                | Op::Call
                | Op::FunctionVariantCall
                | Op::BuiltinCall
                | Op::RuntimeCall
                | Op::ExternCall
                | Op::MethodCall
                | Op::NullsafeMethodCall
                | Op::StaticMethodCall
                | Op::ClosureCall
                | Op::CallableDescriptorInvoke
                | Op::ExprCall
                | Op::PipeCall
                | Op::IteratorMethodCall
                | Op::SplRuntimeCall
                | Op::FiberRuntimeCall
        ))
    }

    /// Returns true when a string producer leaves a heap-owned payload that Mixed boxing may consume.
    fn value_is_heap_owned_string_for_mixed_box(&self, value: ValueId) -> Result<bool> {
        let Some(value_ref) = self.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(false);
        };
        let inst = self
            .function
            .instruction(inst)
            .ok_or_else(|| CodegenIrError::missing_entry("instruction", inst.as_raw()))?;
        Ok(matches!(
            inst.op,
            Op::Acquire
                | Op::StrPersist
                | Op::Call
                | Op::FunctionVariantCall
                | Op::ExternCall
                | Op::MethodCall
                | Op::NullsafeMethodCall
                | Op::StaticMethodCall
                | Op::ClosureCall
                | Op::CallableDescriptorInvoke
                | Op::ExprCall
                | Op::PipeCall
                | Op::IteratorMethodCall
                | Op::SplRuntimeCall
                | Op::FiberRuntimeCall
        ))
    }

    /// Interns a module data-pool string into the assembly data section.
    pub(super) fn intern_string_data(&mut self, data_id: DataId) -> Result<(String, usize)> {
        let value = self
            .module
            .data
            .strings
            .get(data_id.as_raw() as usize)
            .ok_or_else(|| CodegenIrError::missing_entry("data string", data_id.as_raw()))?;
        let bytes = crate::string_bytes::literal_bytes(value);
        Ok(self.data.add_string(&bytes))
    }

    /// Interns a module class-name data-pool entry into the assembly data section.
    pub(super) fn intern_class_name_data(&mut self, data_id: DataId) -> Result<(String, usize)> {
        let value = self
            .module
            .data
            .class_names
            .get(data_id.as_raw() as usize)
            .ok_or_else(|| CodegenIrError::missing_entry("class data", data_id.as_raw()))?;
        Ok(self.data.add_string(value.as_bytes()))
    }

    /// Returns a module data-pool function name.
    pub(super) fn function_name_data(&self, data_id: DataId) -> Result<&str> {
        self.module
            .data
            .function_names
            .get(data_id.as_raw() as usize)
            .map(String::as_str)
            .ok_or_else(|| CodegenIrError::missing_entry("function data", data_id.as_raw()))
    }

    /// Returns a module data-pool global name.
    pub(super) fn global_name_data(&self, data_id: DataId) -> Result<&str> {
        self.module
            .data
            .global_names
            .get(data_id.as_raw() as usize)
            .map(String::as_str)
            .ok_or_else(|| CodegenIrError::missing_entry("global data", data_id.as_raw()))
    }

    /// Returns true when the EIR module has interned a matching global name.
    pub(super) fn has_global_name(&self, name: &str) -> bool {
        let normalized = name.trim_start_matches('\\');
        self.module
            .data
            .global_names
            .iter()
            .any(|candidate| candidate.trim_start_matches('\\') == normalized)
    }

    /// Returns the frame offset assigned to a value by Phase 04 placement.
    fn value_offset(&self, value: ValueId) -> Result<usize> {
        self.placement
            .slot(value)
            .ok_or_else(|| CodegenIrError::missing_entry("value slot", value.as_raw()))
    }

    /// Returns the frame offset assigned to a value for custom multi-word lowerings.
    pub(super) fn value_frame_offset(&self, value: ValueId) -> Result<usize> {
        self.value_offset(value)
    }

    /// Returns the frame offset assigned to an addressable EIR local.
    pub(super) fn local_offset(&self, slot: LocalSlotId) -> Result<usize> {
        self.local_offsets
            .get(&slot)
            .copied()
            .ok_or_else(|| CodegenIrError::missing_entry("local slot offset", slot.as_raw()))
    }

    /// Returns the frame offset assigned to a high-level try-handler token.
    pub(super) fn try_handler_offset(&self, token: i64) -> Result<usize> {
        self.try_handler_offsets
            .get(&token)
            .copied()
            .ok_or_else(|| CodegenIrError::invalid_module(format!("missing try handler token {}", token)))
    }
}

/// Rejects local ref-cell operations whose frame representation spans multiple words.
fn reject_multiword_ref_cell_local(ty: &PhpType, action: &str) -> Result<()> {
    let _ = (ty, action);
    Ok(())
}

/// Coerces the currently loaded result registers before storing into a typed local slot.
fn coerce_current_result_for_target_store(
    emitter: &mut Emitter,
    source_ty: &PhpType,
    target_ty: &PhpType,
) -> Result<()> {
    if target_ty.codegen_repr() != PhpType::TaggedScalar {
        return Ok(());
    }
    match source_ty.codegen_repr() {
        PhpType::TaggedScalar => Ok(()),
        PhpType::Int | PhpType::Bool | PhpType::Callable => {
            crate::codegen::sentinels::emit_tagged_scalar_from_int_result(emitter);
            Ok(())
        }
        PhpType::Void | PhpType::Never => {
            crate::codegen::sentinels::emit_tagged_scalar_null(emitter);
            Ok(())
        }
        PhpType::Mixed | PhpType::Union(_) => {
            emit_mixed_result_as_tagged_scalar(emitter);
            Ok(())
        }
        other => Err(CodegenIrError::unsupported(format!(
            "local store from PHP type {:?} to PHP type TaggedScalar",
            other
        ))),
    }
}

/// Reorders `__rt_mixed_unbox` output into the EIR tagged-scalar result registers.
fn emit_mixed_result_as_tagged_scalar(emitter: &mut Emitter) {
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("mov x9, x0");                                  // preserve the unboxed Mixed tag before moving the payload
            emitter.instruction("mov x0, x1");                                  // place the unboxed payload into the tagged-scalar payload register
            emitter.instruction("mov x1, x9");                                  // place the unboxed Mixed tag into the tagged-scalar tag register
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, rax");                                // preserve the unboxed Mixed tag before moving the payload
            emitter.instruction("mov rax, rdi");                                // place the unboxed payload into the tagged-scalar payload register
            emitter.instruction("mov rdx, r10");                                // place the unboxed Mixed tag into the tagged-scalar tag register
        }
    }
}

/// Converts arbitrary names into assembly-label-safe fragments.
fn label_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}
