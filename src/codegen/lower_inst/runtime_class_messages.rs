//! Purpose:
//! Builds diagnostic messages that have to name the RUNTIME class of a value, so every guard
//! that refuses an object reports the class php-src would report instead of a generic `object`.
//!
//! Called from:
//! - `count()`'s non-`Countable` refusal in `builtins::count_empty`.
//! - The weak-mode typed-property guard in `objects::mixed_property_type_guard`.
//!
//! Key details:
//! - The class name comes from the same dense `_class_name_entries` table `get_class()` reads,
//!   so a message built here and a `get_class()` call on the same value can never disagree.
//! - The lookup is BOUNDS CHECKED against `_class_name_count`. The reserved incomplete-class id
//!   (`-2`) and any id a future runtime introduces would otherwise index past the table and put
//!   arbitrary bytes into a user-visible message; out-of-range ids fall back to a caller-chosen
//!   static name instead.
//! - `emit_static_diagnostic()` is the third copy of the suppressible-diagnostic fragment
//!   sequence this backend needs, so it lives here once instead of being re-spelled beside
//!   each guard that emits one.
//! - The name is BORROWED from the table. A caller that hands the composed message to a
//!   throwable must persist it (`__rt_str_persist`) exactly as it would for any other runtime
//!   string, because `__rt_concat` produces scratch storage.

use crate::codegen::abi;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;

use super::super::context::FunctionContext;

/// Leaves the runtime class name of the object in `object_reg` in the string-result registers.
///
/// `object_reg` holds a bare object pointer, whose first word is the runtime class id, NOT a
/// boxed `Mixed` cell: a caller holding a box unboxes first. `fallback_name` is emitted for an
/// object whose class id is outside the dense table, which is what the reserved incomplete-class
/// id produces.
///
/// The emitter clobbers the scratch pair it reads the table through, so `object_reg` must not be
/// one of them; every caller passes the target's canonical result register.
pub(in crate::codegen::lower_inst) fn emit_runtime_class_name_to_string_result(
    ctx: &mut FunctionContext<'_>,
    object_reg: &str,
    fallback_name: &str,
) {
    let fallback_label = ctx.next_label("runtime_class_name_fallback");
    let done_label = ctx.next_label("runtime_class_name_done");
    let fallback = ctx.data.add_string(fallback_name.as_bytes());
    emit_class_name_lookup(
        ctx.emitter,
        object_reg,
        &fallback,
        &fallback_label,
        &done_label,
    );
}

/// The target-specific half of the lookup, written against the emitter alone so both
/// architectures can be asserted without building a whole function context.
fn emit_class_name_lookup(
    emitter: &mut Emitter,
    object_reg: &str,
    fallback: &(String, usize),
    fallback_label: &str,
    done_label: &str,
) {
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.instruction(&format!("ldr x9, [{}]", object_reg));          // load the receiver's runtime class id
        }
        Arch::X86_64 => {
            emitter.instruction(&format!("mov r9, QWORD PTR [{}]", object_reg)); // load the receiver's runtime class id
        }
    }
    emit_class_name_table_read(emitter, fallback, fallback_label, done_label);
}

/// Reads the dense class-name row for the class id already parked in the lookup scratch register.
fn emit_class_name_table_read(
    emitter: &mut Emitter,
    fallback: &(String, usize),
    fallback_label: &str,
    done_label: &str,
) {
    let (name_ptr_reg, name_len_reg) = abi::string_result_regs(emitter);
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x10", "_class_name_count");
            emitter.instruction("ldr x10, [x10]");                              // load the dense class-name table bound
            emitter.instruction("cmp x9, x10");                                 // validate the class id before indexing metadata
            emitter.instruction(&format!("b.hs {}", fallback_label));           // ids outside the table use the static fallback name
            abi::emit_symbol_address(emitter, "x10", "_class_name_entries");
            emitter.instruction("lsl x11, x9, #4");                             // scale the class id to the 16-byte class-name row
            emitter.instruction("add x10, x10, x11");                           // address the receiver's class-name metadata row
            emitter.instruction(&format!("ldr {}, [x10]", name_ptr_reg));       // borrow the class-name pointer
            emitter.instruction(&format!("ldr {}, [x10, #8]", name_len_reg));   // borrow the class-name byte length
        }
        Arch::X86_64 => {
            emitter.instruction("mov r10, QWORD PTR [rip + _class_name_count]"); // load the dense class-name table bound
            emitter.instruction("cmp r9, r10");                                 // validate the class id before indexing metadata
            emitter.instruction(&format!("jae {}", fallback_label));            // ids outside the table use the static fallback name
            emitter.instruction("lea r10, [rip + _class_name_entries]");        // materialize the class-name metadata table base
            emitter.instruction("shl r9, 4");                                   // scale the class id to the 16-byte class-name row
            emitter.instruction(&format!("mov {}, QWORD PTR [r10 + r9]", name_ptr_reg)); // borrow the class-name pointer
            emitter.instruction(&format!(
                "mov {}, QWORD PTR [r10 + r9 + 8]",
                name_len_reg
            ));                                                                 // borrow the class-name byte length
        }
    }
    abi::emit_jump(emitter, done_label);
    emitter.label(fallback_label);
    abi::emit_symbol_address(emitter, name_ptr_reg, &fallback.0);
    abi::emit_load_int_immediate(emitter, name_len_reg, fallback.1 as i64);
    emitter.label(done_label);
}

