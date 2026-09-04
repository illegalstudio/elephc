//! Purpose:
//! Canonical contracts for PHP surfaces implemented outside the AOT `builtin!`
//! registry, including language constructs, dedicated syntax, preludes, and
//! currently eval-only reflection functions.
//!
//! Called from:
//! - `crate::registry` when assembling the complete shared contract catalog.
//!
//! Key details:
//! - These entries are ordinary shared contracts even though their AOT route is
//!   not a registry binding.
//! - Backend support is joined separately and must not be inferred from this file.

use crate::{Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, TypeSpec};

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
        $name:literal, $area:ident, $kind:ident,
        [$($param:expr),* $(,)?], $variadic:expr, $returns:ident,
        $summary:literal $(, extension: $extension:expr)?
    ) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::$area,
            kind: BuiltinKind::$kind,
            params: &[$($param),*],
            variadic: $variadic,
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
}

pub(crate) static SURFACE_CONTRACTS: &[BuiltinContract] = &[
    surface!(
        "buffer_new",
        Pointers,
        DedicatedSyntax,
        [param!("length", Int)],
        None,
        Mixed,
        "Allocates a raw byte buffer.",
        extension: true
    ),
    surface!(
        "die",
        System,
        LanguageConstruct,
        [param!("status", Int = DefaultSpec::Int(0))],
        None,
        Void,
        "Terminates execution with an optional status."
    ),
    surface!(
        "dir",
        Io,
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
        LanguageConstruct,
        [param!("value", Mixed)],
        None,
        Bool,
        "Determines whether a variable is considered empty."
    ),
    surface!(
        "exit",
        System,
        LanguageConstruct,
        [param!("status", Int = DefaultSpec::Int(0))],
        None,
        Void,
        "Terminates execution with an optional status."
    ),
    surface!(
        "get_called_class",
        Callables,
        Function,
        [],
        None,
        Mixed,
        "Returns the late-static-binding class name in eval context."
    ),
    surface!(
        "get_class_methods",
        Callables,
        Function,
        [param!("object_or_class", Mixed)],
        None,
        Mixed,
        "Returns method names visible on an object or class."
    ),
    surface!(
        "gzclose",
        Io,
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Bool,
        "Closes an open gz-file pointer."
    ),
    surface!(
        "gzdecode",
        String,
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
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Bool,
        "Tests for end-of-file on a gz-file pointer."
    ),
    surface!(
        "gzfile",
        Io,
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
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Mixed,
        "Gets one character from a gz-file pointer."
    ),
    surface!(
        "gzgets",
        Io,
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
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Int,
        "Outputs all remaining data on a gz-file pointer."
    ),
    surface!(
        "gzputs",
        Io,
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
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Bool,
        "Rewinds the position of a gz-file pointer."
    ),
    surface!(
        "gzseek",
        Io,
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
        PreludeProvided,
        [param!("stream", Mixed)],
        None,
        Mixed,
        "Tells the read/write position of a gz-file pointer."
    ),
    surface!(
        "gzwrite",
        Io,
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
        Function,
        [param!("class", Mixed)],
        None,
        Mixed,
        "Returns visible default properties for a class."
    ),
    surface!(
        "hash_copy",
        String,
        PreludeProvided,
        [param!("context", Mixed)],
        None,
        Mixed,
        "Clones an incremental hashing context."
    ),
    surface!(
        "hash_final",
        String,
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
        PreludeProvided,
        [param!("context", Mixed), param!("data", Str)],
        None,
        Mixed,
        "Feeds data into an incremental hashing context."
    ),
    surface!(
        "isset",
        Types,
        LanguageConstruct,
        [param!("var", Mixed)],
        Some("vars"),
        Bool,
        "Determines whether variables are set and are not null."
    ),
    surface!(
        "unset",
        Types,
        LanguageConstruct,
        [param!("var", Mixed)],
        Some("vars"),
        Void,
        "Unsets the given variables."
    ),
    surface!(
        "zlib_decode",
        String,
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
        PreludeProvided,
        [],
        None,
        Mixed,
        "Returns the compression the output layer applied, or false when none did."
    ),
    surface!(
        "zlib_encode",
        String,
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
