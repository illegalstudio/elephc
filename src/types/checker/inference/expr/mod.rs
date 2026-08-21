//! Purpose:
//! Dispatches expression inference for assignments, class references, closures, and side-effecting forms.
//! Feeds statement checking, function call validation, and optimizer-visible type metadata.
//!
//! Called from:
//! - `crate::types::checker::Checker::infer_type()`
//!
//! Key details:
//! - Inference must preserve PHP evaluation errors and avoid treating effectful expressions as pure type facts.

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{Expr, ExprKind};
use crate::span::Span;
use crate::types::{PhpType, TypeEnv};
mod assignments;
mod basic;
mod class_refs;
mod effects;
mod calls_objects;
mod static_closure;
use super::super::Checker;
use super::syntactic::null_coalesce_merge_type;
use static_closure::body_must_not_use_this;
pub(crate) use static_closure::closure_body_uses_this;
impl Checker {
    /// Infers the PHP return type of `expr` in the given `env`.
    ///
    /// This is the top-level dispatcher for expression type inference. It
    /// handles literals, variables, operators, array access, ternaries,
    /// function calls, closures, and all other expression forms. Errors are
    /// returned for type mismatches (e.g. negating a string) or undefined
    /// references. The result feeds statement checking, function call
    /// validation, and optimizer-visible type metadata.
    pub fn infer_type(&mut self, expr: &Expr, env: &TypeEnv) -> Result<PhpType, CompileError> {
        match &expr.kind {
            ExprKind::IncludeValue { .. }
            | ExprKind::BoolLiteral(_)
            | ExprKind::Null
            | ExprKind::StringLiteral(_)
            | ExprKind::IntLiteral(_)
            | ExprKind::FloatLiteral(_)
            | ExprKind::Variable(_)
            | ExprKind::Negate(_)
            | ExprKind::Not(_)
            | ExprKind::ErrorSuppress(_)
            | ExprKind::Print(_)
            | ExprKind::PreIncrement(_)
            | ExprKind::PreDecrement(_)
            | ExprKind::PostIncrement(_)
            | ExprKind::PostDecrement(_)
            | ExprKind::ArrayLiteralAssoc(_)
            | ExprKind::Match { .. }
            | ExprKind::ArrayLiteral(_)
            | ExprKind::Ternary { .. }
            | ExprKind::ShortTernary { .. }
            | ExprKind::Throw(_) => self.infer_basic_expr_type(expr, env),
            ExprKind::ArrayAppend => Ok(PhpType::Void),
            ExprKind::ArrayAccess { .. } => self.infer_dom_aware_array_access(expr, env),
            ExprKind::Cast { .. } => self.infer_dom_aware_cast(expr, env),
            ExprKind::FunctionCall { .. }
            | ExprKind::BufferNew { .. }
            | ExprKind::BitNot(_)
            | ExprKind::NullCoalesce { .. }
            | ExprKind::Pipe { .. }
            | ExprKind::Assignment { .. }
            | ExprKind::ConstRef(_)
            | ExprKind::FirstClassCallable(_)
            | ExprKind::Closure { .. }
            | ExprKind::Spread(_)
            | ExprKind::NamedArg { .. }
            | ExprKind::ClosureCall { .. }
            | ExprKind::ExprCall { .. }
            | ExprKind::BinaryOp { .. }
            | ExprKind::InstanceOf { .. }
            | ExprKind::NewObject { .. }
            | ExprKind::NewDynamic { .. }
            | ExprKind::NewDynamicObject { .. }
            | ExprKind::PropertyAccess { .. }
            | ExprKind::DynamicPropertyAccess { .. }
            | ExprKind::NullsafePropertyAccess { .. }
            | ExprKind::NullsafeDynamicPropertyAccess { .. }
            | ExprKind::StaticPropertyAccess { .. }
            | ExprKind::MethodCall { .. }
            | ExprKind::NullsafeMethodCall { .. }
            | ExprKind::NullsafeDynamicMethodCall { .. }
            | ExprKind::StaticMethodCall { .. }
            | ExprKind::This
            | ExprKind::PtrCast { .. }
            | ExprKind::ClassConstant { .. }
            | ExprKind::ObjectClassName { .. }
            | ExprKind::ScopedConstantAccess { .. }
            | ExprKind::NewScopedObject { .. }
            | ExprKind::Yield { .. }
            | ExprKind::YieldFrom(_)
            | ExprKind::MagicConstant(_) =>
                self.infer_call_or_object_expr_type(expr, env),
            ExprKind::Clone(inner) => self.infer_dom_aware_clone(inner, expr, env),
        }
    }

