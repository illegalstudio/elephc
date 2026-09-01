//! Purpose:
//! Infers primitive, array, branching, cast, and unary expression types.
//!
//! Called from:
//! - "super::Checker::infer_type()" for the basic expression family.
//!
//! Key details:
//! - Preserves PHP array widening, nullability, match joins, and string-offset validation.

use super::{
    is_valid_string_offset_index, merge_match_arm_result_type, Checker,
};
use crate::errors::CompileError;
use crate::parser::ast::{Expr, ExprKind};
use crate::types::{merge_array_key_types, normalized_array_key_type, PhpType, TypeEnv};

impl Checker {
    /// Infers types for literals, variables, arrays, branches, unary operators, and casts.
    pub(super) fn infer_basic_expr_type(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::IncludeValue { .. } => unreachable!(
                "ExprKind::IncludeValue must be expanded by the resolver"
            ),
            ExprKind::BoolLiteral(false) => Ok(PhpType::False),
            ExprKind::BoolLiteral(true) => Ok(PhpType::Bool),
            ExprKind::Null => Ok(PhpType::Void),
            ExprKind::StringLiteral(_) => Ok(PhpType::Str),
            ExprKind::IntLiteral(_) => Ok(PhpType::Int),
            ExprKind::FloatLiteral(_) => Ok(PhpType::Float),
            ExprKind::Variable(name) => self.variable_type_or_eval_dynamic(name, expr.span, env),
            ExprKind::Negate(inner) => {
                let ty = self.infer_type(inner, env)?;
                match ty {
                    PhpType::Int => {
                        if matches!(&inner.kind, ExprKind::IntLiteral(_)) {
                            Ok(PhpType::Int)
                        } else {
                            Ok(PhpType::Mixed)
                        }
                    }
                    PhpType::Float => Ok(PhpType::Float),
                    PhpType::Mixed | PhpType::Bool | PhpType::False | PhpType::Void => {
                        Ok(PhpType::Mixed)
                    }
                    _ => Err(CompileError::new(
                        expr.span,
                        "Cannot negate a non-numeric value",
                    )),
                }
            }
            ExprKind::Not(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Bool)
            }
            ExprKind::ErrorSuppress(inner) => self.infer_type(inner, env),
            ExprKind::Print(inner) => {
                self.infer_type(inner, env)?;
                Ok(PhpType::Int)
            }
            ExprKind::PreIncrement(name) | ExprKind::PreDecrement(name) => match env.get(name) {
                Some(PhpType::Int) => Ok(PhpType::Mixed),
                Some(PhpType::Mixed) => Ok(PhpType::Mixed),
                // PHP's string increment can change the value's type (`"9"++` is
                // `int(10)`), so the pre-form's value is dynamically tagged. EIR lowering
                // gives the local boxed Mixed frame storage for the same reason.
                Some(PhpType::Str) => {
                    self.reject_unboxable_string_incdec(name, expr.span)?;
                    self.record_string_incdec_local(name);
                    Ok(PhpType::Mixed)
                }
                // PHP's `++`/`--` on a float adds or subtracts 1.0 and keeps the float.
                Some(PhpType::Float) => Ok(PhpType::Float),
                Some(PhpType::Bool) | Some(PhpType::False) | Some(PhpType::Void) => {
                    Ok(PhpType::Int)
                }
                Some(other) => Err(CompileError::new(
                    expr.span,
                    &increment_type_error(name, other),
                )),
                None => Err(CompileError::new(
                    expr.span,
                    &format!("Undefined variable: ${}", name),
                )),
            },
            ExprKind::PostIncrement(name) | ExprKind::PostDecrement(name) => match env.get(name) {
                Some(PhpType::Int)
                | Some(PhpType::Bool)
                | Some(PhpType::False)
                | Some(PhpType::Void) => Ok(PhpType::Int),
                Some(PhpType::Mixed) => Ok(PhpType::Mixed),
                // The post-forms yield the value the local held BEFORE the update, so a
                // string local still answers `string` even though the update itself can
                // retype the local (see the pre-form arm).
                Some(PhpType::Str) => {
                    self.reject_unboxable_string_incdec(name, expr.span)?;
                    self.record_string_incdec_local(name);
                    Ok(PhpType::Str)
                }
                // The post-forms yield the float the local held before the update.
                Some(PhpType::Float) => Ok(PhpType::Float),
                Some(other) => Err(CompileError::new(
                    expr.span,
                    &increment_type_error(name, other),
                )),
                None if self.eval_barrier_active => Ok(PhpType::Int),
                None => Err(CompileError::new(
                    expr.span,
                    &format!("Undefined variable: ${}", name),
                )),
            },
            ExprKind::ArrayLiteralAssoc(pairs) => {
                if pairs.is_empty() {
                    return Err(CompileError::new(
                        expr.span,
                        "Cannot infer type of empty associative array literal",
                    ));
                }
                let mut key_ty = normalized_array_key_type(
                    &pairs[0].0,
                    self.infer_type(&pairs[0].0, env)?,
                );
                let mut val_ty = self.infer_type(&pairs[0].1, env)?;
                for (k, v) in &pairs[1..] {
                    let kt = normalized_array_key_type(k, self.infer_type(k, env)?);
                    let vt = self.infer_type(v, env)?;
                    if kt != key_ty {
                        key_ty = merge_array_key_types(key_ty, kt);
                    }
                    if vt != val_ty {
                        val_ty = PhpType::Mixed;
                    }
                }
                Ok(PhpType::AssocArray {
                    key: Box::new(key_ty),
                    value: Box::new(val_ty),
                })
            }
            ExprKind::Match {
                subject,
                arms,
                default,
            } => {
                self.infer_type(subject, env)?;
                let mut result_ty: Option<PhpType> = None;
                for (conditions, result) in arms {
                    for c in conditions {
                        self.infer_type(c, env)?;
                    }
                    let ty = self.match_arm_result_type(result, env)?;
                    result_ty = Some(match result_ty {
                        Some(acc) => merge_match_arm_result_type(self, acc, ty),
                        None => ty,
                    });
                }
                if let Some(d) = default {
                    let ty = self.match_arm_result_type(d, env)?;
                    result_ty = Some(match result_ty {
                        Some(acc) => merge_match_arm_result_type(self, acc, ty),
                        None => ty,
                    });
                }
                Ok(result_ty.unwrap_or(PhpType::Void))
            }
            ExprKind::ArrayLiteral(elems) => {
                if elems.is_empty() {
                    return Ok(PhpType::Array(Box::new(PhpType::Never)));
                }
                if elems.iter().any(|elem| {
                    matches!(
                        &elem.kind,
                        ExprKind::Spread(inner)
                            if matches!(
                                self.infer_type(inner, env),
                                Ok(PhpType::AssocArray { .. })
                            )
                    )
                }) {
                    let value_ty = self.assoc_spread_literal_value_type(elems, env);
                    return Ok(PhpType::AssocArray {
                        key: Box::new(PhpType::Mixed),
                        value: Box::new(value_ty),
                    });
                }
                let mut elem_ty = self.infer_type(&elems[0], env)?;
                for elem in &elems[1..] {
                    let ty = self.infer_type(elem, env)?;
                    if ty != elem_ty {
                        if let Some(merged_ty) = self.merge_array_element_type(&elem_ty, &ty) {
                            elem_ty = merged_ty;
                            continue;
                        }
                        elem_ty = PhpType::Mixed;
                    }
                }
                Ok(PhpType::Array(Box::new(elem_ty)))
            }
            ExprKind::ArrayAccess { array, index } => {
                let arr_ty = self.infer_type(array, env)?;
                let idx_ty = self.infer_type(index, env)?;
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
                                    if !matches!(idx_ty, PhpType::Int | PhpType::Mixed) {
                                        first_index_error =
                                            first_index_error.or(Some("Buffer index must be integer"));
                                        continue;
                                    }
                                    match elem_ty.as_ref() {
                                        PhpType::Packed(name) => {
                                            result_members.push(PhpType::Pointer(Some(name.clone())))
                                        }
                                        _ => result_members.push(*elem_ty.clone()),
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
                        if !matches!(idx_ty, PhpType::Int | PhpType::Mixed) {
                            return Err(CompileError::new(
                                expr.span,
                                "Buffer index must be integer",
                            ));
                        }
                        match elem_ty.as_ref() {
                            PhpType::Packed(name) => Ok(PhpType::Pointer(Some(name.clone()))),
                            _ => Ok(*elem_ty.clone()),
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
                    // `isset($n['k'])` / `$n['k'] ?? $d` reach through a null base in PHP and
                    // answer `false` / the default; only a probe context may do so.
                    PhpType::Void if self.null_probe_depth > 0 => Ok(PhpType::Void),
                    _ => Err(CompileError::new(expr.span, "Cannot index non-array")),
                }
            }
            ExprKind::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                self.infer_type(condition, env)?;
                // Flow-narrowing across the branches: `$x instanceof X ? ... : ...` (and the other
                // recognized guards) narrow `$x` — or a simple `$x->prop` — in the then/else
                // branches. A ternary is a single expression (no intervening writes), so the
                // narrowing is safe to scope to each branch's inference.
                let (then_ty, else_ty) = if let Some(guard) =
                    self.guard_narrowing(condition, env)?
                {
                    let mut then_env = env.clone();
                    then_env.insert(guard.var.clone(), guard.then_ty);
                    let mut else_env = env.clone();
                    else_env.insert(guard.var, guard.else_ty);
                    (
                        self.match_arm_result_type(then_expr, &then_env)?,
                        self.match_arm_result_type(else_expr, &else_env)?,
                    )
                } else {
                    (
                        self.match_arm_result_type(then_expr, env)?,
                        self.match_arm_result_type(else_expr, env)?,
                    )
                };
                // Same Mixed/nullable merge as match arms: heterogeneous heap
                // types must not collapse through the Str-absorbing syntactic join.
                Ok(merge_match_arm_result_type(self, then_ty, else_ty))
            }
            ExprKind::ShortTernary { value, default } => {
                let value_ty = self.match_arm_result_type(value, env)?;
                let default_ty = self.match_arm_result_type(default, env)?;
                Ok(merge_match_arm_result_type(self, value_ty, default_ty))
            }
            ExprKind::Throw(inner) => {
                let thrown_ty = self.infer_type(inner, env)?;
                match thrown_ty {
                    PhpType::Object(type_name)
                        if self.object_type_implements_throwable(&type_name) =>
                    {
                        Ok(PhpType::Void)
                    }
                    PhpType::Object(_) => Err(CompileError::new(
                        expr.span,
                        "Type error: throw requires an object implementing Throwable",
                    )),
                    _ => Err(CompileError::new(
                        expr.span,
                        "Type error: throw requires an object value",
                    )),
                }
            }
            ExprKind::Cast { target, expr } => {
                let source_ty = self.infer_type(expr, env)?;
                use crate::parser::ast::CastType;
                Ok(match target {
                    CastType::Int => PhpType::Int,
                    CastType::Float => PhpType::Float,
                    CastType::String => PhpType::Str,
                    CastType::Bool => PhpType::Bool,
                    CastType::Array
                        if matches!(source_ty.codegen_repr(), PhpType::Object(_)) =>
                    PhpType::AssocArray {
                        key: Box::new(PhpType::Str),
                        value: Box::new(PhpType::Mixed),
                    },
                    CastType::Array
                        if matches!(
                            source_ty.codegen_repr(),
                            PhpType::Mixed | PhpType::Union(_)
                        ) => PhpType::Mixed,
                    CastType::Array => PhpType::Array(Box::new(PhpType::Mixed)),
                    CastType::Void => PhpType::Void,
                })
            }
            _ => unreachable!("non-basic expression routed to basic inference"),
        }
    }
}

/// Formats the diagnostic for `++`/`--` applied to a local elephc cannot update in place.
///
/// `int`, `float`, `bool`, `null`, `string`, and boxed `mixed` locals all have an increment
/// path; everything else (arrays, objects, buffers, pointers) reaches this diagnostic.
fn increment_type_error(name: &str, ty: &PhpType) -> String {
    format!("Cannot increment/decrement ${} of type {:?}", name, ty)
}
