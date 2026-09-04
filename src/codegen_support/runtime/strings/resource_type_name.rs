//! Purpose:
//! Emits `__rt_resource_type_name`, the single place that maps a native PHP
//! resource payload to the type NAME every display site prints — `stream` while the
//! handle is open, `Unknown` once it has been closed.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::strings`.
//! - `crate::codegen::lower_inst::builtins::debug::emit_var_dump_resource` (the
//!   `) of type (` field of `resource(N) of type (T)`).
//! - `crate::codegen::lower_inst::builtins::types::lower_get_resource_type`.
//!
//! Key details:
//! - WHY THIS EXISTS. Both display sites used to bake the name in as a compile-time
//!   literal, so a closed handle kept advertising its original type. Under PHP 8.5.6
//!   `fclose($r); var_dump($r);` prints `resource(5) of type (Unknown)` and
//!   `get_resource_type($r)` answers `"Unknown"` — measured for `fclose`, `pclose`
//!   and `closedir` alike, all three of which collapse to the same name.
//! - THE CLOSED PREDICATE IS THE SIGN BIT, and it is already load-bearing.
//!   `apply_resource_release_sentinel` (`crate::codegen::lower_inst::builtins::io`)
//!   stamps `-id` into the Mixed box's low payload word on close, and
//!   `__rt_resource_id_of` (`crate::codegen_support::runtime::resource_ids`) already
//!   branches on the same bit to recover the id without minting. No real payload can
//!   be negative: descriptors are small positives, `DIR*`/`FILE*`/HashContext handles
//!   are user-space addresses with bit 63 clear, and `EVAL_RESOURCE_PAYLOAD_BASE` is
//!   `1 << 62`.
//! - BOTH NAMES ARE PERSISTENT `.data` LITERALS (`_resource_type_stream` and
//!   `_resource_type_unknown`, defined beside `_resource_id_prefix` in
//!   `crate::codegen_support::runtime::data::fixed`). Nothing is allocated, retained or
//!   freed here, so the `release` the EIR already emits against a `get_resource_type`
//!   result stays the no-op it is today against a `.data` pointer. Returning a
//!   runtime-allocated string instead would turn that release into a double free.
//! - IT IS A LEAF. The body is a sign test and two symbol loads with no `bl`/`call`, so
//!   `ret` is correct and the AArch64 LR-clobber rule does not apply.
//! - ONE PLACE, NOT TWO. The implicit default stream context has a reserved payload so it
//!   can be distinguished without consulting the mutable resource inventory.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::resource_inventory::DEFAULT_CONTEXT_PAYLOAD;

/// Byte length of the open-resource type name `"stream"`.
const RESOURCE_TYPE_STREAM_LEN: i64 = 6;

/// Byte length of the closed-resource type name `"Unknown"`.
const RESOURCE_TYPE_UNKNOWN_LEN: i64 = 7;

/// Resolves a native resource payload to its PHP resource type name.
///
/// # Inputs
/// - `x0` / `rax`: native resource payload, or the `-id` sentinel of a closed handle.
///
/// # Outputs
/// - `x1` / `rax`: pointer to the type-name bytes (the target's string-result pointer register)
/// - `x2` / `rdx`: type-name byte length (the target's string-result length register)
///
/// # ABI details
/// - Leaf helper: no nested `bl`/`call`, so it ends with `ret` on both targets.
/// - Clobbers only the string-result register pair; every other register is untouched.
pub fn emit_resource_type_name(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resource_type_name_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resource_type_name (PHP type label for a resource payload) ---");
    emitter.label_global("__rt_resource_type_name");

    abi::emit_load_int_immediate(emitter, "x9", DEFAULT_CONTEXT_PAYLOAD);
    emitter.instruction("cmp x0, x9");                                          // recognize PHP's implicit default stream context
    emitter.instruction("b.eq __rt_resource_type_name_context");                // the reserved context has its own type label
    emitter.instruction("tbnz x0, #63, __rt_resource_type_name_closed");        // a negative payload is the -id sentinel an explicit close stamped
    abi::emit_symbol_address(emitter, "x1", "_resource_type_stream");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_TYPE_STREAM_LEN);      // an open resource reports the type it was created with
    emitter.instruction("ret");                                                 // return the open type name without touching any other register

    emitter.label("__rt_resource_type_name_closed");
    abi::emit_symbol_address(emitter, "x1", "_resource_type_unknown");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_TYPE_UNKNOWN_LEN);     // PHP renames every closed resource to Unknown, whatever it was
    emitter.instruction("ret");                                                 // return the closed type name without touching any other register

    emitter.label("__rt_resource_type_name_context");
    abi::emit_symbol_address(emitter, "x1", "_resource_type_stream_context");
    abi::emit_load_int_immediate(emitter, "x2", 14);                           // byte length of stream-context
    emitter.instruction("ret");                                                 // return the implicit context type name
}

