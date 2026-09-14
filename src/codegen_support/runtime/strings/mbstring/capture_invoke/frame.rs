//! Purpose:
//! Defines the native invocation layouts shared by V4 capture and V5 query hosts.
//!
//! Called from:
//! - The common native reference invocation emitter.
//!
//! Key details:
//! - Both targets use the same local offsets; AArch64 reserves an additional linkage pair.
//! - Result slots can be reused only after the Rust result buffers have been consumed.

/// One complete callback table, wrapped context, result record, and protected return frame.
#[derive(Clone, Copy)]
pub(super) struct Frame {
    pub name: &'static str,
    pub version: i64,
    pub table_bytes: usize,
    pub context: usize,
    pub state: usize,
    pub result: usize,
    pub result_status: usize,
    pub result_length: usize,
    pub result_kind: usize,
    pub status: usize,
    pub linkage: usize,
    pub allocation: usize,
}

impl Frame {
    /// Preserves the existing V4 capture frame and its optional output-state convention.
    pub(super) fn capture() -> Self {
        Self::layout("__rt_mbstring_capture_invoke", 4, 120, 128)
    }

    /// Reserves the larger V5 query table without overlapping context or bridge results.
    pub(super) fn query() -> Self {
        Self::layout("__rt_mbstring_query_invoke", 5, 144, 144)
    }

    /// Derives aligned native result and linkage slots from the end of a reviewed host table.
    fn layout(name: &'static str, version: i64, table_bytes: usize, context: usize) -> Self {
        let result = context + 16;
        let linkage = result + 64;
        Self { name, version, table_bytes, context, state: context + 8,
            result, result_status: result + 8, result_length: result + 16,
            result_kind: result + 24, status: result + 48, linkage, allocation: linkage + 16 }
    }
}
