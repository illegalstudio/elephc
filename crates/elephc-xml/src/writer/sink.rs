//! Purpose:
//! The in-memory sink behind the writer's `xmlOutputBufferCreateIO` buffer: the growable
//! byte buffer PHP's `smart_str` plays in `php_xmlwriter.c`, with the write and close
//! callbacks libxml2 invokes on it.
//!
//! Called from:
//! - libxml2's output buffer (`xmlOutputBufferWrite`/`Flush`/`Close`) through the
//!   callbacks registered by `crate::writer::Writer::new`.
//!
//! Key details:
//! - The sink is `Box::into_raw`-allocated by `Writer::new` and freed by `close`, which
//!   libxml2 calls exactly once when the output buffer is closed (from
//!   `xmlFreeTextWriter`, or directly when the writer could not be created).
//! - `write` returns the byte count it consumed, which libxml2 uses to shrink its own
//!   buffer; it never fails, so the only error source is libxml2's encoder.

use std::ffi::{c_char, c_int, c_void};

/// The bytes libxml2 has delivered and PHP has not taken yet.
#[derive(Default)]
pub(super) struct Sink {
    /// Delivered output, in order.
    pub(super) buffer: Vec<u8>,
}

/// `xml_writer_stream_write_memory`: appends `len` bytes and reports them consumed.
///
/// # Safety
/// `context` must be the `Sink` registered with the output buffer and `buffer` must point
/// at `len` readable bytes (libxml2 guarantees both).
pub(super) unsafe extern "C" fn write(context: *mut c_void, buffer: *const c_char, len: c_int) -> c_int {
    if context.is_null() || len < 0 {
        return -1;
    }
    if len > 0 && !buffer.is_null() {
        let sink = &mut *(context as *mut Sink);
        sink.buffer
            .extend_from_slice(std::slice::from_raw_parts(buffer as *const u8, len as usize));
    }
    len
}

/// `xml_writer_stream_close_memory`: frees the sink.
///
/// # Safety
/// `context` must be the `Sink` registered with the output buffer, closed exactly once.
pub(super) unsafe extern "C" fn close(context: *mut c_void) -> c_int {
    if !context.is_null() {
        drop(Box::from_raw(context as *mut Sink));
    }
    0
}
