//! Purpose:
//! Builds and patches checker metadata for PHP builtin magic methods types.
//! Supplies synthetic declarations or contract validation for classes and interfaces that user code may reference.
//!
//! Called from:
//! - `crate::types::checker::builtin_types`
//! - `crate::types::checker::driver::init`
//!
//! Key details:
//! - Dummy AST members carry type contracts only; runtime behavior is implemented elsewhere.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, Stmt, StmtKind, TypeExpr, Visibility};
use crate::types::PhpType;

use super::super::Checker;

/// One fatal PHP contract violation on a user-declared `__set_state()` method.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SetStateContractViolation {
    Arity,
    NonStatic,
    ByReference,
    ParameterType { name: String },
    ReturnType,
}

/// Returns the first declaration-time `__set_state()` violation in php-src order.
pub(crate) fn set_state_contract_violation(
    method: &ClassMethod,
) -> Option<SetStateContractViolation> {
    if method.params.len() != 1 {
        return Some(SetStateContractViolation::Arity);
    }
    if method.params.iter().any(|(_, _, _, by_ref)| *by_ref) {
        return Some(SetStateContractViolation::ByReference);
    }
    if !method.is_static {
        return Some(SetStateContractViolation::NonStatic);
    }
    let (name, parameter_type, _, _) = &method.params[0];
    if parameter_type
        .as_ref()
        .is_some_and(|parameter_type| !set_state_parameter_accepts_array(parameter_type))
    {
        return Some(SetStateContractViolation::ParameterType { name: name.clone() });
    }
    if method
        .return_type
        .as_ref()
        .is_some_and(|return_type| !set_state_return_is_object(return_type))
    {
        return Some(SetStateContractViolation::ReturnType);
    }
    None
}

/// Finds the first class-like declaration whose `__set_state()` contract PHP rejects fatally.
pub(crate) fn set_state_contract_error(
    statements: &[Stmt],
) -> Option<(String, u32, SetStateContractViolation)> {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. } => {
                for method in methods {
                    if php_symbol_key(&method.name) == "__set_state" {
                        if let Some(violation) = set_state_contract_violation(method) {
                            return Some((name.clone(), method.span.line, violation));
                        }
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::Synthetic(body) => {
                if let Some(invalid) = set_state_contract_error(body) {
                    return Some(invalid);
                }
            }
            _ => {}
        }
    }
    None
}

/// Collects valid non-public `__set_state()` declarations that php-src warns about.
pub(crate) fn set_state_visibility_warnings(statements: &[Stmt]) -> Vec<(String, u32)> {
    let mut warnings = Vec::new();
    collect_set_state_visibility_warnings(statements, &mut warnings);
    warnings
}

/// Recursively appends php-src-visible `__set_state()` visibility warnings in source order.
fn collect_set_state_visibility_warnings(
    statements: &[Stmt],
    warnings: &mut Vec<(String, u32)>,
) {
    for statement in statements {
        match &statement.kind {
            StmtKind::ClassDecl { name, methods, .. }
            | StmtKind::InterfaceDecl { name, methods, .. }
            | StmtKind::TraitDecl { name, methods, .. } => {
                for method in methods {
                    if php_symbol_key(&method.name) == "__set_state"
                        && set_state_reaches_visibility_check(method)
                        && method.visibility != Visibility::Public
                    {
                        warnings.push((name.clone(), method.span.line));
                    }
                }
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::Synthetic(body) => {
                collect_set_state_visibility_warnings(body, warnings);
            }
            _ => {}
        }
    }
}

/// Returns whether php-src reaches `__set_state()`'s non-fatal visibility check.
fn set_state_reaches_visibility_check(method: &ClassMethod) -> bool {
    method.params.len() == 1
        && !method.params.iter().any(|(_, _, _, by_ref)| *by_ref)
        && method.is_static
}

/// Returns whether a declared parameter type accepts every PHP array value.
fn set_state_parameter_accepts_array(parameter_type: &TypeExpr) -> bool {
    match parameter_type {
        TypeExpr::Array(_) | TypeExpr::Iterable => true,
        TypeExpr::Nullable(inner) => set_state_parameter_accepts_array(inner),
        TypeExpr::Union(members) => members.iter().any(set_state_parameter_accepts_array),
        TypeExpr::Named(name) => matches!(
            name.as_str().trim_start_matches('\\').to_ascii_lowercase().as_str(),
            "array" | "iterable" | "mixed"
        ),
        _ => false,
    }
}

/// Returns whether a declared return type is covariant with php-src's `object` contract.
fn set_state_return_is_object(return_type: &TypeExpr) -> bool {
    match return_type {
        TypeExpr::Never => true,
        TypeExpr::Named(name) => !matches!(
            name.as_str().trim_start_matches('\\').to_ascii_lowercase().as_str(),
            "array"
                | "bool"
                | "callable"
                | "false"
                | "float"
                | "int"
                | "iterable"
                | "mixed"
                | "null"
                | "resource"
                | "string"
                | "true"
                | "void"
        ),
        TypeExpr::Union(members) | TypeExpr::Intersection(members) => {
            !members.is_empty() && members.iter().all(set_state_return_is_object)
        }
        _ => false,
    }
}

/// Patches the type signatures for the property/method interception magic
/// methods on user-declared classes to enforce PHP-correct parameter types.
///
/// For `__get`/`__isset`/`__unset`: parameter 0 is `PhpType::Str` (the property name).
/// For `__set`: parameter 0 is `PhpType::Str`, parameter 1 is `PhpType::Mixed`.
/// For `__call`/`__callStatic`: parameter 0 is `PhpType::Str`, parameter 1 is
/// `PhpType::Array` of `PhpType::Never` (the forwarded argument list).
/// Declared `__isset`/`__unset` return types are validated separately.
/// Does nothing for classes that do not declare these methods.
pub(crate) fn patch_magic_method_signatures(checker: &mut Checker) {
    for class_info in checker.classes.values_mut() {
        for name in ["__get", "__isset", "__unset"] {
            if let Some(sig) = class_info.methods.get_mut(name) {
                if let Some(param) = sig.params.get_mut(0) {
                    param.1 = PhpType::Str;
                }
            }
        }
        if let Some(sig) = class_info.methods.get_mut("__set") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
            if let Some(param) = sig.params.get_mut(1) {
                param.1 = PhpType::Mixed;
            }
        }
        if let Some(sig) = class_info.methods.get_mut("__isset") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
        }
        if let Some(sig) = class_info.methods.get_mut("__unset") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
        }
        if let Some(sig) = class_info.methods.get_mut("__call") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
            if let Some(param) = sig.params.get_mut(1) {
                param.1 = PhpType::Array(Box::new(PhpType::Never));
            }
        }
        // `__callStatic` is a static method, so it lives in `static_methods`.
        if let Some(sig) = class_info.static_methods.get_mut("__callstatic") {
            if let Some(param) = sig.params.get_mut(0) {
                param.1 = PhpType::Str;
            }
            if let Some(param) = sig.params.get_mut(1) {
                param.1 = PhpType::Array(Box::new(PhpType::Never));
            }
        }
    }
}

