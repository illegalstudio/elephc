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
    /// Source-visible signature that defines validation, headers, and the host ABI.
    pub source_sig: FunctionSig,
    /// Physical compiler signature used when the trampoline invokes the PHP body.
    pub internal_sig: FunctionSig,
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
        let source_sig = source_signature(name, sig, stmt.span)?;
        validate_signature(name, &source_sig, stmt.span)?;
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
                source_sig,
                internal_sig: sig.clone(),
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

/// Derives the fixed source contract from the compiler's physical function signature.
///
/// The argument-introspection pass can append one generated variadic collector to every frame
/// when an unrelated `eval()` or backtrace call exists in the program. That slot remains in the
/// internal signature, but it must never become a host-visible parameter. A source variadic,
/// including one paired with the generated argc slot, remains unsupported for fixed exports.
fn source_signature(
    name: &str,
    internal: &FunctionSig,
    span: Span,
) -> Result<FunctionSig, CompileError> {
    if crate::func_args::sig_has_hidden_argc_param(internal)
        || (internal.variadic.is_some() && !crate::func_args::sig_collects_surplus_args(internal))
    {
        return Err(CompileError::new(
            span,
            &format!(
                "exported function '{}' uses variadic parameters; #[Export] requires a fixed parameter list",
                name
            ),
        ));
    }
    if !crate::func_args::sig_collects_surplus_args(internal) {
        return Ok(internal.clone());
    }

    let generated_index = internal.params.len().checked_sub(1).ok_or_else(|| {
        CompileError::new(
            span,
            &format!(
                "internal: exported function '{}' has a generated argument collector without a physical parameter",
                name
            ),
        )
    })?;
    let generated = &internal.params[generated_index];
    if generated.0 != crate::func_args::HIDDEN_ARGS_PARAM
        || generated.1 != PhpType::Array(Box::new(PhpType::Mixed))
    {
        return Err(CompileError::new(
            span,
            &format!(
                "internal: exported function '{}' has a malformed generated argument collector",
                name
            ),
        ));
    }

    let mut source = internal.clone();
    source.params.pop();
    source.param_type_exprs.pop();
    source.param_attributes.pop();
    source.defaults.pop();
    source.ref_params.pop();
    source.declared_params.pop();
    source.variadic = None;
    Ok(source)
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

    /// Adds the `func_args` surplus-argument collector to a signature, the way the desugaring
    /// pass does for every captured frame.
    fn with_generated_collector(mut sig: FunctionSig) -> FunctionSig {
        sig.params.push((
            crate::func_args::HIDDEN_ARGS_PARAM.to_string(),
            PhpType::Array(Box::new(PhpType::Mixed)),
        ));
        sig.param_type_exprs.push(None);
        sig.param_attributes.push(Vec::new());
        sig.defaults.push(None);
        sig.ref_params.push(false);
        sig.declared_params.push(false);
        sig.variadic = Some(crate::func_args::HIDDEN_ARGS_PARAM.to_string());
        sig
    }

    /// Judges the export contract on the source-visible signature.
    ///
    /// `crate::func_args` gives every frame the hidden collector as soon as the program contains
    /// an `eval()` or a backtrace call, so treating it as a declared variadic refused exports
    /// whose own parameter list is fixed, and hid the diagnostic the program actually earned.
    #[test]
    fn accepts_a_fixed_export_carrying_the_generated_collector() {
        let internal = with_generated_collector(signature(
            vec![("input".to_string(), PhpType::Str)],
            PhpType::Str,
        ));
        let source = source_signature("roundtrip", &internal, Span::dummy()).unwrap();
        assert_eq!(source.params, vec![("input".to_string(), PhpType::Str)]);
        assert!(source.variadic.is_none());
        assert!(validate_signature("roundtrip", &source, Span::dummy()).is_ok());
        assert!(is_string_return_signature(&source));
        assert!(crate::func_args::sig_collects_surplus_args(&internal));
    }

    /// Keeps refusing a variadic the source itself declared.
    #[test]
    fn rejects_a_source_declared_variadic_export() {
        let mut sig = signature(
            vec![
                ("input".to_string(), PhpType::Str),
                ("rest".to_string(), PhpType::Array(Box::new(PhpType::Str))),
            ],
            PhpType::Str,
        );
        sig.variadic = Some("rest".to_string());

        let error = validate_signature("spread", &sig, Span::dummy())
            .expect_err("a declared variadic must be rejected");
        assert!(error.message.contains("uses variadic parameters"));
    }

    /// Numbers an unsupported parameter by its SOURCE position, never counting the collector.
    #[test]
    fn numbers_unsupported_parameters_by_source_position() {
        let internal = with_generated_collector(signature(
            vec![
                ("input".to_string(), PhpType::Str),
                ("rows".to_string(), PhpType::Array(Box::new(PhpType::Int))),
            ],
            PhpType::Int,
        ));
        let source = source_signature("rows", &internal, Span::dummy()).unwrap();

        let error = validate_signature("rows", &source, Span::dummy())
            .expect_err("an array parameter must be rejected");
        assert!(error.message.contains("parameter #2"), "{}", error.message);
    }

    /// Rejects the hidden argc shape because it can only accompany a source variadic export.
    #[test]
    fn rejects_a_source_variadic_with_hidden_argc_metadata() {
        let mut sig = signature(
            vec![(
                "rest".to_string(),
                PhpType::Array(Box::new(PhpType::Mixed)),
            )],
            PhpType::Int,
        );
        sig.variadic = Some("rest".to_string());
        sig.params.push((
            crate::func_args::HIDDEN_ARGC_PARAM.to_string(),
            PhpType::Int,
        ));

        let error = source_signature("spread", &sig, Span::dummy())
            .expect_err("hidden argc must remain impossible for fixed exports");
        assert!(error.message.contains("uses variadic parameters"));
    }

    /// Keeps legacy names unchanged and maps namespace separators to C-safe underscores.
    #[test]
    fn maps_export_names_to_stable_c_identifiers() {
        assert_eq!(public_c_name("add_i64"), "add_i64");
        assert_eq!(public_c_name("Demo\\add"), "Demo_add");
        assert_eq!(public_c_name("\\Demo\\roundtrip"), "Demo_roundtrip");
    }
}
