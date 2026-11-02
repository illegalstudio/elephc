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
    TypeSpec, VariadicSpec,
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
    (ref $name:literal, $ty:ident) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: None,
            by_ref: true,
        }
    };
    (ref $name:literal, $ty:ident = $default:expr) => {
        ParamSpec {
            name: $name,
            ty: TypeSpec::$ty,
            default: Some($default),
            by_ref: true,
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
    BuiltinContract {
        id: BuiltinId::from_canonical_name("finfo_open"), name: "finfo_open", area: Area::Io,
        module: PhpModule::Fileinfo, since: None, kind: BuiltinKind::PreludeProvided,
        params: &[
            ParamSpec { name: "flags", ty: TypeSpec::Int, default: Some(DefaultSpec::Int(0)), by_ref: false },
            ParamSpec { name: "magic_database", ty: TypeSpec::Nullable(&TypeSpec::Str), default: Some(DefaultSpec::Null), by_ref: false }
        ],
        variadic: None, min_args: None, max_args: None,
        arity_error: None, returns: TypeSpec::Mixed, by_ref_return: false,
        summary: "Creates a finfo instance using the embedded or a custom magic database.", examples: &[], php_manual: None, deprecation: None,
        extension: false, internal: false, requirements: &[],
    },
    BuiltinContract {
        id: BuiltinId::from_canonical_name("finfo_close"), name: "finfo_close", area: Area::Io,
        module: PhpModule::Fileinfo, since: None, kind: BuiltinKind::PreludeProvided,
        params: &[
            ParamSpec { name: "finfo", ty: TypeSpec::Mixed, default: None, by_ref: false }
        ],
        variadic: None, min_args: None, max_args: None,
        arity_error: None, returns: TypeSpec::Bool, by_ref_return: false,
        summary: "Returns true; PHP 8 finfo instances are freed automatically. Deprecated since PHP 8.5.", examples: &[], php_manual: None,
        deprecation: Some(crate::BuiltinDeprecation::since(crate::PhpVersion::Php85, "as finfo objects are freed automatically")),
        extension: false, internal: false, requirements: &[],
    },
    BuiltinContract {
        id: BuiltinId::from_canonical_name("finfo_set_flags"), name: "finfo_set_flags", area: Area::Io,
        module: PhpModule::Fileinfo, since: None, kind: BuiltinKind::PreludeProvided,
        params: &[
            ParamSpec { name: "finfo", ty: TypeSpec::Mixed, default: None, by_ref: false },
            ParamSpec { name: "flags", ty: TypeSpec::Int, default: None, by_ref: false }
        ],
        variadic: None, min_args: None, max_args: None,
        arity_error: None, returns: TypeSpec::Bool, by_ref_return: false,
        summary: "Changes the persistent fileinfo configuration flags.", examples: &[], php_manual: None, deprecation: None,
        extension: false, internal: false, requirements: &[],
    },
    BuiltinContract {
        id: BuiltinId::from_canonical_name("finfo_buffer"), name: "finfo_buffer", area: Area::Io,
        module: PhpModule::Fileinfo, since: None, kind: BuiltinKind::PreludeProvided,
        params: &[
            ParamSpec { name: "finfo", ty: TypeSpec::Mixed, default: None, by_ref: false },
            ParamSpec { name: "string", ty: TypeSpec::Str, default: None, by_ref: false },
            ParamSpec { name: "flags", ty: TypeSpec::Int, default: Some(DefaultSpec::Int(0)), by_ref: false },
            ParamSpec { name: "context", ty: TypeSpec::Mixed, default: Some(DefaultSpec::Null), by_ref: false }
        ],
        variadic: None, min_args: None, max_args: None,
        arity_error: None, returns: TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), by_ref_return: false,
        summary: "Identifies the type and encoding of binary string contents.", examples: &[], php_manual: None, deprecation: None,
        extension: false, internal: false, requirements: &[],
    },
    BuiltinContract {
        id: BuiltinId::from_canonical_name("finfo_file"), name: "finfo_file", area: Area::Io,
        module: PhpModule::Fileinfo, since: None, kind: BuiltinKind::PreludeProvided,
        params: &[
            ParamSpec { name: "finfo", ty: TypeSpec::Mixed, default: None, by_ref: false },
            ParamSpec { name: "filename", ty: TypeSpec::Str, default: None, by_ref: false },
            ParamSpec { name: "flags", ty: TypeSpec::Int, default: Some(DefaultSpec::Int(0)), by_ref: false },
            ParamSpec { name: "context", ty: TypeSpec::Mixed, default: Some(DefaultSpec::Null), by_ref: false }
        ],
        variadic: None, min_args: None, max_args: None,
        arity_error: None, returns: TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), by_ref_return: false,
        summary: "Identifies the type and encoding of a file or stream URL.", examples: &[], php_manual: None, deprecation: None,
        extension: false, internal: false, requirements: &[],
    },
    BuiltinContract {
        id: BuiltinId::from_canonical_name("mime_content_type"), name: "mime_content_type", area: Area::Io,
        module: PhpModule::Fileinfo, since: None, kind: BuiltinKind::PreludeProvided,
        params: &[
            ParamSpec { name: "filename", ty: TypeSpec::Mixed, default: None, by_ref: false }
        ],
        variadic: None, min_args: None, max_args: None,
        arity_error: None, returns: TypeSpec::Union(&[TypeSpec::Str, TypeSpec::False]), by_ref_return: false,
        summary: "Detects a file or stream resource MIME content type.", examples: &[], php_manual: None, deprecation: None,
        extension: false, internal: false, requirements: &[],
    },
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
    // `PreludeProvided` like `parse_str` below: the AOT side builds it per program from the
    // compiled class declarations (`src/class_vars_prelude.rs`), and the lowering passes the
    // calling scope. The INTERPRETER keeps its own implementation for interpreted code.
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
    // `ext/sodium` sealed boxes, declared by `sodium_prelude` over `__elephc_sodium_box`.
    surface!(
        "sodium_crypto_box_keypair",
        String,
        Sodium,
        PreludeProvided,
        [],
        None,
        Str,
        "Generates a random X25519 keypair (secret key followed by public key)."
    ),
    surface!(
        "sodium_crypto_box_publickey",
        String,
        Sodium,
        PreludeProvided,
        [param!("key_pair", Str)],
        None,
        Str,
        "Extracts the public key from a crypto_box keypair."
    ),
    surface!(
        "sodium_crypto_box_seal",
        String,
        Sodium,
        PreludeProvided,
        [param!("message", Str), param!("public_key", Str)],
        None,
        Str,
        "Encrypts a message anonymously to a public key (sealed box)."
    ),
    surface!(
        "sodium_crypto_box_seal_open",
        String,
        Sodium,
        PreludeProvided,
        [param!("ciphertext", Str), param!("key_pair", Str)],
        None,
        Mixed,
        "Opens a sealed box with a keypair, returning false when it does not authenticate."
    ),
    // The `--web` request prelude's shutdown queue: a PHP declaration over the request-local
    // `__elephc_shutdown_function_state` registry, so it has no `builtin!` binding.
    surface!(
        "register_shutdown_function",
        Web,
        Standard,
        PreludeProvided,
        [param!("callback", Callable)],
        Some(VariadicSpec::value("args")),
        Void,
        "Registers a callback to run when the script terminates."
    ),
    surface!(
        "isset",
        Types,
        Core,
        LanguageConstruct,
        [param!("var", Mixed)],
        Some(VariadicSpec::value("vars")),
        Bool,
        "Determines whether variables are set and are not null."
    ),
    // `PreludeProvided`, not a `catalog_data.rs`/`catalog_data_additional.rs` `Function`
    // contract: the AOT side serves ONLY the plain two-argument call shape, as a conditionally
    // injected PHP-source helper (`__elephc_levenshtein_two_arg` /
    // `src/backend_gap_prelude.rs`), not a `builtin!` registry binding -- the same asymmetry
    // `hash_init` above models. `min_args`/`max_args` stay `None` (php's own arity errors are
    // "at least 2" / "at most 5", not a fixed count), matching the contract's five declared
    // parameters.
    surface!(
        "levenshtein",
        String,
        Standard,
        PreludeProvided,
        [
            param!("string1", Str),
            param!("string2", Str),
            param!("insertion_cost", Int = DefaultSpec::Int(1)),
            param!("replacement_cost", Int = DefaultSpec::Int(1)),
            param!("deletion_cost", Int = DefaultSpec::Int(1)),
        ],
        None,
        Int,
        "Computes the Levenshtein edit distance between two strings."
    ),
    // `PreludeProvided` for the same asymmetry as `levenshtein` above: the AOT side declares
    // `addcslashes` as a conditionally injected AST prelude (`src/addcslashes_prelude.rs`), and
    // the INTERPRETER has its own implementation (`interpreter::builtins::string::addcslashes`),
    // because Twig's `Compiler` runs interpreted and calls it for every string literal it emits.
    surface!(
        "addcslashes",
        String,
        Standard,
        PreludeProvided,
        [param!("string", Str), param!("characters", Str)],
        None,
        Str,
        "Quotes the listed characters with C-style backslash escapes."
    ),
    // `PreludeProvided` for the same asymmetry as `addcslashes` above: the AOT side declares it as
    // a conditionally injected AST prelude (`src/version_compare_prelude.rs`) and the INTERPRETER
    // has its own implementation (`interpreter::builtins::string::version_compare`), because
    // Symfony's password hasher factory calls it from interpreted code on every login. Returns
    // `int` without `$operator`, `bool` with one.
    surface!(
        "version_compare",
        String,
        Standard,
        PreludeProvided,
        [
            param!("version1", Str),
            param!("version2", Str),
            param!("operator", Str = DefaultSpec::Null),
        ],
        None,
        Mixed,
        "Compares two PHP-standardized version number strings."
    ),
    // `PreludeProvided` for the same asymmetry: the AOT side declares it as an AST prelude with
    // `static` locals (`src/strtok_prelude.rs`) and the INTERPRETER keeps the tokenizer state in
    // the request-scoped store (`interpreter::builtins::string::strtok`), because Laravel's
    // `SQLiteProcessor::processColumns` calls it from interpreted code. The two sides keep
    // separate state. Returns `string|false`; the one-argument form continues the stored string.
    surface!(
        "strtok",
        String,
        Standard,
        PreludeProvided,
        [param!("string", Str), param!("token", Str = DefaultSpec::Null)],
        None,
        Mixed,
        "Tokenizes a string by a set of delimiter bytes."
    ),
    // `PreludeProvided` for the same asymmetry again: the AOT side declares it as an AST prelude
    // over the backend's raw `hash_hmac()` (`src/hash_pbkdf2_prelude.rs`) and the INTERPRETER
    // derives the key through `elephc_crypto::pbkdf2`
    // (`interpreter::builtins::string::hash_pbkdf2`), because Symfony's `Pbkdf2PasswordHasher`
    // is interpreted and derives a key in its constructor on every login.
    surface!(
        "hash_pbkdf2",
        String,
        Hash,
        PreludeProvided,
        [
            param!("algo", Str),
            param!("password", Str),
            param!("salt", Str),
            param!("iterations", Int),
            param!("length", Int = DefaultSpec::Int(0)),
            param!("binary", Bool = DefaultSpec::Bool(false)),
            param!("options", Array = DefaultSpec::EmptyArray),
        ],
        None,
        Str,
        "Generates a PBKDF2 key derivation of a supplied password."
    ),
    // `PreludeProvided` for the same asymmetry as `levenshtein` above: the AOT side BUILDS both as
    // prelude declarations over the native one-pattern calls (`src/preg_array_prelude.rs`), and
    // the INTERPRETER has its own implementations (`interpreter::builtins::regex::preg_filter`,
    // `::preg_replace_callback_array`).
    surface!(
        "preg_filter",
        System,
        Pcre,
        PreludeProvided,
        [
            param!("pattern", Mixed),
            param!("replacement", Mixed),
            param!("subject", Mixed),
            param!("limit", Int = DefaultSpec::Int(-1)),
            param!(ref "count", Mixed = DefaultSpec::Null),
        ],
        None,
        Mixed,
        "Performs a regular expression search and replace, keeping only the subjects that matched."
    ),
    surface!(
        "preg_replace_callback_array",
        Callables,
        Pcre,
        PreludeProvided,
        [
            // php: `array $pattern`, which is what `preg_array_prelude` declares.
            param!("pattern", Array),
            param!("subject", Mixed),
            param!("limit", Int = DefaultSpec::Int(-1)),
            param!(ref "count", Mixed = DefaultSpec::Null),
            param!("flags", Int = DefaultSpec::Int(0)),
        ],
        None,
        Mixed,
        "Performs a regular expression search and replace using one callback per pattern."
    ),
    // `PreludeProvided` for the same asymmetry as `levenshtein` above: the AOT side declares
    // `parse_str` as a conditionally injected prelude (`src/parse_str_prelude.rs`) rather than a
    // `builtin!` registry binding, because its mandatory by-reference `$result` is an ordinary
    // by-reference PARAMETER there and needs no runtime symbol. The INTERPRETER has its own
    // implementation (`interpreter::builtins::string::parse_str`), reached from `eval_call`'s
    // ladder, because interpreted code never runs through a compiler prelude.
    surface!(
        "parse_str",
        String,
        Standard,
        PreludeProvided,
        [param!("string", Str), param!(ref "result", Mixed)],
        None,
        Void,
        "Parses a query string into an array of variables."
    ),
    // The `proc_*` family is `PreludeProvided` like `parse_str` above: the AOT side declares it as
    // one conditionally injected AST prelude (`src/proc_prelude.rs`) over the `elephc_proc`
    // bridge, because the by-reference `$pipes` array and the stream resources it holds are
    // ordinary PHP values there and need no runtime symbol of their own. The INTERPRETER has its
    // own homes (`interpreter::builtins::filesystem::proc_open`, ...) over the same bridge crate,
    // since interpreted code never runs through a compiler prelude.
    surface!(
        "proc_open",
        System,
        Standard,
        PreludeProvided,
        [
            ParamSpec {
                name: "command",
                ty: TypeSpec::Union(&[TypeSpec::Array, TypeSpec::Str]),
                default: None,
                by_ref: false,
            },
            param!("descriptor_spec", Array),
            param!(ref "pipes", Mixed),
            ParamSpec {
                name: "cwd",
                ty: TypeSpec::Nullable(&TypeSpec::Str),
                default: Some(DefaultSpec::Null),
                by_ref: false,
            },
            ParamSpec {
                name: "env_vars",
                ty: TypeSpec::Nullable(&TypeSpec::Array),
                default: Some(DefaultSpec::Null),
                by_ref: false,
            },
            ParamSpec {
                name: "options",
                ty: TypeSpec::Nullable(&TypeSpec::Array),
                default: Some(DefaultSpec::Null),
                by_ref: false,
            },
        ],
        None,
        Mixed,
        "Executes a command and opens file pointers for input/output."
    ),
    surface!(
        "proc_close",
        System,
        Standard,
        PreludeProvided,
        [param!("process", Mixed)],
        None,
        Int,
        "Closes a process opened by proc_open() and returns its exit code."
    ),
    surface!(
        "proc_get_status",
        System,
        Standard,
        PreludeProvided,
        [param!("process", Mixed)],
        None,
        Array,
        "Returns information about a process opened by proc_open()."
    ),
    surface!(
        "proc_terminate",
        System,
        Standard,
        PreludeProvided,
        [param!("process", Mixed), param!("signal", Int = DefaultSpec::Int(15))],
        None,
        Bool,
        "Kills a process opened by proc_open()."
    ),
    surface!(
        "proc_nice",
        System,
        Standard,
        PreludeProvided,
        [param!("priority", Int)],
        None,
        Bool,
        "Changes the priority of the current process."
    ),
    // `PreludeProvided` for the same asymmetry as `levenshtein` above: the AOT side serves
    // `var_export` as a conditionally injected PHP-source prelude (`src/var_export_prelude.rs`),
    // not a `builtin!` registry binding, so a `Function` contract would wrongly demand one. The
    // INTERPRETER has a real implementation
    // (`interpreter::builtins::core::var_export`) because interpreted code cannot reach that
    // prelude at all -- Symfony's routing dumper is interpreted and called it.
    surface!(
        "var_export",
        Io,
        Standard,
        PreludeProvided,
        [
            param!("value", Mixed),
            param!("return", Bool = DefaultSpec::Bool(false)),
        ],
        None,
        Mixed,
        "Renders a parsable string representation of a variable."
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
        Some(VariadicSpec::value("vars")),
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
