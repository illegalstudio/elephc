//! Purpose:
//! Stages the shared native query policy used by AOT and eval mb_parse_str calls.
//!
//! Called from:
//! - Typed mbstring builtin lowering and the boxed eval runtime dispatcher.
//!
//! Key details:
//! - Both routes use the live Core INI provider and no SAPI-specific input filter.
//! - The caller retains aligned stack storage until the V5 coordinator retires deferred ownership.

use super::{abi, emit::Emitter, platform::Arch};
use elephc_builtin_contract::mbstring_abi::invoke::{MbNativeCaptureV1, MbNativeQueryV1};

const CAPTURE_MODE_OFFSET: usize = std::mem::offset_of!(MbNativeQueryV1, capture)
    + std::mem::offset_of!(MbNativeCaptureV1, mode);
const CAPTURE_DISCARDED_OFFSET: usize = std::mem::offset_of!(MbNativeQueryV1, capture)
    + std::mem::offset_of!(MbNativeCaptureV1, discarded);
const CONTEXT_OFFSET: usize = std::mem::offset_of!(MbNativeQueryV1, context);
const CONFIGURATION_OFFSET: usize = std::mem::offset_of!(MbNativeQueryV1, configuration);
const FILTER_OFFSET: usize = std::mem::offset_of!(MbNativeQueryV1, filter);

/// Stack-aligned storage for one invocation's native query policy and displaced output owner.
pub(crate) const STATE_BYTES: usize = (std::mem::size_of::<MbNativeQueryV1>() + 15) & !15;

/// Initializes untyped output publication and supplies the live query policy as the sixth C argument.
pub(crate) fn stage(emitter: &mut Emitter, offset: usize) {
    let target = emitter.target;
    let scratch = match target.arch { Arch::AArch64 => "x9", Arch::X86_64 => "r10" };
    let query_configuration = target.extern_symbol("elephc_mbstring_query_configuration_v1");
    abi::emit_load_int_immediate(emitter, scratch, 0);
    for field in [
        CAPTURE_MODE_OFFSET,
        CAPTURE_DISCARDED_OFFSET,
        CONTEXT_OFFSET,
        FILTER_OFFSET,
    ] {
        abi::emit_store_to_sp(emitter, scratch, offset + field);
    }
    abi::emit_extern_symbol_address(emitter, scratch, &query_configuration);
    abi::emit_store_to_sp(emitter, scratch, offset + CONFIGURATION_OFFSET);
    abi::emit_temporary_stack_address(emitter, abi::int_arg_reg_name(target, 5), offset);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Target;

    /// Verifies query-policy staging uses the target C ABI spelling on every supported target.
    #[test]
    fn query_configuration_symbol_is_target_aware() {
        for (name, relocation) in [
            ("macos-aarch64", "_elephc_mbstring_query_configuration_v1@GOTPAGE"),
            ("ios-arm64", "_elephc_mbstring_query_configuration_v1@GOTPAGE"),
            ("ios-sim-arm64", "_elephc_mbstring_query_configuration_v1@GOTPAGE"),
            ("linux-aarch64", "elephc_mbstring_query_configuration_v1"),
            ("linux-x86_64", "elephc_mbstring_query_configuration_v1@GOTPCREL"),
        ] {
            let mut emitter = Emitter::new(Target::parse(name).unwrap());
            stage(&mut emitter, 0);
            let asm = emitter.output();
            assert!(asm.contains(relocation), "{name}:\n{asm}");
        }
    }
}
