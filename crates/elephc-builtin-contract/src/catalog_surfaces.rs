//! Purpose:
//! Canonical contracts for PHP surfaces implemented outside the AOT `builtin!`
//! registry, including language constructs, dedicated syntax, preludes, and
//! compiler transforms, and reflection functions.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - These entries are ordinary shared contracts even though their AOT route is
//!   not a registry binding.
//! - Backend support is joined separately and must not be inferred from this file.

use crate::{
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec,
};

macro_rules! param {
    ($name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: false,
        }
    };
    ($name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: false,
        }
    };
}

macro_rules! surface {
    (
        $name:literal, $area:ident, $module:ident, $kind:ident,
        [$($param:expr),* $(,)?], $variadic:expr, $returns:ident,
        $summary:literal $(, extension: $extension:expr)?
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::$area,
            module: PhpModule::$module,
            since: None,
            kind: BuiltinKind::$kind,
            params: &[$($param),*],
            variadic: $variadic,
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: TypeSpec::$returns,
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: None,
            deprecation: None,
            extension: surface!(@bool $($extension)?),
            internal: false,
            requirements: &[],
        }
    };
    (@bool $value:expr) => { $value };
    (@bool) => { false };
}

pub(crate) static SURFACE_CONTRACTS: &[BuiltinContract] = &[
    surface!(
        "buffer_new",
        Pointers,
        Elephc,
        DedicatedSyntax,
        [param!("length", Int)],
        None,
        Mixed,
        "Allocates a raw byte buffer.",
        extension: true
    ),
    surface!(
        "debug_backtrace",
        Callables,
        Core,
        Function,
        [
            param!("options", Int = DefaultSpec::Int(1)),
            param!("limit", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Generates a PHP backtrace for the active call stack."
    ),
    surface!(
        "debug_print_backtrace",
        Callables,
        Core,
        Function,
        [
            param!("options", Int = DefaultSpec::Int(0)),
            param!("limit", Int = DefaultSpec::Int(0)),
        ],
        None,
        Void,
        "Prints a PHP backtrace for the active call stack."
    ),
    surface!(
        "die",
        System,
        Core,
        LanguageConstruct,
        [param!("status", Int = DefaultSpec::Int(0))],
        None,
        Void,
        "Terminates execution with an optional status."
    ),
    surface!(
        "empty",
        Types,
        Core,
        LanguageConstruct,
        [param!("value", Mixed)],
        None,
        Bool,
        "Determines whether a variable is considered empty."
    ),
    surface!(
        "error_reporting",
        System,
        Core,
        Function,
        [param!("error_level", Mixed = DefaultSpec::Null)],
        None,
        Int,
        "Gets or sets the active error reporting mask."
    ),
    surface!(
        "exit",
        System,
        Core,
        LanguageConstruct,
        [param!("status", Int = DefaultSpec::Int(0))],
        None,
        Void,
        "Terminates execution with an optional status."
    ),
    surface!(
        "func_get_arg",
        Callables,
        Core,
        Function,
        [param!("position", Int)],
        None,
        Mixed,
        "Returns one argument from the current function call."
    ),
    surface!(
        "func_get_args",
        Callables,
        Core,
        Function,
        [],
        None,
        Mixed,
        "Returns the arguments passed to the current function call."
    ),
    surface!(
        "func_num_args",
        Callables,
        Core,
        Function,
        [],
        None,
        Int,
        "Returns the number of arguments passed to the current function call."
    ),
    surface!(
        "get_called_class",
        Callables,
        Core,
        Function,
        [],
        None,
        Str,
        "Returns the late-static-binding class name."
    ),
    surface!(
        "get_defined_constants",
        Callables,
        Core,
        Function,
        [param!("categorize", Bool = DefaultSpec::Bool(false))],
        None,
        Mixed,
        "Returns constants visible to the current program."
    ),
    surface!(
        "get_defined_functions",
        Callables,
        Core,
        Function,
        [param!("exclude_disabled", Bool = DefaultSpec::Bool(true))],
        None,
        Mixed,
        "Returns internal and user-defined function names."
    ),
    surface!(
        "get_defined_vars",
        Callables,
        Core,
        Function,
        [],
        None,
        Mixed,
        "Returns variables visible in the current scope."
    ),
    surface!(
        "get_extension_funcs",
        Callables,
        Core,
        Function,
        [param!("extension", Str)],
        None,
        Mixed,
        "Returns functions exported by a loaded extension or false."
    ),
    surface!(
        "get_included_files",
        Callables,
        Core,
        Function,
        [],
        None,
        Mixed,
        "Returns the files included by the current program."
    ),
    surface!(
        "get_mangled_object_vars",
        Callables,
        Core,
        Function,
        [param!("object", Mixed)],
        None,
        Mixed,
        "Returns an object's properties using PHP's visibility-mangled keys."
    ),
    surface!(
        "get_required_files",
        Callables,
        Core,
        Function,
        [],
        None,
        Mixed,
        "Returns the files included or required by the current program."
    ),
    surface!(
        "get_resources",
        Callables,
        Core,
        Function,
        [param!("type", Mixed = DefaultSpec::Null)],
        None,
        Mixed,
        "Returns currently active resources, optionally filtered by type."
    ),
    surface!(
        "get_class_methods",
        Callables,
        Core,
        Function,
        [param!("object_or_class", Mixed)],
        None,
        Mixed,
        "Returns method names visible on an object or class."
    ),
    surface!(
        "get_class_vars",
        Callables,
        Core,
        Function,
        [param!("class", Mixed)],
        None,
        Mixed,
        "Returns visible default properties for a class."
    ),
    surface!(
        "hash_copy",
        String,
        Hash,
        PreludeProvided,
        [param!("context", Mixed)],
        None,
        Mixed,
        "Clones an incremental hashing context."
    ),
    surface!(
        "hash_final",
        String,
        Hash,
        PreludeProvided,
        [
            param!("context", Mixed),
            param!("binary", Bool = DefaultSpec::Bool(false)),
        ],
        None,
        Mixed,
        "Finalizes an incremental hashing context."
    ),
    surface!(
        "hash_init",
        String,
        Hash,
        PreludeProvided,
        [
            param!("algo", Str),
            param!("flags", Int = DefaultSpec::Int(0)),
            param!("key", Str = DefaultSpec::Str("")),
        ],
        None,
        Mixed,
        "Opens an incremental hashing context."
    ),
    surface!(
        "hash_update",
        String,
        Hash,
        PreludeProvided,
        [param!("context", Mixed), param!("data", Str)],
        None,
        Mixed,
        "Feeds data into an incremental hashing context."
    ),
    surface!(
        "isset",
        Types,
        Core,
        LanguageConstruct,
        [param!("var", Mixed)],
        Some("vars"),
        Bool,
        "Determines whether variables are set and are not null."
    ),
    surface!(
        "restore_error_handler",
        System,
        Core,
        Function,
        [],
        None,
        Bool,
        "Restores the previously active user error handler."
    ),
    surface!(
        "restore_exception_handler",
        System,
        Core,
        Function,
        [],
        None,
        Bool,
        "Restores the previously active uncaught-exception handler."
    ),
    surface!(
        "set_error_handler",
        System,
        Core,
        Function,
        [
            param!("callback", Mixed),
            param!("error_levels", Int = DefaultSpec::ErrorAll),
        ],
        None,
        Mixed,
        "Installs a user error handler and returns the previous handler."
    ),
    surface!(
        "set_exception_handler",
        System,
        Core,
        Function,
        [param!("callback", Mixed)],
        None,
        Mixed,
        "Installs an uncaught-exception handler and returns the previous handler."
    ),
    surface!(
        "unset",
        Types,
        Core,
        LanguageConstruct,
        [param!("var", Mixed)],
        Some("vars"),
        Void,
        "Unsets the given variables."
    ),
    surface!(
        "user_error",
        System,
        Core,
        Function,
        [
            param!("message", Str),
            param!("error_level", Int = DefaultSpec::Int(1_024)),
        ],
        None,
        Bool,
        "Alias of trigger_error."
    ),
];
