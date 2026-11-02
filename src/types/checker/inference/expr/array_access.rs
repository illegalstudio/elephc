//! Purpose:
//! Infers array-offset result types for reads and recursively writable parent chains.
//!
//! Called from:
//! - Basic expression inference and nested assignment validation.
//!
//! Key details:
//! - Read validation remains shared across arrays, strings, buffers and ArrayAccess.
//! - False/null promotion is limited to the write spine, never index expressions.

use super::{is_valid_string_offset_index, Checker};
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{normalized_array_key_type, PhpType, TypeEnv};

impl Checker {
    /// Infers an ordinary offset read without granting write-context conversions.
    pub(super) fn infer_array_access_type(
        &mut self, expr: &Expr, env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let ExprKind::ArrayAccess { array, index } = &expr.kind else { unreachable!() };
        let arr_ty = self.infer_type(array, env)?;
        let idx_ty = self.infer_type(index, env)?;
        self.infer_array_element_type(array, index, expr, arr_ty, idx_ty, false)
    }

    /// Checks writable parents recursively while keeping selector effects in ordinary read context.
    pub(crate) fn infer_nested_write_parent_with_effects(
        &mut self, expr: &Expr, env: &mut TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let ExprKind::ArrayAccess { array, index } = &expr.kind else {
            return self.infer_type_with_assignment_effects(expr, env);
        };
        let arr_ty = self.infer_nested_write_parent_with_effects(array, env)?;
        let idx_ty = self.infer_type_with_assignment_effects(index, env)?;
        self.infer_array_element_type(array, index, expr, arr_ty, idx_ty, true)
    }

    /// Applies the common offset contract after the receiver and selector have been inferred.
    fn infer_array_element_type(
        &mut self, array: &Expr, index: &Expr, expr: &Expr,
        arr_ty: PhpType, idx_ty: PhpType, for_write: bool,
    ) -> Result<PhpType, CompileError> {
        let buffer = matches!(arr_ty, PhpType::Buffer(_));
        self.buffer_read_observations.entry(expr.span)
            .and_modify(|previous| *previous &= buffer)
            .or_insert(buffer);
        let normalized_idx_ty = normalized_array_key_type(index, idx_ty.clone());
        match &arr_ty {
            PhpType::Str => {
                if !is_valid_string_offset_index(index, &idx_ty) {
                    return Err(CompileError::new(
                        expr.span,
                        "String index must be integer",
                    ));
                }
                Ok(PhpType::Str)
            }
            PhpType::Array(elem_ty) => {
                if normalized_idx_ty != PhpType::Int {
                    // PHP allows string keys on indexed arrays: the array
                    // promotes to hash at runtime. Return the element type
                    // widened to Mixed so ?? / isset / reads type-check.
                    Ok(PhpType::Mixed)
                } else if **elem_ty == PhpType::Never {
                    // Never is the element type of an empty indexed array, not a non-returning
                    // read. Nested write-back may still autovivify that selected slot.
                    Ok(PhpType::Void)
                } else {
                    Ok(*elem_ty.clone())
                }
            }
            PhpType::AssocArray { value, .. } => {
                // Assoc arrays accept string or int keys
                Ok(*value.clone())
            }
            PhpType::Object(class_name) => {
                if self.object_type_implements_interface(class_name, "ArrayAccess") {
                    Ok(self.array_access_offset_get_type(class_name))
                } else {
                    Err(CompileError::new(expr.span, "Cannot index non-array"))
                }
            }
            PhpType::Union(members) => {
                let mut result_members = Vec::new();
                let mut saw_indexable_member = false;
                let mut first_index_error = None;
                for member in members {
                    match member {
                        PhpType::Void => result_members.push(PhpType::Void),
                        PhpType::Str => {
                            saw_indexable_member = true;
                            if !is_valid_string_offset_index(index, &idx_ty) {
                                first_index_error =
                                    first_index_error.or(Some("String index must be integer"));
                                continue;
                            }
                            result_members.push(PhpType::Str);
                        }
                        PhpType::Array(elem_ty) => {
                            saw_indexable_member = true;
                            if normalized_idx_ty != PhpType::Int {
                                // String key on indexed array: PHP promotes
                                // to hash at runtime; element may be Mixed.
                                result_members.push(PhpType::Mixed);
                            } else {
                                result_members.push(*elem_ty.clone());
                            }
                        }
                        PhpType::AssocArray { value, .. } => {
                            saw_indexable_member = true;
                            result_members.push(*value.clone());
                        }
                        PhpType::Object(class_name) => {
                            if self.object_type_implements_interface(
                                class_name,
                                "ArrayAccess",
                            ) {
                                saw_indexable_member = true;
                                result_members
                                    .push(self.array_access_offset_get_type(class_name));
                            }
                        }
                        PhpType::Buffer(elem_ty) => {
                            saw_indexable_member = true;
                            if !matches!(idx_ty, PhpType::Int | PhpType::Mixed)
                                && !idx_ty.is_int_float_union()
                            {
                                first_index_error =
                                    first_index_error.or(Some("Buffer index must be integer"));
                                continue;
                            }
                            match elem_ty.as_ref() {
                                PhpType::Packed(name) => {
                                    result_members.push(PhpType::Pointer(Some(name.clone())))
                                }
                                other => result_members.push(other.storage_value_type()),
                            }
                        }
                        _ => {}
                    }
                }
                let has_concrete_result =
                    result_members.iter().any(|member| *member != PhpType::Void);
                if !has_concrete_result && saw_indexable_member {
                    Err(CompileError::new(
                        expr.span,
                        first_index_error.unwrap_or("Cannot index non-array"),
                    ))
                } else if result_members.is_empty() {
                    Err(CompileError::new(expr.span, "Cannot index non-array"))
                } else {
                    Ok(self.normalize_union_type(result_members))
                }
            }
            PhpType::Buffer(elem_ty) => {
                if !matches!(idx_ty, PhpType::Int | PhpType::Mixed)
                    && !idx_ty.is_int_float_union()
                {
                    return Err(CompileError::new(
                        expr.span,
                        "Buffer index must be integer",
                    ));
                }
                // A width-declared element (`buffer<i32>`, `buffer<f32>`) reads as its value
                // type; the storage width only matters to the lowering.
                match elem_ty.as_ref() {
                    PhpType::Packed(name) => Ok(PhpType::Pointer(Some(name.clone()))),
                    other => Ok(other.storage_value_type()),
                }
            }
            // Mixed receivers fall through to runtime dispatch. The
            // boxed payload may carry an indexed array, an assoc
            // hash, or a stdClass; codegen unboxes and routes to
            // the right runtime helper. Missing keys decode to
            // `Mixed(null)` at runtime, mirroring PHP's silent
            // "undefined index" warning behavior for this very
            // common idiom (e.g. `json_decode($json, true)["k"]`).
            PhpType::Mixed => Ok(PhpType::Mixed),
            // Only a writable chain may promote false/null at an already selected array slot.
            // Index expressions and ordinary reads never inherit this allowance.
            PhpType::Bool | PhpType::False | PhpType::Void
                if for_write && matches!(array.kind, ExprKind::ArrayAccess { .. }) =>
            {
                Ok(PhpType::Mixed)
            }
            // `isset($n['k'])` / `$n['k'] ?? $d` reach through a null base in PHP and
            // answer `false` / the default; only a probe context may do so.
            PhpType::Void if self.null_probe_depth > 0 => Ok(PhpType::Void),
            _ => Err(CompileError::new(expr.span, "Cannot index non-array")),
        }
    }
}
