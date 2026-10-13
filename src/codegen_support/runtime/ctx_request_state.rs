//! Purpose:
//! Isolates request-local runtime state introduced alongside the scheduler branch integration.
//!
//! Called from:
//! - Context layout, symbol routing, initialization and drained-owner verification in `super`.
//!
//! Key details:
//! - Names map to disjoint eight-byte slots before output-buffer tables.
//! - Scalar counters are reset on reuse; owner-bearing cells must be drained before reuse.
//! - Nonzero defaults come from immutable configuration, never another context's live state.

use super::{abi, Arch, Emitter};
use crate::codegen_support::RuntimeFeatures;

/// Initial state for one request-local cell.
#[derive(Clone, Copy)]
enum Initial { Zero, One, ErrorReporting }

/// One closed runtime storage contract, shared by layout, routing and ownership checks.
struct Slot { symbol: &'static str, owner: bool, initial: Initial }

const SLOTS: &[Slot] = &[
    Slot { symbol: "_mbstring_catalog_array", owner: true, initial: Initial::Zero },
    Slot { symbol: "_mbstring_deferred_capture_head", owner: true, initial: Initial::Zero },
    Slot { symbol: "_mbstring_deferred_capture_tail", owner: true, initial: Initial::Zero },
    Slot { symbol: "_mbstring_deferred_capture_draining", owner: false, initial: Initial::Zero },
    Slot { symbol: "_mbstring_ini_native_active", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_enabled", owner: false, initial: Initial::One },
    Slot { symbol: "_gc_freeing_unreachable", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_pin_head", owner: true, initial: Initial::Zero },
    Slot { symbol: "_gc_pending_throw", owner: true, initial: Initial::Zero },
    Slot { symbol: "_gc_release_suppressed", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_runs", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_collected", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_application_started", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_collector_started", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_free_started", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_destructor_started", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_collector_time", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_destructor_time", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_free_time", owner: false, initial: Initial::Zero },
    Slot { symbol: "_gc_destructor_depth", owner: false, initial: Initial::Zero },
    Slot { symbol: "_php_error_reporting", owner: false, initial: Initial::ErrorReporting },
    Slot { symbol: "_php_error_handler_value", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_error_handler_callable", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_error_handler_mask", owner: false, initial: Initial::Zero },
    Slot { symbol: "_php_error_handler_context", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_error_handler_context_release", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_error_handler_stack", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_exception_handler_value", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_exception_handler_callable", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_exception_handler_context", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_exception_handler_context_release", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_exception_handler_stack", owner: true, initial: Initial::Zero },
    Slot { symbol: "_php_backtrace_next_line", owner: false, initial: Initial::Zero },
    Slot { symbol: "_rt_diag_suppression", owner: false, initial: Initial::Zero },
    Slot { symbol: "_rt_diag_pending_ptr", owner: false, initial: Initial::Zero },
    Slot { symbol: "_rt_diag_pending_len", owner: false, initial: Initial::Zero },
    Slot { symbol: "_php_diagnostic_file", owner: false, initial: Initial::Zero },
    Slot { symbol: "_php_diagnostic_file_len", owner: false, initial: Initial::Zero },
    Slot { symbol: "_php_diagnostic_line", owner: false, initial: Initial::Zero },
    Slot { symbol: "_magic_set_guard_head", owner: true, initial: Initial::Zero },
    Slot { symbol: "_fiber_main_saved_magic_set_guard", owner: true, initial: Initial::Zero },
    Slot { symbol: "_rt_unmanaged_ref_borrow_top", owner: true, initial: Initial::Zero },
    Slot { symbol: "_resource_inventory_head", owner: true, initial: Initial::Zero },
    Slot { symbol: "_resource_inventory_tail", owner: true, initial: Initial::Zero },
    Slot { symbol: "_unser_temporaries", owner: true, initial: Initial::Zero },
];

/// First byte after the fixed scalar block, before any output-buffer table.
pub(super) const BASE: usize = super::CTX_OB_LEVEL_OFFSET + 8;
/// Exclusive end of this request-state block.
pub(super) const END: usize = BASE + SLOTS.len() * 8;

/// Returns the context offset of a request-local symbol, or leaves other families untouched.
pub(super) fn offset(symbol: &str) -> Option<usize> {
    SLOTS.iter().position(|slot| slot.symbol == symbol).map(|index| BASE + index * 8)
}

/// Initializes a fresh context without touching the incoming PHP/C argument registers.
pub(super) fn initialize(emitter: &mut Emitter, context: &str) {
    // Acquire keeps the slot's state address in x9/r10 and its base in x10/r11.
    // x11/rax are free after a successful claim; neither is an incoming C argument register.
    let scratch = if emitter.target.arch == Arch::AArch64 { "x11" } else { "rax" };
    for (index, slot) in SLOTS.iter().enumerate() {
        let address = BASE + index * 8;
        match slot.initial {
            Initial::Zero => abi::emit_store_zero_to_address(emitter, context, address),
            Initial::One => {
                abi::emit_load_int_immediate(emitter, scratch, 1);
                abi::emit_store_to_address(emitter, scratch, context, address);
            }
            Initial::ErrorReporting => {
                abi::emit_symbol_address(emitter, scratch, "_php_error_reporting_default");
                abi::emit_load_from_address(emitter, scratch, scratch, 0);
                abi::emit_store_to_address(emitter, scratch, context, address);
            }
        }
    }
}

/// Refuses a context release while any new owner-bearing cell remains published.
pub(super) fn verify_drained(emitter: &mut Emitter, dirty: &str) {
    let scratch = if emitter.target.arch == Arch::AArch64 { "x9" } else { "r10" };
    for (index, _) in SLOTS.iter().enumerate().filter(|(_, slot)| slot.owner) {
        super::emit_ctx_load(emitter, scratch, BASE + index * 8);
        match emitter.target.arch {
            Arch::AArch64 => {
                emitter.instruction(&format!("cbnz {scratch}, {dirty}"));       // a published request owner keeps this context claimed
            }
            Arch::X86_64 => {
                emitter.instruction(&format!("test {scratch}, {scratch}"));     // inspect the request-local owner pointer before releasing its arena
                emitter.instruction(&format!("jne {dirty}"));                   // never zero a live owner merely to recycle a context slot
            }
        }
    }
}

/// Invokes a cleanup without losing an older exception or skipping sibling owners after a throw.
pub(super) fn cleanup(emitter: &mut Emitter, helper: &str) {
    let result = abi::int_result_reg(emitter);
    abi::emit_load_int_immediate(emitter, result, 0);
    abi::emit_unary_cleanup_preserving_exception(emitter, helper, result);
}

/// Drains optional request owners while the pooled worker context and fatal boundary are installed.
pub(super) fn drain(emitter: &mut Emitter, features: RuntimeFeatures) {
    if features.handler_state || features.eval_bridge || features.web {
        for kind in ["error", "exception"] {
            let scan = format!("__rt_ctx_drain_{kind}_handlers_scan");
            let active = format!("__rt_ctx_drain_{kind}_handlers_active");
            let done = format!("__rt_ctx_drain_{kind}_handlers_done");
            emitter.label(&scan);
            for suffix in ["stack", "value", "callable", "context", "context_release"] {
                let result = abi::int_result_reg(emitter);
                abi::emit_load_symbol_to_reg(emitter, result, &format!("_php_{kind}_handler_{suffix}"), 0);
                abi::emit_branch_if_int_result_nonzero(emitter, &active);
            }
            abi::emit_jump(emitter, &done);
            emitter.label(&active);
            cleanup(emitter, &format!("__rt_core_{kind}_handler_pop"));
            // Pop restores the preceding node even when a detached owner's destructor throws.
            // Reinspect registrations made by cleanup; never erase a live owner to finish faster.
            abi::emit_jump(emitter, &scan);
            emitter.label(&done);
        }
    }
    if features.mbstring || features.eval_bridge {
        cleanup(emitter, "__rt_mbstring_release_catalog");
        // The native TLS reset must still run when deferred PHP cleanup threw.
        emitter.bl_c("elephc_mbstring_reset_v1");
    }
    // A later destructor may republish request owners. Final verification quarantines
    // those owners (and any accumulated Throwable); it does not reset them silently.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;
    use std::collections::HashSet;

    /// Every new runtime cell routes to a unique aligned slot before the output tables.
    #[test]
    fn integrated_request_state_has_disjoint_context_storage() {
        let mut names = HashSet::new();
        let mut offsets = HashSet::new();
        assert_eq!(END, super::super::CTX_OB_PTRS_OFFSET);
        assert!(END < 4096);
        // The native fixture writes all these cells while two worker threads rendezvous.
        assert_eq!((BASE, SLOTS.len()), (280, 45), "update the C-host request-state fixture when this layout changes");
        let host = include_str!("../../../tests/cdylib_tests.rs");
        assert!(host.contains("#define REQUEST_STATE_BASE 280") && host.contains("#define REQUEST_STATE_WORDS 45"));
        for slot in SLOTS {
            assert!(names.insert(slot.symbol));
            let address = offset(slot.symbol).unwrap();
            assert!(offsets.insert(address));
            assert_eq!(address % 8, 0);
            assert!(BASE <= address && address + 8 <= END);
            for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                emitter.ctx_register = true;
                assert_eq!(super::super::per_context_symbol_offset(&emitter, slot.symbol), Some(address));
                emitter.ctx_register = false;
                assert_eq!(super::super::per_context_symbol_offset(&emitter, slot.symbol), None);
            }
        }
    }

    /// Pool acquisition and main initialization preserve their arbitrary context base registers.
    #[test]
    fn request_state_initialization_and_owner_checks_cover_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            let arch = emitter.target.arch;
            let context = if arch == Arch::AArch64 { "x10" } else { "r11" };
            initialize(&mut emitter, context);
            let asm = emitter.output();
            assert!(asm.contains("_php_error_reporting_default"), "{target}: {asm}");
            assert!(!asm.contains("_php_error_handler_value"), "{target}: handler state must be local");
            assert!(!asm.contains("str xzr, [x28") && !asm.contains("[r14 +"), "{target}: acquisition must not reset the caller");
            if arch == Arch::X86_64 {
                assert!(!asm.lines().any(|line| line.trim_start().starts_with("mov r10,")
                    || line.trim_start().starts_with("lea r10,")),
                    "{target}: initialization must preserve the pool state pointer: {asm}");
            }
            let mut checks = Emitter::new(Target::parse(target).unwrap());
            verify_drained(&mut checks, "request_owners_live");
            let checks = checks.output();
            assert_eq!(checks.matches("request_owners_live").count(), SLOTS.iter().filter(|slot| slot.owner).count());
            assert!(!checks.contains("__rt_heap_free"), "{target}: verification must not erase owners");
        }
    }

    /// Optional cleanup references only emitted feature families and contains PHP cleanup throws.
    #[test]
    fn request_state_draining_respects_feature_gates_on_every_target() {
        for target in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            for feature in ["none", "handler", "eval", "web", "mbstring"] {
                let mut features = RuntimeFeatures::none();
                match feature {
                    "handler" => features.handler_state = true,
                    "eval" => features.eval_bridge = true,
                    "web" => features.web = true,
                    "mbstring" => features.mbstring = true,
                    _ => {}
                }
                let mut emitter = Emitter::new(Target::parse(target).unwrap());
                emitter.ctx_register = true;
                drain(&mut emitter, features);
                let asm = emitter.output();
                let handlers = features.handler_state || features.eval_bridge || features.web;
                let mbstring = features.mbstring || features.eval_bridge;
                for kind in ["error", "exception"] {
                    assert_eq!(asm.contains(&format!("__rt_core_{kind}_handler_pop")), handlers, "{target}: {feature}: {asm}");
                }
                assert_eq!(asm.contains("__rt_mbstring_release_catalog"), mbstring, "{target}: {feature}: {asm}");
                assert_eq!(asm.contains("elephc_mbstring_reset_v1"), mbstring, "{target}: {feature}: {asm}");
                assert_eq!(asm.matches("__rt_cleanup_preserve_exception").count(), usize::from(handlers) * 2 + usize::from(mbstring), "{target}: {feature}: {asm}");
                if mbstring {
                    assert!(asm.rfind("__rt_cleanup_preserve_exception").unwrap() < asm.find("elephc_mbstring_reset_v1").unwrap(), "{target}: {feature}");
                }
                assert!(!asm.contains("__rt_throw_current"), "{target}: {feature}");
            }
        }
    }
}
