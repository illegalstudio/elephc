//! Purpose:
//! Emits target dispatch for JSON string escaping runtime helpers.
//! Provides the runtime assembly used by JSON builtins on the selected target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::system` during runtime emission.
//!
//! Key details:
//! - The exported helper name must stay stable for all JSON encoder call sites.
//! - Both targets reserve the worst-case output (`6 × length + 2`: every byte may become a
//!   `\u00XX` escape) through `__rt_strbuf_reserve_temp` before writing, so a long string gets a
//!   transient heap block instead of running past the 64 KiB `_concat_buf` scratch. Container
//!   encoders append the result and release that block; `__rt_concat_publish` advances
//!   `_concat_off` only for a scratch-backed result.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

mod aarch64;
mod x86_64;

/// Dispatches to the target-specific `__rt_json_encode_str` emitter.
///
/// Writes the `__rt_json_encode_str` global label and all associated helpers
/// to `emitter`.  Input/outputABI:
/// - ARM64:  x1=src ptr,  x2=src len  →  x1=result ptr (in concat_buf), x2=result len
/// - x86_64: rax=src ptr, rdx=src len → rax=result ptr (in concat_buf), rdx=result len
///
/// The emitted helper is callable from generated code after `emit_json_encode_str`
/// has been output.  The symbol name `__rt_json_encode_str` is stable for all call sites.
pub(crate) fn emit_json_encode_str(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit(emitter);
        return;
    }

    aarch64::emit(emitter);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// The short plain-ASCII fast path classifies every byte before it reserves output, so an
    /// escapable byte can never abandon a transient heap block. Covers every supported target.
    #[test]
    fn json_str_fast_path_classifies_before_reserving() {
        for target in [
            "macos-aarch64",
            "ios-arm64",
            "ios-sim-arm64",
            "linux-aarch64",
            "linux-x86_64",
        ] {
            let mut emitter = Emitter::new(Target::parse(target).unwrap());
            emit_json_encode_str(&mut emitter);
            let asm = emitter.output();
            let (quoted, slow) = if target == "linux-x86_64" {
                ("__rt_json_str_quoted_x", "__rt_json_str_size_slow_x")
            } else {
                ("__rt_json_str_quoted", "__rt_json_str_size_slow")
            };
            let quoted_label = format!("{quoted}:");
            let slow_label = format!("{slow}:");
            let fast_path = asm
                .split(&quoted_label)
                .nth(1)
                .unwrap_or_else(|| panic!("{target}: missing quoted fast path\n{asm}"))
                .split(&slow_label)
                .next()
                .unwrap();
            let scan = fast_path.find("scan_loop").unwrap_or_else(|| {
                panic!("{target}: fast path lacks a classification pass\n{fast_path}")
            });
            let reserve = fast_path
                .find("__rt_strbuf_reserve_temp")
                .unwrap_or_else(|| panic!("{target}: fast path lacks a reservation\n{fast_path}"));
            assert!(
                scan < reserve,
                "{target}: fast path reserves before classifying\n{fast_path}"
            );
        }
    }
}
