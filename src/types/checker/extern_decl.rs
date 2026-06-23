//! Purpose:
//! Validates extern function declarations and their type contracts.
//! Builds FFI metadata that later codegen uses to materialize C ABI calls.
//!
//! Called from:
//! - `crate::types::checker::driver::externs`
//!
//! Key details:
//! - Extern signatures must reject unsupported PHP types and preserve ABI-visible parameter/return shapes.

use crate::errors::CompileError;
use crate::parser::ast::CType;
use crate::types::{FunctionSig, PhpType};

use super::Checker;

impl Checker {
    /// Validates an extern function declaration against PHP-side types and C ABI constraints.
    ///
    /// Checks:
    /// - Parameter names are unique within the declaration.
    /// - No parameter uses `void` C type.
    /// - PHP parameter types are supported for FFI (int, float, str, bool, resource, pointer, buffer, callable).
    /// - Integer and float register argument counts each stay within the ARM64 ABI limit of 8.
    /// - Return type is not callable, array, assoc-array, or object.
    ///
    /// Inputs:
    /// - `name`: function name for error messages.
    /// - `params`: C-side extern parameter list with names and C types.
    /// - `return_type`: C return type.
    /// - `php_params`: PHP types for each parameter (zipped with `params`).
    /// - `php_ret`: PHP return type derived from the function body.
    /// - `span`: source location for error reporting.
    ///
    /// Returns `Ok(())` if valid, `Err(CompileError)` otherwise.
    pub(crate) fn validate_extern_function_decl(
        &self,
        name: &str,
        params: &[crate::parser::ast::ExternParam],
        return_type: &CType,
        php_params: &[(String, PhpType)],
        php_ret: &PhpType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let mut seen = std::collections::HashSet::new();
        let mut int_regs = 0usize;
        let mut float_regs = 0usize;

        for (param, (_, php_ty)) in params.iter().zip(php_params.iter()) {
            if !seen.insert(param.name.clone()) {
                return Err(CompileError::new(
                    span,
                    &format!("Duplicate extern parameter: ${}", param.name),
                ));
            }
            if matches!(param.c_type, CType::Void) {
                return Err(CompileError::new(
                    span,
                    "Extern parameters cannot use type void",
                ));
            }
            match php_ty {
                PhpType::Float => float_regs += 1,
                PhpType::Str
                | PhpType::Int
                | PhpType::Bool
                | PhpType::Resource(_)
                | PhpType::Pointer(_)
                | PhpType::Buffer(_)
                | PhpType::Callable => {
                    int_regs += 1;
                }
                PhpType::Void
                | PhpType::Never
                | PhpType::Iterable
                | PhpType::Mixed
                | PhpType::Union(_)
                | PhpType::TaggedScalar
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Packed(_) => {
                    return Err(CompileError::new(
                        span,
                        &format!("Unsupported extern parameter type in {}()", name),
                    ));
                }
            }
        }

        if int_regs > 8 || float_regs > 8 {
            return Err(CompileError::new(
                span,
                &format!(
                    "Extern function '{}' exceeds supported ARM64 register ABI limits (max 8 integer and 8 float arguments)",
                    name
                ),
            ));
        }

        if matches!(return_type, CType::Callable)
            || matches!(
                php_ret,
                PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Object(_)
            )
        {
            return Err(CompileError::new(
                span,
                &format!("Extern function '{}' has an unsupported return type", name),
            ));
        }

        Ok(())
    }

