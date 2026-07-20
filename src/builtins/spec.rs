//! Purpose:
//! Defines the `BuiltinSpec` type that describes a single PHP builtin function:
//! its name, arity, type signature, and shared backend-neutral semantics.
//!
//! Called from:
//! - `crate::builtins::registry` (collected via `inventory`).
//! - Checker, optimizer, EIR lowering, ownership, callable, and runtime consumers
//!   through `crate::builtins::semantics`.
//!
//! Key details:
//! - Every builtin must submit exactly one `BuiltinSpec` via the `builtin!` macro;
//!   duplicate names are detected at registry init time.
//! - All `BuiltinSpec` fields are `'static` so the struct can be used in `const` context
//!   and stored in the `inventory`-collected registry without allocation.

// Checker hooks are public registry metadata but intentionally receive the crate-private
// checker implementation rather than exposing compiler internals as public API.
#![allow(private_interfaces)]

/// Categorises a builtin by functional area, used for documentation grouping
/// and future area-scoped registry queries.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Area {
    /// String manipulation builtins (`strlen`, `substr`, …).
    String,
    /// Array manipulation builtins (`count`, `array_map`, …).
    Array,
    /// Mathematical builtins (`abs`, `pow`, …).
    Math,
    /// I/O builtins (`echo`, `file_put_contents`, …).
    Io,
    /// System / process builtins (`exit`, `getenv`, …).
    System,
    /// Type-inspection and conversion builtins (is_int, gettype, settype, …).
    Types,
    /// Callable / closure builtins (`call_user_func`, …).
    Callables,
    /// SPL data-structure builtins.
    Spl,
    /// Pointer and buffer builtins (elephc extensions).
    Pointers,
}

/// Describes the PHP-level type of a parameter or return value at the `BuiltinSpec`
/// level. Uses only `'static` storage so it can appear in `const` items.
///
/// Add variants here only as the builtin migration surfaces the need; do not
/// pre-populate variants that are not yet referenced by any registered builtin.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TypeSpec {
    /// PHP `int`.
    Int,
    /// PHP `float`.
    Float,
    /// PHP `string`.
    Str,
    /// PHP `bool`.
    Bool,
    /// PHP `mixed`.
    Mixed,
    /// PHP `void` (return position only).
    Void,
}

/// Describes the default value for an optional parameter at the `BuiltinSpec`
/// level. Uses only `'static` and `Copy` types so it can appear in `const` items.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum DefaultSpec {
    /// PHP `null` default.
    Null,
    /// A literal integer default.
    Int(i64),
    /// A literal boolean default.
    Bool(bool),
    /// A literal float default.
    Float(f64),
    /// A literal string default.
    Str(&'static str),
    /// `PHP_INT_MAX` sentinel.
    IntMax,
    /// An empty array `[]` default.
    EmptyArray,
}

/// Describes a single named parameter of a PHP builtin function.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ParamSpec {
    /// The PHP-level parameter name (used for named-argument matching).
    pub name: &'static str,
    /// The PHP-level type of the parameter.
    pub ty: TypeSpec,
    /// The default value for optional parameters, or `None` for required parameters.
    pub default: Option<DefaultSpec>,
    /// Whether the parameter is passed by reference (mutating builtins).
    pub by_ref: bool,
}

/// Context passed to a builtin's optional `check` hook during type-checking.
///
/// Gives the hook access to the checker state, the call site name, the argument
/// list, the source span, and the current type environment so it can emit
/// diagnostics and return a refined return type.
pub struct BuiltinCheckCtx<'a> {
    /// The active type checker (mutable so the hook can emit warnings and errors).
    pub checker: &'a mut crate::types::checker::Checker,
    /// The canonical lower-cased builtin name at the call site.
    pub name: &'a str,
    /// The unevaluated argument expressions passed to the builtin.
    pub args: &'a [crate::parser::ast::Expr],
    /// Source span of the call expression, for diagnostic messages.
    pub span: crate::span::Span,
    /// The type environment active at the call site.
    pub env: &'a crate::types::TypeEnv,
}

/// A type-checking hook for a builtin that needs logic beyond the static parameter list.
///
/// The hook receives a mutable `BuiltinCheckCtx` and returns the refined return
/// `PhpType` for the call, or a `CompileError` if the call is ill-typed.
pub type CheckFn = for<'ctx, 'a> fn(
    &'ctx mut BuiltinCheckCtx<'a>,
) -> Result<crate::types::PhpType, crate::errors::CompileError>;

