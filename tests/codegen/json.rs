//! Purpose:
//! Provides JSON codegen test module wiring.
//! Exercises the JSON implementation through end-to-end PHP compilation and execution.
//!
//! Called from:
//! - `cargo test --test codegen_tests` through the JSON codegen test module.
//!
//! Key details:
//! - Submodules are grouped by JSON surface area so focused filters can run one feature slice.

use crate::support::*;

#[path = "json/constants.rs"]
mod constants;
#[path = "json/last_error.rs"]
mod last_error;
#[path = "json/last_error_msg.rs"]
mod last_error_msg;
#[path = "json/validate.rs"]
mod validate;
#[path = "json/extended_signatures.rs"]
mod extended_signatures;
#[path = "json/jsonserializable.rs"]
mod jsonserializable;
#[path = "json/encode_object.rs"]
mod encode_object;
#[path = "json/encode_jsonserializable.rs"]
mod encode_jsonserializable;
#[path = "json/exception.rs"]
mod exception;
#[path = "json/encode_flags.rs"]
mod encode_flags;
#[path = "json/encode_inf_nan.rs"]
mod encode_inf_nan;
#[path = "json/encode_float_precision.rs"]
mod encode_float_precision;
#[path = "json/encode_depth.rs"]
mod encode_depth;
#[path = "json/encode_invalid_utf8.rs"]
mod encode_invalid_utf8;
#[path = "json/encode_control_chars.rs"]
mod encode_control_chars;
#[path = "json/encode_list_shape.rs"]
mod encode_list_shape;
#[path = "json/decode_mixed.rs"]
mod decode_mixed;
#[path = "json/decode_stdclass.rs"]
mod decode_stdclass;
#[path = "json/mixed_index_access.rs"]
mod mixed_index_access;
#[path = "json/decode_errors.rs"]
mod decode_errors;
#[path = "json/decode_bigint.rs"]
mod decode_bigint;
#[path = "json/case_insensitive.rs"]
mod case_insensitive;
#[path = "json/evaluation_order.rs"]
mod evaluation_order;
