//! Purpose:
//! Sums or multiplies logical PHP array values while preserving numeric result types.
//!
//! Called from:
//! - The typed ArraySum and ArrayProduct backend entries.
//!
//! Key details:
//! - Source snapshots survive warning-handler mutations and exceptional cleanup.
//! - Numeric arithmetic uses the shared overflow-promoting Mixed helpers.
//! - PHP 8.3 warnings do not change the legacy fallback for strings and resources.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch, sentinels};
use crate::codegen_support::try_handlers::{
    TRY_HANDLER_DIAG_DEPTH_OFFSET, TRY_HANDLER_JMP_BUF_OFFSET, TRY_HANDLER_SLOT_SIZE,
};

const FRAME: usize = TRY_HANDLER_SLOT_SIZE + 144;
const HANDLER: usize = FRAME - 16;
const SOURCE: usize = 8;
const CARRY: usize = 16;
const PAYLOAD: usize = 24;
const CURSOR: usize = 32;
const WARNINGS: usize = 40;
const ENTRY: usize = 64;
const WARN_KIND: usize = 72;
const PREVIOUS: usize = 80;
const PENDING: usize = 88;
const NEXT: usize = 96;
const BORROWED: usize = 104;
const ORIGINAL_TAG: usize = 112;

/// Static diagnostic fragments shared with runtime data emission.
pub const ARRAY_AGGREGATE_MESSAGES: &[(&str, &str)] = &[
    ("_aggregate_sum_warning", "Warning: array_sum(): Addition is not supported on type "),
    ("_aggregate_product_warning", "Warning: array_product(): Multiplication is not supported on type "),
    ("_aggregate_leading_numeric", "Warning: A non-numeric value encountered\n"),
    ("_aggregate_array", "array"),
    ("_aggregate_string", "string"),
    ("_aggregate_resource", "resource"),
    ("_aggregate_closure", "Closure"),
    ("_aggregate_object", "object"),
    ("_aggregate_newline", "\n"),
];

/// Emits numeric aggregation entries taking a borrowed C ABI source cell and a warning-profile flag.
pub fn emit_array_numeric_aggregate(emitter: &mut Emitter) {
    emit_aggregate(emitter, false);
    emit_aggregate(emitter, true);
}

