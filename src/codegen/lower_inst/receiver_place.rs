//! Purpose:
//! Resolves where a mutating array/hash builtin's by-reference receiver has to be written back,
//! and republishes a possibly-relocated container pointer into that place.
//!
//! Called from:
//! - `crate::codegen::lower_inst::arrays` (managed associative element references).
//! - `crate::codegen::lower_inst::builtins::arrays` (`array_pop`, `array_shift`, `array_unshift`,
//!   `array_splice`, the sort/shuffle family, `array_multisort`, the hash link sorters).
//! - `crate::codegen::lower_inst::hashes` (`hash_set`).
//!
//! Key details:
//! - Every mutating container builtin copy-on-write splits its receiver first, and a split
//!   RELOCATES the storage. So does any growth path that reaches `__rt_array_grow`. The new
//!   pointer has to reach the place the value was READ from, or the caller keeps pointing at
//!   storage that was already freed.
//! - A plain local is its own frame slot. A by-reference parameter is read with `load_ref_cell`
//!   and must be republished through that slot's ref-cell representation
//!   (`store_value_through_ref_cell_slot`), which is exactly what a bare `store_value_to_local`
//!   on a raw frame slot would skip. A direct declared-property receiver preserves the object SSA
//!   value through ownership and packed-to-hash transitions and republishes into its fixed slot.

use crate::codegen::context::FunctionContext;
use crate::codegen::{CodegenIrError, Result};
use crate::codegen_support::abi;
use crate::ir::{Immediate, LocalSlotId, Op, ValueDef, ValueId};
use crate::types::PhpType;

/// Where a mutating container builtin's receiver has to be written back.
#[derive(Clone)]
pub(super) enum ReceiverPlace {
    /// The receiver was not loaded from a slot this lowering can write back to.
    Opaque,
    /// A plain local frame slot.
    Local(LocalSlotId),
    /// A slot whose value is reached through its ref-cell representation.
    RefCell(LocalSlotId),
    /// A declared object property whose runtime container owner may be replaced by COW.
    Property {
        object: ValueId,
        slot: super::objects::PropertySlot,
    },
    /// Program-global storage a `global $x` somewhere gave this name: the `_eir_global_*` symbol.
    ///
    /// The symbol's word holds the container pointer itself, at `php_type`, and owns that
    /// container (element writes reject a Mixed receiver before reaching this place). A mutating
    /// builtin works on the container in place, so the write-back only has to republish when a
    /// helper handed back a DIFFERENT pointer — a growth or a copy-on-write split — and whether
    /// the previous pointer is then retired depends on the helper's convention
    /// (`RefCellStorePrevious`).
    Global {
        symbol: String,
        php_type: PhpType,
    },
}