    /// Infers collection dimensions with the native DOM/SimpleXML handlers in addition to
    /// the ordinary indexed-array, hash, string, buffer, and ArrayAccess contracts.
    fn infer_dom_aware_array_access(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let ExprKind::ArrayAccess { array, index } = &expr.kind else {
            unreachable!("array-access inference must receive ArrayAccess")
        };
        let array_type = self.infer_type(array, env)?;
        let index_type = self.infer_type(index, env)?;
        let normalized_index_type = crate::types::normalized_array_key_type(index, index_type.clone());
        match &array_type {
            PhpType::Str => {
                if !is_valid_string_offset_index(index, &index_type) {
                    return Err(CompileError::new(expr.span, "String index must be integer"));
                }
                Ok(PhpType::Str)
            }
            PhpType::Array(element) => {
                if normalized_index_type == PhpType::Int {
                    Ok(*element.clone())
                } else {
                    Ok(PhpType::Mixed)
                }
            }
            PhpType::AssocArray { value, .. } => Ok(*value.clone()),
            PhpType::Object(class_name) => {
                if self.object_type_implements_interface(class_name, "ArrayAccess") {
                    Ok(self.array_access_offset_get_type(class_name))
                } else if is_simplexml_element_class(self, class_name) {
                    Ok(PhpType::Union(vec![
                        PhpType::Object(class_name.clone()),
                        PhpType::Void,
                    ]))
                } else if let Some(members) = dom_collection_dimension_result_members(class_name) {
                    Ok(self.normalize_union_type(members))
                } else {
                    Err(CompileError::new(expr.span, "Cannot index non-array"))
                }
            }
            PhpType::Union(members) => {
                let mut results = Vec::new();
                let mut saw_indexable = false;
                let mut first_error = None;
                for member in members {
                    match member {
                        PhpType::Void => results.push(PhpType::Void),
                        PhpType::Str => {
                            saw_indexable = true;
                            if is_valid_string_offset_index(index, &index_type) {
                                results.push(PhpType::Str);
                            } else {
                                first_error.get_or_insert("String index must be integer");
                            }
                        }
                        PhpType::Array(element) => {
                            saw_indexable = true;
                            results.push(if normalized_index_type == PhpType::Int {
                                *element.clone()
                            } else {
                                PhpType::Mixed
                            });
                        }
                        PhpType::AssocArray { value, .. } => {
                            saw_indexable = true;
                            results.push(*value.clone());
                        }
                        PhpType::Object(class_name) => {
                            if self.object_type_implements_interface(class_name, "ArrayAccess") {
                                saw_indexable = true;
                                results.push(self.array_access_offset_get_type(class_name));
                            } else if is_simplexml_element_class(self, class_name) {
                                saw_indexable = true;
                                results.push(PhpType::Object(class_name.clone()));
                                results.push(PhpType::Void);
                            } else if let Some(collection_members) =
                                dom_collection_dimension_result_members(class_name)
                            {
                                saw_indexable = true;
                                results.extend(collection_members);
                            }
                        }
                        PhpType::Buffer(element) => {
                            saw_indexable = true;
                            if matches!(index_type, PhpType::Int | PhpType::Mixed) {
                                match element.as_ref() {
                                    PhpType::Packed(name) => {
                                        results.push(PhpType::Pointer(Some(name.clone())));
                                    }
                                    _ => results.push(*element.clone()),
                                }
                            } else {
                                first_error.get_or_insert("Buffer index must be integer");
                            }
                        }
                        _ => {}
                    }
                }
                let has_concrete_result = results.iter().any(|member| *member != PhpType::Void);
                if !has_concrete_result && saw_indexable {
                    Err(CompileError::new(
                        expr.span,
                        first_error.unwrap_or("Cannot index non-array"),
                    ))
                } else if results.is_empty() {
                    Err(CompileError::new(expr.span, "Cannot index non-array"))
                } else {
                    Ok(self.normalize_union_type(results))
                }
            }
            PhpType::Buffer(element) => {
                if !matches!(index_type, PhpType::Int | PhpType::Mixed) {
                    return Err(CompileError::new(expr.span, "Buffer index must be integer"));
                }
                match element.as_ref() {
                    PhpType::Packed(name) => Ok(PhpType::Pointer(Some(name.clone()))),
                    _ => Ok(*element.clone()),
                }
            }
            PhpType::Mixed => Ok(PhpType::Mixed),
            PhpType::Void if self.null_probe_depth > 0 => Ok(PhpType::Void),
            _ => Err(CompileError::new(expr.span, "Cannot index non-array")),
        }
    }