/// Emits one storage-neutral fold returning an owned numeric cell or zero for an invalid source.
fn emit_aggregate(emitter: &mut Emitter, product: bool) {
    let prefix = if product { "__rt_array_product_boxed" } else { "__rt_array_sum_boxed" };
    let result = abi::int_result_reg(emitter);
    let low = low_reg(emitter);
    let high = high_reg(emitter);
    let arg0 = abi::int_arg_reg_name(emitter.target, 0);
    let arg1 = abi::int_arg_reg_name(emitter.target, 1);
    emitter.blank();
    emitter.label_global(prefix);
    abi::emit_frame_prologue(emitter, FRAME);
    abi::store_at_offset(emitter, arg0, BORROWED);
    abi::store_at_offset(emitter, arg1, WARNINGS);
    abi::emit_reg_move(emitter, result, arg0);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    ins(emitter, "sub x10, x0, #4", "lea r10, [rax - 4]");
    ins(emitter, "cmp x10, #1", "cmp r10, 1");
    branch(emitter, "hi", "a", &format!("{prefix}_invalid"));
    sentinels::emit_branch_if_null_container(emitter, low, abi::secondary_scratch_reg(emitter), &format!("{prefix}_invalid"));
    abi::store_at_offset(emitter, low, PAYLOAD);
    for slot in [SOURCE, CARRY, CURSOR, PENDING, NEXT] { clear_slot(emitter, slot); }
    install_boundary(emitter, prefix);
    abi::load_at_offset(emitter, result, BORROWED);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("mov rsi, rdx");                                    // adapt the snapshot constructor's high-word argument
    }
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, SOURCE);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_load_int_immediate(emitter, low, i64::from(product));
    abi::emit_load_int_immediate(emitter, if emitter.target.arch == Arch::AArch64 { "x2" } else { "rsi" }, 0);
    abi::emit_call_label(emitter, "__rt_mixed_from_value");
    abi::store_at_offset(emitter, result, CARRY);

    emitter.label(&format!("{prefix}_loop"));
    clear_slot(emitter, WARN_KIND);
    abi::load_at_offset(emitter, arg0, PAYLOAD);
    abi::load_at_offset(emitter, arg1, CURSOR);
    abi::emit_call_label(emitter, "__rt_array_iter_next");
    ins(emitter, "cmn x0, #1", "cmp rax, -1");
    branch(emitter, "eq", "e", &format!("{prefix}_cleanup"));
    abi::store_at_offset(emitter, result, CURSOR);
    let fields = if emitter.target.arch == Arch::AArch64 { ["x3", "x4", "x5"] } else { ["r8", "r9", "r10"] };
    for (index, reg) in fields.into_iter().enumerate() {
        abi::store_at_offset(emitter, reg, ENTRY - index * 8);
    }
    abi::emit_frame_slot_address(emitter, result, ENTRY);
    abi::emit_call_label(emitter, "__rt_mixed_unbox");
    for (reg, slot) in [(result, ENTRY), (result, ORIGINAL_TAG), (low, ENTRY - 8), (high, ENTRY - 16)] {
        abi::store_at_offset(emitter, reg, slot);
    }
    abi::emit_branch_if_int_result_zero(emitter, &format!("{prefix}_combine"));
    for (tag, suffix) in [(1, "string"), (2, "combine"), (3, "scalar"), (8, "scalar"), (9, "resource")] {
        ins(emitter, &format!("cmp x0, #{tag}"), &format!("cmp rax, {tag}"));
        branch(emitter, "eq", "e", &format!("{prefix}_{suffix}"));
    }
    abi::emit_jump(emitter, &format!("{prefix}_unsupported"));

    emitter.label(&format!("{prefix}_scalar"));
    abi::emit_frame_slot_address(emitter, result, ENTRY);
    abi::emit_call_label(emitter, "__rt_mixed_cast_int");
    abi::store_at_offset(emitter, result, ENTRY - 8);
    clear_slot(emitter, ENTRY);
    clear_slot(emitter, ENTRY - 16);
    abi::emit_jump(emitter, &format!("{prefix}_combine"));

    emitter.label(&format!("{prefix}_resource"));
    abi::emit_load_int_immediate(emitter, result, 2);
    abi::store_at_offset(emitter, result, WARN_KIND);
    abi::emit_jump(emitter, &format!("{prefix}_scalar"));

    emitter.label(&format!("{prefix}_string"));
    abi::load_at_offset(emitter, arg0, ENTRY - 8);
    abi::load_at_offset(emitter, arg1, ENTRY - 16);
    abi::emit_call_label(emitter, "__rt_str_numeric_value");
    abi::store_at_offset(emitter, result, ENTRY);
    abi::store_at_offset(emitter, low, ENTRY - 8);
    abi::store_at_offset(emitter, high, WARN_KIND);
    clear_slot(emitter, ENTRY - 16);
    abi::load_at_offset(emitter, result, WARN_KIND);
    ins(emitter, "cmp x0, #1", "cmp rax, 1");
    branch(emitter, "ne", "ne", &format!("{prefix}_combine"));
    abi::load_at_offset(emitter, result, WARNINGS);
    abi::emit_branch_if_int_result_zero(emitter, &format!("{prefix}_combine"));
    emit_message(emitter, 2, true);

    emitter.label(&format!("{prefix}_combine"));
    abi::load_at_offset(emitter, result, CARRY);
    abi::emit_frame_slot_address(emitter, low, ENTRY);
    abi::emit_call_label(emitter, if product { "__rt_mixed_numeric_mul" } else { "__rt_mixed_numeric_add" });
    abi::store_at_offset(emitter, result, NEXT);
    release_slot(emitter, CARRY);
    abi::load_at_offset(emitter, result, NEXT);
    clear_slot(emitter, NEXT);
    abi::store_at_offset(emitter, result, CARRY);
    abi::load_at_offset(emitter, result, WARN_KIND);
    ins(emitter, "cmp x0, #2", "cmp rax, 2");
    branch(emitter, "ne", "ne", &format!("{prefix}_loop"));
    emitter.label(&format!("{prefix}_unsupported"));
    abi::load_at_offset(emitter, result, WARNINGS);
    abi::emit_branch_if_int_result_zero(emitter, &format!("{prefix}_loop"));
    emit_unsupported_warning(emitter, prefix, product);
    abi::emit_jump(emitter, &format!("{prefix}_loop"));
    emit_cleanup(emitter, prefix);
}

