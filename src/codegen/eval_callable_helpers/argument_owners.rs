//! Purpose:
//! Owns callable descriptors freshly adapted for native eval method and constructor arguments.
//!
//! Called from:
//! - Eval callable casts and the entry/exit paths of native eval call bridges.
//!
//! Key details:
//! - Original normalized argument cells do not own their newly created adapter descriptors.
//! - A private head slot and native cleanup record survive partial staging and suspended calls.
//! - Nodes detach before protected descriptor retirement; no PHP throw crosses the Rust caller.

use super::*;
use crate::codegen_support::{platform::Arch, try_handlers::TRY_HANDLER_SLOT_SIZE};

const KEEP: &str = "__elephc_eval_callable_argument_keep";
const RELEASE: &str = "__elephc_eval_callable_arguments_release";
const VISITOR: &str = "__elephc_eval_callable_argument_frame_owners";
const ARM_HEAD: usize = 24;

/// Emits shared C-ABI adoption and bounded drain entries independently of candidate tables.
pub(super) fn emit(emitter: &mut Emitter) {
    elephc_memoir::callables::argument_owners::emit_keep(emitter, KEEP, "__rt_gc_candidate_live");
    elephc_memoir::callables::argument_owners::emit_release(emitter, RELEASE, "__rt_callable_descriptor_release");
    elephc_memoir::callables::argument_owners::emit_visitor(emitter, VISITOR, "__rt_gc_frame_owner_slot");
}

/// Addresses the bridge's private head without touching its incoming argument registers.
fn head_address(emitter: &mut Emitter, register: &str, x86_context_offset: usize) {
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_load_int_immediate(emitter, register, ARM_HEAD as i64),
        Arch::X86_64 => {
            abi::emit_frame_slot_address(emitter, register, x86_context_offset + 8);
            return;
        }
    }
    emitter.instruction(&format!("add {register}, x29, {register}"));           // the head follows the borrowed eval context in every ARM bridge
}

/// Publishes one collection owner so native unwind and suspended-frame retirement can drain it.
pub(in crate::codegen) fn begin(
    emitter: &mut Emitter, x86_context_offset: usize, arm_footer: usize,
) {
    let head = abi::tertiary_scratch_reg(emitter);
    head_address(emitter, head, x86_context_offset);
    abi::emit_store_zero_to_address(emitter, head, 0);
    let record = if emitter.target.arch == Arch::AArch64 { arm_footer + 32 }
        else { TRY_HANDLER_SLOT_SIZE };
    abi::emit_link_call_operand_custom_cleanup_at_stack(emitter, head, RELEASE, VISITOR, record);
}

/// Transfers a cast's fresh descriptor into the current bridge's owner collection.
pub(in crate::codegen) fn keep(emitter: &mut Emitter, x86_context_offset: usize) {
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 1), abi::int_result_reg(emitter));
    head_address(emitter, abi::int_arg_reg_name(emitter.target, 0), x86_context_offset);
    abi::emit_call_label(emitter, KEEP);
}

