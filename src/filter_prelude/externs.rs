//! Purpose:
//! Declares the `elephc_filter` bridge symbols the synthetic filter prelude calls.
//!
//! Called from:
//! - `crate::filter_prelude::filter_declarations`.
//!
//! Key details:
//! - Every entry point stays inside the supported eight-integer-argument ABI limit.
//! - Strings cross the boundary pointer/length; string outcomes return as hex so embedded
//!   NUL bytes survive the C string ABI before `hex2bin()` restores them.

use crate::parser::ast::{CType, Program};
use crate::synthetic_class::extern_fn;

/// Returns the bridge's extern declaration block.
pub(super) fn declarations() -> Program {
    vec![
        extern_fn("elephc_filter_apply_int", "elephc_filter")
            .param("flags", CType::Int)
            .param("value", CType::Str)
            .param("min", CType::Int)
            .param("max", CType::Int)
            .param("range_flags", CType::Int)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_bool", "elephc_filter")
            .param("value", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_float", "elephc_filter")
            .param("flags", CType::Int)
            .param("value", CType::Str)
            .param("decimal", CType::Str)
            .param("thousand", CType::Str)
            .param("range_flags", CType::Int)
            .param("min", CType::Float)
            .param("max", CType::Float)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_email", "elephc_filter")
            .param("flags", CType::Int)
            .param("value", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_regexp", "elephc_filter")
            .param("value", CType::Str)
            .param("pattern", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_domain", "elephc_filter")
            .param("flags", CType::Int)
            .param("value", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_ip", "elephc_filter")
            .param("flags", CType::Int)
            .param("value", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_mac", "elephc_filter")
            .param("flags", CType::Int)
            .param("value", CType::Str)
            .param("separator", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_apply_sanitize", "elephc_filter")
            .param("filter", CType::Int)
            .param("flags", CType::Int)
            .param("value", CType::Str)
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_result_kind", "elephc_filter")
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_result_int", "elephc_filter")
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_result_float", "elephc_filter")
            .returns(CType::Float)
            .build(),
        extern_fn("elephc_filter_result_bool", "elephc_filter")
            .returns(CType::Int)
            .build(),
        extern_fn("elephc_filter_result_hex", "elephc_filter")
            .returns(CType::Str)
            .build(),
    ]
}