    /// Preserves SimpleXML wrapper identity for PHP's `(object)` cast.
    fn infer_dom_aware_cast(
        &mut self,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let ExprKind::Cast { target, expr: inner } = &expr.kind else {
            unreachable!("cast inference must receive Cast")
        };
        let source_type = self.infer_type(inner, env)?;
        use crate::parser::ast::CastType;
        Ok(match target {
            CastType::Int => PhpType::Int,
            CastType::Float => PhpType::Float,
            CastType::String => PhpType::Str,
            CastType::Bool => PhpType::Bool,
            CastType::Array => PhpType::Array(Box::new(PhpType::Int)),
            CastType::Object if simplexml_object_cast_preserves_type(self, &source_type) => {
                source_type
            }
            CastType::Object => PhpType::Object("stdClass".to_string()),
        })
    }

    /// Accepts documented loader-failure SimpleXML unions for clone checks.
    fn infer_dom_aware_clone(
        &mut self,
        inner: &Expr,
        expr: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let source_type = self.infer_type(inner, env)?;
        let class_name = match source_type {
            PhpType::Object(class_name) => class_name,
            PhpType::Union(members) => simplexml_clone_union_class(self, &members)
                .ok_or_else(|| CompileError::new(expr.span, "clone requires an object value"))?,
            _ => return Err(CompileError::new(expr.span, "clone requires an object value")),
        };
        self.check_clone_visibility(&class_name, expr.span)?;
        Ok(PhpType::Object(class_name))
    }
    /// Returns a variable type, allowing dynamic eval-created locals after an eval barrier.
    fn variable_type_or_eval_dynamic(
        &self,
        name: &str,
        span: Span,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        env.get(name)
            .cloned()
            .or_else(|| self.eval_barrier_active.then_some(PhpType::Mixed))
            .ok_or_else(|| CompileError::new(span, &format!("Undefined variable: ${}", name)))
    }

    /// Returns the element type of an array literal that contains at least one
    /// spread of an associative array.
    ///
    /// Iterates over `elems`, extracting the value type from each `Spread` that
    /// wraps an `AssocArray`. All spread value types must agree, otherwise
    /// `Mixed` is returned. Non-spread elements are ignored.
    fn assoc_spread_literal_value_type(&mut self, elems: &[Expr], env: &TypeEnv) -> PhpType {
        let mut value_ty = PhpType::Never;
        for elem in elems {
            let ExprKind::Spread(inner) = &elem.kind else {
                continue;
            };
            let next = match self.infer_type(inner, env) {
                Ok(PhpType::Array(elem)) => *elem,
                Ok(PhpType::AssocArray { value, .. }) => *value,
                _ => PhpType::Mixed,
            };
            if matches!(value_ty, PhpType::Never) {
                value_ty = next;
            } else if value_ty != next {
                value_ty = PhpType::Mixed;
            }
        }
        if matches!(value_ty, PhpType::Never) {
            PhpType::Mixed
        } else {
            value_ty
        }
    }

    /// Returns the return type of the `offsetGet` method for `class_name`,
    /// or `Mixed` if no `offsetGet` method is found.
    ///
    /// Looks up `offsetGet` in the class's method table first, then falls back
    /// to the `ArrayAccess` interface. Used when indexing an `Object` that
    /// implements `ArrayAccess`.
    fn array_access_offset_get_type(&self, class_name: &str) -> PhpType {
        self.classes
            .get(class_name)
            .and_then(|class_info| class_info.methods.get("offsetget"))
            .map(|sig| sig.return_type.clone())
            .or_else(|| {
                self.interfaces
                    .get("ArrayAccess")
                    .and_then(|interface_info| interface_info.methods.get("offsetget"))
                    .map(|sig| sig.return_type.clone())
            })
            .unwrap_or(PhpType::Mixed)
    }

