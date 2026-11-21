//! Purpose:
//! Adapts curl's foreign default-output callback to the compiled PHP output funnel.
//!
//! Called from:
//! - The curl runtime facade; its address is published by curl transfer lowerings.
//!
//! Key details:
//! - Keeps the C byte-pointer/length ABI and ordinary output-buffering semantics.
//! - Saves the foreign ctx register, publishes runtime state, then restores the host register.
//! - Names no Rust bridge symbol, so the reusable runtime remains pay-for-use safe.

use crate::codegen_support::{abi, emit::Emitter};

/// Emits a foreign-entry output sink with the same C arguments as the ordinary stdout funnel.
pub(super) fn emit_curl_output_sink(emitter: &mut Emitter) {
    emitter.blank();
    emitter.label_global("__rt_curl_output_sink");
    abi::emit_frame_prologue(emitter, 32);
    super::super::ctx::emit_ctx_save_foreign(emitter, 8);
    super::super::ctx::emit_ctx_publish(emitter);
    abi::emit_call_label(emitter, "__rt_stdout_write");
    super::super::ctx::emit_ctx_restore_foreign(emitter, 8);
    abi::emit_frame_restore(emitter, 32);
    abi::emit_return(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Default curl output uses a foreign-safe entry before reaching context-relative buffering.
    #[test]
    fn curl_output_sink_publishes_and_restores_context_on_every_target() {
        for name in ["macos-aarch64", "ios-arm64", "ios-sim-arm64", "linux-aarch64", "linux-x86_64"] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            emitter.ctx_register = true;
            emit_curl_output_sink(&mut emitter);
            let asm = emitter.output();
            let published = asm.find("_rt_ctx").expect("runtime context publish");
            let delegated = asm.find("__rt_stdout_write").expect("output funnel call");
            assert!(published < delegated, "{name}: publish before reading buffering state");
            assert!(asm[..published].contains(if name == "linux-x86_64" { "r14" } else { "x28" }),
                "{name}: spill foreign context before publishing");
            assert!(asm[delegated..].contains(if name == "linux-x86_64" { "r14" } else { "x28" }),
                "{name}: restore foreign context after writing");
        }
    }
}
