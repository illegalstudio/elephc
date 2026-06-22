//! Purpose:
//! Detects PHP functions marked with `#[Export]` and validates their signatures
//! for cdylib (`--emit cdylib`) emission, returning a table that the codegen
//! C-ABI trampoline emitter consumes.
//!
//! Called from:
//! - `crate::pipeline::compile()` after `crate::types::check_with_target()`.
//!
//! Key details:
//! - Runs after type checking so `FunctionSig.params` carries fully-resolved
//!   PhpTypes and we can reject anything outside the v1 scalar marshaling set
//!   with a single uniform error message.
//! - Only top-level user functions are eligible — methods, closures, arrow
//!   functions, and extern declarations carry their own ABIs and are out of
//!   scope for cdylib export.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

/// A user PHP function flagged with `#[Export]` that the cdylib emitter must
/// expose under its unmangled PHP name with a C-ABI trampoline. Captured after
/// type checking so the signature is fully resolved.
///
/// `sig` and `span` are retained for downstream consumers — codegen passes that
/// emit per-export header entries, generated documentation, and string-return
/// marshaling all need them — even though the v1 trampoline emitter only reads
/// `name`.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct ExportedFunction {
    pub name: String,
    pub sig: FunctionSig,
    pub span: Span,
}

/// Walks the post-typecheck program AST and returns every user function
/// declared with the `#[Export]` attribute, paired with its resolved
/// `FunctionSig`. Signatures are validated against the v1 scalar marshaling
/// rules and rejected with a localized error otherwise.
///
/// Matches both the bare `#[Export]` form and the fully-qualified
/// `#[\Elephc\Export]` form so attribute consumers can be namespace-scoped
/// without changing the export model.
pub fn collect(
    program: &Program,
    functions: &HashMap<String, FunctionSig>,
) -> Result<HashMap<String, ExportedFunction>, CompileError> {
    let mut exports = HashMap::new();
    for stmt in program {
        let StmtKind::FunctionDecl { name, .. } = &stmt.kind else {
            continue;
        };
        if !has_export_attribute(stmt) {
            continue;
        }
        let sig = functions.get(name).ok_or_else(|| {
            CompileError::new(
                stmt.span,
                &format!(
                    "internal: exported function '{}' has no resolved signature",
                    name
                ),
            )
        })?;
        validate_scalar_signature(name, sig, stmt.span)?;
        exports.insert(
            name.clone(),
            ExportedFunction {
                name: name.clone(),
                sig: sig.clone(),
                span: stmt.span,
            },
        );
    }
    Ok(exports)
}

/// Returns `true` if `stmt` carries an `#[Export]` (or `#[\Elephc\Export]`)
/// attribute. The match is on the last segment of the attribute name so both
/// the bare and fully-qualified spellings are accepted.
fn has_export_attribute(stmt: &Stmt) -> bool {
    for group in &stmt.attributes {
        for attr in &group.attributes {
            if attr
                .name
                .parts
                .last()
                .map(|seg| seg == "Export")
                .unwrap_or(false)
            {
                return true;
            }
        }
    }
    false
}

/// Validates that every parameter type and the return type fall within the v1
/// scalar marshaling set. Strings are accepted as inputs (passed as a
/// `const char* ptr, size_t len` pair on the C side) but not as return values
/// in v1 — string-out arrives in a later iteration once the host can free
/// elephc-allocated strings through `elephc_free`.
fn validate_scalar_signature(
    name: &str,
    sig: &FunctionSig,
    span: Span,
) -> Result<(), CompileError> {
    if sig.variadic.is_some() {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' uses variadic parameters; #[Export] v1 requires a fixed parameter list",
                name
            ),
        ));
    }
    if sig.ref_params.iter().any(|by_ref| *by_ref) {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' uses by-reference parameters; #[Export] v1 accepts only by-value scalars",
                name
            ),
        ));
    }
    for (i, (_, ty)) in sig.params.iter().enumerate() {
        if !is_v1_param_type(ty) {
            return Err(CompileError::new(
                span,
                &format!(
                    "exported function '{}' parameter #{} has unsupported type for --emit cdylib v1; supported: int, float, bool, string",
                    name,
                    i + 1
                ),
            ));
        }
    }
    if !is_v1_return_type(&sig.return_type) {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' return type is unsupported for --emit cdylib v1; supported: int, float, bool, void",
                name
            ),
        ));
    }
    Ok(())
}

/// Returns whether `ty` can be marshaled as a v1 C-ABI export parameter.
fn is_v1_param_type(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str
    )
}

/// Returns whether `ty` can be marshaled as a v1 C-ABI export return value.
fn is_v1_return_type(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
    )
}
