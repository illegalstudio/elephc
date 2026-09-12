//! Purpose:
//! Validates schema validation declarations for the checker.
//! Turns parsed declarations into canonical metadata and rejects invalid contracts before code generation.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Declaration metadata must align with name resolution, inheritance flattening, and runtime/codegen expectations.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Attribute, ClassMethod, Expr, ExprKind, StmtKind, TypeExpr, Visibility};
use crate::types::{FunctionSig, PhpType};

use super::super::Checker;

/// Builds a `FunctionSig` from a parsed class method, resolving parameter and return type
/// annotations through the checker. Parameters without type hints use a declared default's
/// syntactic type when one exists, matching free-function signature construction, and otherwise
/// default to `PhpType::Int`.
/// Validates that each declared parameter's default value is compatible with its resolved type.
/// Infers return type from method body when no return annotation is present.
pub(crate) fn build_method_sig(
    checker: &Checker,
    method: &ClassMethod,
    declaring_type: &str,
) -> Result<FunctionSig, CompileError> {
    let method_key = php_symbol_key(&method.name);
    let params: Vec<(String, PhpType)> = method
        .params
        .iter()
        .enumerate()
        .map(|(i, (n, type_ann, default, _))| {
            // User hydration hooks receive a PHP array with arbitrary integer/string
            // keys. Use its boxed declaration contract, including for untyped hooks.
            // Synthetic SPL hooks retain their explicit raw Array(Mixed) ABI.
            if method_key == "__unserialize" && i == 0 && method.span.line != 0 {
                return Ok((n.clone(), PhpType::php_array()));
            }
            let ty = match type_ann {
                Some(type_ann) => checker.resolve_declared_param_type_hint(
                    type_ann,
                    method.span,
                    &format!("Method parameter ${}", n),
                )?,
                None => default
                    .as_ref()
                    .map(super::super::infer_expr_type_syntactic)
                    .unwrap_or(PhpType::Int),
            };
            Ok((n.clone(), ty))
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    let defaults: Vec<Option<Expr>> = method.params.iter().map(|(_, _, d, _)| d.clone()).collect();
    let mut ref_params: Vec<bool> = method.params.iter().map(|(_, _, _, r)| *r).collect();
    for ((param_name, type_ann, default, _), (_, resolved_ty)) in
        method.params.iter().zip(params.iter())
    {
        if type_ann.is_some() {
            checker.validate_schema_parameter_default_type(
                resolved_ty,
                default.as_ref(),
                method.span,
                &format!("Method parameter ${}", param_name),
            )?;
        }
    }
    let return_type = match method.return_type.as_ref() {
        Some(type_ann) => checker.resolve_method_return_type_hint(
            type_ann,
            declaring_type,
            method.span,
            &format!("Method '{}'", method.name),
        )?,
        None => super::super::infer_return_type_syntactic(&method.body),
    };
    if method.variadic.is_some() {
        ref_params.push(method.variadic_by_ref);
    }
    let mut sig = Checker::callable_wrapper_sig(&FunctionSig {
        params,
        param_type_exprs: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.clone())
            .chain(method.variadic.iter().map(|_| method.variadic_type.clone()))
            .collect(),
        param_attributes: method.param_attributes.clone(),
        defaults,
        return_type,
        declared_return: method.return_type.is_some(),
        by_ref_return: method.by_ref_return,
        ref_params,
        declared_params: method
            .params
            .iter()
            .map(|(_, type_ann, _, _)| type_ann.is_some())
            .chain(
                method
                    .variadic
                    .iter()
                    .map(|_| method.variadic_type.is_some()),
            )
            .collect(),
        variadic: method.variadic.clone(),
        deprecation: extract_deprecation(&method.attributes),
    });
    // A declared element type on the variadic (`int ...$xs`) constrains every collected argument.
    // `callable_wrapper_sig` defaults the variadic container to `array<mixed>`; refine it to the
    // declared element type so call validation enforces it.
    if !method.variadic_by_ref {
        if let Some(variadic_type) = &method.variadic_type {
            let elem_ty = checker.resolve_declared_param_type_hint(
                variadic_type,
                method.span,
                &format!(
                    "Method variadic parameter ${}",
                    method.variadic.as_deref().unwrap_or_default()
                ),
            )?;
            if let Some((_, ty)) = sig.params.last_mut() {
                *ty = PhpType::Array(Box::new(elem_ty));
            }
        }
    }
    Ok(sig)
}

/// Returns `Some(reason)` when the attribute list contains a `#[\Deprecated]`
/// marker, with `reason` set to the attribute's first string argument (or an
/// empty string if absent). Match is case-insensitive on the last segment of
/// the attribute name.
pub(crate) fn extract_deprecation(groups: &[crate::parser::ast::AttributeGroup]) -> Option<String> {
    for group in groups {
        for attr in &group.attributes {
            if !matches_global_builtin_attribute(attr, "Deprecated") {
                continue;
            }
            let reason = attr.args.iter().find_map(|expr| match &expr.kind {
                ExprKind::StringLiteral(s) => Some(s.clone()),
                _ => None,
            });
            return Some(reason.unwrap_or_default());
        }
    }
    None
}

/// Returns `true` if `attr` is a global builtin attribute matching `builtin` by name.
/// Fully-qualified names must match exactly (case-insensitive); unqualified names
/// match the last segment case-insensitively. Used to detect `#[\Deprecated]` and similar.
pub(crate) fn matches_global_builtin_attribute(attr: &Attribute, builtin: &str) -> bool {
    let name = attr.name.as_canonical();
    if attr.name.is_fully_qualified() {
        return name.eq_ignore_ascii_case(builtin);
    }
    attr.name.is_unqualified() && name.eq_ignore_ascii_case(builtin)
}

/// Builds a mapping from constructor parameter index to property name for each parameter.
/// For each parameter, searches constructor body for `PropertyAssign` statements where
/// the right-hand side is a Variable with the same name as the parameter; if found,
/// returns `Some(property_name)`, otherwise `None`. Returns empty vec if no constructor.
pub(crate) fn build_constructor_param_map(methods: &[ClassMethod]) -> Vec<Option<String>> {
    let mut param_to_prop = Vec::new();
    if let Some(constructor) = methods
        .iter()
        .find(|m| php_symbol_key(&m.name) == "__construct")
    {
        param_to_prop = constructor
            .params
            .iter()
            .map(|(pname, _, _, _)| {
                for stmt in &constructor.body {
                    if let StmtKind::PropertyAssign {
                        property, value, ..
                    } = &stmt.kind
                    {
                        if let ExprKind::Variable(vn) = &value.kind {
                            if vn == pname {
                                return Some(property.clone());
                            }
                        }
                    }
                }
                None
            })
            .collect();
    }
    param_to_prop
}

/// Returns a numeric rank for visibility levels: `private=0`, `protected=1`, `public=2`.
/// Used to enforce that overriding methods are not less visible than the parent method.
pub(crate) fn visibility_rank(visibility: &Visibility) -> u8 {
    match visibility {
        Visibility::Private => 0,
        Visibility::Protected => 1,
        Visibility::Public => 2,
    }
}

/// The parameter shape a signature's SOURCE declared, with generated slots removed.
///
/// `crate::func_args` appends hidden slots (the surplus-argument collector, and the actual-count
/// parameter that accompanies a source variadic) to every frame it captures, and it captures all
/// of them as soon as the program contains an `eval()` or a backtrace call. Those slots are not
/// declared parameters, so comparing them against a compiler-injected parent signature, which
/// can never carry one, would report a difference the source never wrote.
struct SourceVisibleShape {
    param_count: usize,
    ref_params: Vec<bool>,
    has_defaults: Vec<bool>,
    variadic: Option<String>,
}

impl SourceVisibleShape {
    /// Projects `sig` onto the parameters its source declared.
    fn of(sig: &FunctionSig) -> Self {
        let generated: Vec<bool> = sig
            .params
            .iter()
            .map(|(name, _)| {
                name == crate::func_args::HIDDEN_ARGS_PARAM
                    || name == crate::func_args::HIDDEN_ARGC_PARAM
            })
            .collect();
        let keep = |index: usize| !generated.get(index).copied().unwrap_or(false);
        Self {
            param_count: generated.iter().filter(|hidden| !**hidden).count(),
            ref_params: sig
                .ref_params
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, by_ref)| *by_ref)
                .collect(),
            has_defaults: sig
                .defaults
                .iter()
                .enumerate()
                .filter(|(index, _)| keep(*index))
                .map(|(_, default)| default.is_some())
                .collect(),
            variadic: sig
                .variadic
                .clone()
                .filter(|variadic| variadic != crate::func_args::HIDDEN_ARGS_PARAM),
        }
    }

    /// Counts the declared parameters a caller must supply.
    ///
    /// A variadic parameter is never required, even without a default, which is why the shape
    /// keeps the variadic name rather than folding it into the default flags.
    fn required_param_count(&self) -> usize {
        self.has_defaults
            .iter()
            .enumerate()
            .filter(|(index, has_default)| {
                if self.variadic.is_some() && *index + 1 == self.has_defaults.len() {
                    return false;
                }
                !**has_default
            })
            .count()
    }
}

