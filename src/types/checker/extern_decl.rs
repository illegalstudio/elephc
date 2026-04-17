use crate::errors::CompileError;
use crate::parser::ast::CType;
use crate::types::PhpType;

use super::Checker;

impl Checker {
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
                | PhpType::Pointer(_)
                | PhpType::Buffer(_)
                | PhpType::Callable => {
                    int_regs += 1;
                }
                PhpType::Void
                | PhpType::Mixed
                | PhpType::Union(_)
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

    pub(crate) fn validate_extern_global_decl(
        &self,
        name: &str,
        c_type: &CType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if name == "argc" || name == "argv" {
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

    pub(crate) fn callback_type_is_c_compatible(ty: &PhpType) -> bool {
        matches!(
            ty,
            PhpType::Int | PhpType::Float | PhpType::Bool | PhpType::Pointer(_) | PhpType::Void
        )
    }

    pub(crate) fn register_callback_function(
        &mut self,
        callback_name: &str,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        let decl = self.fn_decls.get(callback_name).cloned().ok_or_else(|| {
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
        if let Some(sig) = self.functions.get(callback_name) {
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
            let param_types = self.initial_function_param_types(callback_name, &decl)?;
            self.resolve_function_signature(callback_name, &decl, param_types)?;
            let sig = self.functions.get(callback_name).cloned().ok_or_else(|| {
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
