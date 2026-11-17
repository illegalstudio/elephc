//! Purpose:
//! Selects PHP scalar tags eligible for direct native cleanup.
//!
//! Called from:
//! - The compiler non-escaping cleanup boundary.
//!
//! Key details:
//! - Memoir validates memory headers; PHP type/tag selection stays here.

use crate::codegen_support::{emit::Emitter, value_boxing::runtime_value_tag};
use crate::types::PhpType;
pub(super) use elephc_memoir::cleanup_fast_paths::scalar_mixed::{PROTECTED, DIRECT};

/// Supplies the four non-owning PHP scalar tags to the memory proof emitter.
pub(super) fn emit_fast_path(emitter: &mut Emitter) {
    let tags = [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::Void]
        .map(|ty| runtime_value_tag(&ty));
    elephc_memoir::cleanup_fast_paths::scalar_mixed::emit_fast_path(emitter, &tags);
}
