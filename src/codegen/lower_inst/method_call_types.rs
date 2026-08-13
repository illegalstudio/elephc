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
    pub(super) cleanup_slots: Vec<CallArgTempCleanup>,
    pub(super) cleanup_bytes: usize,
    pub(super) borrowed_stack_arg_bytes: usize,
}

/// Caller-owned temporary argument that must be released after the call returns.
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

/// How long the caller-side cell for a by-reference argument with no caller variable behind
/// it has to stay alive.
///
/// The distinction is a MEMORY-SAFETY one, not an optimization: a stack cell dies with the
/// caller's frame, so it may only be used when the callee cannot keep the reference.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum RefArgCellLifetime {
    /// The reference cannot outlive the call, so the cell is a caller-stack slot released
    /// with the rest of the by-reference cell block. Every ordinary function and method
    /// call: a PHP callee has no way to bind a by-reference parameter into storage that
    /// survives its own frame.
    CallOnly,
    /// The callee may KEEP the reference. A constructor that promotes a by-reference
    /// parameter (`__construct(public int &$value = 1)`) BORROWS the cell for the whole life
    /// of the object it builds — `crate::types::checker`'s
    /// `apply_reference_property_promotions` documents that such a property holds a borrowed
    /// cell rather than an owned one — so the cell must be heap storage that outlives this
    /// frame. It is never freed, which is a narrower pre-existing defect (one cell per
    /// constructed object, not one per call) that only the object model can fix.
    MayOutliveCall,
}

/// A caller-side stack cell standing in for a by-reference argument that has NO caller
/// variable behind it — an OMITTED optional by-reference argument (`f($x)` against
/// `f($x, int &$out = null)`), or any other operand that is neither a local nor an array
/// element.
///
/// It lives in the same pushed cell block as [`RefArgWriteback`] and is released with it
/// after the call. There is nothing to write back (no source local exists), so the callee's
/// write into it is simply discarded — which is exactly PHP's semantics for an argument the
/// caller never supplied.
pub(super) struct RefArgTempCell {
    pub(super) param_index: usize,
    pub(super) source_value: ValueId,
    /// The cell's storage representation: the callee writes through the pointer with the
    /// PARAMETER's type, so the cell must be that type and not the argument's.
    pub(super) cell_ty: PhpType,
    pub(super) cell_offset: usize,
}

/// A caller-side scalar local boxed into a temporary Mixed by-reference cell.
pub(super) struct RefArgWriteback {
    pub(super) param_index: usize,
    pub(super) source_value: ValueId,
    pub(super) source_slot: LocalSlotId,
    pub(super) source_ty: PhpType,
    pub(super) cell_offset: usize,
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
