//! Purpose:
//! Balances descriptor-invoker argument owners at the borrowed native-call boundary.
//!
//! Called from:
//! - The indexed/associative invoker argument builders and normal/exception exits.
//!
//! Key details:
//! - Arrays and Mixed arguments get independent callee shadows; invocation leases remain caller-owned.
//! - Object and iterable argument leases also remain caller-owned, even when the callee borrows them.
//! - By-value strings have detached buffers owned here, including defaults and scalar coercions.
//! - Frame slots start empty and are cleared before release, including partial preparation failures.
//! - Temporary reference cells are managed owners; borrowed markers and hidden captures are not.
//! - Raw callable descriptors require their own release entry, not the heap-kind dispatcher.

use super::{abi, Emitter, PhpType};

/// Frame-relative slots for the caller-owned argument cells and an in-flight boxed return.
pub(super) struct InvokerArgumentOwners {
    first_offset: usize,
    count: usize,
    frame_size: usize,
    descriptor_slots: Vec<bool>,
}

impl InvokerArgumentOwners {
    /// Appends cleanup slots beyond the existing frame and any native exception boundary.
    pub(super) fn new(base_frame_size: usize, count: usize) -> Self {
        Self {
            first_offset: base_frame_size - 8,
            count,
            frame_size: base_frame_size + ((count + 2) * 8).div_ceil(16) * 16,
            descriptor_slots: vec![false; count],
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
        for index in 0..=self.count + 1 {
            abi::store_at_offset(emitter, zero, self.offset(index));
        }
    }

    /// Records a pushed by-value string buffer or a refcounted argument lease.
    pub(super) fn record_pushed(&mut self, index: usize, ty: &PhpType, emitter: &mut Emitter) {
        let repr = ty.codegen_repr();
        if !matches!(repr, PhpType::Str | PhpType::Callable) && !repr.is_refcounted() {
            return;
        }
        self.record_pushed_reference(index, emitter);
        self.descriptor_slots[index] = repr == PhpType::Callable;
    }

    /// Adopts a managed reference cell or heap value whose pointer is the top pushed word.
    pub(super) fn record_pushed_reference(&mut self, index: usize, emitter: &mut Emitter) {
        assert!(index < self.count, "invoker argument owner exceeds its frame layout");
        self.descriptor_slots[index] = false;
        let scratch = abi::secondary_scratch_reg(emitter);
        abi::emit_load_temporary_stack_slot(emitter, scratch, 0);
        abi::store_at_offset(emitter, scratch, self.offset(index));
    }

    /// Roots an owned conversion input in the boxed-result slot before the native call starts.
    pub(super) fn record_coercion_source(&self, emitter: &mut Emitter) {
        abi::store_at_offset(emitter, abi::int_result_reg(emitter), self.offset(self.count));
    }

    /// Retires the conversion root before its normal release, preserving the converted result.
    pub(super) fn clear_coercion_source(&self, emitter: &mut Emitter) {
        self.clear_slot(self.count, emitter);
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

    /// Finishes all owners after exception escape, accumulating further destructor throws.
    pub(super) fn release_all(&self, emitter: &mut Emitter) {
        let result = abi::int_result_reg(emitter);
        let pending = self.offset(self.count + 1);
        abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
        abi::store_at_offset(emitter, result, pending);
        abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
        for index in 0..=self.count {
            abi::load_at_offset(emitter, result, self.offset(index));
            self.clear_slot(index, emitter);
            crate::codegen_support::runtime::emit_guarded_cleanup_call(
                emitter, self.release_entry(index), result, pending,
            );
        }
        abi::load_at_offset(emitter, result, pending);
        abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    }

    /// Clears one owner before its release so reentrant cleanup cannot consume it twice.
    fn release_slot(&self, index: usize, emitter: &mut Emitter) {
        abi::load_at_offset(emitter, abi::int_result_reg(emitter), self.offset(index));
        self.clear_slot(index, emitter);
        abi::emit_call_label(emitter, self.release_entry(index));
    }

    /// Selects descriptor cleanup only for raw callable arguments, never boxed returns or ref cells.
    fn release_entry(&self, index: usize) -> &'static str {
        if self.descriptor_slots.get(index).copied().unwrap_or(false) {
            "__rt_callable_descriptor_release"
        } else {
            "__rt_decref_any"
        }
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

    /// Raw callable values retire descriptors, while managed callable reference cells retire cells.
    #[test]
    fn invoker_callable_owners_select_typed_cleanup_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let mut owners = InvokerArgumentOwners::new(super::super::INVOKER_BOUNDARY_FRAME_SIZE, 2);
            owners.initialize(&mut emitter);
            owners.record_pushed(0, &PhpType::Callable, &mut emitter);
            owners.record_pushed(1, &PhpType::Callable, &mut emitter);
            owners.record_pushed_reference(1, &mut emitter);
            assert_eq!(owners.release_entry(0), "__rt_callable_descriptor_release");
            assert_eq!(owners.release_entry(1), "__rt_decref_any");
            assert_eq!(owners.release_entry(2), "__rt_decref_any");
            owners.finish_return(&mut emitter);
            owners.release_all(&mut emitter);
            let asm = emitter.output();
            assert_eq!(asm.matches("__rt_callable_descriptor_release").count(), 2, "{name}");
            assert_eq!(asm.matches("__rt_cleanup_invoke").count(), 3, "{name}");
        }
    }

    /// All target layouts keep owners outside the exception record and clean both exit paths.
    #[test]
    fn invoker_argument_owners_cover_normal_and_exception_exits() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            let mut owners = InvokerArgumentOwners::new(super::super::INVOKER_BOUNDARY_FRAME_SIZE, 4);
            assert!(owners.offset(0) > super::super::INVOKER_BOUNDARY_BASE_OFFSET);
            assert!(owners.offset(3) <= owners.frame_size() - 16);
            assert!(owners.offset(5) <= owners.frame_size() - 16);
            assert_eq!(owners.frame_size() % 16, 0);
            owners.initialize(&mut emitter);
            owners.record_pushed(0, &PhpType::Mixed, &mut emitter);
            owners.record_pushed(1, &PhpType::Array(Box::new(PhpType::Int)), &mut emitter);
            owners.record_pushed(2, &PhpType::Int, &mut emitter);
            owners.record_pushed(3, &PhpType::Str, &mut emitter);
            owners.finish_return(&mut emitter);
            owners.release_all(&mut emitter);
            let asm = emitter.output();
            assert!(asm.matches("__rt_decref_any").count() >= 9, "{name}");
            assert_eq!(asm.matches("__rt_cleanup_invoke").count(), 5, "{name}");
        }
    }

    /// Borrowed native object and iterable parameters still have invoker leases on every target.
    #[test]
    fn invoker_tracks_object_and_iterable_argument_leases() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for ty in [PhpType::Object("LeaseOwner".to_string()), PhpType::Iterable] {
                let mut emitter = Emitter::new(crate::codegen::platform::Target::parse(name).unwrap());
                let mut owners = InvokerArgumentOwners::new(super::super::INVOKER_BOUNDARY_FRAME_SIZE, 1);
                owners.record_pushed(0, &ty, &mut emitter);
                assert!(!emitter.output().is_empty(), "{name}: {ty:?}");
            }
        }
    }
}
