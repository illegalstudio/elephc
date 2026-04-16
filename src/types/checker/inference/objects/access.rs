use crate::errors::CompileError;
use crate::parser::ast::Expr;
use crate::types::{PhpType, TypeEnv};

use super::super::super::Checker;

impl Checker {
    pub(crate) fn infer_property_access_type(
        &mut self,
        object: &Expr,
        property: &str,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let obj_ty = self.infer_type(object, env)?;
        if let PhpType::Object(class_name) = &obj_ty {
            if let Some(class_info) = self.classes.get(class_name) {
                if let Some(visibility) = class_info.property_visibilities.get(property) {
                    let declaring_class = class_info
                        .property_declaring_classes
                        .get(property)
                        .map(String::as_str)
                        .unwrap_or(class_name);
                    if !self.can_access_member(declaring_class, visibility) {
                        return Err(CompileError::new(
                            expr.span,
                            &format!(
                                "Cannot access {} property: {}::{}",
                                Self::visibility_label(visibility),
                                class_name,
                                property
                            ),
                        ));
                    }
                }
                if let Some((_, ty)) = class_info.properties.iter().find(|(n, _)| n == property) {
                    return Ok(ty.clone());
                }
                if let Some(sig) = class_info.methods.get("__get") {
                    return Ok(sig.return_type.clone());
                }
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined property: {}::{}", class_name, property),
                ));
            }
        }
        if let PhpType::Pointer(Some(class_name)) = &obj_ty {
            if let Some(field_ty) = self.extern_field_type(class_name, property) {
                return Ok(field_ty);
            }
            if let Some(field_ty) = self.packed_field_type(class_name, property) {
                return Ok(field_ty);
            }
            if self.extern_classes.contains_key(class_name) {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined extern field: {}::{}", class_name, property),
                ));
            }
            if self.packed_classes.contains_key(class_name) {
                return Err(CompileError::new(
                    expr.span,
                    &format!("Undefined packed field: {}::{}", class_name, property),
                ));
            }
        }
        Err(CompileError::new(
            expr.span,
            "Property access requires an object or typed pointer",
        ))
    }

    pub(crate) fn infer_this_type(&mut self, expr: &Expr) -> Result<PhpType, CompileError> {
        if self.current_method_is_static {
            return Err(CompileError::new(
                expr.span,
                "Cannot use $this inside a static method",
            ));
        }
        if let Some(class_name) = &self.current_class {
            Ok(PhpType::Object(class_name.clone()))
        } else {
            Err(CompileError::new(
                expr.span,
                "Cannot use $this outside of a class method",
            ))
        }
    }

    pub(crate) fn infer_ptr_cast_type(
        &mut self,
        target_type: &str,
        inner: &Expr,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let inner_ty = self.infer_type(inner, env)?;
        self.ensure_pointer_type(&inner_ty, expr.span, "ptr_cast()")?;
        let normalized = self
            .normalize_pointer_target_type(target_type)
            .ok_or_else(|| {
                CompileError::new(
                    expr.span,
                    &format!("Unknown ptr_cast target type: {}", target_type),
                )
            })?;
        Ok(PhpType::Pointer(Some(normalized)))
    }
}