/// Complete static descriptor for one PHP builtin function.
///
/// All fields are `'static` so the spec can be declared as a `const` item and
/// collected into the inventory-based registry at link time without heap allocation.
pub struct BuiltinSpec {
    /// The canonical PHP function name (case-preserved, no leading backslash).
    pub name: &'static str,
    /// The functional area this builtin belongs to.
    pub area: Area,
    /// The declared parameter list, in PHP source order.
    pub params: &'static [ParamSpec],
    /// The PHP-level name of the variadic parameter, if any.
    pub variadic: Option<&'static str>,
    /// An optional override for the maximum argument count enforced by the
    /// registry's `check_arity`. When `Some(n)`, `check_arity` rejects calls with
    /// more than `n` arguments even though the declared parameter list (including
    /// optional params) would otherwise permit more. This affects ONLY
    /// `check_arity`; it does not change `function_sig`, `arity_bounds`, or the
    /// parity gate, which all keep the full param-derived bounds. It exists to
    /// preserve a migrated builtin whose legacy CHECK arm enforced a tighter arity
    /// than its declared (golden) signature allowed.
    pub max_args: Option<usize>,
    /// An optional override for the minimum argument count enforced by the
    /// registry's `check_arity`. When `Some(n)`, `check_arity` rejects calls
    /// with fewer than `n` arguments even though the declared parameter list
    /// would otherwise permit fewer (e.g. a variadic golden with min=0 but the
    /// legacy CHECK arm required ≥2). This affects ONLY `check_arity`; it does
    /// not change `function_sig`, `arity_bounds`, or the parity gate.
    pub min_args: Option<usize>,
    /// A verbatim error message used by `check_arity` instead of the standard
    /// derived `"<name>() takes …"` phrasing when an arity mismatch is detected.
    /// When `None`, `check_arity` uses the standard derived message.
    /// Affects ONLY `check_arity`; `function_sig`, `arity_bounds`, and the parity
    /// gate are unaffected.
    pub arity_error: Option<&'static str>,

    /// The declared PHP-level return type. The shared semantic descriptor decides
    /// whether checker and EIR consumers use this declaration, a checked call-site
    /// type, or one shared argument-sensitive resolver.
    pub returns: TypeSpec,
    /// Whether the function returns by reference.
    pub by_ref_return: bool,
    /// Shared backend-neutral semantics consumed by checker, optimizer, EIR, ownership,
    /// requirements, and callable paths.
    pub semantics: crate::builtins::semantics::BuiltinSemantics,
    /// A short one-line summary for generated documentation.
    pub summary: &'static str,
    /// Example PHP snippets demonstrating the builtin, for generated documentation.
    pub examples: &'static [&'static str],
    /// The PHP manual URL fragment (e.g. `"function.strlen"`), if applicable.
    pub php_manual: Option<&'static str>,
    /// A deprecation message, or `None` if the builtin is not deprecated.
    pub deprecation: Option<&'static str>,
    /// When `true`, the builtin is an elephc extension with no PHP equivalent
    /// (`ptr_*`, `zval_*`, `buffer_*`, `class_attribute_*`, …). `--strict-php`
    /// hides extension builtins from user programs: they stop resolving as
    /// builtins, and user code may declare functions with these names, exactly
    /// as under the PHP interpreter. The set is pinned by
    /// `parity_tests::extension_builtin_set_is_pinned`.
    pub extension: bool,
    /// When `true`, the builtin is not PHP-visible and is not emitted in catalogs
    /// or documentation; it is only used internally by the compiler.
    pub internal: bool,
}

inventory::collect!(BuiltinSpec);

#[cfg(test)]
mod macro_tests {
    use crate::builtins::spec::*;
    builtin! { name: "__macro_probe", area: Types, params: [x: Int], returns: Int, semantics: crate::builtins::semantics::test_probe_semantics(), summary: "probe", internal: true }
    builtin! { name: "__macro_ext_probe", area: Types, params: [], returns: Void, semantics: crate::builtins::semantics::test_probe_semantics(), summary: "extension probe", extension: true, internal: true }

    /// Verifies the macro registers a builtin with its semantic descriptor.
    #[test]
    fn macro_registers_builtin() {
        let default_spec = inventory::iter::<BuiltinSpec>
            .into_iter()
            .find(|s| s.name == "__macro_probe")
            .expect("macro probe must be registered");
        assert!(matches!(
            default_spec.semantics.result_ownership,
            crate::builtins::semantics::BuiltinResultOwnership::MayAliasArguments
        ));
    }

    /// Verifies the `extension` flag defaults to false and is set by the macro arm,
    /// so `--strict-php` classification is opt-in per builtin declaration.
    #[test]
    fn macro_registers_extension_flag() {
        let default_spec = inventory::iter::<BuiltinSpec>
            .into_iter()
            .find(|s| s.name == "__macro_probe")
            .expect("macro probe must be registered");
        let ext_spec = inventory::iter::<BuiltinSpec>
            .into_iter()
            .find(|s| s.name == "__macro_ext_probe")
            .expect("extension probe must be registered");
        assert!(!default_spec.extension);
        assert!(ext_spec.extension);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a const BuiltinSpec can be built and read (const-friendly shape).
    #[test]
    fn const_spec_is_constructible() {
        const P: &[ParamSpec] = &[ParamSpec { name: "string", ty: TypeSpec::Str, default: None, by_ref: false }];
        const S: BuiltinSpec = BuiltinSpec {
            name: "strlen", area: Area::String, params: P, variadic: None,
            max_args: None, min_args: None, arity_error: None,
            returns: TypeSpec::Int,
            by_ref_return: false,
            semantics: crate::builtins::semantics::test_probe_semantics(),
            summary: "len", examples: &[], php_manual: None,
            deprecation: None, extension: false, internal: false,
        };
        assert_eq!(S.name, "strlen");
        assert_eq!(S.params.len(), 1);
        assert!(matches!(
            S.semantics.result_ownership,
            crate::builtins::semantics::BuiltinResultOwnership::MayAliasArguments
        ));
    }
}