/// Selects PHP's unsupported type spelling after publishing the complete operation prefix.
fn emit_unsupported_warning(emitter: &mut Emitter, prefix: &str, product: bool) {
    let result = abi::int_result_reg(emitter);
    let ready = format!("{prefix}_warning_type_ready");
    emit_message(emitter, usize::from(product), false);
    abi::load_at_offset(emitter, result, ORIGINAL_TAG);
    for (tag, message) in [(1, 4), (4, 3), (5, 3), (9, 5), (10, 6)] {
        let next = format!("{prefix}_warning_not_{tag}");
        ins(emitter, &format!("cmp x0, #{tag}"), &format!("cmp rax, {tag}"));
        branch(emitter, "ne", "ne", &next);
        emit_message(emitter, message, false);
        abi::emit_jump(emitter, &ready);
        emitter.label(&next);
    }
    let fallback = format!("{prefix}_warning_object_fallback");
    ins(emitter, "cmp x0, #6", "cmp rax, 6");
    branch(emitter, "ne", "ne", &fallback);
    abi::load_at_offset(emitter, result, ENTRY - 8);
    ins(emitter, "ldr x13, [x0]", "mov r8, QWORD PTR [rax]");
    abi::emit_load_symbol_to_reg(emitter, result, "_class_name_count", 0);
    ins(emitter, "cmp x13, x0", "cmp r8, rax");
    branch(emitter, "hs", "ae", &fallback);
    abi::emit_symbol_address(emitter, abi::secondary_scratch_reg(emitter), "_class_name_entries");
    ins(emitter, "add x10, x10, x13, lsl #4", "shl r8, 4");
    if emitter.target.arch == Arch::X86_64 {
        emitter.instruction("add r10, r8");                                     // address the current object's dense class-name row
    }
    let (ptr, len) = warning_regs(emitter);
    abi::emit_load_from_address(emitter, ptr, abi::secondary_scratch_reg(emitter), 0);
    abi::emit_load_from_address(emitter, len, abi::secondary_scratch_reg(emitter), 8);
    abi::emit_reg_move(emitter, result, len);
    abi::emit_branch_if_int_result_zero(emitter, &fallback);
    abi::emit_call_label(emitter, "__rt_diag_warning_fragment");
    abi::emit_jump(emitter, &ready);
    emitter.label(&fallback);
    emit_message(emitter, 7, false);
    emitter.label(&ready);
    emit_message(emitter, 8, true);
}

/// Emits a fixed diagnostic fragment or completes the warning through PHP's handler dispatch.
fn emit_message(emitter: &mut Emitter, index: usize, complete: bool) {
    let (label, message) = ARRAY_AGGREGATE_MESSAGES[index];
    let (ptr, len) = warning_regs(emitter);
    abi::emit_symbol_address(emitter, ptr, label);
    abi::emit_load_int_immediate(emitter, len, message.len() as i64);
    abi::emit_call_label(emitter, if complete { "__rt_diag_warning" } else { "__rt_diag_warning_fragment" });
}

/// Cleans the partial numeric result and source snapshot before returning or rethrowing.
fn emit_cleanup(emitter: &mut Emitter, prefix: &str) {
    let result = abi::int_result_reg(emitter);
    emitter.label(&format!("{prefix}_cleanup"));
    release_slot(emitter, NEXT);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, &format!("{prefix}_release_source"));
    release_slot(emitter, CARRY);
    emitter.label(&format!("{prefix}_release_source"));
    release_slot(emitter, SOURCE);
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, &format!("{prefix}_return"));
    abi::load_at_offset(emitter, low_reg(emitter), PREVIOUS);
    clear_slot(emitter, PREVIOUS);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label(&format!("{prefix}_return"));
    abi::load_at_offset(emitter, result, PREVIOUS);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    restore_boundary(emitter);
    abi::load_at_offset(emitter, result, CARRY);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
    emitter.label(&format!("{prefix}_caught"));
    abi::emit_load_symbol_to_reg(emitter, result, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::load_at_offset(emitter, low_reg(emitter), PENDING);
    abi::store_at_offset(emitter, result, PENDING);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::emit_jump(emitter, &format!("{prefix}_cleanup"));
    emitter.label(&format!("{prefix}_invalid"));
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_frame_restore(emitter, FRAME);
    abi::emit_return(emitter);
}

