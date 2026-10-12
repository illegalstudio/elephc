//! Purpose:
//! The output of register allocation: where each EIR value lives (a physical
//! register, scalar recipe, or stack spill slot) and which callee-saved registers the
//! function must preserve.
//!
//! Called from:
//! - `crate::ir_passes::regalloc` produces it; the `codegen` backend
//!   consumes it through the value-access chokepoints and frame layout.
//!
//! Key details:
//! - Registers are `&'static str` names, matching the rest of the backend.
//! - Unassigned scalar constants have bit-exact recipes instead of stack slots.
//!   Other unassigned values retain their frame slots.

use std::collections::HashMap;

use crate::ir::ValueId;
use super::rematerialize::ScalarConstant;

/// Where a value lives for the span the allocator assigned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Location {
    /// The value lives in this physical register.
    Register(&'static str),
    /// The scalar is reconstructed at its uses and has no stack home.
    Rematerialized,
    /// The value lives in its stack slot (no register assigned).
    Spilled,
}

/// Register-allocation result for one function.
#[derive(Debug, Clone, Default)]
pub struct Allocation {
    registers: HashMap<ValueId, &'static str>,
    callee_saved: Vec<&'static str>,
    constants: HashMap<ValueId, ScalarConstant>,
}

impl Allocation {
    /// Returns an allocation that keeps every value on the stack. Used for the
    /// `stack` fallback and for functions the allocator declines to handle.
    pub fn all_spilled() -> Self {
        Self::default()
    }

    /// Builds an allocation from a value-to-register map and the set of
    /// callee-saved registers it used, deduplicated and ordered for stable
    /// prologue/epilogue emission.
    pub(super) fn from_assignments(
        registers: HashMap<ValueId, &'static str>,
        mut callee_saved: Vec<&'static str>,
    ) -> Self {
        callee_saved.sort_unstable();
        callee_saved.dedup();
        Self {
            registers,
            callee_saved,
            constants: HashMap::new(),
        }
    }

    /// Returns the value's physical register, scalar recipe status, or backing stack home.
    pub fn location(&self, value: ValueId) -> Location {
        match self.registers.get(&value) {
            Some(reg) => Location::Register(reg),
            None if self.constants.contains_key(&value) => Location::Rematerialized,
            None => Location::Spilled,
        }
    }

    /// Returns the register assigned to `value`, if any.
    pub fn register_of(&self, value: ValueId) -> Option<&'static str> {
        self.registers.get(&value).copied()
    }

    /// Attaches scalar recipes, retaining physical homes only where allocation paid for them.
    pub(super) fn with_constants(mut self, constants: HashMap<ValueId, ScalarConstant>) -> Self {
        self.constants = constants;
        self.callee_saved.retain(|reg| self.registers.values().any(|assigned| assigned == reg));
        self
    }

    /// Returns a recipe only when the scalar has no allocated register.
    pub fn rematerialized(&self, value: ValueId) -> Option<ScalarConstant> {
        if self.registers.contains_key(&value) { return None; }
        self.constants.get(&value).copied()
    }

    /// Returns whether a scalar constant uses either a recipe or an allocated register.
    pub fn is_constant(&self, value: ValueId) -> bool {
        self.constants.contains_key(&value)
    }

    /// Returns the callee-saved registers this function must save in its
    /// prologue and restore in its epilogue.
    pub fn used_callee_saved(&self) -> &[&'static str] {
        &self.callee_saved
    }
}
