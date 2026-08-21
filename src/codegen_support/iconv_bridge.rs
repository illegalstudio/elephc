//! Purpose:
//! Publishes the `elephc_iconv` bridge entry points into their runtime function-pointer
//! slots so the shared iconv runtime helpers can call through them.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::iconv`, immediately before each
//!   `__rt_iconv_call` / `__rt_iconv_call_bool` call.
//!
//! Key details:
//! - Publishing at the call site (rather than inside the shared runtime) is what makes a
//!   program reference the bridge symbols at all, so only programs that call an iconv
//!   builtin pull in `-lelephc_iconv`.
//! - Both slots are published together because every operation allocates through the
//!   bridge and must release through it again.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::{abi, platform::Arch};

/// Publishes `elephc_iconv_call` and `elephc_iconv_release` into their runtime slots.
pub(crate) fn publish_elephc_iconv_function_pointers(emitter: &mut Emitter) {
    const ENTRIES: &[(&str, &str)] = &[
        ("elephc_iconv_call", "_elephc_iconv_call_fn"),
        ("elephc_iconv_release", "_elephc_iconv_release_fn"),
    ];
    match emitter.target.arch {
        Arch::AArch64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "x9", &extern_sym);
                abi::emit_symbol_address(emitter, "x10", slot);
                emitter.instruction("str x9, [x10]");                           // publish the elephc-iconv entry into its runtime slot
            }
        }
        Arch::X86_64 => {
            for (c_name, slot) in ENTRIES {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "r9", &extern_sym);
                abi::emit_store_reg_to_symbol(emitter, "r9", slot, 0);          // publish the elephc-iconv entry into its runtime slot
            }
        }
    }
}