/// Validates that user-declared magic methods (`__toString`, `__get`, `__set`,
/// `__isset`, `__unset`, `__call`, `__callStatic`, `__invoke`, `__clone`,
/// `__destruct`)
/// conform to PHP's static/non-static, visibility, arity, and return-type rules.
///
/// Returns `Ok(())` if all declared magic methods are contract-compliant.
/// Returns `Err(CompileError)` with all violations collected if any class fails.
pub(crate) fn validate_magic_method_contracts(checker: &Checker) -> Result<(), CompileError> {
    let mut errors = Vec::new();
    for (class_name, class_info) in &checker.classes {
        for method in &class_info.method_decls {
            match php_symbol_key(&method.name).as_str() {
                "__tostring" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must be non-static: {}::__toString",
                                class_name
                            ),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__toString", class_name),
                        ));
                        continue;
                    }
                    if !method.params.is_empty() || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must take 0 arguments: {}::__toString",
                                class_name
                            ),
                        ));
                        continue;
                    }
                    if class_info
                        .methods
                        .get("__tostring")
                        .map(|sig| sig.return_type.clone())
                        != Some(PhpType::Str)
                    {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must return string: {}::__toString",
                                class_name
                            ),
                        ));
                    }
                }
                "__get" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__get", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__get", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 1 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 1 argument: {}::__get", class_name),
                        ));
                    }
                }
                "__set" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__set", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__set", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 2 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 2 arguments: {}::__set", class_name),
                        ));
                    }
                }
                "__isset" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__isset", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__isset", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 1 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 1 argument: {}::__isset", class_name),
                        ));
                        continue;
                    }
                    if method
                        .return_type
                        .as_ref()
                        .is_some_and(|return_type| !matches!(return_type, TypeExpr::Bool))
                    {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must return bool: {}::__isset", class_name),
                        ));
                    }
                }
                "__unset" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__unset", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__unset", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 1 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 1 argument: {}::__unset", class_name),
                        ));
                        continue;
                    }
                    if method
                        .return_type
                        .as_ref()
                        .is_some_and(|return_type| !matches!(return_type, TypeExpr::Void))
                    {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must return void: {}::__unset", class_name),
                        ));
                    }
                }
                "__call" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__call", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__call", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 2 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 2 arguments: {}::__call", class_name),
                        ));
                    }
                }
                "__invoke" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__invoke", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__invoke", class_name),
                        ));
                    }
                }
                "__clone" => {
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::__clone", class_name),
                        ));
                        continue;
                    }
                    if !method.params.is_empty() || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 0 arguments: {}::__clone", class_name),
                        ));
                        continue;
                    }
                    if method
                        .return_type
                        .as_ref()
                        .is_some_and(|return_type| !matches!(return_type, TypeExpr::Void))
                    {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must return void: {}::__clone", class_name),
                        ));
                    }
                }
                "__destruct" => {
                    // PHP permits any visibility for __destruct (the engine calls
                    // it regardless), so only the non-static and zero-argument
                    // rules are enforced here.
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must be non-static: {}::__destruct",
                                class_name
                            ),
                        ));
                        continue;
                    }
                    if !method.params.is_empty() || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must take 0 arguments: {}::__destruct",
                                class_name
                            ),
                        ));
                    }
                }
                "__callstatic" => {
                    // Unlike the other interception hooks, `__callStatic` must be
                    // declared `public static` (PHP invokes it in a static context).
                    if !method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be static: {}::__callStatic", class_name),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::__callStatic", class_name),
                        ));
                        continue;
                    }
                    if method.params.len() != 2 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!(
                                "Magic method must take 2 arguments: {}::__callStatic",
                                class_name
                            ),
                        ));
                    }
                }
                _ => {}
            }
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(CompileError::from_many(errors))
    }
}
