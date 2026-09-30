//! Purpose:
//! Dumps the generated linux-x86_64 runtime assembly to stdout, so an aarch64 host can
//! assemble and EXECUTE it inside an x86_64 container (GNU as/ld) to reproduce
//! x86-only failures that CI sees and this host cannot run natively.
//!
//! Called from:
//! - `cargo run --example dump_x86_runtime > runtime_x86.s` during x86 debugging.
//!
//! Key details:
//! - `RuntimeFeatures::none()` matches the plain-program feature set; feature-gated helpers
//!   missing at link time show up as unresolved symbols, which is the escalation signal.

fn main() {
    let target = elephc::codegen::platform::Target::parse("linux-x86_64").expect("valid target");
    let runtime_asm = elephc::codegen::generate_runtime_with_features(
        8_388_608,
        target,
        elephc::codegen::RuntimeFeatures::none(),
    );
    print!("{}", runtime_asm);
}
