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
//! - `by_ref` params are not yet needed; `by_ref: false` is hardcoded. Add a marker token
//!   (e.g. `ref name: Ty`) when an area needs it (YAGNI).
//! - A trailing comma after the last field is optional.
//!
//! Canonical field order:
//!   name, area, params, variadic?, max_args?, returns, by_ref_return?, check?, lower,
//!   summary, examples?, php_manual?, deprecation?, internal?
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
/// `name`, `area`, `params`, `variadic`?, `max_args`?, `returns`, `by_ref_return`?,
/// `check`?, `lower`, `summary`, `examples`?, `php_manual`?, `deprecation`?, `internal`?
///
/// `max_args` (optional `usize`) caps the maximum argument count enforced by the
/// registry's `check_arity` only; it does not affect `function_sig` or the parity gate.
///
/// A trailing comma after the last field is optional.
///
/// The `params` list uses `[name: TypeSpec]` or `[name: TypeSpec = DefaultSpec::Variant]`
/// syntax. Defaults are full `DefaultSpec` paths: `DefaultSpec::Null`, `DefaultSpec::Int(5)`,
/// `DefaultSpec::Bool(false)`, etc. Unit and data-carrying variants are both supported.
/// `by_ref` is always `false` at this stage (YAGNI — add a `ref` marker when needed).
#[macro_export]
macro_rules! builtin {
    // Entry rule: all fields in canonical order; optional fields handled via helper rules.
    // The last optional field `internal` has no required trailing comma so that
    //   `internal: true }` works without a final comma in the invocation.
    (
        name: $name:expr,
        area: $area:ident,
        params: [ $($pname:tt : $pty:ident $(= $pdefault:expr)?),* $(,)? ],
        $(variadic: $variadic:expr,)?
        $(max_args: $max_args:expr,)?
        returns: $returns:ident,
        $(by_ref_return: $by_ref_return:expr,)?
        $(check: $check:expr,)?
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
                    const PARAMS: &[$crate::builtins::spec::ParamSpec] = &[
                        $(
                            $crate::builtins::spec::ParamSpec {
                                name: builtin!(@name_str $pname),
                                ty: $crate::builtins::spec::TypeSpec::$pty,
                                default: builtin!(@default $($pdefault)?),
                                by_ref: false,
                            },
                        )*
                    ];
                    PARAMS
                },
                variadic: builtin!(@opt_str $($variadic)?),
                max_args: builtin!(@opt_usize $($max_args)?),
                returns: $crate::builtins::spec::TypeSpec::$returns,
                by_ref_return: builtin!(@opt_bool $($by_ref_return)?),
                check: builtin!(@opt_fn $($check)?),
                lower: $lower,
                summary: $summary,
                examples: builtin!(@opt_examples $($examples)?),
                php_manual: builtin!(@opt_str $($php_manual)?),
                deprecation: builtin!(@opt_str $($deprecation)?),
                internal: builtin!(@opt_bool $($internal)?),
            }
        }
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
