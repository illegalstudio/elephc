//! Purpose:
//! Declares the PHP `proc_open()` family and the internal resource helpers its injected
//! prelude composes, as shared builtin contracts.
//!
//! Called from:
//! - `crate::registry` while assembling the authoritative builtin catalog.
//! - `crate::requirements`, which attaches the `elephc_proc` bridge to [`FUNCTIONS`].
//! - The AOT proc prelude detector, which uses [`FUNCTIONS`] as its exact name set.
//!
//! Key details:
//! - Signatures are php-src 8.5's. A resource has no `TypeSpec`, so resource parameters
//!   and `resource|false` results are `mixed`; the AOT check hooks narrow them.
//! - The `__elephc_*` helpers are `internal: true`: only injected preludes call them.
//! - Entries stay in canonical lowercase name order, as the registry requires.

use crate::{
    Area, BuiltinContract, BuiltinId, BuiltinKind, DefaultSpec, ParamSpec, PhpModule, TypeSpec,
};

/// Canonical PHP names of the public proc family, in stable order.
pub const FUNCTIONS: &[&str] = &[
    "proc_close",
    "proc_get_status",
    "proc_nice",
    "proc_open",
    "proc_terminate",
];

/// Returns the public names as an iterator for requirements/parity joins.
pub fn contract_names() -> impl Iterator<Item = &'static str> {
    FUNCTIONS.iter().copied()
}

/// One ordinary by-value parameter.
macro_rules! param {
    ($name:literal, $ty:expr) => {
        ParamSpec { name: $name, ty: $ty, default: None, by_ref: false }
    };
    ($name:literal, $ty:expr, $default:expr) => {
        ParamSpec { name: $name, ty: $ty, default: Some($default), by_ref: false }
    };
}

/// One proc-family contract; `$internal` selects an `__elephc_*` prelude helper.
macro_rules! proc_contract {
    ($name:literal, $area:ident, $module:ident, [$($param:expr),*], $returns:expr, $internal:literal, $manual:expr, $summary:literal) => {
        BuiltinContract {
            id: BuiltinId::from_canonical_name($name),
            name: $name,
            area: Area::$area,
            module: PhpModule::$module,
            since: None,
            kind: BuiltinKind::Function,
            params: &[$($param),*],
            variadic: None,
            variadic_by_ref: false,
            min_args: None,
            max_args: None,
            arity_error: None,
            returns: $returns,
            by_ref_return: false,
            summary: $summary,
            examples: &[],
            php_manual: $manual,
            deprecation: None,
            extension: false,
            internal: $internal,
            requirements: &[],
        }
    };
}

/// `?string`.
const NULLABLE_STRING: TypeSpec = TypeSpec::Nullable(&TypeSpec::Str);
/// `?array`.
const NULLABLE_ARRAY: TypeSpec = TypeSpec::Nullable(&TypeSpec::Array);

pub(crate) static CONTRACTS: &[BuiltinContract] = &[
    proc_contract!("__elephc_fd_stream", Io, Elephc, [param!("fd", TypeSpec::Int)], TypeSpec::Mixed, true, None,
        "Boxes a native descriptor the proc bridge handed back as a PHP stream resource."),
    proc_contract!("__elephc_proc_resource", System, Elephc, [param!("process", TypeSpec::Int)], TypeSpec::Mixed, true, None,
        "Boxes a proc bridge process id as a PHP process resource."),
    proc_contract!("__elephc_resource_close", Io, Elephc, [param!("resource", TypeSpec::Mixed)], TypeSpec::Void, true, None,
        "Marks a boxed resource closed without touching its native handle."),
    proc_contract!("__elephc_resource_kind", Io, Elephc, [param!("value", TypeSpec::Mixed)], TypeSpec::Int, true, None,
        "Returns a boxed resource's runtime kind, or -1 for a non-resource."),
    proc_contract!("__elephc_resource_payload", Io, Elephc, [param!("value", TypeSpec::Mixed)], TypeSpec::Int, true, None,
        "Returns a boxed resource's native payload, or -1 for a non-resource."),
    proc_contract!("proc_close", System, Standard, [param!("process", TypeSpec::Mixed)], TypeSpec::Int, false,
        Some("function.proc-close"), "Closes a process opened by proc_open() and returns its exit code."),
    proc_contract!("proc_get_status", System, Standard, [param!("process", TypeSpec::Mixed)], TypeSpec::Array, false,
        Some("function.proc-get-status"), "Gets information about a process opened by proc_open()."),
    proc_contract!("proc_nice", System, Standard, [param!("priority", TypeSpec::Int)], TypeSpec::Bool, false,
        Some("function.proc-nice"), "Changes the priority of the current process."),
    BuiltinContract {
        params: &[
            param!("command", TypeSpec::Union(&[TypeSpec::Array, TypeSpec::Str])),
            param!("descriptor_spec", TypeSpec::Array),
            ParamSpec { name: "pipes", ty: TypeSpec::Mixed, default: None, by_ref: true },
            param!("cwd", NULLABLE_STRING, DefaultSpec::Null),
            param!("env_vars", NULLABLE_ARRAY, DefaultSpec::Null),
            param!("options", NULLABLE_ARRAY, DefaultSpec::Null),
        ],
        ..proc_contract!("proc_open", System, Standard, [], TypeSpec::Mixed, false,
            Some("function.proc-open"), "Executes a command and opens file pointers for input/output.")
    },
    proc_contract!("proc_terminate", System, Standard,
        [param!("process", TypeSpec::Mixed), param!("signal", TypeSpec::Int, DefaultSpec::Int(15))],
        TypeSpec::Bool, false, Some("function.proc-terminate"), "Kills a process opened by proc_open()."),
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the catalog stays sorted and covers every public name exactly once.
    #[test]
    fn proc_contracts_are_sorted_and_cover_every_function() {
        let mut previous = "";
        for contract in CONTRACTS {
            assert!(previous < contract.name, "{} must follow {previous}", contract.name);
            assert_eq!(contract.kind, BuiltinKind::Function);
            assert_eq!(contract.internal, contract.name.starts_with("__elephc_"));
            previous = contract.name;
        }
        let public: Vec<_> = CONTRACTS.iter().filter(|c| !c.internal).map(|c| c.name).collect();
        assert_eq!(public, FUNCTIONS);
    }
}
