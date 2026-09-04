//! Purpose:
//! Lowers PHP `eval()` calls through internal EIR AOT functions or the optional
//! libelephc-magician bridge ABI. Materializes persistent eval scope state,
//! synchronizes visible locals, and reloads boxed Mixed cells after execution.
//!
//! Called from:
//! - `crate::codegen::lower_inst::builtins::lower_language_construct_call()`.
//!
//! Key details:
//! - Argument evaluation has already happened in PHP source order during EIR
//!   lowering; this module only materializes the bridge ABI call.
//! - The bridge is target-mangled like other C staticlib symbols.

use std::collections::BTreeSet;
use std::path::Path;

use crate::codegen::eval_ref_arg_helpers::eval_signature_ref_params_supported;
use crate::codegen::platform::Arch;
use crate::codegen::runtime_callable_invoker::RuntimeCallableInvoker;
use crate::codegen::{
    abi, callable_descriptor, emit_box_current_value_as_mixed, CodegenIrError, Result,
};
use crate::ir::{Function, Immediate, Instruction, LocalKind, LocalSlotId, Module, Op, ValueId};
use crate::names::{function_symbol, ir_global_symbol, php_symbol_key};
use crate::parser::ast::{BinOp, Expr, ExprKind, StaticReceiver, TypeExpr, Visibility};
use crate::types::{
    is_php_integer_array_key, AttrArgEntry, AttrArgValue, AttrKey, ClassInfo, FunctionSig,
    InterfaceInfo, PhpType, PropertyHookContract,
};

use super::super::super::context::FunctionContext;
use super::super::{
    expect_data, expect_global_name, expect_operand, function_signature_from_eir, store_if_result,
};

const EVAL_STATUS_PARSE_ERROR: i64 = 1;
const EVAL_STATUS_UNCAUGHT_THROWABLE: i64 = 3;
const EVAL_STATUS_UNSUPPORTED: i64 = 4;
/// Everything php prints before the location of an `eval()` fragment that does not parse.
///
/// php names the exact syntactic complaint and elephc cannot: the bridge answers a status code.
/// See `eval::status::eval_parse_error_message` for the four wordings measured on `php -n` 8.5.6
/// and why this one is the shape chosen.
const EVAL_PARSE_ERROR_PREFIX: &str = "Parse error: syntax error, unexpected end of file";
/// What php exits with after a `Parse error`. Measured on `php -n` 8.5.6: 255, not 1.
const EVAL_PARSE_ERROR_EXIT_STATUS: u32 = 255;
/// What elephc's own eval fatals — which have no php counterpart — exit with.
const EVAL_FATAL_EXIT_STATUS: u32 = 1;
const EVAL_UNSUPPORTED_MESSAGE: &str =
    "Fatal error: eval() fragment uses an unsupported construct\n";
