//! Purpose:
//! Balances descriptor-invoker argument owners at the borrowed native-call boundary.
//!
//! Called from:
//! - The indexed/associative invoker argument builders and normal/exception exits.
//!
//! Key details:
//! - Arrays and Mixed arguments get independent callee shadows; invocation leases remain caller-owned.
//! - By-value strings have detached buffers owned here, including defaults and scalar coercions.
//! - Frame slots start empty and are cleared before release, including partial preparation failures.
//! - Hidden captures and by-reference marker slots are not by-value invocation owners.

use super::{abi, Emitter, FunctionSig, PhpType};

/// Frame-relative slots for the caller-owned argument cells and an in-flight boxed return.
pub(super) struct InvokerArgumentOwners {
    first_offset: usize,
    count: usize,
    frame_size: usize,
}

impl InvokerArgumentOwners {
    /// Appends cleanup slots beyond the existing frame and any native exception boundary.
    pub(super) fn new(base_frame_size: usize, count: usize) -> Self {
        Self {
            first_offset: base_frame_size - 8,
            count,
            frame_size: base_frame_size + ((count + 1) * 8).div_ceil(16) * 16,
        }
    }

    /// Returns the aligned frame size including every argument and the boxed return slot.
    pub(super) fn frame_size(&self) -> usize {
        self.frame_size
    }

    /// Returns the frame offset of an argument slot or the final boxed-return slot.
    fn offset(&self, index: usize) -> usize {
        self.first_offset + index * 8
    }

    /// Clears every cleanup slot without clobbering incoming descriptor/argument ABI registers.
    pub(super) fn initialize(&self, emitter: &mut Emitter) {
        let zero = abi::secondary_scratch_reg(emitter);
        abi::emit_load_int_immediate(emitter, zero, 0);
        for index in 0..=self.count {
            abi::store_at_offset(emitter, zero, self.offset(index));
        }
    }

    /// Records a pushed by-value string buffer or a container with a callee-owned shadow.
    pub(super) fn record_pushed(&self, index: usize, ty: &PhpType, emitter: &mut Emitter) {
        if ty.codegen_repr() != PhpType::Str && !FunctionSig::parameter_needs_owned_shadow(ty, false) {
            return;
        }
        assert!(index < self.count, "invoker argument owner exceeds its frame layout");
        let scratch = abi::secondary_scratch_reg(emitter);
        abi::emit_load_temporary_stack_slot(emitter, scratch, 0);
        abi::store_at_offset(emitter, scratch, self.offset(index));
    }

    /// Preserves the boxed result across cleanup and transfers it only after all releases finish.
    pub(super) fn finish_return(&self, emitter: &mut Emitter) {
        let result = abi::int_result_reg(emitter);
        abi::store_at_offset(emitter, result, self.offset(self.count));
        for index in 0..self.count {
            self.release_slot(index, emitter);
        }
        abi::load_at_offset(emitter, result, self.offset(self.count));
        self.clear_slot(self.count, emitter);
    }

    /// Releases any acquired arguments and an interrupted return after native exception escape.
    pub(super) fn release_all(&self, emitter: &mut Emitter) {
        for index in 0..=self.count {
            self.release_slot(index, emitter);
        }
    }

    /// Clears one owner before its release so reentrant cleanup cannot consume it twice.
    fn release_slot(&self, index: usize, emitter: &mut Emitter) {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), self.offset(index));
        self.clear_slot(index, emitter);
        abi::emit_call_label(emitter, "__rt_decref_any");
    }

    /// Writes an empty cleanup marker while preserving the current result register.
    fn clear_slot(&self, index: usize, emitter: &mut Emitter) {
        let zero = abi::secondary_scratch_reg(emitter);
        abi::emit_load_int_immediate(emitter, zero, 0);
        abi::store_at_offset(emitter, zero, self.offset(index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// All target layouts keep owners outside the exception record and clean both exit paths.
    #[test]
    fn invoker_argument_owners_cover_normal_and_exception_exits() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let owners = InvokerArgumentOwners::new(super::super::INVOKER_BOUNDARY_FRAME_SIZE, 4);
            assert!(owners.offset(0) > super::super::INVOKER_BOUNDARY_BASE_OFFSET);
            assert!(owners.offset(3) <= owners.frame_size() - 16);
            assert_eq!(owners.frame_size() % 16, 0);
            owners.initialize(&mut emitter);
            owners.record_pushed(0, &PhpType::Mixed, &mut emitter);
            owners.record_pushed(1, &PhpType::Array(Box::new(PhpType::Int)), &mut emitter);
            owners.record_pushed(2, &PhpType::Int, &mut emitter);
            owners.record_pushed(3, &PhpType::Str, &mut emitter);
            owners.finish_return(&mut emitter);
            owners.release_all(&mut emitter);
            assert_eq!(emitter.output().matches("__rt_decref_any").count(), 9, "{name}");
        }
    }
}