    /// Infers a match/ternary arm result type for branch merging. Throw arms
    /// produce no value, so their checker type (`Void`, shared with `null`) is
    /// normalized to `Never` here: the merge must distinguish "arm never yields"
    /// (defer to the other arms) from "arm yields null" (keep the merge nullable).
    fn match_arm_result_type(
        &mut self,
        result: &Expr,
        env: &TypeEnv,
    ) -> Result<PhpType, CompileError> {
        let ty = self.infer_type(result, env)?;
        if matches!(result.kind, ExprKind::Throw(_)) {
            return Ok(PhpType::Never);
        }
        Ok(ty)
    }
}

impl Checker {
    /// Checks whether the current scope may invoke a class's `__clone` hook.
    ///
    /// PHP permits `__clone` to be non-public, but the actual `clone $object`
    /// expression must obey the hook's visibility when a hook exists.
    fn check_clone_visibility(&self, class_name: &str, span: Span) -> Result<(), CompileError> {
        let normalized = class_name.trim_start_matches('\\');
        let Some(class_info) = self.classes.get(normalized) else {
            return Ok(());
        };
        let key = php_symbol_key("__clone");
        let Some(visibility) = class_info.method_visibilities.get(&key) else {
            return Ok(());
        };
        let declaring_class = class_info
            .method_declaring_classes
            .get(&key)
            .map(String::as_str)
            .unwrap_or(normalized);
        if self.can_access_member(declaring_class, visibility) {
            return Ok(());
        }
        Err(CompileError::new(
            span,
            &format!(
                "Cannot access {} method: {}::__clone",
                Self::visibility_label(visibility),
                normalized
            ),
        ))
    }
}

/// Returns `true` if `index` is a valid string offset index for a string receiver.
///
/// A valid index is an integer type, or a string literal whose value can be
/// parsed as a PHP string offset (e.g. `"0"`, `"-1"`, `"10"`).
fn is_valid_string_offset_index(index: &Expr, idx_ty: &PhpType) -> bool {
    *idx_ty == PhpType::Int
        || *idx_ty == PhpType::Mixed
        || matches!(
            &index.kind,
            ExprKind::StringLiteral(value)
                if crate::types::parse_php_string_offset_literal(value).is_some()
        )
}

/// Returns php-src's nullable item alternatives for one DOM collection dimension read.
///
/// DOM collection classes expose native dimensions without implementing userland
/// `ArrayAccess`, so their contracts stay independent from the userland interface.
fn dom_collection_dimension_result_members(class_name: &str) -> Option<Vec<PhpType>> {
    Some(match class_name.trim_start_matches('\\') {
        "DOMNodeList" => vec![
            PhpType::Object("DOMElement".to_string()),
            PhpType::Object("DOMNode".to_string()),
            PhpType::Object("DOMNameSpaceNode".to_string()),
            PhpType::Void,
        ],
        "DOMNamedNodeMap" => vec![PhpType::Object("DOMNode".to_string()), PhpType::Void],
        "Dom\\NodeList" => vec![PhpType::Object("Dom\\Node".to_string()), PhpType::Void],
        "Dom\\NamedNodeMap" => vec![PhpType::Object("Dom\\Attr".to_string()), PhpType::Void],
        "Dom\\DtdNamedNodeMap" => vec![
            PhpType::Object("Dom\\Entity".to_string()),
            PhpType::Object("Dom\\Notation".to_string()),
            PhpType::Void,
        ],
        "Dom\\HTMLCollection" => vec![PhpType::Object("Dom\\Element".to_string()), PhpType::Void],
        _ => return None,
    })
}

/// Returns whether a class is `SimpleXMLElement` or one of its userland descendants.
fn is_simplexml_element_class(checker: &Checker, class_name: &str) -> bool {
    let class_name = class_name.trim_start_matches('\\');
    class_name.eq_ignore_ascii_case("SimpleXMLElement")
        || checker.is_subclass_of(class_name, "SimpleXMLElement")
}

