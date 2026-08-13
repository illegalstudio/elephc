//! Purpose:
//! Publishes the `elephc_curl` bridge entry points into the runtime's function-pointer
//! slots, so the shared runtime object's `__rt_curl_*` helpers can call them without ever
//! naming an `elephc_curl_*` symbol themselves.
//!
//! Called from:
//! - Every curl lowering in `crate::codegen::lower_inst::builtins::curl`, immediately
//!   before it calls a `__rt_curl_*` helper.
//!
//! Key details:
//! - THE PUBLISH IS WHAT PULLS THE BRIDGE IN. This code appears only in a program that
//!   lowered a curl builtin, so a curl-free binary never references an `elephc_curl_*`
//!   symbol, never links `-lelephc_curl`, and never needs the managed native `curl`
//!   package (locked decision 4). Mirrors
//!   `crate::codegen_support::hash_crypto::publish_elephc_crypto_function_pointers`.
//! - ALL SLOTS ARE PUBLISHED TOGETHER, at every curl call site, rather than one slot per
//!   builtin. The alternative — publishing only what the current lowering calls — would
//!   leave `_elephc_curl_easy_free_fn` unpublished in a program that creates a handle
//!   through a lowering that does not itself free, and the leak would surface only at
//!   scope exit, far from anything a reader would connect to this file. The cost is a
//!   handful of stores on a path that is about to make a network call.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::codegen_support::runtime::curl_abi_slots;

/// Publishes every `elephc_curl` entry point into its runtime slot.
pub(crate) fn publish_elephc_curl_function_pointers(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => {
            for (c_name, slot) in curl_abi_slots() {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "x9", &extern_sym);
                abi::emit_symbol_address(emitter, "x10", slot);
                emitter.instruction("str x9, [x10]");                           // publish the elephc-curl entry into its runtime slot
            }
        }
        Arch::X86_64 => {
            for (c_name, slot) in curl_abi_slots() {
                let extern_sym = emitter.target.extern_symbol(c_name);
                abi::emit_extern_symbol_address(emitter, "r9", &extern_sym);
                abi::emit_store_reg_to_symbol(emitter, "r9", slot, 0);          // publish the elephc-curl entry into its runtime slot
            }
        }
    }
}
