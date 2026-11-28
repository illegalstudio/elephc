//! Purpose:
//! Emits geometric string append after EIR transfers a destination local's owner.
//!
//! Called from:
//! - `runtime::emitters::emit_runtime` for every supported target.
//!
//! Key details:
//! - Inputs and result use concat's pointer/length ABI; RHS is borrowed.
//! - Unique owned buffers may grow in place; other paths copy before retiring the old owner.
//! - Allocation failure preserves the old local owner until replacement succeeds.

use crate::{Emitter, Arch};

mod aarch64;
mod x86_64;

/// Emits checked, consuming append with fresh metadata for every changed string.
pub fn emit_str_append(emitter: &mut impl Emitter, mbstring: bool) {
    match emitter.arch() {
        Arch::AArch64 => aarch64::emit(emitter, mbstring),
        Arch::X86_64 => x86_64::emit(emitter, mbstring),
    }
}