/// Reports whether `(object)` preserves a direct or fallible SimpleXML wrapper identity.
fn simplexml_object_cast_preserves_type(checker: &Checker, ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(class_name) => is_simplexml_element_class(checker, class_name),
        PhpType::Union(members) => simplexml_clone_union_class(checker, members).is_some(),
        _ => false,
    }
}

/// Extracts one exact SimpleXML class from the documented loader-failure union shape.
fn simplexml_clone_union_class(checker: &Checker, members: &[PhpType]) -> Option<String> {
    let mut class_name = None;
    for member in members {
        match member {
            PhpType::Void | PhpType::Never | PhpType::False => {}
            PhpType::Object(candidate) if is_simplexml_element_class(checker, candidate) => {
                if class_name.as_ref().is_some_and(|existing| existing != candidate) {
                    return None;
                }
                class_name.get_or_insert_with(|| candidate.clone());
            }
            _ => return None,
        }
    }
    class_name
}

/// Merges two match arm result types: identical arms keep their type,
/// `Never`-typed arms (`throw`, normalized at the call site) defer to the
/// other arm's type, `Void`-typed arms (checker `null`) keep the merge
/// nullable so the null arm's value survives return-type-driven coercion.
/// Array pairs widen their element types while keeping the array container; array/false pairs
/// retain the declared PHP sentinel union instead of collapsing to bare Mixed.
/// Object pairs, including supported `false`/null sentinels, retain a normalized
/// union so declared object-union returns and member validation remain precise;
/// every other heterogeneous pair widens to `Mixed` so each arm's runtime value
/// survives instead of being coerced to the first arm's type.
fn merge_match_arm_result_type(checker: &Checker, acc: PhpType, next: PhpType) -> PhpType {
    if acc == next {
        return acc;
    }
    if acc == PhpType::Never {
        return next;
    }
    if next == PhpType::Never {
        return acc;
    }
    if acc == PhpType::Void {
        return nullable_match_arm_type(next);
    }
    if next == PhpType::Void {
        return nullable_match_arm_type(acc);
    }
    if let Some(merged) = merge_array_branch_types(&acc, &next) {
        return merged;
    }
    if matches!(acc, PhpType::Array(_) | PhpType::AssocArray { .. })
        && next == PhpType::False
        || matches!(next, PhpType::Array(_) | PhpType::AssocArray { .. })
            && acc == PhpType::False
    {
        return checker.normalize_union_type(vec![acc, next]);
    }
    if object_union_match_arm_type(&acc) && object_union_match_arm_type(&next) {
        return merge_object_union_match_arm_types(checker, acc, next);
    }
    PhpType::Mixed
}

/// Merges two array branch types elementwise so a heterogeneous `match`/ternary/`?:`/`??`
/// merge stays an array instead of collapsing to bare `Mixed`.
///
/// The checker and lowering share `PhpType::widen_array_branch_element`, so
/// empty-array placeholders defer to populated branches while real element-type
/// disagreements widen to `Mixed`. This keeps the result valid for by-ref `array`
/// parameters, array builtins, and spread. Returns `None` for pairs outside the
/// indexed/indexed or associative/associative shapes, leaving the caller's existing
/// object-union and `Mixed` handling untouched.
fn merge_array_branch_types(acc: &PhpType, next: &PhpType) -> Option<PhpType> {
    match (acc, next) {
        (PhpType::Array(acc_elem), PhpType::Array(next_elem)) => Some(PhpType::Array(Box::new(
            PhpType::widen_array_branch_element(
                (**acc_elem).clone(),
                (**next_elem).clone(),
            ),
        ))),
        (
            PhpType::AssocArray {
                key: acc_key,
                value: acc_value,
            },
            PhpType::AssocArray {
                key: next_key,
                value: next_value,
            },
        ) => Some(PhpType::AssocArray {
            key: Box::new(PhpType::widen_array_branch_element(
                (**acc_key).clone(),
                (**next_key).clone(),
            )),
            value: Box::new(PhpType::widen_array_branch_element(
                (**acc_value).clone(),
                (**next_value).clone(),
            )),
        }),
        _ => None,
    }
}

