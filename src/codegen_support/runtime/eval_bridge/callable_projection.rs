//! Purpose:
//! Adds the hosted callable metadata projection to the generated eval runtime.
//!
//! Called from:
//! - Eval bridge runtime emission when Magician is linked.
//!
//! Key details:
//! - The public output layouts come from the shared ABI contract.

use crate::codegen_support::emit::Emitter;
use elephc_builtin_contract::callable_abi::{CallableParameterViewV1, CallableSignatureViewV1};
use elephc_memoir::callables::projection;
use std::mem::offset_of;

/// Emits both versioned projections using the layout consumed by Magician.
pub(super) fn emit(emitter: &mut Emitter) {
    projection::emit_signature(emitter, [
        offset_of!(CallableSignatureViewV1, parameters),
        offset_of!(CallableSignatureViewV1, name),
        offset_of!(CallableSignatureViewV1, name_length),
        offset_of!(CallableSignatureViewV1, parameter_count),
        offset_of!(CallableSignatureViewV1, regular_count),
        offset_of!(CallableSignatureViewV1, variadic),
    ]);
    projection::emit_parameter(emitter, [
        offset_of!(CallableParameterViewV1, name),
        offset_of!(CallableParameterViewV1, name_length),
        offset_of!(CallableParameterViewV1, type_tag),
        offset_of!(CallableParameterViewV1, by_ref),
    ]);
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::codegen_support::platform::{Arch, Target};
    use std::io::Write;
    use std::process::{Command, Stdio};

    /// The hosted C projections assemble with local branches on each native object format.
    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "requires a cross-target clang assembler")]
    fn native_callable_projections_assemble_on_supported_targets() {
        for (name, triple) in [
            ("macos-aarch64", "arm64-apple-macos11"),
            ("ios-arm64", "arm64-apple-ios13"),
            ("ios-sim-arm64", "arm64-apple-ios13-simulator"),
            ("linux-aarch64", "aarch64-linux-gnu"),
            ("linux-x86_64", "x86_64-linux-gnu"),
        ] {
            // Apple Clang's distribution need only cover its own targets in the default macOS run.
            if cfg!(target_os = "macos") && name.starts_with("linux") {
                continue;
            }
            let target = Target::parse(name).unwrap();
            let mut emitter = Emitter::new(target);
            if target.arch == Arch::X86_64 {
                emitter.emit_text_prelude();
            }
            emit(&mut emitter);
            if !name.starts_with("linux") {
                emitter.raw(".subsections_via_symbols");
            }

            let mut clang = Command::new("clang")
                .args(["-target", triple, "-x", "assembler", "-c", "-", "-o", "/dev/null"])
                .stdin(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .expect("start the target assembler");
            clang
                .stdin
                .take()
                .unwrap()
                .write_all(emitter.output().as_bytes())
                .expect("write the native callable projections");
            let output = clang.wait_with_output().expect("assemble the projections");
            assert!(
                output.status.success(),
                "{name}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
