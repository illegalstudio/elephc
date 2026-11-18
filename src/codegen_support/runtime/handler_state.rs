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

use elephc_memoir::handler_owners::{self, PENDING};

/// Emits the two handler kinds through the same ownership and linked-stack protocol.
pub(super) fn emit_handler_state(emitter: &mut Emitter) {
    for kind in ["error", "exception"] {
        let prefix = format!("__rt_core_{kind}_handler");
        let fields = fields(kind);
        let fields = fields.iter().map(String::as_str).collect::<Vec<_>>();
        let context = fields.len() - 2;
        handler_owners::emit_release(
            emitter, &prefix, [fields[1], fields[2], fields[context], fields[context + 1]],
            emit_finish_cleanup,
        );
        handler_owners::emit_pop(emitter, &prefix, &fields, emit_finish_cleanup);
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