impl ReceiverPlace {
    /// Resolves the writable place a receiver value was loaded from, if any.
    ///
    /// Local loads resolve directly. A declared-property read also resolves through the explicit
    /// retain and optional packed-to-hash conversion emitted by the direct `krsort` write context.
    /// Calls, globals, dynamic properties, and arbitrary expressions remain opaque.
    pub(super) fn resolve(ctx: &FunctionContext<'_>, value: ValueId) -> Result<Self> {
        let Some(value_ref) = ctx.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(Self::Opaque);
        };
        let Some(inst_ref) = ctx.function.instruction(inst) else {
            return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
        };
        if let Some(Immediate::LocalSlot(slot)) = inst_ref.immediate {
            return match inst_ref.op {
                Op::LoadLocal => Ok(Self::Local(slot)),
                Op::LoadRefCell => Ok(Self::RefCell(slot)),
                _ => Ok(Self::Opaque),
            };
        }
        if inst_ref.op == Op::LoadGlobal {
            if let Some(Immediate::GlobalName(data)) = inst_ref.immediate {
                let name = ctx.global_name_data(data)?.to_string();
                return Ok(Self::Global {
                    symbol: crate::names::ir_global_symbol(&name),
                    php_type: ctx.value_php_type(value)?,
                });
            }
        }
        match direct_property_receiver(ctx, value)? {
            Some((object, slot)) => Ok(Self::Property { object, slot }),
            None => Ok(Self::Opaque),
        }
    }

    /// Returns the resolved slot, whichever representation it is reached through.
    ///
    /// Used by the pre-mutation bookkeeping (`release_mutated_source_local_owner`) that only
    /// needs to name the slot; the representation choice belongs to the write-back.
    pub(super) fn slot(&self) -> Option<LocalSlotId> {
        match self {
            Self::Opaque => None,
            Self::Local(slot) | Self::RefCell(slot) => Some(*slot),
            Self::Property { .. } | Self::Global { .. } => None,
        }
    }

    /// Rejects a receiver this lowering could not resolve to a writable slot.
    ///
    /// Only calls that RELOCATE the receiver need this: a mutation that stays inside the existing
    /// payload is correct even for a receiver whose place is opaque. A growth reallocates, and a
    /// grown container lives somewhere else, so a receiver with nowhere to publish the new pointer
    /// must be refused instead of silently dropping the mutation.
    pub(super) fn require_writable(&self, what: &str) -> Result<()> {
        match self {
            Self::Opaque => Err(CodegenIrError::unsupported(format!(
                "{} for a by-reference receiver that is not a local variable slot",
                what
            ))),
            _ => Ok(()),
        }
    }

    /// Gives a consuming COW helper the owner it is allowed to retire.
    ///
    /// A raw local transfers its existing slot owner into the helper. A ref-cell load is only a
    /// borrowed view of storage whose previous owner is retired during write-back, so it needs a
    /// separate helper owner before the call. Acquiring that owner also forces the helper to split
    /// a sole stored value, avoiding same-pointer publication followed by retirement. A direct
    /// property receiver already reaches this layer through an owning `Acquire`; its normal EIR
    /// cleanup retires the replacement transient after the property store retains it. Concrete
    /// containers unboxed from a Mixed raw local already carry a detached owner; releasing the
    /// superseded box transfers that owner into the helper.
    pub(super) fn prepare_consuming_storeback(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<()> {
        match self {
            Self::Local(slot) => ctx.release_mutated_source_local_owner(*slot, value),
            Self::RefCell(_) => {
                let value_ty = ctx.load_value_to_result(value)?;
                abi::emit_incref_if_refcounted(ctx.emitter, &value_ty);
                Ok(())
            }
            // The symbol keeps its own owner of the cell and the helpers mutate the cell in
            // place, so there is no separate owner to hand over here.
            Self::Opaque | Self::Property { .. } | Self::Global { .. } => Ok(()),
        }
    }

    /// Re-prepares a receiver for a second consuming helper after a publish.
    ///
    /// A raw Mixed-widened local's publish moved the load's owner into the slot's new box, so a
    /// plain second `prepare_consuming_storeback` would drop the container with the box; see
    /// `FunctionContext::retake_mutated_source_local_owner`. Every other receiver kind's prepare
    /// is balanced per publish and repeats unchanged.
    pub(super) fn reprepare_consuming_storeback(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<()> {
        if let Self::Local(slot) = self {
            return ctx.retake_mutated_source_local_owner(*slot, value);
        }
        self.prepare_consuming_storeback(ctx, value)
    }

    /// Publishes a receiver a helper will hand to user code, deferring a raw local's retain.
    ///
    /// Same as `store_back_value`, except that a raw Mixed-widened local takes the value's owner
    /// instead of retaining, and the retain is owed after the helper returns — see
    /// `FunctionContext::store_receiver_value_to_local_before_callback`. Returns whether the
    /// caller must emit `retain_receiver_after_callback` once the helper is back.
    pub(super) fn store_back_value_before_callback(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<bool> {
        if let Self::Local(slot) = self {
            return ctx.store_receiver_value_to_local_before_callback(*slot, value);
        }
        self.store_back_value(ctx, value)?;
        Ok(false)
    }

    /// Reloads a local-backed receiver from the place that owns its current value.
    ///
    /// EIR values preserve source evaluation order, so a receiver load may precede an earlier
    /// mutation that copy-on-write splits and republishes the same local. A later mutating use
    /// must not keep using that stale SSA snapshot. Re-materializing the ordinary local load,
    /// including its storage coercion, makes the published slot authoritative before another
    /// consuming helper probes or mutates the container.
    pub(super) fn reload_local_value(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<()> {
        let slot = match self {
            Self::Local(slot) | Self::RefCell(slot) => *slot,
            Self::Global { symbol, php_type } => {
                abi::emit_load_symbol_to_result(ctx.emitter, symbol, php_type);
                return ctx.store_result_value(value);
            }
            Self::Opaque | Self::Property { .. } => return Ok(()),
        };
        let source_ty = ctx.load_local_to_result(slot)?;
        let result_ty = ctx.value_php_type(value)?;
        super::coerce_loaded_local_to_result_type(ctx, &source_ty, &result_ty)?;
        ctx.store_result_value(value)
    }

    /// Publishes the receiver's current pointer back into the place it was read from.
    pub(super) fn store_back(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
        value_php_type: &PhpType,
    ) -> Result<()> {
        match self {
            Self::Opaque => Ok(()),
            Self::Local(slot) => ctx.store_receiver_value_to_local(*slot, value),
            Self::RefCell(slot) => super::store_value_through_ref_cell_slot(
                ctx,
                *slot,
                value,
                value_php_type,
                crate::codegen::lower_inst::local_stores::RefCellStorePrevious::Retire,
            ),
            Self::Property { object, slot } => {
                super::objects::store_mutated_container_property_owner(ctx, *object, slot, value)
            }
            Self::Global { symbol, php_type } => emit_global_receiver_store_back(
                ctx,
                symbol,
                php_type,
                value,
                crate::codegen::lower_inst::local_stores::RefCellStorePrevious::Retire,
            ),
        }
    }

    /// Republishes a container an element-write helper may have relocated.
    ///
    /// `__rt_hash_set`, `__rt_hash_unset`, `__rt_hash_append`, `__rt_hash_spread` and the
    /// element-cell helpers split under the `ensure_unique` convention: a copy-on-write split drops
    /// the mutator's own owner and a growth frees the block it replaced. A raw or global-backed
    /// receiver therefore only PUBLISHES the pointer handed back — retiring the previous occupant
    /// as `store_back_value` does for the mutating builtins released the pre-growth table a second
    /// time. That is how `$_GET` with more than sixteen parameters crashed the web handler: its
    /// population loop grew the table through this write-back, the decref of the old block freed
    /// storage the allocator had already handed to a key string, and the request-end reset then
    /// walked that string as a hash. A ref-cell receiver keeps its retirement because
    /// `prepare_consuming_storeback` gave the helper a separate owner for exactly that release.
    pub(super) fn store_back_container_writeback(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<()> {
        match self {
            Self::Opaque => Ok(()),
            Self::Local(slot) => ctx.store_container_writeback_to_local(*slot, value),
            Self::RefCell(_) | Self::Property { .. } => self.store_back_value(ctx, value),
            Self::Global { symbol, php_type } => emit_global_receiver_store_back(
                ctx,
                symbol,
                php_type,
                value,
                crate::codegen::lower_inst::local_stores::RefCellStorePrevious::Keep,
            ),
        }
    }

    /// Publishes the receiver back using the PHP type EIR recorded for the receiver value.
    ///
    /// The convenience form for the mutating builtins whose receiver keeps its declared container
    /// type across the call, which is all of them: a copy-on-write split or a growth changes the
    /// address, never the element representation.
    pub(super) fn store_back_value(
        &self,
        ctx: &mut FunctionContext<'_>,
        value: ValueId,
    ) -> Result<()> {
        if matches!(self, Self::Opaque) {
            return Ok(());
        }
        let value_ty = ctx.value_php_type(value)?;
        self.store_back(ctx, value, &value_ty)
    }
}

/// Republishes a global-backed receiver only when a helper handed back a different cell.
///
/// The symbol already owns the cell the builtin mutated in place; storing the same pointer again
/// would change nothing, and retiring the "previous" occupant would free the very cell being
/// published. A replacement cell takes the symbol's owner slot; whether the old cell is released
/// here (`Retire`, the mutating builtins' convention) or was already dropped by the helper
/// (`Keep`, the element-write helpers' convention) is the caller's call — see
/// [`ReceiverPlace::store_back_container_writeback`].
fn emit_global_receiver_store_back(
    ctx: &mut FunctionContext<'_>,
    symbol: &str,
    php_type: &PhpType,
    value: ValueId,
    previous: crate::codegen::lower_inst::local_stores::RefCellStorePrevious,
) -> Result<()> {
    let unchanged = ctx.next_label("global_receiver_unchanged");
    let result_reg = abi::int_result_reg(ctx.emitter);
    let old_reg = abi::secondary_scratch_reg(ctx.emitter);
    let new_reg = abi::tertiary_scratch_reg(ctx.emitter);
    ctx.load_value_to_reg(value, new_reg)?;
    abi::emit_load_symbol_to_reg(ctx.emitter, old_reg, symbol, 0);
    match ctx.emitter.target.arch {
        crate::codegen::platform::Arch::AArch64 => {
            ctx.emitter.instruction(&format!("cmp {new_reg}, {old_reg}"));      // did the helper hand back the cell the symbol already owns?
            ctx.emitter.instruction(&format!("b.eq {unchanged}"));              // same cell: nothing to republish or retire
        }
        crate::codegen::platform::Arch::X86_64 => {
            ctx.emitter.instruction(&format!("cmp {new_reg}, {old_reg}"));      // did the helper hand back the cell the symbol already owns?
            ctx.emitter.instruction(&format!("je {unchanged}"));                // same cell: nothing to republish or retire
        }
    }
    abi::emit_store_reg_to_symbol(ctx.emitter, new_reg, symbol, 0);
    if previous == crate::codegen::lower_inst::local_stores::RefCellStorePrevious::Retire {
        abi::emit_reg_move(ctx.emitter, result_reg, old_reg);
        abi::emit_decref_if_refcounted(ctx.emitter, &php_type.codegen_repr());
    }
    ctx.emitter.label(&unchanged);
    Ok(())
}

/// Finds a declared property behind the direct sort path's transparent value transitions.
///
/// `Acquire` owns the borrowed property payload during conversion, `Borrow` and `Move` preserve
/// the same value identity, and `ArrayToHash` changes only its physical representation. None
/// changes the PHP lvalue that must receive the final COW pointer, so they are peeled until the
/// originating `PropGet` is reached.
fn direct_property_receiver(
    ctx: &FunctionContext<'_>,
    mut value: ValueId,
) -> Result<Option<(ValueId, super::objects::PropertySlot)>> {
    loop {
        let Some(value_ref) = ctx.function.value(value) else {
            return Err(CodegenIrError::missing_entry("value", value.as_raw()));
        };
        let ValueDef::Instruction { inst, .. } = value_ref.def else {
            return Ok(None);
        };
        let Some(inst_ref) = ctx.function.instruction(inst) else {
            return Err(CodegenIrError::missing_entry("instruction", inst.as_raw()));
        };
        match inst_ref.op {
            Op::Acquire | Op::Borrow | Op::Move | Op::ArrayToHash => {
                let Some(source) = inst_ref.operands.first().copied() else {
                    return Ok(None);
                };
                value = source;
            }
            Op::PropGet => {
                let Some(object) = inst_ref.operands.first().copied() else {
                    return Ok(None);
                };
                let Ok(slot) =
                    super::objects::resolve_mutated_container_property(ctx, object, inst_ref)
                else {
                    return Ok(None);
                };
                return Ok(Some((object, slot)));
            }
            _ => return Ok(None),
        }
    }
}
