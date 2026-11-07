//! Purpose:
//! Verifies shared fatal paths enter the PHP boundary before any raw diagnostic write.
//!
//! Called from:
//! - Focused library tests and the six real-PHP SDK configurations.
//!
//! Key details:
//! - Every library target retains ordinary cdylib diagnostics while PHP owns hosted reporting.

use super::{arrays, buffers, pointers};
use crate::codegen_support::{emit::Emitter, platform::{AppleVariant, Arch, Platform, Target}};

/// Hosted fatal branches unwind before stderr while ordinary cdylib branches keep their diagnostics.
#[test]
fn hosted_fatal_paths_escape_before_raw_diagnostics() {
    for target in [Target::new(Platform::Linux, Arch::X86_64),
        Target::new(Platform::Linux, Arch::AArch64), Target::new(Platform::MacOS, Arch::AArch64),
        Target::new_apple(Arch::AArch64, AppleVariant::IOS),
        Target::new_apple(Arch::AArch64, AppleVariant::IOSSimulator)] {
        let sort_label = if target.arch == Arch::X86_64 { "__rt_mixed_sort_non_scalar_x86" }
            else { "__rt_mixed_sort_non_scalar" };
        let cases: [(fn(&mut Emitter), &str, &str); 8] = [
            (arrays::emit_array_new, "__rt_array_cap_overflow", "_arr_cap_err_msg"),
            (arrays::emit_hash_new, "__rt_hash_cap_overflow", "_arr_cap_err_msg"),
            (buffers::emit_buffer_new, "__rt_buffer_new_size_fail", "_buffer_alloc_size_msg"),
            (buffers::emit_buffer_bounds_fail, "__rt_buffer_bounds_fail", "_buffer_bounds_msg"),
            (buffers::emit_buffer_use_after_free, "__rt_buffer_use_after_free", "_buffer_uaf_msg"),
            (buffers::emit_buffer_registry_fail, "__rt_buffer_registry_exhausted", "_buffer_registry_exhausted_msg"),
            (pointers::emit_ptr_check_nonnull, "__rt_ptr_check_nonnull", "_ptr_null_err_msg"),
            (arrays::emit_php_compare_slots, sort_label, "_mixed_sort_non_scalar_msg"),
        ];
        for (emit, label, message) in cases {
            for hosted in [false, true] {
                let mut emitter = Emitter::new_cdylib(target);
                emitter.phpext_output = hosted;
                emit(&mut emitter);
                let asm = emitter.output();
                let body = &asm[asm.find(&format!("{label}:")).unwrap()..];
                let diagnostic = body.find(message).unwrap();
                assert_eq!(body[..diagnostic].contains("__rt_throw_current"), hosted,
                    "{target:?} {label} hosted={hosted}: {}", &body[..diagnostic]);
            }
        }
    }
}
