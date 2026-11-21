//! Purpose:
//! Owns canonical managed-heap kinds and memory-boundary layouts.
//!
//! Called from:
//! - The elephc compiler memory subsystem.
//!
//! Key details:
//! - This code runs in the compiler and emits assembly; it is never a target-program library.

/// Shared string heap kind contract.
pub const STRING_HEAP_KIND: u32 = 1;
/// Shared array heap kind contract.
pub const ARRAY_HEAP_KIND: u32 = 2;
/// Shared hash heap kind contract.
pub const HASH_HEAP_KIND: u32 = 3;
/// Shared object heap kind contract.
pub const OBJECT_HEAP_KIND: u32 = 4;
/// Shared mixed heap kind contract.
pub const MIXED_HEAP_KIND: u32 = 5;
/// Shared throwable heap kind contract.
pub const THROWABLE_HEAP_KIND: u32 = 6;
/// Shared concat temp heap kind contract.
pub const CONCAT_TEMP_HEAP_KIND: u32 = 7;
/// Shared reference cell heap kind contract.
pub const REFERENCE_CELL_HEAP_KIND: u32 = 8;
/// Shared callable heap kind contract.
pub const CALLABLE_HEAP_KIND: u32 = 9;
/// Shared mbstring identity tracked contract.
pub const MBSTRING_IDENTITY_TRACKED: u32 = 1 << 24;
/// Shared x86 64 heap magic hi32 contract.
pub const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Stamps the canonical x86_64 heap magic above the low runtime kind bits.
pub fn x86_64_heap_kind_word(low_bits: u32) -> u64 {
    (X86_64_HEAP_MAGIC_HI32 << 32) | u64::from(low_bits)
}

/// Shared try handler slot size contract.
pub const TRY_HANDLER_SLOT_SIZE: usize = 224;

/// Shared try handler diag depth offset contract.
pub const TRY_HANDLER_DIAG_DEPTH_OFFSET: usize = 16;

/// Shared try handler jmp buf offset contract.
pub const TRY_HANDLER_JMP_BUF_OFFSET: usize = 24;

/// Shared boundary active contract.
pub const BOUNDARY_ACTIVE: &str = "_elephc_boundary_active";

/// Shared boundary status contract.
pub const BOUNDARY_STATUS: &str = "_elephc_boundary_status";

/// Shared status allocation failure contract.
pub const STATUS_ALLOCATION_FAILURE: i32 = 3;

/// Legacy scalar null marker accepted at memory-container boundaries.
pub const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// Emits null-container branching using the shared scalar representation contract.
pub fn emit_branch_if_null_container(
    emitter: &mut impl crate::Emitter,
    value_reg: &str,
    scratch_reg: &str,
    label: &str,
) {
    match emitter.arch() {
        crate::Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", value_reg, label));      // zero container pointers take the caller's null path
            super::abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL);
            emitter.instruction(&format!("cmp {}, {}", value_reg, scratch_reg)); // does the container carry the in-band null sentinel?
            emitter.instruction(&format!("b.eq {}", label));                    // sentinel-null containers take the caller's null path
        }
        crate::Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", value_reg, value_reg)); // is the container pointer zero (PHP null)?
            emitter.instruction(&format!("jz {}", label));                      // zero container pointers take the caller's null path
            super::abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL);
            emitter.instruction(&format!("cmp {}, {}", value_reg, scratch_reg)); // does the container carry the in-band null sentinel?
            emitter.instruction(&format!("je {}", label));                      // sentinel-null containers take the caller's null path
        }
    }
}

/// Bytes in a Mixed cell: one tag word and two payload words.
pub const MIXED_CELL_SIZE: usize = 24;
/// Descriptor tag for a reference cell owning a string pair.
pub const STRING_REFERENCE_PAYLOAD_TAG: i64 = 1;

/// Bytes in the payload of a managed native reference cell.
pub const REFERENCE_CELL_SIZE: usize = 16;
/// Indexed-array header tag for inline scalar slots storing a payload and tag pair.
pub const TAGGED_SCALAR_ARRAY_VALUE_TYPE: i64 = 11;
/// Raw nullable pointer-container tag, normalized to integer/null before boxing.
pub const POINTER_ARRAY_VALUE_TYPE: i64 = 13;

/// Typed indexed-array storage kind used by memory eligibility probes.
pub const ARRAY_KIND: crate::HeapKind = crate::HeapKind::new(ARRAY_HEAP_KIND);
/// Typed boxed-cell storage kind used by memory eligibility probes.
pub const MIXED_KIND: crate::HeapKind = crate::HeapKind::new(MIXED_HEAP_KIND);

/// Returns the byte offset of a declared slot following the class id.
/// At the declared slot count this is also the end of the fixed object payload.
pub const fn object_declared_slot_offset(index: usize) -> usize {
    8 + index * 16
}

/// Loads the tag from an already selected raw Mixed cell.
pub fn emit_load_mixed_tag(emitter: &mut impl crate::TargetAbi, dest: &str, cell: &str) {
    emitter.emit_load_from_address(dest, cell, 0);
}

/// Loads the first payload word from an already selected raw Mixed cell.
pub fn emit_load_mixed_low(emitter: &mut impl crate::TargetAbi, dest: &str, cell: &str) {
    emitter.emit_load_from_address(dest, cell, 8);
}

/// Loads the second payload word from an already selected raw Mixed cell.
pub fn emit_load_mixed_high(emitter: &mut impl crate::TargetAbi, dest: &str, cell: &str) {
    emitter.emit_load_from_address(dest, cell, 16);
}
