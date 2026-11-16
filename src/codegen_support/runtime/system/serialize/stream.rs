//! Purpose:
//! Emits the small helpers every serializer writes the `serialize()` output stream through:
//! the stream is a growable `__rt_strbuf_*` buffer owned by `__rt_serialize_call_value`.
//!
//! Called from:
//! - `super::emit_serialize()` during runtime emission.
//!
//! Key details:
//! - `__rt_serialize_call_value` (see `context.rs`) opens a descriptor in its own frame at the
//!   scratch tail, installs it as `_ser_stream`, runs the recursive serializer, and returns the
//!   finished slice: scratch-backed while it fits, otherwise a transient heap block
//!   `__rt_str_persist` adopts. The previous `_ser_stream` is reinstated on every exit, so a
//!   `serialize()` inside `__serialize()` gets its own stream, and a throw frees a grown buffer.
//! - The serializers used to append at `_concat_buf + _concat_off` with no bound, so a result
//!   past 64 KiB overwrote the adjacent globals (`serialize(range(1, 50000))` came back with a
//!   corrupted `a:` header). They now reserve room through `__rt_ser_reserve` first.
//! - Positions that must survive growth (a value's start, an object's prefix, the hoisted
//!   `<count>:{` tail) are stream OFFSETS from `__rt_ser_tell`, never absolute pointers.

use crate::codegen_support::abi;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;

/// Emits `__rt_ser_reserve`, `__rt_ser_tell`, and `__rt_ser_seek`.
///
/// - `__rt_ser_reserve`: `x1`/`rsi` = bytes about to be written. Returns the descriptor in
///   `x0`/`rdi` and the cursor in `x11`/`r11`; clobbers caller-saved registers.
/// - `__rt_ser_tell`: returns the bytes written so far in `x0`/`rax`. Leaf.
/// - `__rt_ser_seek`: `x0`/`rax` = offset; truncates the stream back to it. Leaf.
pub(super) fn emit_serialize_stream(emitter: &mut Emitter) {
    emitter.blank();
    emitter.comment("--- runtime: serialize output stream ---");
    match emitter.target.arch {
        Arch::AArch64 => {
            emitter.label_global("__rt_ser_reserve");
            abi::emit_symbol_address(emitter, "x9", "_ser_stream");
            emitter.instruction("ldr x0, [x9]");                                // active stream descriptor
            emitter.instruction("b __rt_strbuf_ensure");                        // grow it when x1 more bytes would not fit

            emitter.label_global("__rt_ser_tell");
            abi::emit_symbol_address(emitter, "x9", "_ser_stream");
            emitter.instruction("ldr x9, [x9]");                                // active stream descriptor
            emitter.instruction("ldr x10, [x9, #8]");                           // stream cursor
            emitter.instruction("ldr x9, [x9]");                                // stream start
            emitter.instruction("sub x0, x10, x9");                             // bytes written so far
            emitter.instruction("ret");                                         // return the stream offset

            emitter.label_global("__rt_ser_seek");
            abi::emit_symbol_address(emitter, "x9", "_ser_stream");
            emitter.instruction("ldr x9, [x9]");                                // active stream descriptor
            emitter.instruction("ldr x10, [x9]");                               // stream start
            emitter.instruction("add x10, x10, x0");                            // cursor at the requested offset
            emitter.instruction("str x10, [x9, #8]");                           // truncate the stream back to it
            emitter.instruction("ret");                                         // return to the serializer
        }
        Arch::X86_64 => {
            emitter.label_global("__rt_ser_reserve");
            abi::emit_load_symbol_to_reg(emitter, "rdi", "_ser_stream", 0);
            emitter.instruction("jmp __rt_strbuf_ensure");                      // grow it when rsi more bytes would not fit

            emitter.label_global("__rt_ser_tell");
            abi::emit_load_symbol_to_reg(emitter, "r8", "_ser_stream", 0);
            emitter.instruction("mov rax, QWORD PTR [r8 + 8]");                 // stream cursor
            emitter.instruction("sub rax, QWORD PTR [r8]");                     // bytes written so far
            emitter.instruction("ret");                                         // return the stream offset

            emitter.label_global("__rt_ser_seek");
            abi::emit_load_symbol_to_reg(emitter, "r8", "_ser_stream", 0);
            emitter.instruction("add rax, QWORD PTR [r8]");                     // cursor at the requested offset
            emitter.instruction("mov QWORD PTR [r8 + 8], rax");                 // truncate the stream back to it
            emitter.instruction("ret");                                         // return to the serializer
        }
    }
}
