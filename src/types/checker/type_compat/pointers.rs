use crate::errors::CompileError;
use crate::types::{packed_type_size, PhpType};

use super::super::Checker;

impl Checker {
    pub(crate) fn is_pointer_type(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Pointer(_))
    }

    pub(crate) fn pointer_types_compatible(left: &PhpType, right: &PhpType) -> bool {
        matches!((left, right), (PhpType::Pointer(_), PhpType::Pointer(_)))
    }

    pub(crate) fn normalize_pointer_target_type(&self, target_type: &str) -> Option<String> {
        match target_type {
            "int" | "integer" => Some("int".to_string()),
            "float" | "double" | "real" => Some("float".to_string()),
            "bool" | "boolean" => Some("bool".to_string()),
            "string" => Some("string".to_string()),
            "ptr" | "pointer" => Some("ptr".to_string()),
            class_name if self.classes.contains_key(class_name) => Some(class_name.to_string()),
            class_name if self.packed_classes.contains_key(class_name) => {
                Some(class_name.to_string())
            }
            class_name if self.extern_classes.contains_key(class_name) => {
                Some(class_name.to_string())
            }
            _ => None,
        }
    }

    pub(crate) fn resolve_type_expr(
        &self,
        type_expr: &crate::parser::ast::TypeExpr,
        span: crate::span::Span,
    ) -> Result<PhpType, CompileError> {
        match type_expr {
            crate::parser::ast::TypeExpr::Int => Ok(PhpType::Int),
            crate::parser::ast::TypeExpr::Float => Ok(PhpType::Float),
            crate::parser::ast::TypeExpr::Bool => Ok(PhpType::Bool),
            crate::parser::ast::TypeExpr::Str => Ok(PhpType::Str),
            crate::parser::ast::TypeExpr::Void => Ok(PhpType::Void),
            crate::parser::ast::TypeExpr::Nullable(inner) => {
                let inner_ty = self.resolve_type_expr(inner, span)?;
                Ok(self.normalize_union_type(vec![inner_ty, PhpType::Void]))
            }
            crate::parser::ast::TypeExpr::Union(members) => {
                let resolved = members
                    .iter()
                    .map(|member| self.resolve_type_expr(member, span))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.normalize_union_type(resolved))
            }
            crate::parser::ast::TypeExpr::Ptr(target) => {
                let normalized = match target {
                    Some(name) => self
                        .normalize_pointer_target_type(name.as_str())
                        .ok_or_else(|| {
                            CompileError::new(
                                span,
                                &format!("Unknown pointer target type: {}", name.as_str()),
                            )
                        })?,
                    None => return Ok(PhpType::Pointer(None)),
                };
                Ok(PhpType::Pointer(Some(normalized)))
            }
            crate::parser::ast::TypeExpr::Buffer(inner) => {
                let inner_ty = self.resolve_type_expr(inner, span)?;
                if packed_type_size(&inner_ty, &self.packed_classes).is_none() {
                    return Err(CompileError::new(
                        span,
                        "buffer<T> requires a POD scalar, pointer, or packed class element type",
                    ));
                }
                Ok(PhpType::Buffer(Box::new(inner_ty)))
            }
            crate::parser::ast::TypeExpr::Named(name) => match name.as_str() {
                "string" => Ok(PhpType::Str),
                "mixed" => Ok(PhpType::Mixed),
                "callable" => Ok(PhpType::Callable),
                "void" => Ok(PhpType::Void),
                "array" => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
                _ if self.classes.contains_key(name.as_str())
                    || self.declared_classes.contains(name.as_str())
                    || self.interfaces.contains_key(name.as_str())
                    || self.declared_interfaces.contains(name.as_str())
                    || self.extern_classes.contains_key(name.as_str()) =>
                {
                    Ok(PhpType::Object(name.as_str().to_string()))
                }
                _ if self.packed_classes.contains_key(name.as_str()) => {
                    Ok(PhpType::Packed(name.as_str().to_string()))
                }
                _ => Err(CompileError::new(
                    span,
                    &format!("Unknown type: {}", name.as_str()),
                )),
            },
        }
    }

    pub(crate) fn extern_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.extern_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    pub(crate) fn packed_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.packed_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    pub(crate) fn ensure_pointer_type(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
        context: &str,
    ) -> Result<(), CompileError> {
        if Self::is_pointer_type(ty) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                &format!("{} requires a pointer argument", context),
            ))
        }
    }

    pub(crate) fn ensure_word_pointer_value(
        &self,
        ty: &PhpType,
        span: crate::span::Span,
    ) -> Result<(), CompileError> {
        if matches!(
            ty,
            PhpType::Int | PhpType::Bool | PhpType::Void | PhpType::Pointer(_)
        ) {
            Ok(())
        } else {
            Err(CompileError::new(
                span,
                "ptr_set() value must be int, bool, null, or pointer",
            ))
        }
    }
}
