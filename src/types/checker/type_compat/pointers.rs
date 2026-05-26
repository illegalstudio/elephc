//! Purpose:
//! Checks type compatibility for pointers cases.
//! Supports the central assignability predicate used by declarations, calls, returns, and assignments.
//!
//! Called from:
//! - `crate::types::checker::type_compat`
//!
//! Key details:
//! - Rules here define accepted programs, so PHP covariance, inheritance, and extension-specific constraints must stay explicit.

use crate::errors::CompileError;
use crate::types::{packed_type_size, PhpType};

use super::super::Checker;

impl Checker {
    /// Returns true if `ty` is a `PhpType::Pointer`.
    pub(crate) fn is_pointer_type(ty: &PhpType) -> bool {
        matches!(ty, PhpType::Pointer(_))
    }

    /// Returns true if both `left` and `right` are `PhpType::Pointer` (any target).
    /// Pointers of different target types are considered compatible at this level.
    pub(crate) fn pointer_types_compatible(left: &PhpType, right: &PhpType) -> bool {
        matches!((left, right), (PhpType::Pointer(_), PhpType::Pointer(_)))
    }

    /// Normalizes a pointer target type name string to its canonical form.
    /// Maps PHP aliases (int/integer, float/double/real, bool/boolean, ptr/pointer) to
    /// their canonical equivalents, validates class/packed/extern names, and returns
    /// `None` for unknown types.
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

    /// Resolves a `TypeExpr` AST node to a `PhpType`. Handles all primitive types, nullable,
    /// union, pointer, buffer, and named (class/interface/packed/extern) type expressions.
    /// Validates that buffer element types are POD-sized.
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
            crate::parser::ast::TypeExpr::Never => Ok(PhpType::Never),
            crate::parser::ast::TypeExpr::Iterable => Ok(PhpType::Iterable),
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
            crate::parser::ast::TypeExpr::Named(name) => {
                let name_str = name.as_str();
                match name_str.to_ascii_lowercase().as_str() {
                    "string" => Ok(PhpType::Str),
                    "mixed" => Ok(PhpType::Mixed),
                    "callable" => Ok(PhpType::Callable),
                    "void" => Ok(PhpType::Void),
                    "array" => Ok(PhpType::Array(Box::new(PhpType::Mixed))),
                    _ if self.classes.contains_key(name_str)
                        || self.declared_classes.contains(name_str)
                        || self.interfaces.contains_key(name_str)
                        || self.declared_interfaces.contains(name_str)
                        || self.extern_classes.contains_key(name_str) =>
                    {
                        Ok(PhpType::Object(name_str.to_string()))
                    }
                    _ if self.packed_classes.contains_key(name_str) => {
                        Ok(PhpType::Packed(name_str.to_string()))
                    }
                    _ => Err(CompileError::new(
                        span,
                        &format!("Unknown type: {}", name_str),
                    )),
                }
            },
        }
    }

    /// Looks up the `PhpType` of an `extern` class field by `class_name` and `field_name`.
    /// Returns `None` if the class or field does not exist.
    pub(crate) fn extern_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.extern_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    /// Looks up the `PhpType` of a `packed class` field by `class_name` and `field_name`.
    /// Returns `None` if the class or field does not exist.
    pub(crate) fn packed_field_type(&self, class_name: &str, field_name: &str) -> Option<PhpType> {
        self.packed_classes.get(class_name).and_then(|class_info| {
            class_info
                .fields
                .iter()
                .find(|field| field.name == field_name)
                .map(|field| field.php_type.clone())
        })
    }

    /// Validates that `ty` is a pointer type. Emits an error with `context` if not.
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

    /// Validates that `ty` is a valid word-pointer value for `ptr_set()`: must be `Int`,
    /// `Bool`, `Void`, or `Pointer`. Emits a specific error otherwise.
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
