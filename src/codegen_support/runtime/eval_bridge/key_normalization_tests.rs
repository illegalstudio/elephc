//! Purpose:
//! Keeps the shared eval key normalizer a callable routine across runtime helper sections.
//!
//! Called from:
//! - Eval bridge emitter tests in supported-target CI.
//!
//! Key details:
//! - The normalizer owns its frame and is called from several independent helpers.
//! - Its entry must survive Mach-O local-label rewriting and have an ELF function section.
//! - The reflector-list helpers hand eval the direct object arrays compiled readers expect.

use super::*;
use crate::codegen_support::emit::localize_internal_labels;
use crate::codegen_support::platform::Target;

/// Cross-helper calls retain the normalizer entry while its internal branches remain local.
#[test]
fn eval_key_normalizer_has_an_independent_entry_on_every_target() {
    let entry = "__elephc_eval_key_normalize";
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        emitter.dead_strip = true;
        if target.arch == Arch::X86_64 { emit_x86_64_arrays(&mut emitter); }
        else { emit_aarch64_arrays(&mut emitter); }
        let internal = emitter.take_internal_labels();
        assert!(!internal.contains(entry), "{name}: a cross-helper entry cannot become assembler-local");
        let assembly = localize_internal_labels(&emitter.output(), &internal);
        assert!(assembly.contains(&format!(".globl {entry}\n")), "{name}: retain the independently callable entry");
        assert!(assembly.contains(&format!("{entry}:\n")), "{name}");
        let call = if target.arch == Arch::X86_64 { "call" } else { "bl" };
        assert!(assembly.contains(&format!("{call} {entry}")), "{name}: callers keep the exact entry symbol");
        if target.platform == crate::codegen_support::platform::Platform::Linux {
            assert!(assembly.contains(&format!(".section .text.{entry},")), "{name}: independent ELF helper section");
        } else {
            assert!(internal.contains(&format!("{entry}_done")), "{name}: internal branches stay within the helper atom");
        }
    }
}

/// Eval's reflector lists are direct object arrays on every target: the allocator stamps the
/// object value_type and the append goes through the retaining runtime push, so a compiled
/// `array<Class>` reader of `__methods` or `__parameters` finds object pointers, never Mixed cells.
#[test]
fn eval_object_array_helpers_build_direct_object_arrays_on_every_target() {
    for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
        let target = Target::parse(name).unwrap();
        let mut emitter = Emitter::new(target);
        if target.arch == Arch::X86_64 { emit_x86_64_arrays(&mut emitter); }
        else { emit_aarch64_arrays(&mut emitter); }
        let assembly = emitter.output();
        let new_body = assembly
            .split("__elephc_eval_value_object_array_new:\n").nth(1)
            .and_then(|rest| rest.split("__elephc_eval_value_object_array_push").next())
            .unwrap_or_else(|| panic!("{name}: the object-array allocator is emitted"));
        let stamp = if target.arch == Arch::X86_64 { "mov r11, 6\n" } else { "mov x11, #6\n" };
        assert!(new_body.contains(stamp), "{name}: the allocator stamps value_type 6 (object)");
        let slot_width = if target.arch == Arch::X86_64 { "mov rsi, 8\n" } else { "mov x1, #8\n" };
        assert!(new_body.contains(slot_width), "{name}: object slots are one pointer wide");
        let push_body = assembly
            .split("__elephc_eval_value_object_array_push:\n").nth(1)
            .unwrap_or_else(|| panic!("{name}: the object-array append is emitted"));
        let call = if target.arch == Arch::X86_64 { "call" } else { "bl" };
        assert!(push_body.contains(&format!("{call} __rt_array_push_refcounted")),
            "{name}: the append retains the object through the runtime push");
        assert!(push_body.contains(&format!("{call} __rt_mixed_unbox")),
            "{name}: the append unboxes the Mixed object handle before pushing its payload");
    }
}
