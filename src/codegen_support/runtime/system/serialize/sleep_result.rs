//! Purpose:
//! Iterates raw or boxed __sleep() name arrays while retaining their owned return storage.
//!
//! Called from:
//! - The native object serializer after its __sleep() method returns.
//!
//! Key details:
//! - Indexed and associative name arrays share the logical runtime iterator.
//! - The enclosing magic-result boundary owns names and converted strings across nested throws.
//! - Invalid return shapes warn and replace the provisional object prefix with PHP null.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};
use crate::codegen_support::runtime::data::{SLEEP_WARNING_PREFIX, SLEEP_WARNING_SUFFIX};

const FRAME: usize = 96;
const CONTEXT: usize = 8;
const NAMES: usize = 16;
const OBJECT: usize = 24;
const CURSOR: usize = 32;
const CONCAT: usize = 40;
const VALUE_HI: usize = 48;
const VALUE_LO: usize = 56;
const VALUE_TAG: usize = 64;

/// Selects the target encoding for one shared serializer operation.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // emit the target-specific form of this shared serialization step
}

/// Compares the integer result and branches only within the current helper.
fn branch_eq(emitter: &mut Emitter, value: i64, label: &str) {
    let arm = if value < 0 { format!("cmn x0, #{}", -value) } else { format!("cmp x0, #{value}") };
    ins(emitter, &arm, &format!("cmp rax, {value}"));
    ins(emitter, &format!("b.eq {label}"), &format!("je {label}"));
}

/// Appends literal syntax without changing the owner slots or the cursor.
fn append(emitter: &mut Emitter, bytes: &[u8]) {
    match emitter.target.arch {
        Arch::AArch64 => super::emit_append_literal_aarch64(emitter, bytes, "sleep result syntax"),
        Arch::X86_64 => super::emit_append_literal_x86_64(emitter, bytes, "sleep result syntax"),
    }
}

