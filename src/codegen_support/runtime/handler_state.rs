//! Purpose:
//! Owns release and stack restoration for native PHP error and exception handlers.
//!
//! Called from:
//! - AOT handler lowering, eval registration wrappers, and web request reset.
//!
//! Key details:
//! - Active owners are detached together before any destructor or eval release runs.
//! - Popping an empty stack still clears the active registration.
//! - Cleanup throws propagate only after sibling owners and the preceding stack node are handled.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

const PENDING: usize = 8;

/// Emits the two handler kinds through the same ownership and linked-stack protocol.
pub(super) fn emit_handler_state(emitter: &mut Emitter) {
    for kind in ["error", "exception"] {
        emit_release(emitter, kind);
        emit_pop(emitter, kind);
    }
}

/// Returns handler fields in their ABI-defined linked-node order.
fn fields(kind: &str) -> Vec<String> {
    let mut suffixes = vec!["stack", "value", "callable"];
    if kind == "error" {
        suffixes.push("mask");
    }
    suffixes.extend(["context", "context_release"]);
    suffixes.iter().map(|suffix| format!("_php_{kind}_handler_{suffix}")).collect()
}

/// Releases a detached snapshot, keeping captured contexts alive until descriptors are gone.
fn emit_release(emitter: &mut Emitter, kind: &str) {
    let stack = if emitter.target.arch == Arch::AArch64 { "sp" } else { "rsp" };
    let result = abi::int_result_reg(emitter);
    emitter.blank();
    emitter.label_global(&format!("__rt_core_{kind}_handler_release"));
    abi::emit_frame_prologue(emitter, 64);
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    for (index, suffix) in ["value", "callable", "context", "context_release"].iter().enumerate() {
        let symbol = format!("_php_{kind}_handler_{suffix}");
        abi::emit_load_symbol_to_reg(emitter, result, &symbol, 0);
        abi::emit_store_to_address(emitter, result, stack, index * 8);
        abi::emit_store_zero_to_symbol(emitter, &symbol, 0);
    }
    for (offset, helper) in [(0, "__rt_decref_mixed"), (8, "__rt_callable_descriptor_release")] {
        let skip = format!("__rt_core_{kind}_handler_release_skip_{offset}");
        abi::emit_load_from_address(emitter, result, stack, offset);
        abi::emit_branch_if_int_result_zero(emitter, &skip);
        super::exceptions::emit_guarded_cleanup_call(emitter, helper, result, PENDING);
        emitter.label(&skip);
    }
    let done = format!("__rt_core_{kind}_handler_release_done");
    abi::emit_load_from_address(emitter, result, stack, 24);
    abi::emit_branch_if_int_result_zero(emitter, &done);
    abi::emit_reg_move(emitter, abi::int_arg_reg_name(emitter.target, 0), result);
    abi::emit_load_from_address(emitter, abi::int_arg_reg_name(emitter.target, 1), stack, 16);
    abi::emit_frame_slot_address(emitter, abi::int_arg_reg_name(emitter.target, 2), PENDING);
    abi::emit_call_label(emitter, "__rt_cleanup_invoke");
    emitter.label(&done);
    emit_finish_cleanup(emitter, &format!("__rt_core_{kind}_handler_release_return"), 64);
}

/// Clears the active handler before checking whether a previous registration exists.
fn emit_pop(emitter: &mut Emitter, kind: &str) {
    let stack = if emitter.target.arch == Arch::AArch64 { "sp" } else { "rsp" };
    let result = abi::int_result_reg(emitter);
    let scratch = abi::secondary_scratch_reg(emitter);
    let done = format!("__rt_core_{kind}_handler_pop_done");
    emitter.blank();
    emitter.label_global(&format!("__rt_core_{kind}_handler_pop"));
    abi::emit_frame_prologue(emitter, 48);
    abi::emit_store_zero_to_local_slot(emitter, PENDING);
    abi::emit_load_int_immediate(emitter, result, 0);
    super::exceptions::emit_guarded_cleanup_call(
        emitter, &format!("__rt_core_{kind}_handler_release"), result, PENDING,
    );
    abi::emit_load_symbol_to_reg(emitter, result, &format!("_php_{kind}_handler_stack"), 0);
    abi::emit_branch_if_int_result_zero(emitter, &done);
    abi::emit_store_to_address(emitter, result, stack, 0);
    for (index, symbol) in fields(kind).iter().enumerate() {
        abi::emit_load_from_address(emitter, result, stack, 0);
        abi::emit_load_from_address(emitter, scratch, result, index * 8);
        abi::emit_store_reg_to_symbol(emitter, scratch, symbol, 0);
    }
    abi::emit_load_from_address(emitter, result, stack, 0);
    abi::emit_call_label(emitter, "__rt_heap_free");
    emitter.label(&done);
    emit_finish_cleanup(emitter, &format!("__rt_core_{kind}_handler_pop_return"), 48);
}

