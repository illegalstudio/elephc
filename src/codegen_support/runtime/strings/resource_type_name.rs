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
//! - THE CLOSED PREDICATE IS THE REGISTRY, with the sign bit kept as the legacy
//!   fallback. Since the generation-safe registry migration, a migrated resource's
//!   payload is an OPAQUE HANDLE, not a descriptor, and close publishes the Closed
//!   state on the registry slot instead of stamping a sentinel — so the sign test
//!   alone reported `stream` forever on a closed handle. The body now resolves the
//!   payload with `__rt_resource_lookup_any`: a slot whose status is not Live is
//!   closed, and NO slot at all means the payload is still a raw descriptor from an
//!   unmigrated path, which is therefore open. Dropping the raw-descriptor fallback
//!   would report `Unknown` for resources that are genuinely open.
//! - THE LEGACY SIGN BIT still short-circuits first, and it is load-bearing.
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
//! - IT IS NO LONGER A LEAF. Consulting the registry costs one `bl`/`call`, so the
//!   AArch64 LR-clobber rule now applies: the body saves `x30` before the lookup and
//!   restores it on both post-lookup arms. The sentinel path branches out BEFORE the
//!   frame is reserved, which is why the closed literal has two entry labels — one
//!   that releases the frame and one that never took it.
//! - ONE PLACE, NOT TWO. PHP has further resource type names this compiler does not yet
//!   distinguish (`stream-context` from `stream_context_create()`, `stream filter` from
//!   `stream_filter_append()`). Concentrating the payload-to-name mapping here keeps
//!   that a one-file extension rather than a change to every display site.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::resources::layout::{
    RESOURCE_KIND_CONTEXT, RESOURCE_KIND_FILTER, RESOURCE_STATUS_LIVE, SLOT_KIND_OFFSET,
    SLOT_STATUS_OFFSET,
};

/// Byte length of the open-resource type name `"stream"`.
const RESOURCE_TYPE_STREAM_LEN: i64 = 6;

/// Byte length of the filter-resource type name `"stream filter"`.
///
/// php gives a filter its own resource type: `var_dump(stream_filter_append(...))` prints
/// `resource(6) of type (stream filter)`, and `get_resource_type()` agrees. Reporting `stream`
/// for it made the two kinds indistinguishable from PHP even though the registry has told them
/// apart since filters became resources.
const RESOURCE_TYPE_STREAM_FILTER_LEN: i64 = 13;

/// Byte length of the context-resource type name `"stream-context"`.
///
/// php gives a context its own type: `var_dump(stream_context_create([]))` prints
/// `resource(6) of type (stream-context)`. `get_resource_type()` already answered that here,
/// through a different path, so the two disagreed about the same resource.
const RESOURCE_TYPE_STREAM_CONTEXT_LEN: i64 = 14;

/// Byte length of the closed-resource type name `"Unknown"`.
const RESOURCE_TYPE_UNKNOWN_LEN: i64 = 7;

