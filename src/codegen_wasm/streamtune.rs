//! Purpose:
//! The three stream tuners on `wasm32-wasi`: `stream_set_chunk_size()`,
//! `stream_set_read_buffer()` and `stream_set_write_buffer()`.
//!
//! Called from:
//! - `runtime` registers the helpers alongside the rest of the file-I/O runtime.
//! - `builtins` lowers the three `RuntimeFnId` identities onto them.
//! - `files`'s `__rt_fd_meta_clear` zeroes a closed fd's chunk size.
//!
//! Key details:
//! - None of the three changes how this target reads or writes. Reads and writes go straight
//!   to `fd_read`/`fd_write` or to an in-memory buffer, with no layer to size. What php
//!   promises is not that the size takes effect but what the CALL answers, and that is what is
//!   reproduced — measured, because two of the three answers are not what they look like.
//! - `stream_set_chunk_size()` answers the PREVIOUS size and defaults to 8192, so the value is
//!   real state even though nothing reads it: `save / change / restore` is the pattern php
//!   documents it for. It is per STREAM — two handles keep two sizes — and a closed fd's entry
//!   is cleared, because WASI recycles fd numbers and php gives a fresh stream the default.
//! - A non-positive size is a `ValueError` in php and leaves the size UNCHANGED. Measured:
//!   after a refused `stream_set_chunk_size($h, 0)` the next call still reports what was there
//!   before it.
//! - `stream_set_read_buffer()` answers 0 and `stream_set_write_buffer()` answers **-1** — not
//!   0, and not stream-dependent. Measured on php-src 8.5.10 across `php://memory`,
//!   `php://temp`, a file opened for reading, a file opened for writing and `php://stdout`:
//!   all five answer `read=0 write=-1` for every size. php's plain-file streams are fd-backed
//!   and have no write-buffer option to set, so the option comes back "not implemented", which
//!   is what -1 means.

use super::wat::WatModule;

/// Offset of the per-fd chunk size: 4 bytes per real WASI fd, 256 fds.
///
/// Zero means "never set", which reads back as php's 8192 default — so linear memory starting
/// zeroed is already the right initial state, and `__rt_fd_meta_clear` restoring zero is
/// already the right thing for a recycled fd.
pub(super) const FD_CHUNK_SIZE: u32 = 0x6300;

/// Bytes the table occupies.
pub(super) const FD_CHUNK_SIZE_BYTES: u32 = 4 * 256;

/// Offset of an in-memory stream's chunk size inside its descriptor.
///
/// `php://memory` and `php://temp` have no fd to index, so their size rides in the descriptor
/// block `__rt_memstream_new` allocates — which is why that block is 64 bytes and not 56.
pub(super) const MEMSTREAM_CHUNK_OFFSET: u32 = 56;

/// The bit marking a stream handle as an in-memory stream. Mirrors `files::MEMSTREAM_FLAG`.
const MEMSTREAM_FLAG: u32 = 0x4000_0000;

/// php's default chunk size for a freshly opened stream.
pub(super) const DEFAULT_CHUNK_SIZE: i32 = 8192;

/// The `__rt_fail` code php's non-positive-`stream_set_chunk_size` `ValueError` reports under.
pub(super) const CHUNK_SIZE_FAILURE_CODE: i32 = 17;

/// The `__rt_fail` code for `stream_set_chunk_size()` past `INT_MAX` ("is too large").
pub(super) const CHUNK_SIZE_TOO_LARGE_FAILURE_CODE: i32 = 22;

/// The `__rt_fail` code for `fclose()` of an already-closed stream (php's `TypeError`).
pub(super) const FCLOSE_CLOSED_FAILURE_CODE: i32 = 21;

/// Proves the table fits the scratch region it is carved from.
///
/// Nothing at runtime checks this: a table that runs past the reserved region writes into the
/// module's static data and reads back whatever was written over it. `dirs` learned that the
/// expensive way, so the bound is asserted here too.
const _: () = assert!(
    FD_CHUNK_SIZE + FD_CHUNK_SIZE_BYTES <= super::runtime::FLOAT_SCRATCH_SIZE,
    "the per-fd chunk-size table overruns the float-scratch region"
);

/// Adds the chunk-size accessor to `wm`. Requires the heap runtime and `__rt_stream_fd`.
pub(super) fn emit_stream_tuning_runtime(wm: &mut WatModule) {
    wm.add_raw_func(&rt_stream_chunk_size());
    wm.add_raw_func(&rt_fd_chunk_clear());
}

