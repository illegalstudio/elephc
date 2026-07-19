//! Purpose:
//! Emits the `__rt_array_key_exists` runtime helper assembly for array key exists.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Array helpers operate on runtime array headers and element cells; mutations must respect capacity and COW contracts.
//! - This helper used to inline a bounds check against the array header's first word.
//!   That is only correct on kind-2 (packed) storage: an `Array(_)`-typed local can be
//!   backed by *hash* storage at runtime (a mixed-key write promotes the storage, while
//!   the checker only promotes the static type to `AssocArray` at a provably string-keyed
//!   write), and on a hash header the first word is the live-entry COUNT, not a length —
//!   so `array_key_exists(0, $promoted)` answered `true` for a key that does not exist.
//!   It is now a thin adapter over the storage-kind-dispatching
//!   `__rt_array_key_exists_mixed_key`, whose packed arm performs the very same
//!   bounds check and whose hash arm delegates to `__rt_hash_get`'s found flag.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Checks whether an integer key exists in an indexed array, whatever its runtime storage kind.
///
/// ABI: x0 = array pointer, x1 = integer key (AArch64); rdi / rsi (x86_64).
/// Returns: x0 (AArch64) / rax (x86_64) = 1 if the key exists, 0 otherwise.
///
/// Tail-branches into `__rt_array_key_exists_mixed_key` after tagging the key as an integer
/// one (`key_hi = -1`, the int-key sentinel that helper and `__rt_hash_get` both expect).
/// The branch is a tail call, not a `bl`/`call`, so no frame is needed and the link register
/// still points at this helper's caller.
pub fn emit_array_key_exists(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_array_key_exists_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: array_key_exists ---");
    emitter.label_global("__rt_array_key_exists");

    emitter.instruction("mov x2, #-1");                                         // key_hi sentinel: tag the incoming key as an integer key
    emitter.instruction("b __rt_array_key_exists_mixed_key");                   // tail-call the storage-kind-dispatching presence probe
}

/// x86_64 Linux variant of `emit_array_key_exists`.
/// Uses the System V AMD64 ABI: rdi = array pointer, rsi = integer key, rax = return value.
fn emit_array_key_exists_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: array_key_exists ---");
    emitter.label_global("__rt_array_key_exists");

    emitter.instruction("mov rdx, -1");                                         // key_hi sentinel: tag the incoming key as an integer key
    emitter.instruction("jmp __rt_array_key_exists_mixed_key");                 // tail-call the storage-kind-dispatching presence probe
}
