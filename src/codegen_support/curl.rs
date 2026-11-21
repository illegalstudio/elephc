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
//!   package — the pay-for-use guarantee this crate holds for every optional native
//!   surface. Mirrors
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

/// Publishes the compiled program's terminal-output funnel to the curl bridge.
///
/// `__rt_stdout_write` is the single indirection every `echo` travels through: the
/// `print_r` capture buffer, the output-handler discard, the `ob_*` stack, the `--web`
/// response capture, and only then the `write(1, …)` syscall. PHP's default `curl_exec()`
/// write handler goes through the engine's output layer for the same reason, so
/// `ob_start(); curl_exec($ch); $html = ob_get_clean();` captures the body — where the
/// bridge's own `write(1, …)` bypassed all of it and left the buffer empty (issue #875).
///
/// EMITTED AS A CALL, not as one of the published function-pointer SLOTS above, because the
/// direction is reversed: those let the runtime call INTO the bridge, while this hands the
/// bridge an address to call BACK with. The bridge stores it opaquely and never names a
/// `__rt_*` symbol, which is what keeps `elephc-curl` linkable on its own.
///
/// It clobbers the argument registers, so it must be emitted BEFORE the call's own operands
/// are loaded. Publishing it at every transfer site rather than once at startup keeps the
/// pay-for-use property this module holds: a curl-free binary emits none of this.
pub(crate) fn publish_elephc_curl_output_sink(emitter: &mut Emitter) {
    match emitter.target.arch {
        Arch::AArch64 => abi::emit_symbol_address(emitter, "x0", "__rt_curl_output_sink"),
        Arch::X86_64 => abi::emit_symbol_address(emitter, "rdi", "__rt_curl_output_sink"),
    }
    emitter.bl_c("elephc_curl_set_output_sink");                                // hand the bridge a foreign-safe entry to the ordinary output funnel
}

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