/// Installs a resumable owner boundary before allocation or warning dispatch.
fn install_boundary(emitter: &mut Emitter, prefix: &str) {
    let result = abi::int_result_reg(emitter);
    for (symbol, slot) in [
        ("_exc_value", PREVIOUS), ("_exc_handler_top", HANDLER),
        ("_exc_call_frame_top", HANDLER - 8),
        ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET),
    ] {
        abi::emit_load_symbol_to_reg(emitter, result, symbol, 0);
        abi::store_at_offset(emitter, result, slot);
    }
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_frame_slot_address(emitter, result, HANDLER);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_handler_top", 0);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 0), HANDLER - TRY_HANDLER_JMP_BUF_OFFSET);
    emitter.bl_c("setjmp");                                                     // preserve the snapshot and partial number when a warning handler throws
    abi::emit_branch_if_int_result_nonzero(emitter, &format!("{prefix}_caught"));
}

/// Restores the enclosing exception handler and diagnostic depth.
fn restore_boundary(emitter: &mut Emitter) {
    let result = abi::int_result_reg(emitter);
    for (symbol, slot) in [("_exc_handler_top", HANDLER), ("_rt_diag_suppression", HANDLER - TRY_HANDLER_DIAG_DEPTH_OFFSET)] {
        abi::load_at_offset(emitter, result, slot);
        abi::emit_store_reg_to_symbol(emitter, result, symbol, 0);
    }
}

/// Clears one root without clobbering a loaded owner or exception-chain argument.
fn clear_slot(emitter: &mut Emitter, slot: usize) {
    let scratch = abi::secondary_scratch_reg(emitter);
    abi::emit_load_int_immediate(emitter, scratch, 0);
    abi::store_at_offset(emitter, scratch, slot);
}

/// Retires a boxed owner after clearing its root, including when cleanup later resumes.
fn release_slot(emitter: &mut Emitter, slot: usize) {
    abi::load_at_offset(emitter, abi::int_result_reg(emitter), slot);
    clear_slot(emitter, slot);
    abi::emit_call_label(emitter, "__rt_decref_mixed");
}

/// Returns the low payload register of the Mixed unbox ABI.
fn low_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x1" } else { "rdi" }
}

/// Returns the high payload register of the Mixed unbox ABI.
fn high_reg(emitter: &Emitter) -> &'static str {
    if emitter.target.arch == Arch::AArch64 { "x2" } else { "rdx" }
}

/// Returns the borrowed byte-pair arguments used by diagnostic dispatch.
fn warning_regs(emitter: &Emitter) -> (&'static str, &'static str) {
    if emitter.target.arch == Arch::AArch64 { ("x1", "x2") } else { ("rdi", "rsi") }
}

/// Emits equivalent conditional branches for the selected target.
fn branch(emitter: &mut Emitter, arm: &str, x86: &str, target: &str) {
    ins(emitter, &format!("b.{arm} {target}"), &format!("j{x86} {target}"));
}

/// Emits one equivalent aggregate operation on either native architecture.
fn ins(emitter: &mut Emitter, arm: &str, x86: &str) {
    emitter.instruction(if emitter.target.arch == Arch::AArch64 { arm } else { x86 }); // preserve the numeric aggregate and ownership contract across native ABIs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Both aggregates install cleanup before ownership acquisition or user warning dispatch.
    #[test]
    fn numeric_aggregate_snapshots_and_unwinds_on_all_targets() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for product in [false, true] {
                let mut emitter = Emitter::new(target);
                emit_aggregate(&mut emitter, product);
                let asm = emitter.output();
                let boundary = asm.find(&target.extern_symbol("setjmp")).unwrap();
                let acquire = asm.find("__rt_mixed_from_value").unwrap();
                let warning = asm.find("__rt_diag_warning").unwrap();
                assert!(boundary < acquire && acquire < warning, "{name}");
                assert!(asm.contains("__rt_array_iter_next"), "{name}");
                assert!(asm.contains("__rt_str_numeric_value"), "{name}");
                assert!(asm.contains("__rt_exception_chain"), "{name}");
                assert!(asm.contains("__rt_throw_current"), "{name}");
                let operation = if product { "__rt_mixed_numeric_mul" } else { "__rt_mixed_numeric_add" };
                assert!(asm.contains(operation), "{name}");
                assert!(!asm.contains("__rt_mixed_clone"), "{name}: stack cells stay borrowed");
                if target.arch == Arch::X86_64 {
                    assert!(asm.contains("mov rsi, rdx"), "preserve high snapshot words");
                }
            }
        }
    }
}