const EVAL_RUNTIME_FATAL_MESSAGE: &str = "Fatal error: eval() runtime failed\n";
const EVAL_STACK_BYTES: usize = 96;
const EVAL_RESULT_VALUE_CELL_OFFSET: usize = 8;
const EVAL_RESULT_ERROR_OFFSET: usize = 16;
const EVAL_CONTEXT_HANDLE_OFFSET: usize = 24;
const EVAL_SCOPE_HANDLE_OFFSET: usize = 32;
const EVAL_TEMP_CELL_OFFSET: usize = 40;
const EVAL_CODE_PTR_OFFSET: usize = 48;
const EVAL_CODE_LEN_OFFSET: usize = 56;
const EVAL_GLOBAL_SCOPE_HANDLE_OFFSET: usize = 64;
const EVAL_CALLED_CLASS_PTR_OFFSET: usize = 72;
const EVAL_CALLED_CLASS_LEN_OFFSET: usize = 80;
const EVAL_SCOPE_FLAG_PRESENT: i64 = 1;
const EVAL_SCOPE_FLAG_OWNED: i64 = 1 << 4;
const EVAL_CLASS_LOOKUP_GET_CLASS: i64 = 0;
const EVAL_CLASS_LOOKUP_GET_PARENT_CLASS: i64 = 1;
const EVAL_MEMBER_LOOKUP_METHOD_EXISTS: i64 = 0;
const EVAL_MEMBER_LOOKUP_PROPERTY_EXISTS: i64 = 1;
const EVAL_CLASS_RELATION_IMPLEMENTS: i64 = 0;
const EVAL_CLASS_RELATION_PARENTS: i64 = 1;
const EVAL_CLASS_RELATION_USES: i64 = 2;
const EVAL_CALLABLE_ARG_ARRAY_OFFSET: usize = EVAL_CODE_PTR_OFFSET;
const CALLED_CLASS_ID_PARAM: &str = "__elephc_called_class_id";
const NATIVE_DEFAULT_NULL: i64 = 0;
const NATIVE_DEFAULT_BOOL: i64 = 1;
const NATIVE_DEFAULT_INT: i64 = 2;
const NATIVE_DEFAULT_FLOAT: i64 = 3;
const NATIVE_DEFAULT_EMPTY_ARRAY: i64 = 4;
const NATIVE_PROPERTY_REQUIRES_GET: i64 = 1;
const NATIVE_PROPERTY_REQUIRES_SET: i64 = 2;
const NATIVE_MEMBER_ATTRIBUTE_METHOD: u8 = 0;
const NATIVE_MEMBER_ATTRIBUTE_PROPERTY: u8 = 1;
const NATIVE_MEMBER_ATTRIBUTE_CLASS_CONSTANT: u8 = 2;
const NATIVE_MEMBER_ATTRIBUTE_CLASS: u8 = 3;
const NATIVE_ATTRIBUTE_ARGS_UNSUPPORTED: u8 = 0;
const NATIVE_ATTRIBUTE_ARGS_SUPPORTED: u8 = 1;
const NATIVE_ATTRIBUTE_ARG_NULL: u8 = 0;
const NATIVE_ATTRIBUTE_ARG_BOOL: u8 = 1;
const NATIVE_ATTRIBUTE_ARG_INT: u8 = 2;
const NATIVE_ATTRIBUTE_ARG_STRING: u8 = 3;
const NATIVE_ATTRIBUTE_ARG_NAMED: u8 = 4;
const NATIVE_ATTRIBUTE_ARG_FLOAT: u8 = 5;
const NATIVE_ATTRIBUTE_ARG_ARRAY: u8 = 6;
const NATIVE_OBJECT_DEFAULT_ARG_SCALAR: u8 = 0;
const NATIVE_OBJECT_DEFAULT_ARG_STRING: u8 = 1;
const NATIVE_OBJECT_DEFAULT_ARG_OBJECT: u8 = 2;
const NATIVE_OBJECT_DEFAULT_ARG_NAMED: u8 = 3;
const NATIVE_OBJECT_DEFAULT_ARG_ARRAY: u8 = 4;
const NATIVE_ARRAY_DEFAULT_KEY_AUTO: u8 = 0;
const NATIVE_ARRAY_DEFAULT_KEY_INT: u8 = 1;
const NATIVE_ARRAY_DEFAULT_KEY_STRING: u8 = 2;
const MAX_NATIVE_OBJECT_DEFAULT_ARGS: usize = u8::MAX as usize;
const MAX_NATIVE_DEFAULT_CONSTANT_DEPTH: usize = 16;

/// Local slot metadata needed for conservative eval scope synchronization.
#[derive(Clone)]
struct EvalSyncLocal {
    name: String,
    slot: LocalSlotId,
    ty: PhpType,
}

/// Program-global metadata synchronized with eval `global` aliases.
#[derive(Clone)]
struct EvalSyncGlobal {
    name: String,
    ty: PhpType,
}

/// Source location for one direct Mixed parameter passed into scope-read eval AOT.
enum EvalScopeReadParamSource {
    Local(EvalSyncLocal),
    Null,
}

/// Local-to-global alias metadata inherited by eval from the caller function scope.
#[derive(Clone)]
struct EvalGlobalAlias {
    name: String,
    global_name: String,
}

/// A module-local function that can be registered with the eval context.
struct EvalNativeFunctionRegistration {
    name: String,
    signature: FunctionSig,
    bridge_supported: bool,
}

/// A module-local method signature that can be registered with the eval context.
struct EvalNativeMethodRegistration {
    class_name: String,
    method_name: String,
    is_static: bool,
    signature: FunctionSig,
    bridge_supported: bool,
}

/// A module-local constructor signature that can be registered with the eval context.
struct EvalNativeConstructorRegistration {
    class_name: String,
    signature: FunctionSig,
    bridge_supported: bool,
}

/// Static metadata used while converting AOT defaults into eval bridge values.
struct EvalNativeDefaultContext<'a> {
    module: &'a Module,
    current_class: Option<&'a str>,
}

impl<'a> EvalNativeDefaultContext<'a> {
    /// Builds a default-materialization context for global function defaults.
    fn global(module: &'a Module) -> Self {
        Self {
            module,
            current_class: None,
        }
    }

    /// Builds a default-materialization context for class-like member defaults.
    fn for_class(module: &'a Module, class_name: &'a str) -> Self {
        Self {
            module,
            current_class: Some(class_name),
        }
    }
}

/// A module-local property type that can be registered with the eval context.
struct EvalNativePropertyTypeRegistration {
    class_name: String,
    property_name: String,
    type_spec: String,
}

