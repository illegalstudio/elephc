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
];
