//! Purpose:
//! Emits the `__rt_hash_count` runtime helper assembly for hash count.
//! Keeps PHP array/hash storage, heap ownership, and target-specific ABI variants in one focused emitter.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via `crate::codegen_support::runtime::arrays`.
//!
//! Key details:
//! - Hash helpers must normalize PHP keys and preserve bucket layout, ownership, and iteration conventions.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use elephc_memoir::hash_layout::{self, HeaderField};

/// hash_count: get the number of entries in a hash table.
/// Input:  `x0`/`rdi` = hash table pointer.
/// Output: `x0`/`rax` = count.
pub fn emit_hash_count(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        emit_hash_count_linux_x86_64(emitter);
        return;
    }

    emitter.blank();
    emitter.comment("--- runtime: hash_count ---");
    emitter.label_global("__rt_hash_count");

    hash_layout::emit_load_header_field(emitter, "x0", "x0", HeaderField::Count);
    emitter.instruction("ret");                                                 // return to caller
}

/// Emits the Linux x86_64 variant of `__rt_hash_count`.
fn emit_hash_count_linux_x86_64(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: hash_count ---");
    emitter.label_global("__rt_hash_count");

    hash_layout::emit_load_header_field(emitter, "rax", "rdi", HeaderField::Count);
    emitter.instruction("ret");                                                 // return count to caller
}
