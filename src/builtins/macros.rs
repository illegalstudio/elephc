//! Purpose:
//! Provides the `builtin!` declarative macro used to register PHP builtin function
//! descriptors into the inventory-based registry at link time.
//!
//! Called from:
//! - Each `crate::builtins::<area>::<name>` leaf file via `#[macro_use]` on this module.
//!
//! Key details:
//! - This module is included with `#[macro_use]` so the macro is available crate-wide
//!   without explicit import at every call site.
//! - Fields must appear in the CANONICAL ORDER listed below; optional fields may be omitted.
//! - The `params` list syntax is `[name: TypeSpec, name: TypeSpec = DefaultSpec::Variant, ...]`.
//!   Defaults are written as full `DefaultSpec::` paths (unit or data-carrying), e.g.
//!   `= DefaultSpec::Null`, `= DefaultSpec::Int(5)`, `= DefaultSpec::Bool(false)`.
//!   This avoids `macro_rules!`' limitation that `expr` fragments cannot be spliced after `::`.
//! - An optional leading `ref` per parameter marks it as by-reference (`by_ref: true`).
//!   Syntax: `params: [ref array: Mixed, offset: Int]`. Parameters without `ref` are by-value.
//! - A trailing comma after the last field is optional.
//!
//! Canonical field order:
//!   name, area, params, variadic?, min_args?, max_args?, arity_error?, returns, by_ref_return?,
//!   check?, lower, summary, examples?, php_manual?, deprecation?, internal?
//!
//! Example:
//! ```ignore
//! builtin! {
//!     name: "strlen",
//!     area: String,
//!     params: [string: Str],
//!     returns: Int,
//!     lower: strlen_lower,
//!     summary: "Returns the length of a string.",
//!     php_manual: "function.strlen",
//! }
//! ```