/// x86_64 counterpart of `emit_resource_type_name`.
///
/// The sign test must run BEFORE the symbol load, because `rax` is both the payload
/// input and the string-result pointer output on this target.
fn emit_resource_type_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_type_name (PHP type label for a resource payload) ---");
    emitter.label_global("__rt_resource_type_name");

    abi::emit_load_int_immediate(emitter, "r10", DEFAULT_CONTEXT_PAYLOAD);
    emitter.instruction("cmp rax, r10");                                        // recognize PHP's implicit default stream context
    emitter.instruction("je __rt_resource_type_name_context_x86");              // the reserved context has its own type label
    emitter.instruction("test rax, rax");                                       // inspect the payload sign before rax is reused as the result pointer
    emitter.instruction("js __rt_resource_type_name_closed_x86");               // a negative payload is the -id sentinel an explicit close stamped
    abi::emit_symbol_address(emitter, "rax", "_resource_type_stream");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_TYPE_STREAM_LEN);     // an open resource reports the type it was created with
    emitter.instruction("ret");                                                 // return the open type name without touching any other register

    emitter.label("__rt_resource_type_name_closed_x86");
    abi::emit_symbol_address(emitter, "rax", "_resource_type_unknown");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_TYPE_UNKNOWN_LEN);    // PHP renames every closed resource to Unknown, whatever it was
    emitter.instruction("ret");                                                 // return the closed type name without touching any other register

    emitter.label("__rt_resource_type_name_context_x86");
    abi::emit_symbol_address(emitter, "rax", "_resource_type_stream_context");
    abi::emit_load_int_immediate(emitter, "rdx", 14);                           // byte length of stream-context
    emitter.instruction("ret");                                                 // return the implicit context type name
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Platform, Target};

    /// The advertised lengths must equal the literals `data::fixed` defines, or the
    /// display sites would print truncated or over-long names.
    #[test]
    fn the_advertised_lengths_match_the_literal_bytes() {
        assert_eq!(RESOURCE_TYPE_STREAM_LEN as usize, "stream".len());
        assert_eq!(RESOURCE_TYPE_UNKNOWN_LEN as usize, "Unknown".len());
    }

    /// Pins the whole AArch64 body as an ordered, exact-line block.
    ///
    /// Full lines, not substrings: `contains("mov x2, #6")` also matches `mov x2, #64`,
    /// so a length or symbol swap would sail through a substring pin.
    #[test]
    fn aarch64_emits_the_full_open_and_closed_arms() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_resource_type_name(&mut emitter);
        let asm = emitter.output();
        let expected = concat!(
            "__rt_resource_type_name:\n",
            "    movz x9, #0xffff\n",
            "    movk x9, #0xffff, lsl #16\n",
            "    movk x9, #0xffff, lsl #32\n",
            "    movk x9, #0x3fff, lsl #48\n",
            "    cmp x0, x9\n",
            "    b.eq __rt_resource_type_name_context\n",
            "    tbnz x0, #63, __rt_resource_type_name_closed\n",
            "    adrp x1, _resource_type_stream@PAGE\n",
            "    add x1, x1, _resource_type_stream@PAGEOFF\n",
            "    mov x2, #6\n",
            "    ret\n",
            "__rt_resource_type_name_closed:\n",
            "    adrp x1, _resource_type_unknown@PAGE\n",
            "    add x1, x1, _resource_type_unknown@PAGEOFF\n",
            "    mov x2, #7\n",
            "    ret\n",
            "__rt_resource_type_name_context:\n",
            "    adrp x1, _resource_type_stream_context@PAGE\n",
            "    add x1, x1, _resource_type_stream_context@PAGEOFF\n",
            "    mov x2, #14\n",
            "    ret\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// Pins the whole x86_64 body as an ordered, exact-line block, so the two targets
    /// cannot drift the way an aarch64-only pin has let them drift before.
    #[test]
    fn x86_64_emits_the_full_open_and_closed_arms() {
        let mut emitter = Emitter::new(Target::new(Platform::Linux, Arch::X86_64));
        emit_resource_type_name(&mut emitter);
        let asm = emitter.output();
        let expected = concat!(
            "__rt_resource_type_name:\n",
            "    mov r10, 4611686018427387903\n",
            "    cmp rax, r10\n",
            "    je __rt_resource_type_name_context_x86\n",
            "    test rax, rax\n",
            "    js __rt_resource_type_name_closed_x86\n",
            "    lea rax, [rip + _resource_type_stream]\n",
            "    mov rdx, 6\n",
            "    ret\n",
            "__rt_resource_type_name_closed_x86:\n",
            "    lea rax, [rip + _resource_type_unknown]\n",
            "    mov rdx, 7\n",
            "    ret\n",
            "__rt_resource_type_name_context_x86:\n",
            "    lea rax, [rip + _resource_type_stream_context]\n",
            "    mov rdx, 14\n",
            "    ret\n",
        );
        assert!(asm.contains(expected), "expected block missing:\n{asm}");
    }

    /// Each arm must reference exactly ONE of the two literals. Without this a swapped
    /// pair — open resources named `Unknown`, closed ones named `stream` — still matches
    /// any pin that only checks both symbols are present somewhere in the body.
    #[test]
    fn neither_arm_references_the_other_arms_literal_on_either_target() {
        for (target, closed_label) in [
            (
                Target::new(Platform::MacOS, Arch::AArch64),
                "__rt_resource_type_name_closed:\n",
            ),
            (
                Target::new(Platform::Linux, Arch::X86_64),
                "__rt_resource_type_name_closed_x86:\n",
            ),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_type_name(&mut emitter);
            let asm = emitter.output();
            let (open_arm, closed_and_context) = asm
                .split_once(closed_label)
                .unwrap_or_else(|| panic!("missing closed arm for {target:?}:\n{asm}"));
            let context_label = match target.arch {
                Arch::AArch64 => "__rt_resource_type_name_context:\n",
                Arch::X86_64 => "__rt_resource_type_name_context_x86:\n",
            };
            let (closed_arm, context_arm) = closed_and_context
                .split_once(context_label)
                .unwrap_or_else(|| panic!("missing context arm for {target:?}:\n{asm}"));
            assert!(
                !open_arm.contains("_resource_type_unknown"),
                "the open arm must not name the closed literal ({target:?}):\n{open_arm}"
            );
            assert!(
                open_arm.contains("_resource_type_stream"),
                "the open arm must name the open literal ({target:?}):\n{open_arm}"
            );
            assert!(
                !closed_arm.contains("_resource_type_stream"),
                "the closed arm must not name the open literal ({target:?}):\n{closed_arm}"
            );
            assert!(
                closed_arm.contains("_resource_type_unknown"),
                "the closed arm must name the closed literal ({target:?}):\n{closed_arm}"
            );
            assert!(
                context_arm.contains("_resource_type_stream_context"),
                "the context arm must name the context literal ({target:?}):\n{context_arm}"
            );
        }
    }

    /// The helper must stay a leaf on AArch64: a body containing `bl` would have to end
    /// `b __rt_next` instead of `ret`, and both call sites invoke it mid-render with a
    /// live `x30`.
    #[test]
    fn aarch64_stays_a_leaf_helper() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_resource_type_name(&mut emitter);
        let asm = emitter.output();
        assert!(!asm.contains("    bl "), "must contain no nested call:\n{asm}");
        assert!(asm.contains("    ret\n"), "must return with ret:\n{asm}");
    }

    /// The helper must never consult the resource-id registry: reading `_resource_id_next`
    /// here would mint an id for a handle that is merely being displayed and shift the id
    /// the next `fopen()` is owed.
    #[test]
    fn the_helper_never_touches_the_resource_id_registry_on_either_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_resource_type_name(&mut emitter);
            let asm = emitter.output();
            assert!(
                !asm.contains("_resource_id_next")
                    && !asm.contains("_resource_id_keys")
                    && !asm.contains("_resource_id_vals"),
                "the type-name helper must not reach the id registry ({target:?}):\n{asm}"
            );
        }
    }
}