/// Resolves a native resource payload to its PHP resource type name.
///
/// # Inputs
/// - `x0` / `rax`: opaque registry handle, a raw descriptor from an unmigrated path,
///   or the legacy `-id` sentinel of a closed handle.
///
/// # Outputs
/// - `x1` / `rax`: pointer to the type-name bytes (the target's string-result pointer register)
/// - `x2` / `rdx`: type-name byte length (the target's string-result length register)
///
/// # ABI details
/// - Calls `__rt_resource_lookup_any`, so it saves and restores `x30` around the lookup
///   and still ends with `ret` on both targets.
/// - Clobbers the string-result register pair plus whatever the lookup clobbers. The
///   single call site (`emit_var_dump_resource`) keeps nothing else live across it: the
///   payload is popped into the result register immediately before, and the emitted
///   `write_stdout` immediately after consumes only the returned pair.
pub fn emit_resource_type_name(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_resource_type_name_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: resource_type_name (PHP type label for a resource payload) ---");
    emitter.label_global("__rt_resource_type_name");

    emitter.instruction("tbnz x0, #63, __rt_resource_type_name_closed");        // a negative payload is the legacy -id sentinel an explicit close stamped
    emitter.instruction("sub sp, sp, #16");                                     // reserve a frame for the registry lookup
    emitter.instruction("str x30, [sp, #8]");                                   // save the caller link register across the call
    emitter.instruction("bl __rt_resource_lookup_any");                         // resolve the payload as an opaque registry handle
    emitter.instruction("cbz x0, __rt_resource_type_name_open_pop");            // no slot: a legacy raw descriptor, still open
    emitter.instruction(&format!(
        "ldr x9, [x0, #{}]", SLOT_STATUS_OFFSET
    ));                                                                         // load the lifecycle status of the resolved slot
    emitter.instruction(&format!(
        "cmp x9, #{}", RESOURCE_STATUS_LIVE
    ));                                                                         // only a Live slot still reports its original type
    emitter.instruction("b.ne __rt_resource_type_name_closed_pop");             // Closing and Closed both render as Unknown
    emitter.instruction(&format!("ldr x9, [x0, #{}]", SLOT_KIND_OFFSET));       // which kind of resource is in this slot?
    emitter.instruction(&format!("cmp x9, #{}", RESOURCE_KIND_FILTER));
    emitter.instruction("b.eq __rt_resource_type_name_filter_pop");             // a filter names itself
    emitter.instruction(&format!("cmp x9, #{}", RESOURCE_KIND_CONTEXT));
    emitter.instruction("b.eq __rt_resource_type_name_context_pop");            // and so does a context
    emitter.label("__rt_resource_type_name_open_pop");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the lookup frame
    abi::emit_symbol_address(emitter, "x1", "_resource_type_stream");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_TYPE_STREAM_LEN);      // an open resource reports the type it was created with
    emitter.instruction("ret");                                                 // return the open type name

    emitter.label("__rt_resource_type_name_filter_pop");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the lookup frame
    abi::emit_symbol_address(emitter, "x1", "_resource_type_stream_filter");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_TYPE_STREAM_FILTER_LEN); // php's own name for a filter
    emitter.instruction("ret");                                                 // return the filter type name

    emitter.label("__rt_resource_type_name_context_pop");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the lookup frame
    abi::emit_symbol_address(emitter, "x1", "_resource_type_stream_context");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_TYPE_STREAM_CONTEXT_LEN); // php's own name for a context
    emitter.instruction("ret");                                                 // return the context type name

    emitter.label("__rt_resource_type_name_closed_pop");
    emitter.instruction("ldr x30, [sp, #8]");                                   // restore the caller link register
    emitter.instruction("add sp, sp, #16");                                     // release the lookup frame
    emitter.label("__rt_resource_type_name_closed");
    abi::emit_symbol_address(emitter, "x1", "_resource_type_unknown");
    abi::emit_load_int_immediate(emitter, "x2", RESOURCE_TYPE_UNKNOWN_LEN);     // PHP renames every closed resource to Unknown, whatever it was
    emitter.instruction("ret");                                                 // return the closed type name without touching any other register
}

