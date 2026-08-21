//! Purpose:
//! Provides the native PHP DOM/libxml/SimpleXML bridge behind a versioned C ABI.
//! Owns execution contexts, validated flat requests, result lifetimes, and native engine adapters.
//!
//! Called from:
//! - Elephc-generated programs that use the locked internal extension registry.
//! - `cargo test -p elephc-dom` for ABI, handle, parser, and ownership validation.
//!
//! Key details:
//! - No Rust reference, container, enum layout, or panic crosses the exported C boundary.
//! - libxml2 and Lexbor state is addressed only through generation-checked opaque handles.

mod abi;
mod context;
mod dispatch;
mod exports;
mod generated;
mod handles;
mod host;
mod native;
mod native_simplexml_handlers;
mod objects;
mod request;
mod result_tree;
mod runtime_value;

pub use abi::{
    Diagnostic, DomClassMetadataEntry, HostCall, HostVTable, RequestHeader, ResultHeader,
    Value, ABI_VERSION, DOM_CLASS_NO_PARENT, STATUS_ABI_ERROR, STATUS_FATAL,
    STATUS_INTERNAL_PANIC, STATUS_MALFORMED_REQUEST, STATUS_OK, STATUS_THROW,
};
pub use exports::{
    elephc_dom_call, elephc_dom_context_free, elephc_dom_context_new,
    elephc_dom_context_reset, elephc_dom_context_set_class_metadata,
    elephc_dom_result_release,
};
pub use runtime_value::{
    elephc_dom_measure_runtime_value, elephc_dom_write_runtime_value,
    RuntimeClassName, RuntimeValueMeasure, RuntimeValueWriteContext,
};

#[cfg(test)]
mod tests;
#[cfg(test)]
mod simplexml_foundation_tests;
#[cfg(test)]
mod route_coverage_tests;