/// Emits the owning result boundary and its borrowed names-array iteration callback.
pub(super) fn emit_sleep_result(emitter: &mut Emitter) {
    super::magic_result::emit_owned_result_boundary(
        emitter, "__rt_serialize_sleep_result", "__rt_serialize_sleep_body", true,
    );
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let (low, high) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rdx"),
    };
    emitter.blank();
    emitter.label_global("__rt_serialize_sleep_body");
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, result, CONTEXT);
    abi::emit_load_from_address(emitter, scratch, result, 24);
    abi::store_at_offset(emitter, scratch, OBJECT);
    abi::emit_load_from_address(emitter, result, result, 16);
    abi::store_at_offset(emitter, result, NAMES);
    abi::emit_call_label(emitter, "__rt_heap_kind");
    branch_eq(emitter, 5, "__rt_sleep_boxed_names");
    branch_eq(emitter, 2, "__rt_sleep_names_ready");
    branch_eq(emitter, 3, "__rt_sleep_names_ready");
    abi::emit_jump(emitter, "__rt_sleep_invalid_return");
    emitter.label("__rt_sleep_boxed_names");
    abi::load_at_offset(emitter, result, NAMES);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    branch_eq(emitter, 4, "__rt_sleep_unboxed_names");
    branch_eq(emitter, 5, "__rt_sleep_unboxed_names");
    abi::emit_jump(emitter, "__rt_sleep_invalid_return");
    emitter.label("__rt_sleep_unboxed_names");
    abi::store_at_offset(emitter, low, NAMES);
    emitter.label("__rt_sleep_names_ready");
    abi::load_at_offset(emitter, result, NAMES);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_call_label(emitter, "__rt_serialize_uint");
    append(emitter, b":{");
    abi::emit_store_zero_to_local_slot(emitter, CURSOR);

    emitter.label("__rt_sleep_name_loop");
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), NAMES);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    branch_eq(emitter, -1, "__rt_sleep_names_done");
    abi::store_at_offset(emitter, result, CURSOR);
    let (tag, lo, hi) = match emitter.target.arch {
        Arch::AArch64 => ("x3", "x4", "x5"),
        Arch::X86_64 => ("r8", "r9", "r10"),
    };
    abi::store_at_offset(emitter, tag, VALUE_TAG);
    abi::store_at_offset(emitter, lo, VALUE_LO);
    abi::store_at_offset(emitter, hi, VALUE_HI);
    abi::emit_frame_slot_address(emitter, result, VALUE_TAG);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    abi::store_at_offset(emitter, result, VALUE_TAG);
    abi::store_at_offset(emitter, low, VALUE_LO);
    abi::store_at_offset(emitter, high, VALUE_HI);
    abi::emit_load_symbol_to_reg(emitter, scratch, "_concat_off", 0);
    abi::store_at_offset(emitter, scratch, CONCAT);
    branch_eq(emitter, 1, "__rt_sleep_name_convert");
    emit_names_warning(emitter);
    emitter.label("__rt_sleep_name_convert");
    abi::emit_frame_slot_address(emitter, result, VALUE_TAG);
    abi::emit_owned_mixed_string(emitter, "__rt_sleep_name_string", "__rt_sleep_name_persist");
    let (string, length) = abi::string_result_regs(emitter);
    abi::load_at_offset(emitter, scratch, CONTEXT);
    abi::emit_store_to_address(emitter, string, scratch, 8);
    abi::store_at_offset(emitter, string, VALUE_LO);
    abi::store_at_offset(emitter, length, VALUE_HI);
    abi::load_at_offset(emitter, result, CONCAT);
    abi::emit_store_reg_to_symbol(emitter, result, "_concat_off", 0);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 0), OBJECT);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 1), VALUE_LO);
    abi::load_at_offset(emitter, abi::int_arg_reg_name(emitter.target, 2), VALUE_HI);
    abi::emit_call_label(emitter, "__rt_serialize_named_prop");
    abi::load_at_offset(emitter, scratch, CONTEXT);
    abi::emit_load_from_address(emitter, result, scratch, 8);
    abi::emit_store_zero_to_address(emitter, scratch, 8);
    abi::emit_call_label(emitter, "__rt_decref_any");
    abi::emit_jump(emitter, "__rt_sleep_name_loop");
    emitter.label("__rt_sleep_names_done");
    append(emitter, b"}");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);

    emitter.label("__rt_sleep_invalid_return");
    emit_names_warning(emitter);
    abi::load_at_offset(emitter, result, CONTEXT);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_store_reg_to_symbol(emitter, result, "_concat_off", 0);
    append(emitter, b"N;");
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Formats a class-qualified warning without releasing any of the callback's borrowed operands.
fn emit_names_warning(emitter: &mut Emitter) {
    let (ptr, len) = match emitter.target.arch {
        Arch::AArch64 => ("x1", "x2"),
        Arch::X86_64 => ("rdi", "rsi"),
    };
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_symbol_address(emitter, ptr, "_sleep_warning_prefix");
    abi::emit_load_int_immediate(emitter, len, SLEEP_WARNING_PREFIX.len() as i64);
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::load_at_offset(emitter, result, OBJECT);
    abi::emit_load_from_address(emitter, result, result, 0);
    abi::emit_symbol_address(emitter, scratch, "_class_name_entries");
    ins(emitter, "add x10, x10, x0, lsl #4", "shl rax, 4");
    if emitter.target.arch == Arch::X86_64 {
        ins(emitter, "nop", "add r10, rax");
    }
    abi::emit_load_from_address(emitter, ptr, scratch, 0);
    abi::emit_load_from_address(emitter, len, scratch, 8);
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_symbol_address(emitter, ptr, "_sleep_warning_suffix");
    abi::emit_load_int_immediate(emitter, len, SLEEP_WARNING_SUFFIX.len() as i64);
    abi::emit_call_label(emitter, "__rt_diag_warning");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Warning fragments use the diagnostic ABI, not the ordinary string result registers.
    #[test]
    fn sleep_warning_arguments_match_each_target_diagnostic_abi() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_names_warning(&mut emitter);
            let arch = emitter.target.arch;
            let asm = emitter.output();
            let (address, length, class_name) = if arch == Arch::AArch64 {
                ("adrp x1,", "mov x2,", "ldr x1, [x10]")
            } else {
                ("lea rdi,", "mov rsi,", "mov rdi, QWORD PTR [r10]")
            };
            for symbol in ["_sleep_warning_prefix", "_sleep_warning_suffix"] {
                assert!(asm.lines().any(|line| line.trim_start().starts_with(address) && line.contains(symbol)), "{name}: {asm}");
            }
            assert!(asm.contains(length), "{name}: {asm}");
            assert!(asm.contains(class_name), "{name}: {asm}");
        }
    }

    /// Every ABI owns conversion results across nested hooks and distinguishes boxed arrays from headers.
    #[test]
    fn sleep_result_owners_and_logical_iteration_cover_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_sleep_result(&mut emitter);
            let arch = emitter.target.arch;
            let asm = emitter.output();
            let (owner, body) = asm.split_once("__rt_serialize_sleep_body:").unwrap();
            assert_eq!(owner.matches("__rt_cleanup_invoke").count(), 3, "{name}");
            // ARM64 materializes one address with adrp/add; x86_64 uses one lea.
            // Count address starts, not every textual reference to the release symbol.
            let address_start = if arch == Arch::AArch64 { "adrp x0," } else { "lea rdi," };
            let releases = owner.lines().filter(|line| {
                line.trim_start().starts_with(address_start) && line.contains("__rt_decref_any")
            }).count();
            assert_eq!(releases, 2, "{name}");
            assert!(body.find("__rt_heap_kind").unwrap() < body.find("__rt_mixed_unbox").unwrap(), "{name}");
            assert!(body.contains("__rt_array_iter_next"), "{name}");
            assert!(body.contains("__rt_serialize_named_prop"), "{name}");
            assert!(body.contains("__rt_sleep_invalid_return:"), "{name}");
        }
    }
}
