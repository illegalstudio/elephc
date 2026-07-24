//! Purpose:
//! Canonical home for the in-band runtime sentinel constants and the tagged-scalar
//! (`PhpType::TaggedScalar`) value helpers shared across codegen.
//!
//! Called from:
//! - `crate::codegen` emitters that produce or detect sentinel-encoded or tagged null values.
//! - x86_64 heap emitters/checkers that stamp or validate the uniform heap-header kind word.
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
//! - x86_64 heap headers carry `X86_64_HEAP_MAGIC_HI32` ("ELPH") in the high 32 bits. Every
//!   stamp must go through `x86_64_heap_kind_word`; every magic check must compare against
//!   `X86_64_HEAP_MAGIC_HI32`. Local copies of either constant are forbidden.

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

/// Branches to `label` when the container pointer in `value_reg` represents PHP null:
/// either a zero pointer or the in-band `NULL_SENTINEL` that missed reads of refcounted
/// slots materialize. Clobbers `scratch_reg` with the sentinel bit pattern. Used to keep
/// container reads from dereferencing a null/sentinel receiver (issue #526).
pub(crate) fn emit_branch_if_null_container(
    emitter: &mut Emitter,
    value_reg: &str,
    scratch_reg: &str,
    label: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("cbz {}, {}", value_reg, label));      // zero container pointers take the caller's null path
            super::abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL);
            emitter.instruction(&format!("cmp {}, {}", value_reg, scratch_reg)); // does the container carry the in-band null sentinel?
            emitter.instruction(&format!("b.eq {}", label));                    // sentinel-null containers take the caller's null path
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("test {}, {}", value_reg, value_reg)); // is the container pointer zero (PHP null)?
            emitter.instruction(&format!("jz {}", label));                      // zero container pointers take the caller's null path
            super::abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL);
            emitter.instruction(&format!("cmp {}, {}", value_reg, scratch_reg)); // does the container carry the in-band null sentinel?
            emitter.instruction(&format!("je {}", label));                      // sentinel-null containers take the caller's null path
        }
    }
}

