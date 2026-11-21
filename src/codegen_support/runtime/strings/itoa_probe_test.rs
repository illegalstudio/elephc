//! Purpose:
//! Dumps the legacy AArch64 integer-formatting emitter for local assembly inspection.
//!
//! Called from:
//! - The diagnostic probe test when this module is explicitly included.
//!
//! Key details:
//! - The local artifact directory `/tmp/ctx_bench` must already exist.
//! - Symbol emission is checked; this diagnostic is not native execution evidence.
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform, Target};

/// Writes the legacy itoa assembly to the local probe artifact and checks its runtime symbol.
#[test]
fn dump_legacy_itoa_asm() {
    let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
    crate::codegen_support::runtime::strings::emit_itoa(&mut emitter);
    let asm = emitter.output();
    std::fs::write("/tmp/ctx_bench/itoa_legacy.s", &asm).unwrap();
    assert!(asm.contains("__rt_itoa"), "itoa must be emitted");
}
