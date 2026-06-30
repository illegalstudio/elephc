//! Purpose:
//! Defines the `BuiltinSpec` type that describes a single PHP builtin function:
//! its name, arity, type signature, purity, and codegen lowering hook.
//!
//! Called from:
//! - `crate::builtins::registry` (collected via `inventory`).
//! - `crate::types::checker::builtins` and `crate::codegen_ir::lower_inst::builtins`
//!   (consumed during type-check and codegen dispatch).
//!
//! Key details:
//! - Every builtin must submit exactly one `BuiltinSpec` via the `builtin!` macro;
//!   duplicate names are detected at registry init time.
//! - All `BuiltinSpec` fields are `'static` so the struct can be used in `const` context
//!   and stored in the `inventory`-collected registry without allocation.

// These new types reference pub(crate) types (Checker, FunctionContext) through their
// pub interfaces; that mismatch is intentional and will be resolved when the migration
// elevates or unifies those visibilities. Dead-code warnings are expected during the
// multi-task migration before the registry wires the types into active code paths.
#![allow(dead_code, private_interfaces)]

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
    /// Callable / closure builtins (`call_user_func`, …).
    Callables,
    /// SPL data-structure builtins.
    Spl,
    /// Pointer and buffer builtins (elephc extensions).
    Pointers,
    /// Internal compiler builtins not exposed as PHP-visible functions.
    Internal,
}

/// Describes the PHP-level type of a parameter or return value at the `BuiltinSpec`
/// level. Uses only `'static` storage so it can appear in `const` items.
///
/// Add variants here only as the builtin migration surfaces the need; do not
/// pre-populate variants that are not yet referenced by any registered builtin.
#[derive(Clone, Copy, PartialEq, Debug)]
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
    /// PHP `null`.
    Null,
    /// PHP `void` (return position only).
    Void,
    /// A homogeneous PHP array with element type `T` (`T[]`).
    ArrayOf(&'static TypeSpec),
    /// A PHP associative array with value type `T`.
    AssocOf(&'static TypeSpec),
    /// A union of two or more PHP types.
    Union(&'static [TypeSpec]),
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
    /// `PHP_INT_MIN` sentinel.
    IntMin,
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

/// The assembly-lowering hook for a builtin, called by the EIR backend.
///
/// Receives the active per-function backend context and the `BuiltinCall` instruction,
/// and emits the required assembly. Returns a `CodegenIrError` if the lowering path
/// is not yet implemented for this target.
pub type LowerFn = for<'ctx, 'f, 'i> fn(
    &'ctx mut crate::codegen_ir::context::FunctionContext<'f>,
    &'i crate::ir::Instruction,
) -> Result<(), crate::codegen_ir::CodegenIrError>;

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
    /// The PHP-level return type.
    pub returns: TypeSpec,
    /// Whether the function returns by reference.
    pub by_ref_return: bool,
    /// An optional type-checking hook for builtins whose return type depends
    /// on the argument types or values.
    pub check: Option<CheckFn>,
    /// The assembly-lowering hook called by the EIR backend for this builtin.
    pub lower: LowerFn,
    /// A short one-line summary for generated documentation.
    pub summary: &'static str,
    /// Example PHP snippets demonstrating the builtin, for generated documentation.
    pub examples: &'static [&'static str],
    /// The PHP manual URL fragment (e.g. `"function.strlen"`), if applicable.
    pub php_manual: Option<&'static str>,
    /// A deprecation message, or `None` if the builtin is not deprecated.
    pub deprecation: Option<&'static str>,
    /// When `true`, the builtin is not PHP-visible and is not emitted in catalogs
    /// or documentation; it is only used internally by the compiler.
    pub internal: bool,
}

inventory::collect!(BuiltinSpec);

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies a const BuiltinSpec can be built and read (const-friendly shape).
    #[test]
    fn const_spec_is_constructible() {
        const P: &[ParamSpec] = &[ParamSpec { name: "string", ty: TypeSpec::Str, default: None, by_ref: false }];
        const S: BuiltinSpec = BuiltinSpec {
            name: "strlen", area: Area::String, params: P, variadic: None,
            returns: TypeSpec::Int, by_ref_return: false, check: None,
            lower: noop_lower, summary: "len", examples: &[], php_manual: None,
            deprecation: None, internal: false,
        };
        assert_eq!(S.name, "strlen");
        assert_eq!(S.params.len(), 1);
    }
    fn noop_lower(_c: &mut crate::codegen_ir::context::FunctionContext, _i: &crate::ir::Instruction)
        -> Result<(), crate::codegen_ir::CodegenIrError> { Ok(()) }
}
