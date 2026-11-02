//! Purpose:
//! Checks that float sorting emits its complete runtime for every supported target.
//!
//! Called from:
//! - The compiler library test harness.
//!
//! Key details:
//! - Native execution tests cover ordering, stability and temporary storage cleanup.
//! - This gate includes both iOS variants so their shared ARM64 path cannot be omitted.

use super::*;
use crate::codegen_support::platform::Target;

/// Every supported target provides the entry, its callees and matching allocation symbols.
#[test]
fn float_sort_emits_complete_helpers_on_every_supported_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).expect("supported target");
        let mut emitter = Emitter::new(target);
        emit_float_sort(&mut emitter);
        let output = emitter.output();
        for label in ["__rt_sort_float", "__rt_sort_mixed", "__rt_float_sort_compare", "__rt_float_sort_swap",
                      "__rt_float_sort_small", "__rt_float_sort_range", "__rt_float_sort_insert"] {
            assert!(output.contains(&format!("{label}:")), "{name}: {label}");
        }
        for comparator in ["__rt_float_compare_slots", "__rt_float_compare_slots_desc",
                           "__rt_php_compare_slots", "__rt_php_compare_slots_desc"] {
            assert!(output.contains(comparator), "{name}: {comparator}");
        }
        assert!(output.contains(&target.extern_symbol("malloc")), "{name}");
        assert!(output.contains(&target.extern_symbol("free")), "{name}");
        assert!(output.contains("__rt_heap_allocation_failed"), "{name}");
    }
}
