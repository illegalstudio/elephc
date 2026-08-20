//! Purpose:
//! Exposes shared target, runtime, metadata, and assembly support used by EIR code generation.
//!
//! Called from:
//! - `crate::codegen` and the compile pipeline.
//!
//! Key details:
//! - Keeps this module as a narrow API surface; state and class-inventory logic live in dedicated modules.

pub(crate) mod abi;
pub(crate) mod arrays;
pub(crate) mod bcmath;
pub(crate) mod callable_descriptor;
pub(crate) mod callable_dispatch;
pub(crate) mod callable_invoker_args;
pub(crate) mod cdylib;
mod compilation_context;
pub(crate) mod data_section;
mod declaration_order;
mod driver_support;
pub(crate) mod dynamic_new;
mod emitted_classes;
pub(crate) mod emit;
pub(crate) mod hash_crypto;
pub(crate) mod iconv_bridge;
pub(crate) mod interface_wrappers;
pub(crate) mod phar_stream;
/// Platform module.
pub mod platform;
mod prescan;
mod program_usage;
pub(crate) mod reflection;
pub(crate) mod runtime;
mod runtime_features;
pub(crate) mod sentinels;
pub(crate) mod stream_filters;
pub(crate) mod tls;
pub(crate) mod try_handlers;
mod value_boxing;
pub(crate) mod visibility;
mod wrappers;

pub(crate) use arrays::emit_array_value_type_stamp;
pub(crate) use compilation_context::{compile_is_web_sapi, compile_php_version, linked_extensions};
pub use compilation_context::{
    autoload_rule_count, set_autoload_rule_count, set_compile_profile, set_linked_extensions,
};
pub(crate) use declaration_order::{
    declared_class_names, declared_interface_names, declared_trait_names,
};
pub use declaration_order::prepare_declared_name_order;
pub(crate) use driver_support::{emit_write_current_string_stderr, emit_write_literal_stderr};
#[allow(unused_imports)]
pub use driver_support::{
    generate_runtime, generate_runtime_with_features, generate_runtime_with_features_pic,
};
use emitted_classes::collect_emitted_class_names;
pub(crate) use prescan::collect_constants;
pub use runtime_features::{LinkRequirement, RuntimeFeatures};
pub use runtime_features::{
    link_requirements_for_runtime_features, runtime_features_for_program_and_classes,
};
pub(crate) use sentinels::NULL_SENTINEL;
pub(crate) use value_boxing::{
    emit_box_current_owned_value_as_mixed, emit_box_current_value_as_mixed,
    emit_box_runtime_payload_as_mixed, emit_release_pushed_refcounted_temp_after_array_push,
    runtime_value_tag,
};
pub(crate) use wrappers::{
    emit_callback_wrapper, emit_extern_callback_trampoline, emit_fiber_wrapper,
    DeferredCallbackWrapper, DeferredExternCallbackTrampoline, DeferredFiberWrapper,
};
