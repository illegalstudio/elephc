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
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, PhpVersion,
    TypeSpec,
};

macro_rules! param {
    ($name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: false,
writes: None,
        }
    };
    ($name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: false,
writes: None,
        }
    };
}

macro_rules! surface {
    (
        $name:literal, $area:ident, $module:ident, $kind:ident,
        [$($param:expr),* $(,)?], $variadic:expr, $returns:ident,
        $summary:literal $(, since: $since:ident)? $(, extension: $extension:expr)?
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::$area,
            module: PhpModule::$module,
            since: surface!(@since $($since)?),
            kind: BuiltinKind::$kind,
            params: &[$($param),*],
            variadic: $variadic,
            variadic_by_ref: false,
            variadic_writes: None,
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
    (@since $version:ident) => { Some(PhpVersion::$version) };
    (@since) => { None };
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
        Array,
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
        "dir",
        Io,
        Standard,
        PreludeProvided,
        [
            param!("directory", Str),
            param!("context", Mixed = DefaultSpec::Null),
        ],
        None,
        Mixed,
        "Opens a directory and returns a Directory object, or false."
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
        "Returns internal and user-defined function names. Elephc has no disable_functions configuration, so exclude_disabled is accepted but does not change the result."
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
        "get_error_handler",
        System,
        Core,
        Function,
        [],
        None,
        Mixed,
        "Returns the currently active user error handler, or null when none is installed.",
        since: Php85
    ),
    surface!(
        "get_exception_handler",
        System,
        Core,
        Function,
        [],
        None,
        Mixed,
        "Returns the currently active uncaught-exception handler, or null when none is installed.",
        since: Php85
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
        Array,
        "Returns visible PHP method names, excluding generated property-hook accessors. AOT supports direct calls, literal call_user_func calls, first-class callables, and argument unpacking, with an object, a class-name string, or a boxed value whose runtime tag is an object or a string; any other tag throws TypeError. Runtime-selected callable targets are unsupported."
    ),
    surface!(
        "gzclose",
        Io,
        Zlib,
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Bool,
        "Closes an open gz-file pointer."
    ),
    surface!(
        "gzdecode",
        String,
        Zlib,
        PreludeProvided,
        [
            param!("data", Str),
            param!("max_length", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Decodes a gzip-framed string."
    ),
    surface!(
        "gzencode",
        String,
        Zlib,
        PreludeProvided,
        [
            param!("data", Str),
            param!("level", Int = DefaultSpec::Int(-1)),
            param!("encoding", Int = DefaultSpec::Int(31)),
        ],
        None,
        Mixed,
        "Compresses a string with the gzip framing."
    ),
    surface!(
        "gzeof",
        Io,
        Zlib,
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Bool,
        "Tests for end-of-file on a gz-file pointer."
    ),
    surface!(
        "gzfile",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("filename", Str),
            param!("use_include_path", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Reads an entire gz-file into an array of lines."
    ),
    surface!(
        "gzgetc",
        Io,
        Zlib,
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Mixed,
        "Gets one character from a gz-file pointer."
    ),
    surface!(
        "gzgets",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("stream", Mixed),
            param!("length", Int = DefaultSpec::Null),
        ],
        None,
        Mixed,
        "Gets one line from a gz-file pointer."
    ),
    surface!(
        "gzopen",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("filename", Str),
            param!("mode", Str),
            param!("use_include_path", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Opens a gz-file pointer on the zlib compression wrapper."
    ),
    surface!(
        "gzpassthru",
        Io,
        Zlib,
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Int,
        "Outputs all remaining data on a gz-file pointer."
    ),
    surface!(
        "gzputs",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("stream", Mixed),
            param!("data", Str),
            param!("length", Int = DefaultSpec::Null),
        ],
        None,
        Mixed,
        "Alias of gzwrite()."
    ),
    surface!(
        "gzread",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("stream", Mixed),
            param!("length", Int),
        ],
        None,
        Mixed,
        "Reads up to length bytes from a gz-file pointer."
    ),
    surface!(
        "gzrewind",
        Io,
        Zlib,
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Bool,
        "Rewinds the position of a gz-file pointer."
    ),
    surface!(
        "gzseek",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("stream", Mixed),
            param!("offset", Int),
            param!("whence", Int = DefaultSpec::Int(0)),
        ],
        None,
        Int,
        "Seeks on a gz-file pointer."
    ),
    surface!(
        "gztell",
        Io,
        Zlib,
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Mixed,
        "Tells the read/write position of a gz-file pointer."
    ),
    surface!(
        "gzwrite",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("stream", Mixed),
            param!("data", Str),
            param!("length", Int = DefaultSpec::Null),
        ],
        None,
        Mixed,
        "Writes a string to a gz-file pointer."
    ),
    surface!(
        "readgzfile",
        Io,
        Zlib,
        PreludeProvided,
        [
            param!("filename", Str),
            param!("use_include_path", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Outputs a gz-file and answers the byte count."
    ),
    surface!(
        "get_class_vars",
        Callables,
        Core,
        Function,
        [param!("class", Mixed)],
        None,
        Array,
        "Returns visible default properties for a class, excluding virtual properties. Uninitialized backed properties are returned as null. AOT supports direct calls, literal call_user_func calls, first-class callables, and argument unpacking, with a class-name string that may be a boxed runtime value; a non-string runtime tag throws TypeError. Runtime-selected callable targets are unsupported."
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
    surface!(
        "zlib_decode",
        String,
        Zlib,
        PreludeProvided,
        [
            param!("data", Str),
            param!("max_length", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Decompresses a raw, zlib or gzip framed string, detecting which."
    ),
    surface!(
        "zlib_get_coding_type",
        String,
        Zlib,
        PreludeProvided,
        [],
        None,
        Mixed,
        "Returns the compression the output layer applied, or false when none did."
    ),
    surface!(
        "zlib_encode",
        String,
        Zlib,
        PreludeProvided,
        [
            param!("data", Str),
            param!("encoding", Int),
            param!("level", Int = DefaultSpec::Int(-1)),
        ],
        None,
        Mixed,
        "Compresses a string with the requested zlib framing."
    ),
];