/// Prepends a static fragment to the message held in the string-result registers.
pub(in crate::codegen::lower_inst) fn emit_concat_static_prefix(
    ctx: &mut FunctionContext<'_>,
    prefix: &str,
) {
    let (text_ptr, text_len) = abi::string_result_regs(ctx.emitter);
    let (right_ptr, right_len) = concat_right_operand_regs(ctx.emitter);
    let (prefix_label, prefix_len) = ctx.data.add_string(prefix.as_bytes());
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_ptr, text_ptr));              // move the built text into the concat right operand
    ctx.emitter
        .instruction(&format!("mov {}, {}", right_len, text_len));              // move its length into the concat right operand
    abi::emit_symbol_address(ctx.emitter, text_ptr, &prefix_label);
    abi::emit_load_int_immediate(ctx.emitter, text_len, prefix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// Appends a static fragment to the message held in the string-result registers.
pub(in crate::codegen::lower_inst) fn emit_concat_static_suffix(
    ctx: &mut FunctionContext<'_>,
    suffix: &str,
) {
    let (right_ptr, right_len) = concat_right_operand_regs(ctx.emitter);
    let (suffix_label, suffix_len) = ctx.data.add_string(suffix.as_bytes());
    abi::emit_symbol_address(ctx.emitter, right_ptr, &suffix_label);
    abi::emit_load_int_immediate(ctx.emitter, right_len, suffix_len as i64);
    abi::emit_call_label(ctx.emitter, "__rt_concat");
}

/// The register pair the suppressible-diagnostic helpers read their fragment from.
pub(in crate::codegen::lower_inst) fn diagnostic_fragment_regs(
    emitter: &Emitter,
) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}

/// Emits one suppressible static diagnostic fragment, completing the line when `complete`.
pub(in crate::codegen::lower_inst) fn emit_static_diagnostic(
    ctx: &mut FunctionContext<'_>,
    message: &str,
    complete: bool,
) {
    let (label, len) = ctx.data.add_string(message.as_bytes());
    let (ptr_reg, len_reg) = diagnostic_fragment_regs(ctx.emitter);
    abi::emit_symbol_address(ctx.emitter, ptr_reg, &label);
    abi::emit_load_int_immediate(ctx.emitter, len_reg, len as i64);
    abi::emit_call_label(
        ctx.emitter,
        if complete {
            "__rt_diag_warning"
        } else {
            "__rt_diag_warning_fragment"
        },
    );
}

/// The register pair `__rt_concat` reads its right operand from.
fn concat_right_operand_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    match emitter.target.arch {
        Arch::AArch64 => ("x3", "x4"),
        Arch::X86_64 => ("rdi", "rsi"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The class-name lookup must bound the class id before indexing the dense table, on every
    /// supported target.
    ///
    /// Host execution covers one architecture only, and the two halves of this emitter are
    /// written independently. A missing bound check or a wrong row scale is invisible on the
    /// host and puts arbitrary bytes from beyond the table into a user-visible `TypeError`
    /// everywhere else, so both halves are asserted here instead of at one runtime.
    #[test]
    fn the_class_name_lookup_is_bounds_checked_on_every_target() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            let object_reg = abi::int_result_reg(&emitter);
            let fallback = ("_fallback_name".to_string(), 6);
            emit_class_name_lookup(
                &mut emitter,
                object_reg,
                &fallback,
                ".L_fallback",
                ".L_done",
            );
            let asm = emitter.output();
            let bound = asm
                .find("_class_name_count")
                .unwrap_or_else(|| panic!("{name}: the lookup must bound the class id: {asm}"));
            let table = asm.find("_class_name_entries").unwrap_or_else(|| {
                panic!("{name}: the lookup must read the dense class-name table: {asm}")
            });
            assert!(
                bound < table,
                "{name}: the bound check must precede the table read, got `{asm}`"
            );
            assert!(
                asm.contains(".L_fallback"),
                "{name}: an out-of-range class id must reach the static fallback, got `{asm}`"
            );
        }
    }
}
