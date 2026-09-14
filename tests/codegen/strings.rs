//! Purpose:
//! Groups the strings integration test submodules into the parent suite.
//!
//! Called from:
//! - `cargo test` through Rust's test harness.
//!
//! Key details:
//! - Submodules group focused fixtures for search, transform, encoding, iconv, formatting, interpolation and hashes, and related suites.

use crate::support::*;

#[path = "strings/mbstring_regex_match.rs"]
mod mbstring_regex_match;
#[path = "strings/mbstring_regex_capture.rs"]
mod mbstring_regex_capture;
#[path = "strings/mbstring_regex_capture_eval.rs"]
mod mbstring_regex_capture_eval;
#[path = "strings/mbstring_regex_capture_callbacks.rs"]
mod mbstring_regex_capture_callbacks;
#[path = "strings/mbstring_regex_search.rs"]
mod mbstring_regex_search;
#[path = "strings/mbstring_regex_split.rs"]
mod mbstring_regex_split;
#[path = "strings/mbstring_regex_replace.rs"]
mod mbstring_regex_replace;
#[path = "strings/mbstring_regex_callback.rs"]
mod mbstring_regex_callback;
#[path = "strings/mbstring_exception.rs"]
mod mbstring_exception;

#[path = "strings/search.rs"]
mod search;
#[path = "strings/transform.rs"]
mod transform;
#[path = "strings/encoding.rs"]
mod encoding;
#[path = "strings/iconv.rs"]
mod iconv;
#[path = "strings/formatting.rs"]
mod formatting;
#[path = "strings/interpolation_and_hashes.rs"]
mod interpolation_and_hashes;
#[path = "strings/misc.rs"]
mod misc;
#[path = "strings/openssl.rs"]
mod openssl;
#[path = "strings/parse_url.rs"]
mod parse_url;

#[path = "strings/mbstring.rs"]
mod mbstring;
#[path = "strings/mbstring_coercion.rs"]
mod mbstring_coercion;
#[path = "strings/mbstring_scalar.rs"]
mod mbstring_scalar;
#[path = "strings/mbstring_arrays.rs"]
mod mbstring_arrays;
#[path = "strings/mbstring_catalog.rs"]
mod mbstring_catalog;
#[path = "strings/mbstring_spreads.rs"]
mod mbstring_spreads;
#[path = "strings/mbstring_substitute.rs"]
mod mbstring_substitute;

#[path = "strings/mbstring_detect_order.rs"]
mod mbstring_detect_order;

#[path = "strings/mbstring_entities.rs"]
mod mbstring_entities;

#[path = "strings/mbstring_detect.rs"]
mod mbstring_detect;

#[path = "strings/mbstring_conversion.rs"]
mod mbstring_conversion;

#[path = "strings/mbstring_mime.rs"]
mod mbstring_mime;

#[path = "strings/mbstring_info.rs"]
mod mbstring_info;

#[path = "strings/mbstring_http_input.rs"]
mod mbstring_http_input;

#[path = "strings/mbstring_startup.rs"]
mod mbstring_startup;
#[path = "strings/mbstring_parse_str.rs"]
mod mbstring_parse_str;
#[path = "strings/mbstring_response.rs"]
mod mbstring_response;
mod mbstring_output_handler;

#[path = "strings/mbstring_regex_settings.rs"]
mod mbstring_regex_settings;
