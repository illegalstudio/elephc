//! Purpose:
//! Emits `__rt_resource_id_burn`, which advances the PHP-visible resource-id cursor by one
//! without creating a resource.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::io`, for the whole-file readers.
//!
//! Key details:
//! - php's `file_get_contents()` and `file_put_contents()` OPEN a stream internally, so each call
//!   consumes one resource id even though the caller never sees a handle. elephc reads and writes
//!   those files with raw syscalls and minted nothing, so every id after the first such call was
//!   one lower than php's — visible through `var_dump($handle)`, `(int) $handle` and
//!   `get_resource_id()`. Measured on `php -n` 8.5.6: with NO prior I/O the two agree, and each
//!   `file_get_contents`/`file_put_contents` shifts them apart by exactly one.
//! - The cursor is never reused, which is why advancing it is all that is needed: php does not
//!   hand the id back when its internal stream closes either.

use crate::codegen_support::{abi, emit::Emitter, platform::Arch};

/// Emits `__rt_resource_id_burn()`, consuming one PHP resource id.
///
/// Clobbers only the scratch registers and preserves the string-result pair, so it can be spliced
/// into a reader's lowering without disturbing the bytes it is carrying.
pub fn emit_resource_id_burn(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: resource_id_burn ---");
    emitter.label_global("__rt_resource_id_burn");
    match emitter.target.arch {
        Arch::AArch64 => {
            abi::emit_symbol_address(emitter, "x9", "_resource_id_next");
            emitter.instruction("ldr x10, [x9]");                               // the next PHP-visible id
            emitter.instruction("add x10, x10, #1");                            // php spent one on its internal stream
            emitter.instruction("str x10, [x9]");                               // publish the advanced cursor
            emitter.instruction("ret");
        }
        Arch::X86_64 => {
            abi::emit_symbol_address(emitter, "r10", "_resource_id_next");
            emitter.instruction("mov r11, QWORD PTR [r10]");                    // the next PHP-visible id
            emitter.instruction("add r11, 1");                                  // php spent one on its internal stream
            emitter.instruction("mov QWORD PTR [r10], r11");                    // publish the advanced cursor
            emitter.instruction("ret");
        }
    }
}
