//! Purpose:
//! Canonical home for the in-band runtime sentinel constants and the tagged-scalar
//! (`PhpType::TaggedScalar`) value helpers shared across codegen.
//!
//! Called from:
//! - `crate::codegen` emitters that produce or detect sentinel-encoded or tagged null values.
//!
//! Key details:
//! - The null sentinel is an in-band i64 (`PHP_INT_MAX - 1`): every i64 bit pattern is a valid
//!   PHP int, so the real integer `9223372036854775806` collides with it. The structural fix
//!   (`NullRepr::Tagged`) replaces sentinel checks with the tagged scalar representation.
//! - The uninitialized-property sentinel lives in a separate metadata word (`offset + 8`),
//!   never in the value word, so it does not collide with property values.
//! - A tagged scalar is two words: payload in the integer result register (`x0`/`rax`), tag in
//!   the adjacent register (`x1`/`rdx`); on the stack the payload sits at `offset` and the tag
//!   at `offset - 8`, mirroring the `Str` pointer/length layout. Tag values reuse the runtime
//!   value tag scheme so a tagged scalar is word-compatible with a Mixed cell's tag/payload.

use std::cell::Cell;

use super::emit::Emitter;
use super::platform::Arch;

/// Selects how codegen represents PHP `null` in scalar slots.
///
/// `Tagged` (default) gives null-capable scalar slots the inline two-word
/// `PhpType::TaggedScalar` representation, making the full i64 range representable.
/// `Sentinel` is the legacy opt-out: it stores the in-band `NULL_SENTINEL` i64, which
/// collides with the real integer `9223372036854775806`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NullRepr {
    /// In-band `PHP_INT_MAX - 1` sentinel in one-word scalar slots (legacy opt-out).
    Sentinel,
    /// Inline two-word `{payload, tag}` scalars for null-capable slots (default).
    #[default]
    Tagged,
}

thread_local! {
    /// Active null representation for the compilation running on this thread. One compilation
    /// is single-threaded, so a thread-local avoids threading the flag through every emitter
    /// signature; `generate`/`generate_user_asm` set it unconditionally at entry.
    static NULL_REPR: Cell<NullRepr> = const { Cell::new(NullRepr::Tagged) };
}

/// Installs the null representation for the compilation running on this thread. Must run
/// before type checking: parameter specialization consults it for null-capable widening.
pub fn set_null_repr(repr: NullRepr) {
    NULL_REPR.with(|cell| cell.set(repr));
}

/// Returns true when the active compilation uses the tagged null representation.
pub(crate) fn null_repr_is_tagged() -> bool {
    NULL_REPR.with(|cell| cell.get()) == NullRepr::Tagged
}

/// In-band null marker for unboxed scalar slots: `0x7fff_ffff_ffff_fffe` (= `PHP_INT_MAX - 1`).
pub(crate) const NULL_SENTINEL: i64 = 0x7fff_ffff_ffff_fffe;

/// Marker stored in a typed property's metadata word while the property is uninitialized:
/// `0x7fff_ffff_ffff_fffd` (= `PHP_INT_MAX - 2`).
pub(crate) const UNINITIALIZED_TYPED_PROPERTY_SENTINEL: i64 = 0x7fff_ffff_ffff_fffd;

/// Runtime value tag carried by a tagged scalar holding a PHP int (matches
/// `runtime_value_tag(PhpType::Int)`).
pub(crate) const TAGGED_SCALAR_TAG_INT: i64 = 0;

/// Runtime value tag carried by a tagged scalar holding PHP null (matches
/// `runtime_value_tag(PhpType::Void)`).
pub(crate) const TAGGED_SCALAR_TAG_NULL: i64 = 8;

/// Indexed-array header value_type for inline `{payload, tag}` tagged-scalar slots.
/// This is an internal array-storage tag, not a boxed Mixed runtime value tag.
pub(crate) const TAGGED_SCALAR_ARRAY_VALUE_TYPE: i64 = 11;

