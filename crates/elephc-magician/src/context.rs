//! Purpose:
//! Declares the opaque process-level eval context handle and shared metadata types.
//! Registry and runtime-state method families live in focused child modules.
//!
//! Called from:
//! - `crate::abi`
//! - `crate::__elephc_eval_execute()`
//!
//! Key details:
//! - The handle is intentionally opaque to generated code.
//! - No Rust-owned layout is promised across the C ABI.

mod alias_metadata;
mod class_metadata;
mod classes_aliases;
mod classlike_objects;
mod closure_metadata;
mod core;
mod functions;
mod global_registry;
mod native_defaults;
mod native_function;
mod native_metadata;
mod native_signatures;
mod normalization;
mod reference_metadata;
mod reflection_registry;
mod runtime_state;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};

use crate::abi::ABI_VERSION;
use crate::eval_ir::{
    EvalAttribute, EvalClass, EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalEnum,
    EvalFunction, EvalInterface, EvalInterfaceMethod, EvalInterfaceProperty, EvalParameterType,
    EvalTrait, EvalTraitAdaptation, EvalVisibility,
};
use crate::scope::ElephcEvalScope;
use crate::stream_resources::EvalStreamResources;
use crate::value::{RuntimeCell, RuntimeCellHandle};

pub use alias_metadata::*;
pub use closure_metadata::*;
pub use core::*;
pub(crate) use global_registry::*;
pub use native_defaults::*;
pub use native_function::*;
use normalization::*;
pub use reference_metadata::*;

#[cfg(not(test))]
static GLOBAL_EVAL_CLASSES: OnceLock<Mutex<GlobalEvalClassRegistry>> = OnceLock::new();

thread_local! {
    static NATIVE_FRAME_CALLED_CLASS_OVERRIDES: RefCell<Vec<NativeFrameCalledClassOverride>> =
        RefCell::new(Vec::new());
}