/// A module-local interface property contract that can be registered with the eval context.
struct EvalNativeInterfacePropertyRegistration {
    interface_name: String,
    declaring_interface_name: String,
    property_name: String,
    type_spec: String,
    requires_get: bool,
    requires_set: bool,
}

/// A module-local abstract class property contract that can be registered with the eval context.
struct EvalNativeAbstractPropertyRegistration {
    class_name: String,
    declaring_class_name: String,
    property_name: String,
    type_spec: String,
    requires_get: bool,
    requires_set: bool,
}

/// A module-local property default that can be registered with the eval context.
struct EvalNativePropertyDefaultRegistration {
    class_name: String,
    property_name: String,
    default: EvalNativeCallableDefault,
}

/// A module-local member attribute that can be registered with the eval context.
struct EvalNativeMemberAttributeRegistration {
    owner_kind: u8,
    class_name: String,
    member_name: String,
    attribute_name: String,
    attribute_args: Option<Vec<AttrArgEntry>>,
}

/// Native callable default that can be registered with libelephc-magician.
enum EvalNativeCallableDefault {
    Scalar {
        kind: i64,
        payload: i64,
    },
    String(String),
    Array(Vec<EvalNativeCallableArrayDefaultElement>),
    Object {
        class_name: String,
        args: Vec<EvalNativeCallableObjectDefaultArg>,
    },
}

/// Array element metadata for a native callable default registered with eval.
struct EvalNativeCallableArrayDefaultElement {
    key: Option<EvalNativeCallableArrayDefaultKey>,
    default: EvalNativeCallableDefault,
}

/// Static array key metadata for a native callable default registered with eval.
enum EvalNativeCallableArrayDefaultKey {
    Int(i64),
    String(String),
}

/// Constructor argument metadata for an object-valued native callable default.
struct EvalNativeCallableObjectDefaultArg {
    name: Option<String>,
    default: EvalNativeCallableDefault,
}


mod calls;
mod scope_access;
mod dynamic_calls;
mod introspection;
mod symbol_queries;
mod argument_results;
mod context_registration;
mod registration_collection;
mod member_collection;
mod signature_metadata;
mod default_expressions;
mod default_constants;
mod function_registration;
mod method_registration;
mod constructor_registration;
mod property_attribute_registration;
mod constructor_params;
mod function_params;
mod scope_context;
mod sync_inventory;
mod scope_io;
mod scope_reload;
mod status;

#[allow(unused_imports)]
use calls::*;
#[allow(unused_imports)]
use scope_access::*;
#[allow(unused_imports)]
use dynamic_calls::*;
#[allow(unused_imports)]
use introspection::*;
#[allow(unused_imports)]
use symbol_queries::*;
#[allow(unused_imports)]
use argument_results::*;
#[allow(unused_imports)]
use context_registration::*;
#[allow(unused_imports)]
use registration_collection::*;
#[allow(unused_imports)]
use member_collection::*;
#[allow(unused_imports)]
use signature_metadata::*;
#[allow(unused_imports)]
use default_expressions::*;
#[allow(unused_imports)]
use default_constants::*;
#[allow(unused_imports)]
use function_registration::*;
#[allow(unused_imports)]
use method_registration::*;
#[allow(unused_imports)]
use constructor_registration::*;
#[allow(unused_imports)]
use property_attribute_registration::*;
#[allow(unused_imports)]
use constructor_params::*;
#[allow(unused_imports)]
use function_params::*;
#[allow(unused_imports)]
use scope_context::*;
#[allow(unused_imports)]
use sync_inventory::*;
#[allow(unused_imports)]
use scope_io::*;
#[allow(unused_imports)]
use scope_reload::*;
#[allow(unused_imports)]
use status::*;

pub(super) use calls::lower_eval;
pub(super) use dynamic_calls::{
    lower_eval_function_call, lower_eval_function_call_array, lower_eval_method_call,
    lower_eval_native_frame_static_method_call, lower_eval_native_frame_static_property_get,
    lower_eval_native_frame_static_property_set, lower_eval_object_new,
    lower_eval_object_new_dynamic_fallback, lower_eval_static_method_call,
};
pub(super) use introspection::{
    lower_eval_callable_call_array, lower_eval_class_relation, lower_eval_is_callable,
    lower_eval_member_exists, lower_eval_object_class_name, lower_eval_object_is_a,
    lower_eval_object_is_a_dynamic,
};
pub(super) use scope_access::{lower_eval_scope_get, lower_eval_scope_set};
pub(super) use symbol_queries::{
    has_eval_context, lower_eval_class_constant_fetch, lower_eval_class_exists,
    lower_eval_constant_exists, lower_eval_constant_fetch, lower_eval_function_exists,
    lower_eval_static_property_get, lower_eval_static_property_set,
};
