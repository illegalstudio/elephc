//! Purpose:
//! Detects PHP functions marked with `#[Export]` and validates their signatures
//! for library (`--emit cdylib` or `--emit staticlib`) emission, returning a table that the codegen
//! C-ABI trampoline emitter consumes.
//!
//! Called from:
//! - `crate::pipeline::compile()` after `crate::types::check_with_target()`.
//!
//! Key details:
//! - Runs after type checking so `FunctionSig.params` carries fully-resolved
//!   PhpTypes and can reject anything outside the public scalar/string marshaling set
//!   with a single uniform error message.
//! - Only top-level user functions are eligible — methods, closures, arrow
//!   functions, and extern declarations carry their own ABIs and are out of
//!   scope for library export.
//! - Namespaced PHP names receive deterministic C-safe public symbols while
//!   unnamespaced C identifiers preserve their existing ABI spelling.

use std::collections::HashMap;

use crate::errors::CompileError;
use crate::parser::ast::{Program, Stmt, StmtKind};
use crate::span::Span;
use crate::types::{FunctionSig, PhpType};

mod header;
mod safety;

pub use header::{render_c_header, ELEPHC_ABI_VERSION};
pub use safety::validate_cdylib_call_graph;

/// A user PHP function flagged with `#[Export]` that the library emitter must
/// expose through a C-ABI trampoline. Captured after type checking so the
/// signature and public C symbol are fully resolved.
#[derive(Clone, Debug)]
pub struct ExportedFunction {
    pub name: String,
    pub c_name: String,
    pub sig: FunctionSig,
    pub span: Span,
}

/// Walks the post-typecheck program AST and returns every user function
/// declared with the `#[Export]` attribute, paired with its resolved
/// `FunctionSig`. Signatures are validated against the public marshaling
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
    let mut public_symbols = lifecycle_symbols()
        .into_iter()
        .map(|symbol| (symbol.to_string(), "cdylib lifecycle ABI".to_string()))
        .collect::<HashMap<_, _>>();
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
        validate_signature(name, sig, stmt.span)?;
        let c_name = public_c_name(name);
        if let Some(existing) = public_symbols.insert(c_name.clone(), name.clone()) {
            return Err(CompileError::new(
                stmt.span,
                &format!(
                    "exported function '{}' maps to C symbol '{}', which is already used by {}; rename one export to keep the cdylib ABI unambiguous",
                    name, c_name, existing
                ),
            ));
        }
        exports.insert(
            name.clone(),
            ExportedFunction {
                name: name.clone(),
                c_name,
                sig: sig.clone(),
                span: stmt.span,
            },
        );
    }
    Ok(exports)
}

/// Returns the fixed lifecycle symbols reserved by every generated cdylib.
fn lifecycle_symbols() -> [&'static str; 6] {
    [
        "elephc_abi_version",
        "elephc_init",
        "elephc_shutdown",
        "elephc_last_status",
        "elephc_last_error",
        "elephc_free",
    ]
}

/// Maps a PHP export name to a stable C identifier used by assembly and headers.
///
/// Existing unnamespaced C identifiers retain their spelling. Namespaced or
/// otherwise non-C names replace each invalid character with `_`; `collect()`
/// rejects the rare collision instead of emitting an ambiguous ABI.
pub fn public_c_name(php_name: &str) -> String {
    let mut mapped = php_name
        .trim_start_matches('\\')
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || character == '_' {
                character
            } else {
                '_'
            }
        })
        .collect::<String>();
    if mapped.is_empty() {
        mapped.push_str("elephc_export");
    }
    if mapped.as_bytes()[0].is_ascii_digit() {
        mapped.insert(0, '_');
    }
    mapped
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

