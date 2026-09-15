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
//! - Arity and variadic rules are checked against the SOURCE-visible signature: the
//!   `func_args` pass may have appended its hidden collector to any method (every frame gets
//!   one as soon as the program contains an `eval()`), and that synthetic parameter must not
//!   make a contract-compliant magic method look variadic or over-long.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, TypeExpr, Visibility};
use crate::types::PhpType;

use super::super::Checker;

/// Returns the parameters the source actually declared for `method`.
///
/// Only `crate::func_args::HIDDEN_ARGS_PARAM` is filtered out. Every other parameter, including
/// a user-declared variadic, stays visible so the contract rules below keep rejecting it.
fn source_param_count(method: &ClassMethod) -> usize {
    method
        .params
        .iter()
        .filter(|(name, ..)| name != crate::func_args::HIDDEN_ARGS_PARAM)
        .count()
}

/// Returns whether the source declared a variadic parameter on `method`.
///
/// The `func_args` collector occupies the same AST slot as a source variadic, so comparing the
/// name is the only way to tell a PHP-visible `...$args` from the generated one.
fn declares_source_variadic(method: &ClassMethod) -> bool {
    method
        .variadic
        .as_deref()
        .is_some_and(|variadic| variadic != crate::func_args::HIDDEN_ARGS_PARAM)
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
                    if source_param_count(method) != 0 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 1 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 2 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 1 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 1 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 2 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 0 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 0 || declares_source_variadic(method) {
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
                    if source_param_count(method) != 2 || declares_source_variadic(method) {
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
