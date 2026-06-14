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
use crate::parser::ast::Visibility;
use crate::types::PhpType;

use super::super::Checker;

/// Patches the type signatures for the property/method interception magic
/// methods on user-declared classes to enforce PHP-correct parameter types.
///
/// For `__get`/`__isset`/`__unset`: parameter 0 is `PhpType::Str` (the property name).
/// For `__set`: parameter 0 is `PhpType::Str`, parameter 1 is `PhpType::Mixed`.
/// For `__call`/`__callStatic`: parameter 0 is `PhpType::Str`, parameter 1 is
/// `PhpType::Array` of `PhpType::Never` (the forwarded argument list).
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
/// `__isset`, `__unset`, `__call`, `__callStatic`, `__invoke`, `__destruct`)
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
                magic @ ("__isset" | "__unset") => {
                    // `isset($obj->prop)`/`unset($obj->prop)` on an undeclared
                    // property dispatch here; both take the property name only.
                    let pretty = if magic == "__isset" { "__isset" } else { "__unset" };
                    if method.is_static {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be non-static: {}::{}", class_name, pretty),
                        ));
                        continue;
                    }
                    if method.visibility != Visibility::Public {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must be public: {}::{}", class_name, pretty),
                        ));
                        continue;
                    }
                    if method.params.len() != 1 || method.variadic.is_some() {
                        errors.push(CompileError::new(
                            method.span,
                            &format!("Magic method must take 1 argument: {}::{}", class_name, pretty),
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
