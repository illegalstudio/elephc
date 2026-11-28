//! Purpose:
//! Supplies native application request boundaries with ambient stream-owner storage.
//!
//! Called from:
//! - EIR web application emission.
//!
//! Key details:
//! - Memoir emits storage transitions for application requests.

pub(super) use elephc_memoir::buffers::stream_context_owners::emit;
