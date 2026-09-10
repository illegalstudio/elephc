//! Purpose:
//! Implements MIME header operations using the shared mbstring encoding catalog.
//!
//! Called from:
//! - Shared mbstring ABI adapters and PHP compatibility fixtures.
//!
//! Key details:
//! - MIME conversion uses fixed question-mark replacement independently of request settings.

mod decode;
mod encode;

pub use decode::decode_header;
pub use encode::encode_header;
