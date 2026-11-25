//! Purpose:
//! Validates predefined Core attribute targets and diagnoses discarded NoDiscard calls.
//!
//! Called from:
//! - The checker driver, before declaration tables exist, and expression-statement checking.
//! - Name resolution and reflection factories, through `flags`.
//!
//! Key details:
//! - Walks resolved declarations so interface, trait, and enum members are covered and each error
//!   reports the offending attribute's own span. PHP itself reports the declaration's line.
//! - Runs first and stops at the first error: PHP rejects an invalid target while compiling,
//!   before inheritance errors exist. This is why `no matching parent` never hides a target error.
//! - Masks follow the compile profile; a name the profile does not predefine is a user attribute.
//! - DelayedTargetValidation defers target errors to ReflectionAttribute::newInstance.
//! - Canonical names keep namespaced user attributes separate from global Core markers.

use super::Checker;
use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::{
    AttributeGroup, ClassConst, ClassMethod, ClassProperty, EnumCaseDecl, Expr, ExprKind, Program,
    StaticReceiver, StmtKind, TypeExpr,
};
use crate::source::SourceMode;
use crate::types::{collect_attribute_names, PhpType, TypeEnv};
use elephc_builtin_contract::{
    core_attribute_target_error, CORE_ATTRIBUTE_IS_REPEATABLE, CORE_ATTRIBUTE_TARGET_CLASS,
    CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT, CORE_ATTRIBUTE_TARGET_FUNCTION, CORE_ATTRIBUTE_TARGET_METHOD,
    CORE_ATTRIBUTE_TARGET_PARAMETER, CORE_ATTRIBUTE_TARGET_PROPERTY,
};

/// Returns the target mask and repeatability of an attribute the compile profile predefines.
pub(crate) fn flags(name: &str) -> Option<i64> {
    elephc_builtin_contract::core_attribute_flags(name, crate::codegen_support::compile_php_version())
}

/// Checks user declarations in source order. Preludes declare the newest surface on every profile.
pub(super) fn validate(program: &Program) -> Result<(), CompileError> {
    for stmt in program.iter().filter(|stmt| stmt.source_mode != SourceMode::Internal) {
        match &stmt.kind {
            StmtKind::FunctionDecl { param_attributes, return_type, .. } => {
                validate_attributes(&stmt.attributes, CORE_ATTRIBUTE_TARGET_FUNCTION)?;
                if has_no_discard(&stmt.attributes) && returns_nothing(return_type.as_ref()) {
                    return Err(CompileError::new(stmt.span, "NoDiscard cannot apply to a void or never function"));
                }
                for attributes in param_attributes {
                    validate_attributes(attributes, CORE_ATTRIBUTE_TARGET_PARAMETER)?;
                }
            }
            StmtKind::ClassDecl { properties, methods, constants, .. }
            | StmtKind::InterfaceDecl { properties, methods, constants, .. }
            | StmtKind::TraitDecl { properties, methods, constants, .. } => {
                validate_attributes(&stmt.attributes, CORE_ATTRIBUTE_TARGET_CLASS)?;
                validate_members(properties, methods, constants, &[])?;
            }
            StmtKind::EnumDecl { cases, methods, constants, .. } => {
                validate_attributes(&stmt.attributes, CORE_ATTRIBUTE_TARGET_CLASS)?;
                validate_members(&[], methods, constants, cases)?;
            }
            StmtKind::PackedClassDecl { .. } => {
                validate_attributes(&stmt.attributes, CORE_ATTRIBUTE_TARGET_CLASS)?;
            }
            StmtKind::NamespaceBlock { body, .. } => validate(body)?,
            _ => {}
        }
    }
    Ok(())
}

/// Checks class-like members in source order, then NoDiscard's return contract on methods.
fn validate_members(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    constants: &[ClassConst],
    cases: &[EnumCaseDecl],
) -> Result<(), CompileError> {
    // Parameters first: the stable sort then keeps an attribute kept on both copies of a promoted
    // parameter reported against the parameter, as PHP does.
    let parameters = methods.iter().flat_map(|method| &method.param_attributes);
    let mut sites: Vec<_> = parameters.map(|attributes| (attributes, CORE_ATTRIBUTE_TARGET_PARAMETER))
        .chain(methods.iter().map(|method| (&method.attributes, CORE_ATTRIBUTE_TARGET_METHOD)))
        .chain(properties.iter().map(|property| (&property.attributes, CORE_ATTRIBUTE_TARGET_PROPERTY)))
        .chain(constants.iter().map(|constant| (&constant.attributes, CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT)))
        .chain(cases.iter().map(|case| (&case.attributes, CORE_ATTRIBUTE_TARGET_CLASS_CONSTANT)))
        .collect();
    sites.sort_by_key(|(attributes, _)| attributes.first().map(|group| (group.span.line, group.span.col)));
    for (attributes, target) in sites {
        validate_attributes(attributes, target)?;
    }
    for method in methods {
        if has_no_discard(&method.attributes)
            && (returns_nothing(method.return_type.as_ref())
                || matches!(method.name.to_ascii_lowercase().as_str(), "__construct" | "__destruct"))
        {
            return Err(CompileError::new(method.span, "NoDiscard cannot apply to a void or never method"));
        }
    }
    Ok(())
}

/// Returns whether a declared return type can never yield a value.
fn returns_nothing(return_type: Option<&TypeExpr>) -> bool {
    matches!(return_type, Some(TypeExpr::Void | TypeExpr::Never))
}

/// Applies PHP's per-attribute order: the target unless deferred, then repetition anywhere in the
/// declaration's list, so a repeat is reported ahead of a later attribute's target.
fn validate_attributes(groups: &[AttributeGroup], target: i64) -> Result<(), CompileError> {
    let attributes: Vec<_> = groups.iter().flat_map(|group| &group.attributes).collect();
    let delayed = attributes.iter().any(|attribute| is_core_attribute(attribute.name.as_str(), "DelayedTargetValidation"));
    for attribute in &attributes {
        let name = attribute.name.as_str();
        let Some(flags) = flags(name) else { continue };
        if !delayed && flags & target == 0 {
            return Err(CompileError::new(attribute.span, &core_attribute_target_error(name, target, flags)));
        }
        if flags & CORE_ATTRIBUTE_IS_REPEATABLE == 0
            && attributes.iter().filter(|other| other.name.as_str().eq_ignore_ascii_case(name)).count() > 1
        {
            return Err(CompileError::new(attribute.span, &format!("Attribute \"{name}\" must not be repeated")));
        }
    }
    Ok(())
}

/// Returns whether `name` is the global Core attribute `core` under the compile profile.
fn is_core_attribute(name: &str, core: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case(core) && flags(core).is_some()
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
        if attributes.iter().any(|name| is_core_attribute(name, "NoDiscard")) {
            checker.warnings.push(CompileWarning::new(expression.span,
                &format!("The return value of {name} should either be used or explicitly ignored")));
        }
    }
}

/// Reads the same marker from AST attributes when only the declaration is available.
pub(crate) fn has_no_discard(attributes: &[AttributeGroup]) -> bool {
    attributes.iter().flat_map(|group| &group.attributes)
        .any(|attribute| is_core_attribute(attribute.name.as_str(), "NoDiscard"))
}
