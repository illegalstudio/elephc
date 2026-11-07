//! Purpose:
//! Keeps the shared eval key normalizer a callable routine across runtime helper sections.
//!
//! Called from:
//! - Eval bridge emitter tests in supported-target CI.
//!
//! Key details:
//! - The normalizer owns its frame and is called from several independent helpers.
//! - Its entry must survive Mach-O local-label rewriting and have an ELF function section.

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
