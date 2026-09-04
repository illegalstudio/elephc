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
//! - The compact Throwable payload's creation-line slot lives here for the same reason as the
//!   heap-header word: it is written by a dozen emitters across `codegen` and read by the runtime
//!   emitters in `codegen_support::runtime`, which cannot see `codegen`. Every allocator of that
//!   payload must write the slot, since `__rt_heap_alloc` recycles blocks without zeroing them.

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
            emitter.instruction(&format!("mov x1, #{}", TAGGED_SCALAR_TAG_NULL)); // runtime tag 8 marks the tagged scalar as PHP null
        }
        Arch::X86_64 => {
            super::abi::emit_load_int_immediate(emitter, "rax", NULL_SENTINEL);
            emitter.instruction(&format!("mov rdx, {}", TAGGED_SCALAR_TAG_NULL)); // runtime tag 8 marks the tagged scalar as PHP null
        }
    }
}

/// Tags the integer currently in the result register as a non-null tagged scalar by loading
/// the int tag into the tag register. The payload word is left untouched.
pub(crate) fn emit_tagged_scalar_from_int_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("mov x1, #{}", TAGGED_SCALAR_TAG_INT)); // runtime tag 0 marks the tagged scalar payload as an int
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov rdx, {}", TAGGED_SCALAR_TAG_INT)); // runtime tag 0 marks the tagged scalar payload as an int
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


/// Materializes the float-slot null marker in the float result register: the canonical
/// [`NULL_SENTINEL`] word reinterpreted as an IEEE-754 double.
///
/// `0x7fff_ffff_ffff_fffe` has an all-ones exponent, its mantissa MSB set, and a non-zero
/// remaining payload, so as a double it is a *quiet NaN* — no ordinary float arithmetic
/// produces it, and it differs from PHP's own `NAN` constant (`0x7ff8_0000_0000_0000`), so an
/// array element that genuinely stores `NAN` still reads back as a hit and never as a miss.
/// Reusing `NULL_SENTINEL` keeps float slots on the same in-band marker word that every other
/// unboxed scalar slot already uses instead of inventing a third null mechanism.
///
/// Only *silent* element reads (the ones behind `??`, `isset()` and `empty()`) may emit this;
/// a warned read keeps materializing `0.0` so a plain `$a[$missing]` does not start rendering
/// as `NAN` in value position. Clobbers the secondary scratch register.
pub(crate) fn emit_float_null_sentinel(emitter: &mut Emitter) {
    let scratch = super::abi::secondary_scratch_reg(emitter);
    super::abi::emit_load_int_immediate(emitter, scratch, NULL_SENTINEL);
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("fmov d0, {}", scratch));              // reinterpret the in-band null sentinel word as the float miss marker
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("movq xmm0, {}", scratch));            // reinterpret the in-band null sentinel word as the float miss marker
        }
    }
}

/// Copies the raw bits of the float result register into the integer result register so a
/// caller can compare them against [`NULL_SENTINEL`] exactly. A bit comparison is required
/// here: float compare instructions report the sentinel NaN as *unordered*, not equal.
pub(crate) fn emit_float_result_bits_to_int_result(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction("fmov x0, d0");                                 // move the float payload bits where the sentinel comparison can see them
        }
        Arch::X86_64 => {
            emitter.instruction("movq rax, xmm0");                              // move the float payload bits where the sentinel comparison can see them
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

/// Byte offset of the creation line inside the 56-byte compact Throwable payload.
///
/// The payload is `class_id@0`, `message ptr@8`, `message len@16`, `code@24`, `previous@40`;
/// offset 32 was the one word never written, so the line fits without growing the allocation or
/// disturbing any existing reader.
///
/// PHP records the line where a Throwable is CONSTRUCTED, so every emitter that allocates this
/// payload must write the slot — `__rt_heap_alloc` recycles blocks without zeroing them, and an
/// unwritten slot hands `Throwable::getLine()` the previous owner's bytes. Emitters that cannot
/// know the line write `0`, which the readers treat as "origin unknown" and omit.
///
/// Read by `Throwable::getLine()` in `lower_inst.rs` and by `__rt_report_uncaught_exception`.
pub(crate) const THROWABLE_CREATION_LINE_OFFSET: u64 = 32;

/// Byte offset of the "this Throwable's recorded trace is COMPLETE" proof.
///
/// A trace that is SHORT is not an approximation — `#0 {main}` where php names a frame asserts the
/// stack was empty — so the report says nothing unless the chain is known whole. The proof travels
/// ON THE VALUE rather than in a global because it is a property of the site that CONSTRUCTED this
/// exception, and by report time that site is long gone: a global would still be holding whatever
/// the last construction left, and would answer for an exception it knows nothing about.
///
/// The payload is 56 bytes — class_id@0, message@8/16, code@24, line@32, previous@40 — so this is
/// the last slot, and it was unused.
pub(crate) const THROWABLE_TRACE_EXACT_OFFSET: u64 = 48;

/// Clears the creation-line slot of a freshly allocated Throwable payload in `payload_reg`.
///
/// For the emitters that synthesize a Throwable with no user `new` behind it — an
/// `ArithmeticError` from a division, a `ValueError` from a clamp, a `TypeError` from an argument
/// check — there is no source line to record, and PHP would report the internal call site rather
/// than anything these emitters know. Writing zero says "unknown" explicitly; leaving the slot
/// untouched would let recycled heap bytes read back as a plausible-looking line number.
pub(crate) fn emit_throwable_creation_line_unknown(emitter: &mut Emitter, payload_reg: &str) {
    // No user `new` behind this Throwable — it was built by a runtime helper, which is exactly
    // the position php-src's own internal throws are in. php names the CALL the program made
    // into the builtin, and that line is published at every call into a class the program did
    // not declare. Zero, the previous answer, is not a line php ever prints.
    let line_reg = match emitter.target.arch {
        Arch::AArch64 => "x9",
        Arch::X86_64 => "r10",
    };
    assert_ne!(
        line_reg, payload_reg,
        "the published line needs a register the payload does not already hold"
    );
    crate::codegen_support::abi::emit_load_symbol_to_reg(
        emitter,
        line_reg,
        "_rt_internal_call_line",
        0,
    );
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!(
            "str {}, [{}, #{}]",
            line_reg, payload_reg, THROWABLE_CREATION_LINE_OFFSET
        )), // the line of the call into the builtin, or zero when there was none
        Arch::X86_64 => emitter.instruction(&format!(
            "mov QWORD PTR [{} + {}], {}",
            payload_reg, THROWABLE_CREATION_LINE_OFFSET, line_reg
        )), // the line of the call into the builtin, or zero when there was none
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

    /// Locks the float reading of the null sentinel: it must be a quiet NaN (so no ordinary
    /// arithmetic result collides with it) and it must differ from PHP's `NAN` constant (so a
    /// genuinely stored `NAN` element is never mistaken for a missing key).
    #[test]
    fn null_sentinel_reads_as_a_distinct_quiet_nan() {
        let sentinel = f64::from_bits(NULL_SENTINEL as u64);
        assert!(sentinel.is_nan(), "float miss marker must be a NaN");
        // Quiet NaN: mantissa MSB set.
        assert_ne!(NULL_SENTINEL as u64 & (1 << 51), 0, "must be a quiet NaN");
        assert_ne!(
            NULL_SENTINEL as u64,
            f64::NAN.to_bits(),
            "must not collide with the canonical NAN a PHP program can store"
        );
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
