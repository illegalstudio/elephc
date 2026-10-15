//! Purpose:
//! Pins exceptional ownership of SPL offset conversion on every supported target.
//!
//! Called from:
//! - Focused compiler runtime emitter tests.
//!
//! Key details:
//! - Parsing numeric strings and buffering float-string diagnostics may allocate.
//! - Every ordinary box release must detach its exceptional owner first.

use super::*;
use crate::codegen_support::platform::Target;

/// Verifies a guard covers parsing and fragments, and all normal releases retire that guard.
#[test]
fn spl_offset_conversion_guards_allocating_paths_on_all_targets() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let mut emitter = Emitter::new(Target::parse(name).unwrap());
        match emitter.target.arch {
            Arch::AArch64 => emit_convert_aarch64(&mut emitter),
            Arch::X86_64 => emit_convert_x86_64(&mut emitter),
        }
        let assembly = emitter.output();
        let guard = assembly.find("__rt_exception_guard_owned").expect("offset owner must be guarded");
        let parse = assembly.find("__rt_str_numeric_value").unwrap();
        let fragment = assembly.find("__rt_diag_warning_fragment").unwrap();
        assert!(guard < parse && guard < fragment, "{name}: guard must precede allocations");
        assert_eq!(assembly.matches("__rt_exception_guard_owned").count(), 1, "{name}");
        assert_eq!(assembly.matches("__rt_exception_unguard_owned").count(), 3, "{name}");
        let mut last_release = 0;
        for (release, _) in assembly.match_indices("__rt_decref_mixed") {
            assert!(assembly[last_release..release].contains("__rt_exception_unguard_owned"),
                "{name}: release must detach its guard");
            last_release = release + "__rt_decref_mixed".len();
        }
        assert_eq!(assembly.matches("__rt_decref_mixed").count(), 3, "{name}");
    }
}

