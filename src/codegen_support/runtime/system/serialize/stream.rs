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

use crate::codegen_support::emit::Emitter;

/// Emits `__rt_ser_reserve`, `__rt_ser_tell`, and `__rt_ser_seek`.
///
/// - `__rt_ser_reserve`: `x1`/`rsi` = bytes about to be written. Returns the descriptor in
///   `x0`/`rdi` and the cursor in `x11`/`r11`; clobbers caller-saved registers.
/// - `__rt_ser_tell`: returns the bytes written so far in `x0`/`rax`. Leaf.
/// - `__rt_ser_seek`: `x0`/`rax` = offset; truncates the stream back to it. Leaf.
pub(super) fn emit_serialize_stream(emitter: &mut Emitter) {
    elephc_memoir::strings::scoped_stream::emit_serializer_helpers(emitter);
}