/// Returns the register holding a tagged scalar's tag word; the payload word lives in the
/// integer result register. AArch64: `x1`. x86_64: `rdx` (mirrors the `Str` second word).
pub(crate) fn tagged_scalar_tag_reg(emitter: &Emitter) -> &'static str {
    match emitter.target.arch {
        Arch::AArch64 => "x1",
        Arch::X86_64 => "rdx",
    }
}

/// Materializes PHP null as a tagged scalar: the canonical null payload word plus the null
/// tag in the tag register. The payload uses `NULL_SENTINEL` (not zero) so boxing a tagged
/// null into a Mixed cell produces exactly the same `{tag 8, sentinel payload}` words as the
/// legacy boxed null — `__rt_mixed_strict_eq` compares payload words even for the null tag.
pub(crate) fn emit_tagged_scalar_null(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            super::abi::emit_load_int_immediate(emitter, "x0", NULL_SENTINEL);
            emitter.instruction(&format!("mov x1, #{}", TAGGED_SCALAR_TAG_NULL));
            // runtime tag 8 marks the tagged scalar as PHP null
        }
        Arch::X86_64 => {
            super::abi::emit_load_int_immediate(emitter, "rax", NULL_SENTINEL);
            emitter.instruction(&format!("mov rdx, {}", TAGGED_SCALAR_TAG_NULL));
            // runtime tag 8 marks the tagged scalar as PHP null
        }
    }
}

/// Tags the integer currently in the result register as a non-null tagged scalar by loading
/// the int tag into the tag register. The payload word is left untouched.
pub(crate) fn emit_tagged_scalar_from_int_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x1, #{}", TAGGED_SCALAR_TAG_INT));
            // runtime tag 0 marks the tagged scalar payload as an int
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdx, {}", TAGGED_SCALAR_TAG_INT));
            // runtime tag 0 marks the tagged scalar payload as an int
        }
    }
}

/// Branches to `label` when the tagged scalar in the result registers is PHP null
/// (tag register == null tag).
pub(crate) fn emit_branch_if_tagged_scalar_null(emitter: &mut Emitter, label: &str) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x1, #{}", TAGGED_SCALAR_TAG_NULL)); // does the tagged scalar carry the runtime null tag?
            emitter.instruction(&format!("b.eq {}", label));                    // branch when the tagged scalar is PHP null
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("cmp rdx, {}", TAGGED_SCALAR_TAG_NULL)); // does the tagged scalar carry the runtime null tag?
            emitter.instruction(&format!("je {}", label));                      // branch when the tagged scalar is PHP null
        }
    }
}

/// Narrows the tagged scalar in the result registers to a plain int, coercing null to zero
/// (PHP `(int)null === 0`). Leaves the int payload in the integer result register.
pub(crate) fn emit_tagged_scalar_to_int_null_as_zero(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cmp x1, #{}", TAGGED_SCALAR_TAG_NULL)); // does the tagged scalar carry the runtime null tag?
            emitter.instruction("csel x0, xzr, x0, eq");                        // replace the payload with zero when the tagged scalar is null
        }
        Arch::X86_64 => {
            emitter.instruction("xor r11, r11");                                // materialize the zero replacement for a null tagged scalar payload
            emitter.instruction(&format!("cmp rdx, {}", TAGGED_SCALAR_TAG_NULL)); // does the tagged scalar carry the runtime null tag?
            emitter.instruction("cmove rax, r11");                              // replace the payload with zero when the tagged scalar is null
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Locks the canonical null sentinel bit pattern shared by every producer and consumer.
    #[test]
    fn null_sentinel_constant_value() {
        assert_eq!(NULL_SENTINEL, 0x7fff_ffff_ffff_fffe_u64 as i64);
        assert_eq!(NULL_SENTINEL, i64::MAX - 1);
    }

    /// Locks the uninitialized-typed-property sentinel bit pattern used in property metadata.
    #[test]
    fn uninitialized_property_sentinel_constant_value() {
        assert_eq!(
            UNINITIALIZED_TYPED_PROPERTY_SENTINEL,
            0x7fff_ffff_ffff_fffd_u64 as i64
        );
        assert_eq!(UNINITIALIZED_TYPED_PROPERTY_SENTINEL, i64::MAX - 2);
    }
}
