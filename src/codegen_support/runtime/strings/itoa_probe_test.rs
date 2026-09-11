//! Probe: dump the legacy __rt_itoa asm for inspection.
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::{Arch, Platform, Target};

#[test]
fn dump_legacy_itoa_asm() {
    let mut emitter = Emitter::new(Target::new(Platform::MacOS, Arch::AArch64));
    crate::codegen_support::runtime::strings::emit_itoa(&mut emitter);
    let asm = emitter.output();
    std::fs::write("/tmp/ctx_bench/itoa_legacy.s", &asm).unwrap();
    assert!(asm.contains("__rt_itoa"), "itoa must be emitted");
}