/// Validates that `child_sig` is compatible with `parent_sig` for override purposes.
/// Checks parameter count, ref params, defaults layout, variadic flag, and required param count.
/// Reports errors with `context` and `kind` (e.g., "overriding method") in the message.
///
/// `compare_generated_abi` is true only when both signatures originate in PHP source. Compiler-
/// injected contracts are synthesized after the `func_args` pass and can never carry its hidden
/// collector or actual-count parameter, so comparing those slots against such a contract would
/// report an ABI difference the source never declared.
pub(crate) fn validate_signature_compatibility(
    span: crate::span::Span,
    owner_name: &str,
    method_name: &str,
    child_sig: &FunctionSig,
    parent_sig: &FunctionSig,
    kind: &str,
    context: &str,
    compare_generated_abi: bool,
) -> Result<(), CompileError> {
    // The hidden variadic that collects surplus positional arguments for
    // `func_num_args()`/`func_get_args()`/`func_get_arg()` is a real ABI parameter, so an
    // inherited signature that does not carry it cannot dispatch to a body that does.
    // Report that directly instead of the generic parameter-count mismatch, which names a
    // parameter the source never wrote.
    if compare_generated_abi
        && (crate::func_args::sig_collects_surplus_args(child_sig)
            != crate::func_args::sig_collects_surplus_args(parent_sig)
            || crate::func_args::sig_has_hidden_argc_param(child_sig)
                != crate::func_args::sig_has_hidden_argc_param(parent_sig))
    {
        return Err(CompileError::new(
            span,
            &format!(
                "func_num_args()/func_get_args()/func_get_arg() are not supported in {}::{} when {} {}: the inherited signature cannot be widened to collect surplus arguments",
                owner_name, method_name, context, kind
            ),
        ));
    }

    let child = SourceVisibleShape::of(child_sig);
    let parent = SourceVisibleShape::of(parent_sig);

    if child.param_count != parent.param_count {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child.ref_params != parent.ref_params {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change pass-by-reference parameters when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child.has_defaults != parent.has_defaults {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change optional parameter layout when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child.variadic != parent.variadic {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change variadic parameter shape when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    if child.required_param_count() != parent.required_param_count() {
        return Err(CompileError::new(
            span,
            &format!(
                "Cannot change required parameter count when {} {}: {}::{}",
                context, kind, owner_name, method_name
            ),
        ));
    }

    Ok(())
}

/// Returns `true` if `actual` is a compatible declared return type for `expected`.
/// Allows `PhpType::Never` (unreachable) as a wildcard match. Otherwise delegates to
/// `checker.type_accepts(expected, actual)` for standard subtype checking.
pub(crate) fn declared_return_type_compatible(
    checker: &Checker,
    expected: &PhpType,
    actual: &PhpType,
) -> bool {
    matches!(actual, PhpType::Never) || checker.type_accepts(expected, actual)
}

/// Returns true for PDO's internal SQLSTATE-aware widening of `Exception::getCode()`.
pub(crate) fn is_pdo_exception_get_code_contract(
    class_name: &str,
    method_name: &str,
    return_type: &PhpType,
) -> bool {
    let PhpType::Union(types) = return_type else {
        return false;
    };
    class_name.trim_start_matches('\\') == "PDOException"
        && php_symbol_key(method_name) == "getcode"
        && types.len() == 2
        && types.contains(&PhpType::Str)
        && types.contains(&PhpType::Int)
}

/// Checks a preserved late-static parent/interface return against a child declaration.
///
/// A concrete class name cannot replace `static`: that would become unsound for further
/// subclasses. `never` remains a valid covariant narrowing, while compound returns are
/// compared after binding only their `static` members to the current receiver.
pub(crate) fn late_static_return_compatible(
    checker: &Checker,
    expected: Option<&TypeExpr>,
    actual: Option<&TypeExpr>,
    actual_resolved: &PhpType,
    receiver_type: &str,
    span: crate::span::Span,
) -> Result<Option<bool>, CompileError> {
    let Some(expected) = expected else {
        return Ok(None);
    };
    if matches!(actual_resolved, PhpType::Never) {
        return Ok(Some(true));
    }
    let Some(actual) = actual.filter(|return_type| return_type.contains_late_static()) else {
        return Ok(Some(false));
    };
    let expected =
        checker.resolve_late_static_return_type_hint(expected, receiver_type, span)?;
    let actual = checker.resolve_late_static_return_type_hint(actual, receiver_type, span)?;
    Ok(Some(declared_return_type_compatible(
        checker, &expected, &actual,
    )))
}

/// Returns whether a class-like symbol was declared in PHP source.
///
/// Unknown owners stay strict. Only a symbol proven compiler-injected may suppress comparison of
/// the generated `func_args` ABI slots.
pub(super) fn declaration_is_source(checker: &Checker, owner: &str) -> bool {
    checker
        .classes
        .get(owner)
        .map(|info| info.declaration_span != crate::span::Span::dummy())
        .or_else(|| {
            checker
                .interfaces
                .get(owner)
                .map(|info| info.declaration_span != crate::span::Span::dummy())
        })
        .unwrap_or(true)
}

/// Validates that `method` can override `parent_sig` in class `class_name`.
/// Builds the child signature via `build_method_sig`, skips validation for `__construct`,
/// checks signature compatibility, and ensures the child does not remove a declared
/// return type when the parent has one or make it incompatible.
pub(crate) fn validate_override_signature(
    checker: &Checker,
    class: &crate::types::traits::FlattenedClass,
    method: &ClassMethod,
    parent_sig: &FunctionSig,
    parent_late_static_return: Option<&TypeExpr>,
    is_static: bool,
    parent_declaration_is_source: bool,
) -> Result<(), CompileError> {
    let kind = if is_static { "static method" } else { "method" };
    let class_name = class.name.as_str();
    let child_sig = build_method_sig(checker, method, class_name)?;
    if php_symbol_key(&method.name) == "__construct" {
        return Ok(());
    }
    validate_signature_compatibility(
        method.span,
        class_name,
        &method.name,
        &child_sig,
        parent_sig,
        kind,
        "overriding",
        parent_declaration_is_source,
    )?;
    if parent_sig.declared_return && !child_sig.declared_return {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{} without declaring a compatible return type (parent returns {})",
                kind, class_name, method.name, parent_sig.return_type
            ),
        ));
    }
    let late_static_compatible = late_static_return_compatible(
        checker,
        parent_late_static_return,
        method.return_type.as_ref(),
        &child_sig.return_type,
        class_name,
        method.span,
    )?;
    let return_compatible = is_pdo_exception_get_code_contract(
        class_name,
        &method.name,
        &child_sig.return_type,
    ) || late_static_compatible.unwrap_or_else(|| {
        declared_return_type_compatible(
            checker,
            &parent_sig.return_type,
            &child_sig.return_type,
        ) || covariant_self_return_compatible(
            checker,
            class_name,
            class.extends.as_deref(),
            &class.implements,
            parent_sig,
            &child_sig,
        )
    });
    if parent_sig.declared_return && !return_compatible {
        return Err(CompileError::new(
            method.span,
            &format!(
                "Cannot override {} {}::{} with incompatible return type {} (parent returns {})",
                kind, class_name, method.name, child_sig.return_type, parent_sig.return_type
            ),
        ));
    }
    Ok(())
}

/// Returns true when a child method may return the child class itself against a wider parent return.
///
/// Covers PHP covariant returns (`parent::w(): Base` overridden as `w(): static` / `w(): Child`)
/// while the child is mid-construction — `type_accepts` cannot see the subclass edge yet because
/// `checker.classes` lacks the child, but the parent class is already registered.
fn covariant_self_return_compatible(
    checker: &Checker,
    class_name: &str,
    extends: Option<&str>,
    implements: &[String],
    parent_sig: &FunctionSig,
    child_sig: &FunctionSig,
) -> bool {
    match (&parent_sig.return_type, &child_sig.return_type) {
        (PhpType::Object(expected_name), PhpType::Object(actual_name))
            if actual_name == class_name =>
        {
            if expected_name == class_name {
                return true;
            }
            if let Some(parent) = extends {
                if parent == expected_name
                    || checker.is_subclass_of(parent, expected_name)
                    || checker.class_implements_interface(parent, expected_name)
                {
                    return true;
                }
            }
            implements.iter().any(|iface| {
                iface == expected_name
                    || checker.interface_extends_interface(iface, expected_name)
            })
        }
        _ => false,
    }
}