/// x86_64 counterpart of `emit_resource_type_name`.
///
/// The sign test must run BEFORE the symbol load, because `rax` is both the payload
/// input and the string-result pointer output on this target.
fn emit_resource_type_name_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_type_name (PHP type label for a resource payload) ---");
    emitter.label_global("__rt_resource_type_name");

    emitter.instruction("test rax, rax");                                       // inspect the payload sign before rax is reused as the result pointer
    emitter.instruction("js __rt_resource_type_name_closed_x86");               // a negative payload is the legacy -id sentinel an explicit close stamped
    emitter.instruction("sub rsp, 8");                                          // realign the stack for the registry lookup
    emitter.instruction("mov rdi, rax");                                        // pass the payload as an opaque registry handle
    emitter.instruction("call __rt_resource_lookup_any");                       // resolve the handle to its registry slot
    emitter.instruction("test rax, rax");                                       // did the payload resolve to a slot at all?
    emitter.instruction("jz __rt_resource_type_name_open_x86");                 // no slot: a legacy raw descriptor, still open
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}", SLOT_STATUS_OFFSET, RESOURCE_STATUS_LIVE
    ));                                                                         // only a Live slot still reports its original type
    emitter.instruction("jne __rt_resource_type_name_closed_pop_x86");          // Closing and Closed both render as Unknown
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}", SLOT_KIND_OFFSET, RESOURCE_KIND_FILTER
    ));                                                                         // which kind of resource is in this slot?
    emitter.instruction("je __rt_resource_type_name_filter_pop_x86");           // a filter names itself
    emitter.instruction(&format!(
        "cmp QWORD PTR [rax + {}], {}", SLOT_KIND_OFFSET, RESOURCE_KIND_CONTEXT
    ));
    emitter.instruction("je __rt_resource_type_name_context_pop_x86");          // and so does a context
    emitter.label("__rt_resource_type_name_open_x86");
    emitter.instruction("add rsp, 8");                                          // release the alignment padding
    abi::emit_symbol_address(emitter, "rax", "_resource_type_stream");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_TYPE_STREAM_LEN);     // an open resource reports the type it was created with
    emitter.instruction("ret");                                                 // return the open type name without touching any other register

    emitter.label("__rt_resource_type_name_filter_pop_x86");
    emitter.instruction("add rsp, 8");                                          // release the alignment padding
    abi::emit_symbol_address(emitter, "rax", "_resource_type_stream_filter");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_TYPE_STREAM_FILTER_LEN); // php's own name for a filter
    emitter.instruction("ret");                                                 // return the filter type name

    emitter.label("__rt_resource_type_name_context_pop_x86");
    emitter.instruction("add rsp, 8");                                          // release the alignment padding
    abi::emit_symbol_address(emitter, "rax", "_resource_type_stream_context");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_TYPE_STREAM_CONTEXT_LEN); // php's own name for a context
    emitter.instruction("ret");                                                 // return the context type name

    emitter.label("__rt_resource_type_name_closed_pop_x86");
    emitter.instruction("add rsp, 8");                                          // release the alignment padding
    emitter.label("__rt_resource_type_name_closed_x86");
    abi::emit_symbol_address(emitter, "rax", "_resource_type_unknown");
    abi::emit_load_int_immediate(emitter, "rdx", RESOURCE_TYPE_UNKNOWN_LEN);    // PHP renames every closed resource to Unknown, whatever it was
    emitter.instruction("ret");                                                 // return the closed type name without touching any other register
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
            "    tbnz x0, #63, __rt_resource_type_name_closed\n",
            "    sub sp, sp, #16\n",
            "    str x30, [sp, #8]\n",
            "    bl __rt_resource_lookup_any\n",
            "    cbz x0, __rt_resource_type_name_open_pop\n",
            "    ldr x9, [x0, #16]\n",
            "    cmp x9, #1\n",
            "    b.ne __rt_resource_type_name_closed_pop\n",
            "    ldr x9, [x0, #8]\n",
            "    cmp x9, #3\n",
            "    b.eq __rt_resource_type_name_filter_pop\n",
            "    cmp x9, #2\n",
            "    b.eq __rt_resource_type_name_context_pop\n",
            "__rt_resource_type_name_open_pop:\n",
            "    ldr x30, [sp, #8]\n",
            "    add sp, sp, #16\n",
            "    adrp x1, _resource_type_stream@PAGE\n",
            "    add x1, x1, _resource_type_stream@PAGEOFF\n",
            "    mov x2, #6\n",
            "    ret\n",
            "__rt_resource_type_name_filter_pop:\n",
            "    ldr x30, [sp, #8]\n",
            "    add sp, sp, #16\n",
            "    adrp x1, _resource_type_stream_filter@PAGE\n",
            "    add x1, x1, _resource_type_stream_filter@PAGEOFF\n",
            "    mov x2, #13\n",
            "    ret\n",
            "__rt_resource_type_name_context_pop:\n",
            "    ldr x30, [sp, #8]\n",
            "    add sp, sp, #16\n",
            "    adrp x1, _resource_type_stream_context@PAGE\n",
            "    add x1, x1, _resource_type_stream_context@PAGEOFF\n",
            "    mov x2, #14\n",
            "    ret\n",
            "__rt_resource_type_name_closed_pop:\n",
            "    ldr x30, [sp, #8]\n",
            "    add sp, sp, #16\n",
            "__rt_resource_type_name_closed:\n",
            "    adrp x1, _resource_type_unknown@PAGE\n",
            "    add x1, x1, _resource_type_unknown@PAGEOFF\n",
            "    mov x2, #7\n",
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
            "    test rax, rax\n",
            "    js __rt_resource_type_name_closed_x86\n",
            "    sub rsp, 8\n",
            "    mov rdi, rax\n",
            "    call __rt_resource_lookup_any\n",
            "    test rax, rax\n",
            "    jz __rt_resource_type_name_open_x86\n",
            "    cmp QWORD PTR [rax + 16], 1\n",
            "    jne __rt_resource_type_name_closed_pop_x86\n",
            "    cmp QWORD PTR [rax + 8], 3\n",
            "    je __rt_resource_type_name_filter_pop_x86\n",
            "    cmp QWORD PTR [rax + 8], 2\n",
            "    je __rt_resource_type_name_context_pop_x86\n",
            "__rt_resource_type_name_open_x86:\n",
            "    add rsp, 8\n",
            "    lea rax, [rip + _resource_type_stream]\n",
            "    mov rdx, 6\n",
            "    ret\n",
            "__rt_resource_type_name_filter_pop_x86:\n",
            "    add rsp, 8\n",
            "    lea rax, [rip + _resource_type_stream_filter]\n",
            "    mov rdx, 13\n",
            "    ret\n",
            "__rt_resource_type_name_context_pop_x86:\n",
            "    add rsp, 8\n",
            "    lea rax, [rip + _resource_type_stream_context]\n",
            "    mov rdx, 14\n",
            "    ret\n",
            "__rt_resource_type_name_closed_pop_x86:\n",
            "    add rsp, 8\n",
            "__rt_resource_type_name_closed_x86:\n",
            "    lea rax, [rip + _resource_type_unknown]\n",
            "    mov rdx, 7\n",
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
            let (open_arm, closed_arm) = asm
                .split_once(closed_label)
                .unwrap_or_else(|| panic!("missing closed arm for {target:?}:\n{asm}"));
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
        }
    }

    /// The helper is NO LONGER a leaf, so every path that calls the registry must save
    /// and restore `x30` — the single call site in `emit_var_dump_resource` invokes it
    /// mid-render with a live link register, and a clobbered `x30` would return into the
    /// middle of the caller's output sequence.
    ///
    /// The sentinel path is deliberately exempt: it branches to the closed arm BEFORE the
    /// frame is reserved, which is why the closed literal has two entries — one that pops
    /// the frame and one that does not.
    #[test]
    fn every_calling_path_saves_and_restores_the_link_register() {
        let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
        emit_resource_type_name(&mut emitter);
        let asm = emitter.output();
        assert_eq!(
            asm.matches("    bl ").count(),
            1,
            "exactly one registry lookup is expected:\n{asm}"
        );
        assert_eq!(
            asm.matches("    str x30, [sp, #8]\n").count(),
            1,
            "the link register must be saved once, before the lookup:\n{asm}"
        );
        assert_eq!(
            asm.matches("    ldr x30, [sp, #8]\n").count(),
            4,
            "every post-lookup arm must restore the link register:\n{asm}"
        );
        assert_eq!(
            asm.matches("    sub sp, sp, #16\n").count(),
            1,
            "the frame is reserved once:\n{asm}"
        );
        assert_eq!(
            asm.matches("    add sp, sp, #16\n").count(),
            4,
            "every post-lookup arm must release the frame:\n{asm}"
        );
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