/// Completes normal, failed-preparation and caught-throw exits before returning through Rust.
pub(in crate::codegen) fn finish(
    emitter: &mut Emitter, x86_context_offset: usize, arm_footer: usize,
    frame_bytes: usize, boxed_result: bool, label_prefix: &str,
) {
    let result = abi::int_result_reg(emitter);
    match emitter.target.arch {
        Arch::AArch64 => emitter.instruction(&format!("sub sp, x29, #{arm_footer}")), // discard any incomplete staging before bounded owner cleanup
        Arch::X86_64 => emitter.instruction(&format!("lea rsp, [rbp - {frame_bytes}]")), // reclaim the complete bridge frame after preparation failure
    }
    abi::emit_push_reg(emitter, result);
    let record = if emitter.target.arch == Arch::AArch64 { arm_footer + 32 }
        else { TRY_HANDLER_SLOT_SIZE };
    abi::emit_unlink_call_operand_owner_at_stack(emitter, record + 16);
    head_address(emitter, abi::int_arg_reg_name(emitter.target, 0), x86_context_offset);
    abi::emit_call_label(emitter, RELEASE);
    let done = format!("{label_prefix}_callable_owners_done");
    abi::emit_branch_if_int_result_zero(emitter, &done);
    if boxed_result {
        abi::emit_load_temporary_stack_slot(emitter, abi::int_arg_reg_name(emitter.target, 1), 0);
        abi::emit_symbol_address(emitter, abi::int_arg_reg_name(emitter.target, 0), "__rt_decref_mixed");
        abi::emit_call_label(emitter, "__rt_cleanup_preserve_exception");
    }
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_store_to_sp(emitter, result, 0);
    emitter.label(&done);
    abi::emit_pop_reg(emitter, result);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Preserves assembly and internal labels captured from the original compiler at efc6f031c5.
    #[test]
    fn memoir_remaining_ownership_preserves_eval_assembly() {
        use crate::codegen_support::{emit::Emitter, platform::Target};
        use sha2::{Digest, Sha256};

        let mut fingerprints = Vec::new();
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for mode in 0..6 {
                let mut emitter = Emitter::new(Target::parse(name).unwrap());
                emitter.pic_data_refs = mode == 1 || mode == 2;
                emitter.cdylib_boundary = mode == 2 || mode == 3;
                emitter.phpext_output = mode == 4;
                emitter.dead_strip = mode == 5;
                super::emit(&mut emitter);
                let mut labels = emitter.take_internal_labels().into_iter().collect::<Vec<_>>();
                labels.sort();
                let hash = Sha256::digest(format!("{}\n{labels:?}", emitter.output()));
                fingerprints.push(format!("{name} {mode} {hash:x}"));
            }
        }
        assert_eq!(fingerprints.join("\n"), include_str!("memoir_argument_owners_baseline.txt").trim());
    }

    /// All bridges publish a complete fixed cleanup record and detach it before normal draining.
    #[test]
    fn eval_callable_argument_owners_cover_all_bridge_layouts_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let target = Target::parse(name).unwrap();
            for (context, footer, base, boxed) in [(64, 48, 128, false), (64, 48, 128, true), (72, 64, 144, true)] {
                let mut emitter = Emitter::new(target);
                begin(&mut emitter, context, footer);
                keep(&mut emitter, context);
                finish(&mut emitter, context, footer, base + TRY_HANDLER_SLOT_SIZE, boxed, "done");
                let asm = emitter.output();
                let publish = asm.find("publish temporary call operand owner").unwrap();
                let adopt = asm.find(KEEP).unwrap();
                let detach = asm.find("detach temporary call operand owner").unwrap();
                let drain = asm.rfind(RELEASE).unwrap();
                assert!(publish < adopt && adopt < detach && detach < drain, "{name}: {asm}");
                assert!(asm.contains(VISITOR), "{name}: enumerate descriptor edges inside saved Fiber graphs");
                assert_eq!(asm.contains("__rt_cleanup_preserve_exception"), boxed, "{name}: result retirement");
                if target.arch == Arch::AArch64 {
                    assert!(asm.contains(&format!("sub sp, x29, #{footer}")), "{name}: recover failed staging");
                    assert!(asm.contains(&format!("[sp, #{}]", footer + 48)), "{name}: fixed record payload");
                } else {
                    assert!(asm.contains(&format!("lea rsp, [rbp - {}]", base + TRY_HANDLER_SLOT_SIZE)),
                        "{name}: recover the original bridge frame");
                    assert!(asm.contains(&format!("[rbp - {}]", context + 8)), "{name}: private head");
                }
            }
        }
    }

    /// Reentrant cleanup sees a detached node, and every descriptor release has its own boundary.
    #[test]
    fn eval_callable_argument_owner_drain_detaches_before_releasing_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit(&mut emitter);
            let asm = emitter.output();
            let body = asm.split(&format!("{RELEASE}:\n")).nth(1).unwrap();
            let free = body.find("__rt_heap_free").unwrap();
            let release = body.find("__rt_callable_descriptor_release").unwrap();
            let boundary = body.find("__rt_cleanup_preserve_exception").unwrap();
            assert!(free < release && release < boundary, "{name}: {body}");
            assert!(!body.contains("__rt_throw_current"), "{name}: cleanup cannot unwind into Rust");
            assert!(asm.contains("__rt_gc_candidate_live"), "{name}: static descriptors need no owner");
            let visitor = asm.split(&format!("{VISITOR}:\n")).nth(1).unwrap();
            assert!(visitor.contains("__rt_gc_frame_owner_slot"), "{name}: account each descriptor edge");
            for entry in ["__rt_heap_alloc", "__rt_heap_free", "__rt_cleanup_preserve_exception"] {
                assert!(!visitor.contains(entry), "{name}: the GC visitor cannot mutate owners");
            }
        }
    }
}
