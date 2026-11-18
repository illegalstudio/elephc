//! Purpose:
//! Keeps values overwritten by reentrant capture initialization alive through the PHP request.
//!
//! Called from:
//! - Native capture invocation after coordinator cleanup and mbstring request teardown.
//!
//! Key details:
//! - Adoption transfers one existing boxed owner without copying its PHP value.
//! - Raw queue nodes are not PHP graph edges, so retained values remain external GC roots.
//! - Draining detaches each node before PHP callbacks and includes newly adopted values.
//! - Queue order is adoption order; PHP object-store shutdown ordering is a separate concern.

use super::*;
use elephc_memoir::deferred_owners::{self, DeferredQueue};

const HEAD: &str = "_mbstring_deferred_capture_head";
const TAIL: &str = "_mbstring_deferred_capture_tail";
const ACTIVE: &str = "_mbstring_deferred_capture_draining";
const ADOPT: &str = "__rt_mbstring_defer_capture";
const DRAIN: &str = "__rt_mbstring_release_deferred_captures";

/// Emits native unary ownership adoption and a resumable request-end drain returning a pending flag.
pub(super) fn emit(emitter: &mut Emitter) {
    deferred_owners::emit(emitter, DeferredQueue { head: HEAD, tail: TAIL, active: ACTIVE, adopt: ADOPT, drain: DRAIN });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Ensures queue publication keeps its value outside the AArch64 symbol-address scratch.
    #[test]
    fn deferred_capture_queue_uses_distinct_symbol_and_value_registers_on_all_targets() {
        for name in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            emit(&mut emitter);
            let assembly = emitter.output();

            assert!(assembly.contains(DRAIN), "missing deferred drain for {name}");
            if target.arch == Arch::AArch64 {
                assert!(assembly.contains("str x10, [x9]"), "missing safe queue publication for {name}");
                assert!(!assembly.contains("str x9, [x9]"), "self-clobbering queue publication for {name}");
            } else {
                assert!(assembly.contains("r10"), "missing x86 queue scratch for {name}");
            }
        }
    }
}
