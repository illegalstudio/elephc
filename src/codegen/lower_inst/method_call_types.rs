//! Purpose:
//! Defines shared method-call targets, cleanup state, and runtime dispatch enums.
//!
//! Called from:
//! - `crate::codegen::lower_inst::lower_instruction()` and sibling lowering helpers.
//!
//! Key details:
//! - Preserves EIR ownership, ABI ordering, runtime symbols, and target-aware lowering.

use super::*;

/// Resolved method metadata needed to issue a direct method call.
pub(super) struct MethodCallTarget {
    pub(super) impl_class: String,
    pub(super) method_key: String,
    pub(super) dynamic_slot: Option<usize>,
    pub(super) params: Vec<PhpType>,
    pub(super) ref_params: Vec<bool>,
    pub(super) return_ty: PhpType,
    pub(super) by_ref_return: bool,
    pub(super) source_abi: bool,
}

/// Concrete runtime class branch available to a `Mixed` receiver method call.
pub(super) struct MixedMethodCandidate {
    pub(super) class_id: u64,
    pub(super) class_name: String,
    pub(super) target: MethodCallTarget,
}

/// Outgoing call argument state that must be cleaned up after the call returns.
pub(super) struct CallArgMaterialization {
    pub(super) overflow_bytes: usize,
    pub(super) ref_writebacks: Vec<RefArgWriteback>,
    pub(super) ref_temp_cells: Vec<RefArgTempCell>,
    pub(super) preleased_ref_cells: Vec<PreleasedRefArgCell>,
    pub(super) cleanup_slots: Vec<CallArgTempCleanup>,
    pub(super) cleanup_bytes: usize,
    /// Stack records that transfer conversion cleanup ownership only during exceptional unwind.
    pub(super) cleanup_guard_bytes: usize,
    pub(super) borrowed_stack_arg_bytes: usize,
}

/// A managed element cell acquired during EIR argument evaluation without an EIR call ledger.
/// The shared ABI cleanup retires its published frame owner after call writeback completes.
pub(super) struct PreleasedRefArgCell {
    pub(super) owner_slot: LocalSlotId,
    pub(super) cell_ty: PhpType,
}

/// Caller-owned coercion with an adjacent unwind record, retired on return or throw.
pub(super) struct CallArgTempCleanup {
    pub(super) param_index: usize,
    pub(super) offset: usize,
    pub(super) ty: PhpType,
}

/// Caller-side stack Mixed cell borrowed by a read-only callee.
pub(super) struct BorrowedStackMixedArg {
    pub(super) param_index: usize,
    pub(super) offset: usize,
    pub(super) source_ty: PhpType,
}

/// Determines whether omitted reference cells can retire the caller's lease after the call.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RefArgCellLifetime {
    /// The caller owns a managed lease until return or throw. An escaping closure may
    /// retain the cell independently, so this lifetime does not prohibit reference escape.
    CallOnly,
    /// The callee may keep an unowned raw reference property. Exact Mixed constructor
    /// references retain managed cells and use CallOnly. Other promoted payloads still
    /// borrow their cell for the object's lifetime, so their persistent fallback outlives
    /// this frame. That legacy fallback is not freed until their ownership model changes.
    MayOutliveCall,
}

/// A managed cell for a reference argument without a caller variable, typically a default.
/// Its stack slot holds the cell pointer, followed by an unwind record for the caller lease.
/// There is no source location to write back; escaping closures retain independent leases.
pub(super) struct RefArgTempCell {
    pub(super) param_index: usize,
    pub(super) source_value: ValueId,
    /// The cell's storage representation: the callee writes through the pointer with the
    /// PARAMETER's type, so the cell must be that type and not the argument's.
    pub(super) cell_ty: PhpType,
    pub(super) cell_offset: usize,
    /// An element-reference operand already is a managed cell. The caller retains that cell
    /// instead of allocating a second cell, preserving identity with the hash entry.
    pub(super) retain_existing: bool,
}

/// A caller-side local boxed into a temporary Mixed by-reference cell.
///
/// The cell is published as a call-operand owner, so an exception thrown by the callee
/// releases it (and the value it holds) while unwinding; the normal path unlinks it before
/// writing the value back.
pub(super) struct RefArgWriteback {
    pub(super) param_index: usize,
    /// Further by-reference parameters bound to the same local (`f($v, $v)`): they share this
    /// cell, so every write through any of them is seen by the others and written back once.
    pub(super) alias_param_indexes: Vec<usize>,
    pub(super) source_slot: LocalSlotId,
    pub(super) source_ty: PhpType,
    pub(super) cell_offset: usize,
}

impl RefArgWriteback {
    /// Whether by-reference parameter `param_index` passes this writeback's cell.
    pub(super) fn covers(&self, param_index: usize) -> bool {
        self.param_index == param_index || self.alias_param_indexes.contains(&param_index)
    }
}

/// Runtime dispatch path for EIR `RuntimeCall` instructions that mean ArrayAccess indexing.
pub(super) enum ArrayAccessRuntimeDispatch {
    Concrete(String),
    Interface { boxed_receiver: bool },
}

/// Source for the hidden called-class id passed to static method bodies.
pub(super) enum CalledClassIdArg {
    Immediate(u64),
    Local(LocalSlotId),
    ThisObject(LocalSlotId),
}