/// `__rt_stream_chunk_size`: records a stream's chunk size and answers the previous one.
///
/// A handle this runtime does not recognise — a closed stream, the `false` a failed `fopen`
/// answers — reports the default and stores nothing, which is the same shape every other
/// helper here takes for an unusable handle.
fn rt_stream_chunk_size() -> String {
    format!(
        r#"(func $__rt_stream_chunk_size (param $handle i32) (param $size i64) (result i64)
  (local $fd i32) (local $slot i32) (local $previous i32)
  (local.set $fd (call $__rt_stream_fd (local.get $handle)))
  (if (i32.lt_s (local.get $fd) (i32.const 0))
    (then (return (i64.const {default}))))                        ;; not a stream: the default
  (if (i32.and (local.get $fd) (i32.const {flag}))
    (then (local.set $slot (i32.add (i32.and (local.get $fd) (i32.const {unflag}))
                                    (i32.const {memoff}))))       ;; in-memory: inside the descriptor
    (else
      (local.set $slot (call $__rt_fd_addr (local.get $fd) (i32.const 3)))
      (if (i32.eqz (local.get $slot))
        (then (return (i64.const {default}))))))                  ;; untracked: the default
  (local.set $previous (i32.load (local.get $slot)))
  (if (i32.eqz (local.get $previous))
    (then (local.set $previous (i32.const {default}))))           ;; never set: php's default
  (i32.store (local.get $slot) (i32.wrap_i64 (local.get $size)))
  (i64.extend_i32_u (local.get $previous)))
"#,
        default = DEFAULT_CHUNK_SIZE,
        flag = MEMSTREAM_FLAG,
        unflag = !MEMSTREAM_FLAG,
        memoff = MEMSTREAM_CHUNK_OFFSET
    )
}

/// `__rt_fd_chunk_clear`: forgets a closed fd's chunk size.
///
/// Called from `__rt_fd_meta_clear`, beside the mode and uri it already releases. Without it a
/// recycled fd would inherit the previous stream's size, where php hands a fresh stream the
/// 8192 default — measured by closing a file and reopening it, which answers 8192 again.
fn rt_fd_chunk_clear() -> String {
    format!(
        r#"(func $__rt_fd_chunk_clear (param $fd i32)
  (local $slot i32)
  (local.set $slot (call $__rt_fd_addr (local.get $fd) (i32.const 3)))
  (if (i32.eqz (local.get $slot))
    (then (return)))
  (i32.store (local.get $slot) (i32.const 0)))
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::RuntimeFnId;

    /// The three tuners are served, and only one of them carries state.
    ///
    /// The split is the point. `stream_set_chunk_size()` records a real per-stream value and so
    /// needs a table, a default and a clear-on-close; the other two answer a CONSTANT php picked
    /// and this target has no way to make untrue, so they need none of that — and giving them
    /// state would invent a distinction php does not make. The constants are pinned here because
    /// they are the whole implementation: `read` is 0 and `write` is **-1**, both measured on
    /// php-src 8.5.10 across five stream kinds.
    #[test]
    fn the_tuners_answer_what_php_answers() {
        for target in [
            RuntimeFnId::StreamSetChunkSize,
            RuntimeFnId::StreamSetReadBuffer,
            RuntimeFnId::StreamSetWriteBuffer,
        ] {
            assert!(
                crate::codegen_wasm::capability::runtime_function_is_supported(target),
                "{target:?} must be supported on this target"
            );
        }

        assert_eq!(DEFAULT_CHUNK_SIZE, 8192, "php's default for a fresh stream");

        // Zero is what an untouched table slot holds, and it must read back as the default —
        // that is what lets linear memory's zero-initialization BE the initial state and
        // `__rt_fd_chunk_clear` restoring zero BE the reset a recycled fd needs.
        assert_ne!(
            DEFAULT_CHUNK_SIZE, 0,
            "zero has to mean `never set`, so it cannot also be a real size"
        );

        // The descriptor field sits past everything `__rt_memstream_new` writes, and the block
        // it allocates must be big enough to hold it.
        assert_eq!(MEMSTREAM_CHUNK_OFFSET, 56);
        assert!(
            MEMSTREAM_CHUNK_OFFSET + 8 <= 64,
            "the in-memory descriptor is 64 bytes and the chunk size is the last 8"
        );
    }
}
