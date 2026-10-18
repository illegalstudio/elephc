//! Purpose:
//! Validates predefined Core attribute targets and diagnoses discarded NoDiscard calls.
//!
//! Called from:
//! - Final checker validation and expression-statement checking.
//!
//! Key details:
//! - DelayedTargetValidation defers target errors to ReflectionAttribute::newInstance.
//! - Canonical names keep namespaced user attributes separate from global Core markers.

use super::Checker;
use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::{AttributeGroup, Expr, ExprKind, StaticReceiver};
use crate::span::Span;
use crate::types::{collect_attribute_names, PhpType, TypeEnv};

/// Returns the target mask and repeatability of a predefined global Core attribute.
pub(crate) fn flags(name: &str) -> Option<i64> {
    elephc_builtin_contract::core_attribute_flags(name)
}

/// Checks all retained declarations, including parameter and member metadata.
pub(super) fn validate(checker: &Checker) -> Result<(), CompileError> {
    for declaration in checker.fn_decls.values() {
        validate_names(&collect_attribute_names(&declaration.attributes), 2, declaration.span)?;
        if has_no_discard(&declaration.attributes)
            && matches!(declaration.return_type.as_ref(), Some(crate::parser::ast::TypeExpr::Void | crate::parser::ast::TypeExpr::Never))
        {
            return Err(CompileError::new(declaration.span, "NoDiscard cannot apply to a void or never function"));
        }
        for attributes in &declaration.param_attributes {
            validate_names(&collect_attribute_names(attributes), 32, declaration.span)?;
        }
    }
    for class in checker.classes.values() {
        validate_names(&class.attribute_names, 1, Span::dummy())?;
        for names in class.property_attribute_names.values() { validate_names(names, 8, Span::dummy())?; }
        for names in class.constant_attribute_names.values() { validate_names(names, 16, Span::dummy())?; }
        for names in class.method_attribute_names.values() { validate_names(names, 4, Span::dummy())?; }
        for method in &class.method_decls {
            if has_no_discard(&method.attributes)
                && (matches!(method.return_type.as_ref(), Some(crate::parser::ast::TypeExpr::Void | crate::parser::ast::TypeExpr::Never))
                    || matches!(method.name.to_ascii_lowercase().as_str(), "__construct" | "__destruct"))
            {
                return Err(CompileError::new(method.span, "NoDiscard cannot apply to a void or never method"));
            }
        }
        for signature in class.methods.values().chain(class.static_methods.values()) {
            for attributes in &signature.param_attributes {
                validate_names(&collect_attribute_names(attributes), 32, Span::dummy())?;
            }
        }
    }
    Ok(())
}

/// Applies immediate Core validation unless a declaration explicitly defers its targets.
fn validate_names(names: &[String], target: i64, span: Span) -> Result<(), CompileError> {
    let delayed = names.iter().any(|name| name.trim_start_matches('\\').eq_ignore_ascii_case("DelayedTargetValidation"));
    for (index, name) in names.iter().enumerate() {
        let Some(flags) = flags(name) else { continue };
        if flags & elephc_builtin_contract::CORE_ATTRIBUTE_IS_REPEATABLE == 0 && names[..index].iter().any(|previous| previous.eq_ignore_ascii_case(name)) {
            return Err(CompileError::new(span, &format!("Attribute \"{name}\" must not be repeated")));
        }
        if !delayed && flags & target == 0 {
            return Err(CompileError::new(span, &format!("Attribute \"{name}\" cannot target {}", target_name(target))));
        }
    }
    Ok(())
}

/// Formats a PHP attribute target bit for diagnostics.
pub(crate) fn target_name(target: i64) -> &'static str {
    match target { 1 => "class", 2 => "function", 4 => "method", 8 => "property", 16 => "class constant", 32 => "parameter", _ => "declaration" }
}

/// Warns when a directly identified NoDiscard function or method loses its result.
pub(super) fn check_discarded(checker: &mut Checker, expression: &Expr, env: &TypeEnv) {
    let selected = match &expression.kind {
        ExprKind::FunctionCall { name, .. } => checker.fn_decls.get(name.as_str())
            .map(|declaration| (format!("{}()", name.as_str()), collect_attribute_names(&declaration.attributes))),
        ExprKind::StaticMethodCall { receiver: StaticReceiver::Named(class), method, .. } => checker.classes.get(class.as_str())
            .and_then(|info| info.method_attribute_names.get(&crate::names::php_symbol_key(method)))
            .map(|names| (format!("{}::{method}()", class.as_str()), names.clone())),
        ExprKind::MethodCall { object, method, .. } => {
            let class = match &object.kind {
                ExprKind::Variable(name) => env.get(name).and_then(|ty| match ty { PhpType::Object(class) => Some(class.as_str()), _ => None }),
                ExprKind::This => checker.current_class.as_deref(),
                _ => None,
            };
            class.and_then(|class| checker.classes.get(class).and_then(|info| info.method_attribute_names.get(&crate::names::php_symbol_key(method)))
                .map(|names| (format!("{class}::{method}()"), names.clone())))
        }
        _ => None,
    };
    if let Some((name, attributes)) = selected {
        if attributes.iter().any(|name| name.trim_start_matches('\\').eq_ignore_ascii_case("NoDiscard")) {
            checker.warnings.push(CompileWarning::new(expression.span,
                &format!("The return value of {name} should either be used or explicitly ignored")));
        }
    }
}

/// Reads the same marker from AST attributes when only the declaration is available.
pub(crate) fn has_no_discard(attributes: &[AttributeGroup]) -> bool {
    attributes.iter().flat_map(|group| &group.attributes)
        .any(|attribute| attribute.name.as_canonical().eq_ignore_ascii_case("NoDiscard"))
}