/// Joins the non-null value and default types of `??`.
///
/// Array pairs use the same elementwise branch join as `match` and ternaries.
///
/// Every other pair goes through [`null_coalesce_merge_type`] rather than
/// `wider_type_syntactic`. `??` is not a widening: both arms are reachable, so a
/// join that answers with ONE arm's type describes the other arm wrongly. The
/// coercion order `wider_type_syntactic` implements is right for the operators
/// that own it (a binary `+` really does coerce its operands to one type) and
/// wrong here — `$m[$k] ?? 'MISS'` over a float map would have been typed `Str`,
/// so a hit was read back through a string representation. When the two arms have
/// no common type, `Mixed` is the honest answer: it keeps the value boxed with its
/// tag, and both arms survive.
fn merge_null_coalesce_result_type(value: PhpType, default: PhpType) -> PhpType {
    merge_array_branch_types(&value, &default)
        .unwrap_or_else(|| null_coalesce_merge_type(&value, &default))
}

/// Joins object/sentinel branch types at their existing compatible supertype
/// when one accepts the other, otherwise retaining a normalized union. A null
/// member from either side is restored after comparing the non-null members.
fn merge_object_union_match_arm_types(
    checker: &Checker,
    acc: PhpType,
    next: PhpType,
) -> PhpType {
    let nullable = Checker::union_contains_void(&acc) || Checker::union_contains_void(&next);
    let acc_object = checker.strip_void_from_union(&acc);
    let next_object = checker.strip_void_from_union(&next);
    let merged = if checker.type_accepts(&acc_object, &next_object) {
        acc_object
    } else if checker.type_accepts(&next_object, &acc_object) {
        next_object
    } else {
        checker.normalize_union_type(vec![acc_object, next_object])
    };
    if nullable {
        nullable_match_arm_type(merged)
    } else {
        merged
    }
}

/// Returns whether a branch type contains only concrete objects plus supported
/// `false`/null sentinel members, which can be preserved as a checker-level
/// union even though codegen materializes it through boxed `Mixed` storage.
fn object_union_match_arm_type(ty: &PhpType) -> bool {
    match ty {
        PhpType::Object(_) | PhpType::False => true,
        PhpType::Union(members) => members
            .iter()
            .all(|member| matches!(member, PhpType::Object(_) | PhpType::False | PhpType::Void)),
        _ => false,
    }
}

