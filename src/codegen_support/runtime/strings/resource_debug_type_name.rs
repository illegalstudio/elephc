//! Purpose:
//! Emits `__rt_resource_debug_type_name`, the single place that maps a native PHP
//! resource payload to the name `get_debug_type()` prints — `resource (stream)` while
//! the handle is open, `resource (closed)` once it has been closed.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::builtins::scalar_metadata::lower_get_debug_type` for an
//!   unboxed `Resource` operand and for the tag-9 arm of a boxed Mixed operand.
//!
//! Key details:
//! - WHY THIS EXISTS. `get_debug_type()` used to answer the compile-time literal
//!   `"resource"`, which PHP never prints for a resource: measured on PHP 8.5.6, an open
//!   `fopen()`/`opendir()` handle is `"resource (stream)"` and a closed one — `fclose`,
//!   `pclose` and `closedir` alike — is `"resource (closed)"`. The parenthesised part is
//!   the resource's *display* type, which is why an already-closed handle stops naming
//!   what it used to be.
//! - IT IS NOT `__rt_resource_type_name`. That helper answers `get_resource_type()`,
//!   whose closed spelling is `"Unknown"`, not `"closed"` — the two names disagree on the
//!   same handle, so they cannot share a literal.
//! - THE CLOSED PREDICATE IS THE SIGN BIT, exactly as in
//!   `crate::codegen_support::runtime::strings::resource_type_name`:
//!   `apply_resource_release_sentinel` stamps `-id` into the payload word on close, and no
//!   live payload can be negative (descriptors are small positives, `DIR*`/`FILE*` handles
//!   are user-space addresses with bit 63 clear, `EVAL_RESOURCE_PAYLOAD_BASE` is `1 << 62`).
//! - BOTH NAMES ARE PERSISTENT `.data` LITERALS (`_resource_debug_type_stream` and
//!   `_resource_debug_type_closed`, defined beside `_resource_type_stream` in
//!   `crate::codegen_support::runtime::data::fixed`). Nothing is allocated, retained or
//!   freed here, so the release the EIR emits against a `get_debug_type()` result stays the
//!   no-op it is today against a `.data` pointer.
//! - IT IS A LEAF. The body is a sign test and two symbol loads with no `bl`/`call`, so
//!   `ret` is correct and the AArch64 LR-clobber rule does not apply.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Byte length of the open-resource debug name `"resource (stream)"`.
const RESOURCE_DEBUG_TYPE_STREAM_LEN: i64 = 17;

/// Byte length of the closed-resource debug name `"resource (closed)"`.
const RESOURCE_DEBUG_TYPE_CLOSED_LEN: i64 = 17;

/// Resolves a native resource payload to the name `get_debug_type()` prints for it.
///
/// # Inputs
/// - `x0` / `rax`: native resource payload, or the `-id` sentinel of a closed handle.
///
/// # Outputs
/// - `x1` / `rax`: pointer to the name bytes (the target's string-result pointer register)
/// - `x2` / `rdx`: name byte length (the target's string-result length register)
///
/// # ABI details
/// - Leaf helper: no nested `bl`/`call`, so it ends with `ret` on both targets.
/// - Clobbers only the string-result register pair; every other register is untouched.
pub fn emit_resource_debug_type_name(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resource_debug_type_name_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resource_debug_type_name (get_debug_type label for a resource) ---");
    emitter.label_global("__rt_resource_debug_type_name");

    emitter.instruction("tbnz x0, #63, __rt_resource_debug_type_name_closed");  // a negative payload is the -id sentinel an explicit close stamped
    abi::emit_symbol_address(emitter, "x1", "_resource_debug_type_stream");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_DEBUG_TYPE_STREAM_LEN); // an open handle keeps its display type in the parentheses
    emitter.instruction("ret");                                                 // return the open debug name without touching any other register

    emitter.label("__rt_resource_debug_type_name_closed");
    abi::emit_symbol_address(emitter, "x1", "_resource_debug_type_closed");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_DEBUG_TYPE_CLOSED_LEN); // PHP renames every closed resource to (closed), whatever it was
    emitter.instruction("ret");                                                 // return the closed debug name without touching any other register
}