    /// Validates an extern class field declaration's C type.
    ///
    /// Rejects `void` and `callable` C types on extern fields; all other C types are permitted.
    ///
    /// Inputs:
    /// - `class_name`: class name for error messages.
    /// - `field`: the extern field with its name and C type.
    /// - `span`: source location for error reporting.
    pub(crate) fn validate_extern_field_decl(
        &self,
        class_name: &str,
        field: &crate::parser::ast::ExternField,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if matches!(field.c_type, CType::Void | CType::Callable) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Extern class '{}' field ${} uses an unsupported type",
                    class_name, field.name
                ),
            ));
        }
        Ok(())
    }

    /// Validates an extern global variable declaration.
    ///
    /// Checks:
    /// - Name is not `argc` or `argv` (reserved superglobals).
    /// - No duplicate extern global with the same name already registered.
    /// - C type is not `void` or `callable`.
    ///
    /// Inputs:
    /// - `name`: variable name (without `$` prefix) for error messages.
    /// - `c_type`: the C type of the global.
    /// - `span`: source location for error reporting.
    pub(crate) fn validate_extern_global_decl(
        &self,
        name: &str,
        c_type: &CType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if name == "argc" || name == "argv" || crate::superglobals::is_superglobal(name) {
            return Err(CompileError::new(
                span,
                &format!(
                    "extern global ${} would shadow a reserved superglobal",
                    name
                ),
            ));
        }
        if self.extern_globals.contains_key(name) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate extern global declaration: ${}", name),
            ));
        }
        if matches!(c_type, CType::Void | CType::Callable) {
            return Err(CompileError::new(
                span,
                &format!("Extern global ${} uses an unsupported type", name),
            ));
        }
        Ok(())
    }

    /// Returns `true` if `ty` is a PHP type that is representable in C for callbacks.
    ///
    /// C-compatible callback types are: `int`, `float`, `bool`, `pointer`, and `void`.
    pub(crate) fn callback_type_is_c_compatible(ty: &PhpType) -> bool {
        matches!(
            ty,
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Pointer(_) | PhpType::Void
        )
    }

    /// Validates a callable descriptor signature against C callback constraints.
    ///
    /// Extern callback slots are raw function pointers, so the callable must use a fixed
    /// arity, no defaults, no by-reference parameters, and only scalar/pointer/void
    /// ABI-visible types.
    pub(crate) fn validate_callback_signature(
        sig: &FunctionSig,
        label: &str,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if sig.variadic.is_some() {
            return Err(CompileError::new(
                span,
                &format!("{} cannot be variadic", label),
            ));
        }
        if sig.defaults.iter().any(|d| d.is_some()) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use default parameters", label),
            ));
        }
        if sig.ref_params.iter().any(|is_ref| *is_ref) {
            return Err(CompileError::new(
                span,
                &format!("{} cannot use pass-by-reference parameters", label),
            ));
        }
        if !Self::callback_type_is_c_compatible(&sig.return_type) {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} uses an unsupported return type; only int, float, bool, ptr, and void are supported",
                    label
                ),
            ));
        }
        if sig
            .params
            .iter()
            .any(|(_, ty)| !Self::callback_type_is_c_compatible(ty))
        {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} uses unsupported C callback types; only int, float, bool, ptr, and void are supported",
                    label
                ),
            ));
        }
        Ok(())
    }

    /// Registers a PHP function as a C callback after validating C ABI compatibility.
    ///
    /// Validates that the function:
    /// - Is not variadic.
    /// - Has no default parameter values.
    /// - Has no pass-by-reference parameters.
    /// - Has C-compatible return and parameter types (int, float, bool, pointer, void).
    ///
    /// If the function signature is not yet resolved, resolves it first via
    /// `initial_function_param_types` and `resolve_function_signature`.
    ///
    /// Inputs:
    /// - `callback_name`: name of the function to register as a callback.
    /// - `span`: source location for error reporting.
    ///
    /// Returns `Ok(())` if compatible, `Err(CompileError)` if validation fails.
    pub(crate) fn register_callback_function(
        &mut self,
        callback_name: &str,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let canonical_callback_name = self
            .canonical_function_name_folded(callback_name)
            .unwrap_or_else(|| callback_name.to_string());
        let decl = self
            .fn_decls
            .get(&canonical_callback_name)
            .cloned()
            .ok_or_else(|| {
                CompileError::new(
                    span,
                    &format!("Undefined callback function: {}", callback_name),
                )
            })?;

        if decl.variadic.is_some() {
            return Err(CompileError::new(
                span,
                &format!("Callback function '{}' cannot be variadic", callback_name),
            ));
        }
        if decl.defaults.iter().any(|d| d.is_some()) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Callback function '{}' cannot use default parameters",
                    callback_name
                ),
            ));
        }
        if decl.ref_params.iter().any(|is_ref| *is_ref) {
            return Err(CompileError::new(
                span,
                &format!(
                    "Callback function '{}' cannot use pass-by-reference parameters",
                    callback_name
                ),
            ));
        }
        if let Some(sig) = self.functions.get(&canonical_callback_name) {
            if !Self::callback_type_is_c_compatible(&sig.return_type) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Callback function '{}' uses an unsupported return type; only int, float, bool, ptr, and void are supported",
                        callback_name
                    ),
                ));
            }
            if sig
                .params
                .iter()
                .any(|(_, ty)| !Self::callback_type_is_c_compatible(ty))
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Callback function '{}' uses unsupported C callback types; only int, float, bool, ptr, and void are supported",
                        callback_name
                    ),
                ));
            }
        } else {
            let param_types = self.initial_function_param_types(&canonical_callback_name, &decl)?;
            self.resolve_function_signature(&canonical_callback_name, &decl, param_types)?;
            let sig = self.functions.get(&canonical_callback_name).cloned().ok_or_else(|| {
                CompileError::new(
                    span,
                    &format!("Undefined callback function: {}", callback_name),
                )
            })?;
            if !Self::callback_type_is_c_compatible(&sig.return_type) {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Callback function '{}' uses an unsupported return type; only int, float, bool, ptr, and void are supported",
                        callback_name
                    ),
                ));
            }
            if sig
                .params
                .iter()
                .any(|(_, ty)| !Self::callback_type_is_c_compatible(ty))
            {
                return Err(CompileError::new(
                    span,
                    &format!(
                        "Callback function '{}' uses unsupported C callback types; only int, float, bool, ptr, and void are supported",
                        callback_name
                    ),
                ));
            }
        }

        let _ = decl;
        Ok(())
    }
}