/// Validates that every parameter and return type has a defined cdylib ABI.
/// Scalar C signatures remain unchanged; every string return uses the binary-safe
/// status/out-parameter surface while preserving the fixed scalar/string inputs.
fn validate_signature(
    name: &str,
    sig: &FunctionSig,
    span: Span,
) -> Result<(), CompileError> {
    if sig.variadic.is_some() {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' uses variadic parameters; #[Export] requires a fixed parameter list",
                name
            ),
        ));
    }
    if sig.ref_params.iter().any(|by_ref| *by_ref) {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' uses by-reference parameters; #[Export] accepts only by-value scalars",
                name
            ),
        ));
    }
    if sig.by_ref_return {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' returns by reference; #[Export] accepts only by-value results",
                name
            ),
        ));
    }
    for (i, (_, ty)) in sig.params.iter().enumerate() {
        if !is_scalar_param_type(ty) {
            return Err(CompileError::new(
                span,
                &format!(
                    "exported function '{}' parameter #{} has unsupported type for --emit cdylib; supported: int, float, bool, string",
                    name,
                    i + 1
                ),
            ));
        }
    }
    if sig.return_type == PhpType::Str {
        return Ok(());
    }
    if !is_scalar_return_type(&sig.return_type) {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' return type is unsupported for --emit cdylib; supported: int, float, bool, void",
                name
            ),
        ));
    }
    Ok(())
}

/// Returns whether `sig` uses the binary-safe caller-owned string result ABI.
pub fn is_string_return_signature(sig: &FunctionSig) -> bool {
    sig.return_type == PhpType::Str
        && sig.variadic.is_none()
        && !sig.by_ref_return
        && !sig.ref_params.iter().any(|by_ref| *by_ref)
}

/// Returns whether `ty` can be marshaled as a scalar C-ABI export parameter.
fn is_scalar_param_type(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Str
    )
}

/// Returns whether `ty` can be marshaled as a scalar C-ABI export return value.
fn is_scalar_return_type(ty: &PhpType) -> bool {
    matches!(
        ty,
        PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Void
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a fully declared fixed signature for export validation tests.
    fn signature(params: Vec<(String, PhpType)>, return_type: PhpType) -> FunctionSig {
        let len = params.len();
        FunctionSig {
            params,
            param_type_exprs: vec![None; len],
            param_attributes: vec![Vec::new(); len],
            defaults: vec![None; len],
            return_type,
            declared_return: true,
            by_ref_return: false,
            ref_params: vec![false; len],
            declared_params: vec![true; len],
            variadic: None,
            deprecation: None,
            is_generator: false,
        }
    }

    /// Accepts every fixed scalar/string input shape for a caller-owned string result.
    #[test]
    fn accepts_all_fixed_string_return_shapes() {
        for sig in [
            signature(Vec::new(), PhpType::Str),
            signature(vec![("input".to_string(), PhpType::Int)], PhpType::Str),
            signature(
                vec![
                    ("left".to_string(), PhpType::Str),
                    ("right".to_string(), PhpType::Str),
                ],
                PhpType::Str,
            ),
        ] {
            assert!(validate_signature("owned_string", &sig, Span::dummy()).is_ok());
            assert!(is_string_return_signature(&sig));
        }
    }

    /// Rejects every by-reference result because the public ABI returns values only.
    #[test]
    fn rejects_by_reference_returns() {
        for return_type in [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::Str] {
            let mut sig = signature(Vec::new(), return_type);
            sig.by_ref_return = true;

            let error = validate_signature("borrowed", &sig, Span::dummy())
                .expect_err("by-reference result must be rejected");
            assert!(error.message.contains("returns by reference"));
            assert!(!is_string_return_signature(&sig));
        }
    }

    /// Preserves the existing scalar-return export contract unchanged.
    #[test]
    fn keeps_existing_scalar_return_signatures() {
        for return_type in [PhpType::Int, PhpType::Float, PhpType::Bool, PhpType::Void] {
            let sig = signature(vec![("input".to_string(), PhpType::Str)], return_type);
            assert!(validate_signature("scalar", &sig, Span::dummy()).is_ok());
            assert!(!is_string_return_signature(&sig));
        }
    }

    /// Keeps legacy names unchanged and maps namespace separators to C-safe underscores.
    #[test]
    fn maps_export_names_to_stable_c_identifiers() {
        assert_eq!(public_c_name("add_i64"), "add_i64");
        assert_eq!(public_c_name("Demo\\add"), "Demo_add");
        assert_eq!(public_c_name("\\Demo\\roundtrip"), "Demo_roundtrip");
    }
}
