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
mod call_frames;
mod callable_descriptor;
mod class_metadata;
mod classes_aliases;
mod classlike_objects;
mod closure_metadata;
mod core;
mod functions;
mod generators;
mod generics;
mod global_registry;
mod local_types;
mod native_catalog;
mod native_defaults;
mod native_function;
mod native_metadata;
mod native_signatures;
mod normalization;
pub(crate) mod pcntl_runtime;
mod reference_metadata;
mod reflection_registry;
mod runtime_state;
mod request_includes;
mod request_statics;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use std::sync::Arc;
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};

use crate::abi::ABI_VERSION;
use crate::eval_ir::{
    EvalAttribute, EvalClass, EvalClassConstant, EvalClassMethod, EvalClassProperty, EvalEnum,
    EvalFunction, EvalInterface, EvalInterfaceMethod, EvalInterfaceProperty, EvalParameterType,
    EvalParameterTypeVariant, EvalTrait, EvalTraitAdaptation, EvalVisibility,
};
use crate::scope::ElephcEvalScope;
use crate::stream_resources::EvalStreamResources;
use crate::value::{RuntimeCell, RuntimeCellHandle};

pub use alias_metadata::*;
pub use call_frames::*;
pub(crate) use callable_descriptor::*;
pub use class_metadata::EvalClassMethodRef;
pub use classlike_objects::{
    eval_array_iterator_class_is_a, eval_builtin_class_is_a, EvalArrayIteratorState,
};
pub use closure_metadata::*;
pub use reflection_registry::EvalReflectionKind;
pub use generators::*;
pub use core::*;
pub use functions::{EvalNativeGlobalConstant, EvalNativeUserConstant};
pub(crate) use global_registry::*;
use native_catalog::SharedNative;
pub use native_defaults::*;
pub use native_metadata::NativeClassConstantDefault;
pub use native_function::*;
use normalization::*;
use local_types::EvalLocalReferenceTypes;
pub use reference_metadata::*;
pub(crate) use request_includes::{
    install_native_include_hooks, request_include_marks, NativeIncludeHooks,
};
pub(crate) use request_statics::{reset_request_statics, with_request_strtok};

#[cfg(not(test))]
static GLOBAL_EVAL_CLASSES: OnceLock<Mutex<GlobalEvalClassRegistry>> = OnceLock::new();
#[cfg(not(test))]
static GLOBAL_EVAL_FUNCTIONS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
#[cfg(not(test))]
static GLOBAL_EVAL_CONSTANTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
#[cfg(not(test))]
static GLOBAL_EVAL_ORPHAN_CONSTANTS: OnceLock<Mutex<HashMap<String, usize>>> = OnceLock::new();
#[cfg(not(test))]
static GLOBAL_EVAL_AUTOLOAD_CONTEXTS: OnceLock<Mutex<Vec<usize>>> = OnceLock::new();
#[cfg(not(test))]
static GLOBAL_EVAL_AOT_METADATA: OnceLock<Mutex<Option<Arc<GlobalEvalAotMetadata>>>> =
    OnceLock::new();

thread_local! {
    static NATIVE_FRAME_CALLED_CLASS_OVERRIDES: RefCell<Vec<NativeFrameCalledClassOverride>> =
        RefCell::new(Vec::new());
    /// Lexical class of the COMPILED frame currently calling into the bridge.
    ///
    /// A compiled body that owns no eval context hands the bridge a null caller, and the
    /// interpreter then has nothing to read a calling scope from -- so a protected method
    /// reached from such a body was refused "from global scope". This carries the one fact
    /// that was missing, without an eval context and without widening the call ABI.
    static NATIVE_CALLER_CLASSES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}
