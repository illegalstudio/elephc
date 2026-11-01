//! Purpose:
//! Groups eval implementations for PHP-visible builtins and related helpers.
//! Submodules are organized by builtin domain while this module re-exports the
//! callable surface expected by the core interpreter.
//!
//! Called from:
//! - `crate::interpreter::eval_call()` and positional builtin dispatch paths.
//!
//! Key details:
//! - Builtin modules are children of `interpreter`, so they can reuse core EvalIR
//!   execution helpers without widening crate-level visibility.
//! - Runtime value creation and PHP coercions still flow through `RuntimeValueOps`.

#[macro_use]
mod macros;

mod array;
mod class_metadata;
pub(super) mod collection_builder;
mod core;
#[cfg(feature = "curl")]
mod curl;
mod fileinfo;
mod filesystem;
mod formatting;
mod gmp;
mod hooks;
mod json;
pub(crate) use json::eval_json_encode_serialized_object;
mod math;
mod network_env;
mod pcntl;
mod raw_memory;
mod random;
mod ref_targets;
mod regex;
mod registry;
mod scalars;
mod spec;
pub(in crate::interpreter) mod spl;
mod string;
mod symbols;

pub(crate) use symbols::eval_registered_spl_autoload_functions_result;
mod time;
mod types;
mod xml;
mod tokenizer;

pub(super) use array::*;
pub(super) use class_metadata::*;
pub(super) use core::*;
#[cfg(feature = "curl")]
pub(super) use curl::*;
pub(super) use fileinfo::*;
pub(super) use filesystem::*;
pub(super) use formatting::*;
pub(super) use gmp::*;
pub(super) use json::*;
pub(super) use math::*;
pub(super) use network_env::*;
pub(super) use pcntl::*;
pub(super) use raw_memory::*;
pub(super) use random::*;
pub(super) use ref_targets::*;
pub(super) use regex::*;
pub(super) use registry::*;
pub(super) use scalars::*;
pub(super) use string::*;
pub(crate) use string::highlight::eval_highlight_render;
pub(super) use symbols::*;
#[cfg(not(test))]
pub(crate) use core::eval_unserialize_declared_object_from_hash;
#[cfg(not(test))]
pub(crate) use core::eval_serialize_object_fragment;
#[cfg(not(test))]
pub(crate) use core::eval_throwable_trace_value;
pub(in crate::interpreter) use core::eval_throwable_string;
pub(crate) use symbols::eval_class_name_is_a_bridge;
pub(crate) use symbols::eval_spl_autoload_class;
pub(crate) use symbols::eval_spl_autoload_classlike_definition;
pub(super) use time::*;
pub(super) use xml::*;
pub(super) use tokenizer::*;
pub(super) use types::*;