/// Registers a PHP builtin descriptor into the `inventory`-based registry.
///
/// Fields must appear in canonical order (optional fields may be omitted):
/// `name`, `area`, `params`, `variadic`?, `min_args`?, `max_args`?, `arity_error`?,
/// `returns`, `by_ref_return`?, `check`?, `lower`, `summary`, `examples`?, `php_manual`?,
/// `deprecation`?, `internal`?
///
/// `max_args` (optional `usize`) caps the maximum argument count enforced by the
/// registry's `check_arity` only; it does not affect `function_sig` or the parity gate.
/// `min_args` (optional `usize`) raises the enforced minimum in `check_arity` only.
/// `arity_error` (optional `&'static str`) overrides the standard arity error message.
/// `lazy_check` (optional `bool`, default `false`) skips the registry's standard pre-inference
/// loop before calling the `check` hook. Use when the check hook must control argument
/// inference order (e.g., to pass object-element type hints to an unannotated closure before
/// `infer_type` is called on it). When `true`, the check hook is responsible for calling
/// `infer_type` on each argument as needed.
///
/// A trailing comma after the last field is optional.
///
/// The `params` list uses `[name: TypeSpec]` or `[name: TypeSpec = DefaultSpec::Variant]`
/// syntax. Defaults are full `DefaultSpec` paths: `DefaultSpec::Null`, `DefaultSpec::Int(5)`,
/// `DefaultSpec::Bool(false)`, etc. Unit and data-carrying variants are both supported.
/// An optional leading `ref` per parameter marks it as by-reference: `params: [ref array: Mixed, ...]`
/// emits `by_ref: true` for that parameter. Parameters without `ref` are by-value (`by_ref: false`).
#[macro_export]
macro_rules! builtin {
    // Entry rule: all fields in canonical order; optional fields handled via helper rules.
    // The last optional field `internal` has no required trailing comma so that
    //   `internal: true }` works without a final comma in the invocation.
    (
        name: $name:expr,
        area: $area:ident,
        params: [ $($params:tt)* ],
        $(variadic: $variadic:expr,)?
        $(min_args: $min_args:expr,)?
        $(max_args: $max_args:expr,)?
        $(arity_error: $arity_error:expr,)?
        returns: $returns:ident,
        $(by_ref_return: $by_ref_return:expr,)?
        $(check: $check:expr,)?
        $(lazy_check: $lazy_check:expr,)?
        lower: $lower:expr,
        summary: $summary:expr,
        $(examples: $examples:expr,)?
        $(php_manual: $php_manual:expr,)?
        $(deprecation: $deprecation:expr,)?
        $(internal: $internal:expr)?
        $(,)?
    ) => {
        inventory::submit! {
            $crate::builtins::spec::BuiltinSpec {
                name: $name,
                area: $crate::builtins::spec::Area::$area,
                params: {
                    const PARAMS: &[$crate::builtins::spec::ParamSpec] =
                        builtin!(@params [ $($params)* ] -> []);
                    PARAMS
                },
                variadic: builtin!(@opt_str $($variadic)?),
                max_args: builtin!(@opt_usize $($max_args)?),
                min_args: builtin!(@opt_usize $($min_args)?),
                arity_error: builtin!(@opt_str $($arity_error)?),
                returns: $crate::builtins::spec::TypeSpec::$returns,
                by_ref_return: builtin!(@opt_bool $($by_ref_return)?),
                check: builtin!(@opt_fn $($check)?),
                lazy_check: builtin!(@opt_bool $($lazy_check)?),
                lower: $lower,
                summary: $summary,
                examples: builtin!(@opt_examples $($examples)?),
                php_manual: builtin!(@opt_str $($php_manual)?),
                deprecation: builtin!(@opt_str $($deprecation)?),
                internal: builtin!(@opt_bool $($internal)?),
            }
        }
    };

    // @params muncher: accumulator-style recursive parser for the params list.
    // Arms with a leading `ref` keyword must appear BEFORE the normal arms so the
    // keyword is consumed as the by-reference marker before the normal name-match fires.

    // Done: emit the accumulated ParamSpec list as a const-promotable slice.
    (@params [] -> [$($acc:tt)*]) => { &[ $($acc)* ] };

    // by-ref param WITH default.
    (@params [ ref $pname:tt : $pty:ident = $pdefault:expr $(, $($rest:tt)*)? ] -> [$($acc:tt)*]) => {
        builtin!(@params [ $($($rest)*)? ] -> [ $($acc)*
            $crate::builtins::spec::ParamSpec {
                name: builtin!(@name_str $pname),
                ty: $crate::builtins::spec::TypeSpec::$pty,
                default: Some($pdefault),
                by_ref: true,
            },
        ])
    };

    // by-ref param WITHOUT default.
    (@params [ ref $pname:tt : $pty:ident $(, $($rest:tt)*)? ] -> [$($acc:tt)*]) => {
        builtin!(@params [ $($($rest)*)? ] -> [ $($acc)*
            $crate::builtins::spec::ParamSpec {
                name: builtin!(@name_str $pname),
                ty: $crate::builtins::spec::TypeSpec::$pty,
                default: None,
                by_ref: true,
            },
        ])
    };

    // normal param WITH default.
    (@params [ $pname:tt : $pty:ident = $pdefault:expr $(, $($rest:tt)*)? ] -> [$($acc:tt)*]) => {
        builtin!(@params [ $($($rest)*)? ] -> [ $($acc)*
            $crate::builtins::spec::ParamSpec {
                name: builtin!(@name_str $pname),
                ty: $crate::builtins::spec::TypeSpec::$pty,
                default: Some($pdefault),
                by_ref: false,
            },
        ])
    };

    // normal param WITHOUT default.
    (@params [ $pname:tt : $pty:ident $(, $($rest:tt)*)? ] -> [$($acc:tt)*]) => {
        builtin!(@params [ $($($rest)*)? ] -> [ $($acc)*
            $crate::builtins::spec::ParamSpec {
                name: builtin!(@name_str $pname),
                ty: $crate::builtins::spec::TypeSpec::$pty,
                default: None,
                by_ref: false,
            },
        ])
    };

    // Helper: convert a param-name token to a &'static str.
    // Handles raw identifiers (e.g. r#break → "break") and explicit string literals.
    // Raw identifiers that are Rust keywords must be listed explicitly because
    // `stringify!(r#keyword)` preserves the `r#` prefix on some Rust toolchains.
    (@name_str r#break) => { "break" };
    (@name_str r#continue) => { "continue" };
    (@name_str r#type) => { "type" };
    (@name_str r#match) => { "match" };
    (@name_str r#return) => { "return" };
    (@name_str r#use) => { "use" };
    (@name_str r#mod) => { "mod" };
    (@name_str r#fn) => { "fn" };
    (@name_str r#let) => { "let" };
    (@name_str r#move) => { "move" };
    (@name_str r#ref) => { "ref" };
    (@name_str r#static) => { "static" };
    (@name_str r#const) => { "const" };
    (@name_str r#trait) => { "trait" };
    (@name_str r#impl) => { "impl" };
    (@name_str r#where) => { "where" };
    (@name_str r#as) => { "as" };
    (@name_str r#in) => { "in" };
    (@name_str r#loop) => { "loop" };
    // Fallback: a normal identifier, stringified as-is.
    (@name_str $name:ident) => { stringify!($name) };
    // Explicit string literal (for future use if needed).
    (@name_str $name:literal) => { $name };

    // Helper: optional DefaultSpec — present wraps in Some, absent yields None.
    // Users write the full DefaultSpec path: `DefaultSpec::Null`, `DefaultSpec::Int(5)`, etc.
    (@default $val:expr) => { Some($val) };
    (@default) => { None };

    // Helper: optional &'static str — present yields Some, absent yields None.
    (@opt_str $val:expr) => { Some($val) };
    (@opt_str) => { None };

    // Helper: optional usize (max_args override) — present yields Some, absent yields None.
    (@opt_usize $val:expr) => { Some($val) };
    (@opt_usize) => { None };

    // Helper: optional bool — present yields the value, absent yields false.
    (@opt_bool $val:expr) => { $val };
    (@opt_bool) => { false };

    // Helper: optional CheckFn — present yields Some, absent yields None.
    (@opt_fn $val:expr) => { Some($val) };
    (@opt_fn) => { None };

    // Helper: optional examples slice — present yields the value, absent yields empty slice.
    (@opt_examples $val:expr) => { $val };
    (@opt_examples) => { &[] };
}