/// Widens a match arm type to also admit PHP null, for merges where another
/// arm is a `null` literal.
fn nullable_match_arm_type(ty: PhpType) -> PhpType {
    match ty {
        PhpType::Mixed => PhpType::Mixed,
        PhpType::Union(members) if members.contains(&PhpType::Void) => PhpType::Union(members),
        PhpType::Union(mut members) => {
            members.push(PhpType::Void);
            PhpType::Union(members)
        }
        other => PhpType::Union(vec![other, PhpType::Void]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two indexed arrays with divergent element types must merge to `array<mixed>`
    /// (issue #587), keeping the branch result usable as an array.
    #[test]
    fn test_merge_array_branch_types_widens_heterogeneous_indexed() {
        let merged = merge_array_branch_types(
            &PhpType::Array(Box::new(PhpType::Int)),
            &PhpType::Array(Box::new(PhpType::Str)),
        );
        assert_eq!(merged, Some(PhpType::Array(Box::new(PhpType::Mixed))));
    }

    /// Two associative arrays whose value types differ must widen elementwise to
    /// `array<string, mixed>` rather than collapsing to bare `Mixed`.
    #[test]
    fn test_merge_array_branch_types_widens_heterogeneous_assoc() {
        let merged = merge_array_branch_types(
            &PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            },
            &PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Str),
            },
        );
        assert_eq!(
            merged,
            Some(PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Mixed),
            })
        );
    }

    /// Arrays that agree on their element type keep it (the `widen` no-op), so the
    /// fix never over-widens a homogeneous merge.
    #[test]
    fn test_merge_array_branch_types_keeps_shared_element() {
        let merged = merge_array_branch_types(
            &PhpType::Array(Box::new(PhpType::Int)),
            &PhpType::Array(Box::new(PhpType::Int)),
        );
        assert_eq!(merged, Some(PhpType::Array(Box::new(PhpType::Int))));
    }

    /// An empty array's `Never` element placeholder contributes no value and must
    /// defer to the populated branch, matching merge-temp storage.
    #[test]
    fn test_merge_array_branch_types_keeps_populated_element_against_empty() {
        let merged = merge_array_branch_types(
            &PhpType::Array(Box::new(PhpType::Never)),
            &PhpType::Array(Box::new(PhpType::Int)),
        );
        assert_eq!(merged, Some(PhpType::Array(Box::new(PhpType::Int))));
    }

    /// A real null element is not an empty-array placeholder, so null/int
    /// alternatives require boxed `Mixed` elements.
    #[test]
    fn test_merge_array_branch_types_widens_null_and_int_elements() {
        let merged = merge_array_branch_types(
            &PhpType::Array(Box::new(PhpType::Void)),
            &PhpType::Array(Box::new(PhpType::Int)),
        );
        assert_eq!(merged, Some(PhpType::Array(Box::new(PhpType::Mixed))));
    }

    /// Null coalescing uses the same array-specific join instead of letting the
    /// left element type win through the syntactic fallback.
    #[test]
    fn test_merge_null_coalesce_result_type_widens_array_elements() {
        let merged = merge_null_coalesce_result_type(
            PhpType::Array(Box::new(PhpType::Int)),
            PhpType::Array(Box::new(PhpType::Str)),
        );
        assert_eq!(merged, PhpType::Array(Box::new(PhpType::Mixed)));
    }

    /// An indexed-vs-associative mix is not covered by the elementwise rule and must
    /// return `None`, matching the lowering side and preserving `Mixed` handling.
    #[test]
    fn test_merge_array_branch_types_rejects_indexed_assoc_mix() {
        let merged = merge_array_branch_types(
            &PhpType::Array(Box::new(PhpType::Int)),
            &PhpType::AssocArray {
                key: Box::new(PhpType::Int),
                value: Box::new(PhpType::Str),
            },
        );
        assert_eq!(merged, None);
    }

    /// Non-array pairs (scalars, objects, `null`) must return `None` so scalar unions,
    /// object unions, and nullable merges are left to their existing handling.
    #[test]
    fn test_merge_array_branch_types_rejects_non_array() {
        assert_eq!(merge_array_branch_types(&PhpType::Int, &PhpType::Str), None);
        assert_eq!(
            merge_array_branch_types(
                &PhpType::Object("A".to_string()),
                &PhpType::Object("B".to_string())
            ),
            None
        );
        assert_eq!(
            merge_array_branch_types(&PhpType::Array(Box::new(PhpType::Int)), &PhpType::Void),
            None
        );
    }
}

impl Checker {

    /// Rejects `++` / `--` on a `string` local whose storage cannot be boxed to `Mixed`.
    ///
    /// The operator can retype its target (`"9"++` is `int(10)`), which elephc implements by
    /// giving the local boxed `Mixed` frame storage. Two storage shapes cannot take that
    /// contract: a by-reference parameter aliases a caller slot whose declared `string` type
    /// the callee must not change, and a `static` local's initializer writes its symbol with
    /// the declared `string` representation before any boxing store runs. Both are rejected
    /// here so the program gets a source-level diagnostic instead of a backend error or a
    /// silently wrong value.
    fn reject_unboxable_string_incdec(
        &self,
        name: &str,
        span: Span,
    ) -> Result<(), CompileError> {
        let storage = if self.active_ref_params.contains(name) {
            "a by-reference parameter"
        } else if self.active_statics.contains(name) {
            "a static local"
        } else {
            return Ok(());
        };
        Err(CompileError::new(
            span,
            &format!(
                "Cannot increment/decrement ${} of type string: it is {}, and PHP's string \
                 increment can change the value's type (\"9\"++ is int(10)), which that \
                 storage cannot hold. Copy it into a plain local first.",
                name, storage
            ),
        ))
    }

    /// Records that `name` is a `string` local used as a `++` / `--` target in the
    /// function-like scope currently being checked.
    ///
    /// EIR lowering reads this contract (through `CheckResult::string_incdec_locals`) and
    /// gives the local boxed `Mixed` frame storage from its first store. Without it the
    /// slot only widens at the increment, and every earlier or later `string`-typed read
    /// of the same slot has to detach an owned copy out of the boxed cell — one leaked
    /// heap block per executed read, unbounded inside a loop.
    fn record_string_incdec_local(&mut self, name: &str) {
        self.string_incdec_locals
            .insert((self.current_loop_storage_scope.clone(), name.to_string()));
    }
}