/// x86_64 counterpart of `emit_resource_debug_type_name`.
///
/// The sign test must run BEFORE the symbol load, because `rax` is both the payload
/// input and the string-result pointer output on this target.
fn emit_resource_debug_type_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_debug_type_name (get_debug_type label for a resource) ---");
    emitter.label_global("__rt_resource_debug_type_name");

    emitter.instruction("test rax, rax");                                       // inspect the payload sign before rax is reused as the result pointer
    emitter.instruction("js __rt_resource_debug_type_name_closed_x86");         // a negative payload is the -id sentinel an explicit close stamped
    abi::emit_symbol_address(emitter, "rax", "_resource_debug_type_stream");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_DEBUG_TYPE_STREAM_LEN); // an open handle keeps its display type in the parentheses
    emitter.instruction("ret");                                                 // return the open debug name without touching any other register

    emitter.label("__rt_resource_debug_type_name_closed_x86");
    abi::emit_symbol_address(emitter, "rax", "_resource_debug_type_closed");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_DEBUG_TYPE_CLOSED_LEN); // PHP renames every closed resource to (closed), whatever it was
    emitter.instruction("ret");                                                 // return the closed debug name without touching any other register
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// The advertised lengths must equal the literals `data::fixed` defines, or
    /// `get_debug_type()` would print a truncated or over-long name.
    #[test]
    fn the_advertised_lengths_match_the_literal_bytes() {
        assert_eq!(RESOURCE_DEBUG_TYPE_STREAM_LEN as usize, "resource (stream)".len());
        assert_eq!(RESOURCE_DEBUG_TYPE_CLOSED_LEN as usize, "resource (closed)".len());
    }

    /// Pins the whole AArch64 body as an ordered, exact-line block.
    ///
    /// Full lines, not substrings: `contains("mov x2, #17")` also matches `mov x2, #170`,
    /// so a length or symbol swap would sail through a substring pin.
    #[test]
    fn aarch64_emits_the_full_open_and_closed_arms() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_resource_debug_type_name(&mut emitter);
        let asm = emitter.output();
        let expected = concat!(
            "__rt_resource_debug_type_name:\n",
            "    tbnz x0, #63, __rt_resource_debug_type_name_closed\n",
            "    adrp x1, _resource_debug_type_stream@PAGE\n",
            "    add x1, x1, _resource_debug_type_stream@PAGEOFF\n",
            "    mov x2, #17\n",
            "    ret\n",
            "__rt_resource_debug_type_name_closed:\n",
            "    adrp x1, _resource_debug_type_closed@PAGE\n",
            "    add x1, x1, _resource_debug_type_closed@PAGEOFF\n",
            "    mov x2, #17\n",
            "    ret\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// Pins the whole x86_64 body as an ordered, exact-line block, so the two targets
    /// cannot drift the way an aarch64-only pin has let them drift before.
    #[test]
    fn x86_64_emits_the_full_open_and_closed_arms() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_resource_debug_type_name(&mut emitter);
        let asm = emitter.output();
        let expected = concat!(
            "__rt_resource_debug_type_name:\n",
            "    test rax, rax\n",
            "    js __rt_resource_debug_type_name_closed_x86\n",
            "    lea rax, [rip + _resource_debug_type_stream]\n",
            "    mov rdx, 17\n",
            "    ret\n",
            "__rt_resource_debug_type_name_closed_x86:\n",
            "    lea rax, [rip + _resource_debug_type_closed]\n",
            "    mov rdx, 17\n",
            "    ret\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// Each arm must reference exactly ONE of the two literals. Both names are the same
    /// LENGTH, so a swapped pair would print a plausible-looking wrong name that no
    /// length assertion can catch.
    #[test]
    fn neither_arm_references_the_other_arms_literal_on_either_target() {
        for (target, closed_label) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "__rt_resource_debug_type_name_closed:\n",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "__rt_resource_debug_type_name_closed_x86:\n",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_debug_type_name(&mut emitter);
            let asm = emitter.output();
            let (open_arm, closed_arm) = asm
                .split_once(closed_label)
                .unwrap_or_else(|| panic!("missing closed arm for {target:?}:\n{asm}"));
            assert!(
                !open_arm.contains("_resource_debug_type_closed"),
                "the open arm must not name the closed literal ({target:?}):\n{open_arm}"
            );
            assert!(
                open_arm.contains("_resource_debug_type_stream"),
                "the open arm must name the open literal ({target:?}):\n{open_arm}"
            );
            assert!(
                !closed_arm.contains("_resource_debug_type_stream"),
                "the closed arm must not name the open literal ({target:?}):\n{closed_arm}"
            );
            assert!(
                closed_arm.contains("_resource_debug_type_closed"),
                "the closed arm must name the closed literal ({target:?}):\n{closed_arm}"
            );
        }
    }

    /// The helper must stay a leaf on AArch64: a body containing `bl` would have to end
    /// `b __rt_next` instead of `ret`, and the call sites invoke it mid-render.
    #[test]
    fn aarch64_stays_a_leaf_helper() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_resource_debug_type_name(&mut emitter);
        let asm = emitter.output();
        assert!(!asm.contains("    bl "), "must contain no nested call:\n{asm}");
        assert!(asm.contains("    ret\n"), "must return with ret:\n{asm}");
    }

    /// The helper must never consult the resource-id registry: reading `_resource_id_next`
    /// here would mint an id for a handle that is merely being named and shift the id the
    /// next `fopen()` is owed.
    #[test]
    fn the_helper_never_touches_the_resource_id_registry_on_either_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_debug_type_name(&mut emitter);
            let asm = emitter.output();
            assert!(
                !asm.contains("_resource_id_next")
                    && !asm.contains("_resource_id_keys")
                    && !asm.contains("_resource_id_vals"),
                "the debug-name helper must not reach the id registry ({target:?}):\n{asm}"
            );
        }
    }

    /// `get_resource_type()` and `get_debug_type()` disagree on a closed handle
    /// (`"Unknown"` vs `"resource (closed)"`), so the two helpers must not share literals.
    #[test]
    fn the_debug_name_helper_does_not_reuse_the_get_resource_type_literals() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_debug_type_name(&mut emitter);
            let asm = emitter.output();
            assert!(
                !asm.contains("_resource_type_unknown"),
                "get_debug_type must not print the get_resource_type closed name ({target:?}):\n{asm}"
            );
        }
    }
}