/// Propagates a pending destructor throw only after the caller has finished its handler cleanup.
fn emit_finish_cleanup(emitter: &mut Emitter, return_label: &str, frame: usize) {
    let result = abi::int_result_reg(emitter);
    let older = match emitter.target.arch { Arch::AArch64 => "x1", Arch::X86_64 => "rdi" };
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_branch_if_int_result_zero(emitter, return_label);
    abi::emit_load_symbol_to_reg(emitter, older, "_exc_value", 0);
    abi::emit_store_zero_to_symbol(emitter, "_exc_value", 0);
    abi::emit_call_label(emitter, "__rt_exception_chain");
    abi::load_at_offset(emitter, result, PENDING);
    abi::emit_store_reg_to_symbol(emitter, result, "_exc_value", 0);
    abi::emit_frame_restore(emitter, frame);
    abi::emit_jump(emitter, "__rt_throw_current");
    emitter.label(return_label);
    abi::emit_frame_restore(emitter, frame);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{AppleVariant, Platform, Target};

    /// Every owner is guarded and the previous stack node retires before a cleanup throw escapes.
    #[test]
    fn handler_cleanup_contains_throws_until_stack_restoration_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emit_handler_state(&mut emitter);
            let asm = emitter.output();
            for kind in ["error", "exception"] {
                let release = asm.split_once(&format!("__rt_core_{kind}_handler_release:\n")).unwrap().1;
                let release = release.split_once(&format!("__rt_core_{kind}_handler_pop:\n")).unwrap().0;
                assert_eq!(release.matches("__rt_cleanup_invoke").count(), 3, "{name}: {kind}");
                assert!(release.rfind("__rt_cleanup_invoke").unwrap() < release.find("__rt_throw_current").unwrap(), "{name}");
                let pop = asm.split_once(&format!("__rt_core_{kind}_handler_pop:\n")).unwrap().1;
                let guarded = pop.find("__rt_cleanup_invoke").unwrap();
                let restore = pop.find(&format!("_php_{kind}_handler_stack")).unwrap();
                let free = pop.find("__rt_heap_free").unwrap();
                let propagate = pop.find("__rt_throw_current").unwrap();
                assert!(guarded < restore && restore < free && free < propagate, "{name}: {kind}");
            }
        }
    }

    /// All targets clear active owners before the empty-stack branch and support eval releases.
    #[test]
    fn handler_pop_releases_before_empty_stack_check_on_every_target() {
        for target in [
            Target::new(Platform::MacOS, Arch::AArch64),
            Target::new_apple(Arch::AArch64, AppleVariant::IOS),
            Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator),
            Target::new(Platform::Linux, Arch::AArch64),
            Target::new(Platform::Linux, Arch::X86_64),
        ] {
            let mut emitter = Emitter::new(target);
            emit_handler_state(&mut emitter);
            let asm = emitter.output();
            for kind in ["error", "exception"] {
                let pop = asm.split(&format!("__rt_core_{kind}_handler_pop:")).nth(1).unwrap();
                assert!(pop.find(&format!("__rt_core_{kind}_handler_release")).unwrap()
                    < pop.find(&format!("_php_{kind}_handler_stack")).unwrap());
                assert!(asm.contains(&format!("_php_{kind}_handler_context_release")));
            }
        }
    }
}