/// Replaces the in-band `NULL_SENTINEL` in `value_reg` with a zero pointer, leaving any
/// other value untouched. Clobbers `scratch_reg` with the sentinel bit pattern. Used where
/// a null container must be representable by the cheap zero check alone — e.g. the
/// by-reference foreach iterator source slot, whose live length is re-read every iteration
/// (issue #556). Zero-pointer inputs are already normal and pass through unchanged.
pub(crate) fn emit_normalize_null_container_to_zero(
    emitter: &mut Emitter,
    value_reg: &str,
    scratch_reg: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            super::abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL);
            emitter.instruction(&format!("cmp {}, {}", value_reg, scratch_reg)); // does the container carry the in-band null sentinel?
            emitter.instruction(&format!("csel {}, xzr, {}, eq", value_reg, value_reg)); // fold the sentinel into the canonical zero container pointer
        }
        Arch::X86_64 => {
            super::abi::emit_load_int_immediate(emitter, scratch_reg, NULL_SENTINEL);
            emitter.instruction(&format!("cmp {}, {}", value_reg, scratch_reg)); // does the container carry the in-band null sentinel?
            emitter.instruction(&format!("mov {}, 0", scratch_reg));            // materialize the zero replacement without disturbing the comparison flags
            emitter.instruction(&format!("cmove {}, {}", value_reg, scratch_reg)); // fold the sentinel into the canonical zero container pointer
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


/// High 32 bits of the x86_64 uniform heap-header kind word (`"ELPH"` in ASCII bytes
/// `0x45 0x4C 0x50 0x48`). Every x86_64 allocation is stamped with this marker; the
/// refcount and free helpers ignore pointers whose header does not carry it, so an
/// emitter that stamps a different value silently opts its objects out of refcounting.
///
/// Checkers must compare against this constant. Stampers must build the full kind word
/// through [`x86_64_heap_kind_word`] instead of hand-typing the concatenated immediate.
pub(crate) const X86_64_HEAP_MAGIC_HI32: u64 = 0x454C5048;

/// Builds the full x86_64 heap-header kind word: magic in the high 32 bits and `low_bits`
/// in the low 32 bits. `low_bits` is the packed kind/COW/value_type field (plain kinds such
/// as `1`/`4`/`5`/`6`, or wider encodings such as `0x8003` / `0x80ff` / `0x8702`). Pass `0`
/// when only the magic marker should be materialised (as in `__rt_heap_alloc` before the
/// caller fills the kind). Emitters must use this instead of a hand-typed literal — a
/// transposed magic assembles fine but makes every refcount operation on the stamped object
/// a silent no-op (issue #482).
pub(crate) fn x86_64_heap_kind_word(low_bits: u32) -> u64 {
    (X86_64_HEAP_MAGIC_HI32 << 32) | u64::from(low_bits)
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

    /// Verifies the x86_64 heap kind word carries the canonical magic ("ELPH") in the
    /// high word and arbitrary low bits — including wide COW/kind encodings.
    #[test]
    fn test_x86_64_heap_kind_word_layout() {
        assert_eq!(x86_64_heap_kind_word(6), 0x454C_5048_0000_0006);
        assert_eq!(x86_64_heap_kind_word(4), 0x454C_5048_0000_0004);
        assert_eq!(x86_64_heap_kind_word(0), 0x454C_5048_0000_0000);
        assert_eq!(x86_64_heap_kind_word(0x8003), 0x454C_5048_0000_8003);
        assert_eq!(x86_64_heap_kind_word(0x8702), 0x454C_5048_0000_8702);
        // Transposed "EHPL" built from bytes so this test source never embeds that hex.
        let transposed_hi =
            u64::from(u32::from_be_bytes([b'E', b'H', b'P', b'L']));
        assert_ne!(x86_64_heap_kind_word(6) >> 32, transposed_hi, "transposed magic");
    }

    /// Repo lint: no emitter may hand-type the transposed heap magic again.
    #[test]
    fn test_no_transposed_heap_magic_in_source() {
        let needle = ["0x4548", "504c"].concat();
        let hits = scan_src_for_normalized_needle(&needle);
        assert!(
            hits.is_empty(),
            "transposed x86_64 heap magic found in: {hits:?}"
        );
    }

    /// Repo lint: the canonical heap magic lives only in this module — no local copies,
    /// and no hand-typed canonical-magic immediates outside documentation comments.
    #[test]
    fn test_heap_magic_only_defined_in_sentinels() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut local_consts = Vec::new();
        let mut hardcoded_immediates = Vec::new();
        scan_heap_magic_policy(&src, &mut local_consts, &mut hardcoded_immediates);
        assert!(
            local_consts.is_empty(),
            "local X86_64_HEAP_MAGIC_HI32 copies found in: {local_consts:?}"
        );
        assert!(
            hardcoded_immediates.is_empty(),
            "hand-typed canonical heap-magic immediates found in: {hardcoded_immediates:?}"
        );
    }

    /// Walks `src/` looking for a normalized hex needle; returns matching file paths.
    fn scan_src_for_normalized_needle(needle: &str) -> Vec<String> {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut hits = Vec::new();
        scan_normalized(&src, needle, &mut hits);
        hits
    }

    /// Recursively scans Rust sources for a normalized (lowercase, underscore-stripped) needle.
    fn scan_normalized(dir: &std::path::Path, needle: &str, hits: &mut Vec<String>) {
        for entry in std::fs::read_dir(dir).expect("readable src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                scan_normalized(&path, needle, hits);
            } else if path.extension().is_some_and(|e| e == "rs") {
                let body = std::fs::read_to_string(&path).expect("readable source");
                let normalized = body.to_lowercase().replace('_', "");
                if normalized.contains(needle) {
                    hits.push(path.display().to_string());
                }
            }
        }
    }

    /// Recursively enforces the single-source heap-magic policy outside this module.
    fn scan_heap_magic_policy(
        dir: &std::path::Path,
        local_consts: &mut Vec<String>,
        hardcoded_immediates: &mut Vec<String>,
    ) {
        for entry in std::fs::read_dir(dir).expect("readable src dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                scan_heap_magic_policy(&path, local_consts, hardcoded_immediates);
                continue;
            }
            if !path.extension().is_some_and(|e| e == "rs") {
                continue;
            }
            if path.ends_with("codegen_support/sentinels.rs") {
                continue;
            }
            let body = std::fs::read_to_string(&path).expect("readable source");
            if body.contains("const X86_64_HEAP_MAGIC_HI32") {
                local_consts.push(path.display().to_string());
            }
            for (idx, line) in body.lines().enumerate() {
                let trimmed = line.trim_start();
                if trimmed.starts_with("//") || trimmed.starts_with("///") {
                    continue;
                }
                let code = match line.find("//") {
                    Some(pos) => &line[..pos],
                    None => line,
                };
                let normalized = code.to_lowercase().replace('_', "");
                let magic = ["0x454c", "5048"].concat();
                if normalized.contains(&magic) {
                    hardcoded_immediates
                        .push(format!("{}:{}", path.display(), idx + 1));
                }
            }
        }
    }
}
