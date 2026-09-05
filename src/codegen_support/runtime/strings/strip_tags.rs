//! Purpose:
//! Emits `__rt_strip_tags` for PHP 8.5 `strip_tags()`, including the optional
//! allow-list, for every supported target.
//!
//! Called from:
//! - `crate::codegen_support::runtime::emitters::emit_runtime()` via
//!   `crate::codegen_support::runtime::strings`.
//!
//! Key details:
//! - Ports php-src `php_strip_tags_ex(..., allow_tag_spaces=0)` and `php_tag_find`.
//! - Input is a subject pointer/length plus an allow-list pointer/length. A zero
//!   allow length means "strip every tag". Output is a fresh string in the
//!   target string-result registers, reserved through `__rt_concat_reserve`.
//! - HTML comments and PHP tags are always stripped and cannot be allow-listed.

use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

mod aarch64;
mod x86_64;

/// Emits `__rt_strip_tags` for the active target.
pub fn emit_strip_tags(emitter: &mut Emitter) {
    if emitter.target.arch == Arch::X86_64 {
        x86_64::emit_strip_tags_linux_x86_64(emitter);
        return;
    }
    aarch64::emit_strip_tags_aarch64(emitter);
}

/// Emits one strip_tags helper instruction.
pub(super) fn emit_st(emitter: &mut Emitter, inst: &str) {
    emitter.instruction(inst);                                                  // PHP strip_tags state-machine instruction
}
