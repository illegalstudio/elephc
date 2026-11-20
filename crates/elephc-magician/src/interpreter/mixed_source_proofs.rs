//! Purpose:
//! Validates logical eval sources independently from their boxed runtime occupants.
//!
//! Called from:
//! - Source-aware operand evaluation and declared callable/member boundaries.
//!
//! Key details:
//! - Shared generic Type terms carry alternatives; runtime tags never prove a mixed source.
//! - Existing runtime adapters still own PHP conversions, value errors and object handlers.

use super::*;
use elephc_generics::Type;
use elephc_semantics::StrictFeature;

/// Removes conditional content obligations without manufacturing a proof of the whole value kind.
pub(super) fn value_source(ty: Type) -> Type {
    /// Retains only unconditional outer-kind evidence from a compound metadata source.
    fn retained(ty: Type) -> Option<Type> {
        if crate::eval_ir::array_contents_schema(&ty).is_some() { return None; }
        match ty {
            Type::Intersection(members) => {
                let mut members = members.into_iter().filter_map(retained).collect::<Vec<_>>();
                match members.len() { 0 => None, 1 => members.pop(), _ => Some(Type::Intersection(members)) }
            }
            Type::Union(members) => Some(Type::Union(members.into_iter().map(value_source).collect())),
            Type::Nullable(inner) => Some(Type::Nullable(Box::new(value_source(*inner)))),
            ty => Some(ty),
        }
    }
    retained(ty).unwrap_or_else(|| Type::atom("mixed"))
}

/// Recognizes dynamic outer alternatives without treating mixed container elements as outer mixed.
pub(super) fn dynamic_source(ty: &Type) -> bool {
    matches!(ty, Type::Union(_) | Type::Nullable(_)) || *ty == Type::atom("mixed")
}

/// Checks each logical alternative while retaining nominal and structural type terms intact.
pub(super) fn every_source(ty: &Type, supported: &mut impl FnMut(&Type) -> bool) -> bool {
    match ty {
        Type::Atom(name) if name == "never" => true,
        Type::Atom(name) if name == "mixed" => false,
        Type::Union(members) => members.iter().all(|member| every_source(member, supported)),
        Type::Nullable(inner) => supported(&Type::atom("null")) && every_source(inner, supported),
        _ => supported(ty),
    }
}

/// Rejects an unsupported dynamic source after operand effects and before its value is consumed.
pub(super) fn require_source(ty: &Type, operation: &str,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
    mut supported: impl FnMut(&Type, &ElephcEvalContext) -> bool,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(StrictFeature::Mixed) || !dynamic_source(ty)
        || every_source(ty, &mut |ty| supported(ty, context)) { return Ok(()); }
    eval_throw_type_error(&format!("--strict-mixed: narrow {ty} before {operation}; every alternative must support the operation"),
        context, values)
}

/// Tests all operand pairs, including array union, without using an incidental runtime kind.
pub(super) fn require_binary(op: EvalBinOp, left: &Type, right: &Type,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(StrictFeature::Mixed)
        || !matches!(op, EvalBinOp::Add | EvalBinOp::Sub | EvalBinOp::Mul | EvalBinOp::Div
            | EvalBinOp::Mod | EvalBinOp::Pow | EvalBinOp::BitAnd | EvalBinOp::BitOr
            | EvalBinOp::BitXor | EvalBinOp::ShiftLeft | EvalBinOp::ShiftRight | EvalBinOp::Concat)
        || !dynamic_source(left) && !dynamic_source(right) { return Ok(()); }
    let accepted = every_source(left, &mut |left| every_source(right,
        &mut |right| binary_pair_supported(op, left, right, context)));
    if accepted { return Ok(()); }
    eval_throw_type_error(&format!("--strict-mixed: narrow {left} and {right} before this operation; every alternative must support it"),
        context, values)
}

/// Resolves scalar pair domains; internal object capabilities are supplied by their shared contracts.
fn binary_pair_supported(op: EvalBinOp, left: &Type, right: &Type,
    context: &ElephcEvalContext,
) -> bool {
    if object_pair_supported(op, left, right, context) { return true; }
    if op == EvalBinOp::Add && array_source(left) && array_source(right) { return true; }
    if op == EvalBinOp::Concat {
        if context.semantic_profile().contains(StrictFeature::Strings) {
            return *left == Type::atom("string") && *right == Type::atom("string");
        }
        return string_context_source(left, context) && string_context_source(right, context);
    }
    if let Some(rule) = numeric_domains::operator(op).and_then(|operator|
        elephc_semantics::numeric_domains::operand_rule(context.semantic_profile(), operator)) {
        return scalar_source_kind(left).is_some_and(|kind| rule.domain.allows(kind))
            && scalar_source_kind(right).is_some_and(|kind| rule.domain.allows(kind));
    }
    let accepts = |ty| scalar_atom(ty, &["int", "float", "string", "bool", "false", "true", "null", "void"]);
    accepts(left) && accepts(right)
}

/// Identifies array shape terms without inspecting their element alternatives.
fn array_source(ty: &Type) -> bool {
    matches!(ty, Type::Atom(name) | Type::Apply(name, _) if name == "array")
}

/// Recognizes scalar source atoms without flattening nullable or nominal metadata.
fn scalar_atom(ty: &Type, names: &[&str]) -> bool {
    matches!(ty, Type::Atom(name) if names.contains(&name.as_str()))
}

/// Resolves internal handlers through the same reviewed class contracts as AOT operator planning.
pub(super) fn operator_class(ty: &Type, context: &ElephcEvalContext) -> Option<&'static elephc_builtin_contract::ClassContract> {
    let Type::Atom(name) = ty else { return None; };
    elephc_builtin_contract::lookup_class(name).filter(|class| class.operators.is_some())
        .or_else(|| context.class_native_parent_name(name).as_deref()
            .and_then(elephc_builtin_contract::lookup_class).filter(|class| class.operators.is_some()))
}

/// Checks a resolved class handler's logical operands before the ordinary handler runs.
fn object_pair_supported(op: EvalBinOp, left: &Type, right: &Type,
    context: &ElephcEvalContext,
) -> bool {
    use elephc_builtin_contract::{ObjectArithmetic as A, ObjectArithmeticResult, ObjectBitwise as B, ObjectShift as S};
    let Some(class) = operator_class(left, context).or_else(|| operator_class(right, context)) else { return false; };
    let Some(operators) = class.operators else { return false; };
    let accepts = |ty: &Type| operators.accepts_any_operands
        || scalar_atom(ty, &["int", "float", "bool", "false", "true", "string"])
        || scalar_atom(ty, &["null", "void"]) && operators.arithmetic_result == ObjectArithmeticResult::Number
        || operator_class(ty, context).is_some_and(|other| other.name.eq_ignore_ascii_case(class.name)
            || operators.arithmetic_result == ObjectArithmeticResult::Number
                && other.operators.is_some_and(|other| std::ptr::eq(other, operators)));
    if !accepts(left) || !accepts(right) { return false; }
    match op {
        EvalBinOp::Add => operators.arithmetic_method(A::Add).is_some(),
        EvalBinOp::Sub => operators.arithmetic_method(A::Sub).is_some(),
        EvalBinOp::Mul => operators.arithmetic_method(A::Mul).is_some(),
        EvalBinOp::Div => operators.arithmetic_method(A::Div).is_some(),
        EvalBinOp::Mod => operators.arithmetic_method(A::Mod).is_some(),
        EvalBinOp::Pow => operators.arithmetic_method(A::Pow).is_some(),
        EvalBinOp::BitAnd => operators.bitwise_method(B::And).is_some(),
        EvalBinOp::BitOr => operators.bitwise_method(B::Or).is_some(),
        EvalBinOp::BitXor => operators.bitwise_method(B::Xor).is_some(),
        EvalBinOp::ShiftLeft => operators.shift_method(S::Left).is_some(),
        EvalBinOp::ShiftRight => operators.shift_method(S::Right).is_some(),
        _ => false,
    }
}

/// Reads native operator result kinds from the same reviewed handler capability contract.
pub(super) fn object_result_source(op: EvalBinOp,left: &Type,right: &Type,
    context: &ElephcEvalContext,
) -> Option<Type> {
    if !object_pair_supported(op,left,right,context) {return None;}
    let class=operator_class(left,context).or_else(||operator_class(right,context))?;
    Some(if class.operators?.arithmetic_result==elephc_builtin_contract::ObjectArithmeticResult::Object {
        Type::atom(class.name)
    } else {Type::Union(vec![Type::atom("int"),Type::atom("float")])})
}

/// Selected boolean signatures require every represented source alternative to be boolean.
pub(super) fn require_condition(ty: &Type,context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(),EvalStatus> {
    if !context.semantic_profile().contains(StrictFeature::Bools) {return Ok(());}
    require_source(ty,"a boolean condition",context,values,
        |ty,_|scalar_atom(ty,&["bool","true","false"]))
}

/// Uses nominal Stringable evidence without invoking user code to manufacture a source proof.
pub(super) fn string_context_source(ty: &Type, context: &ElephcEvalContext) -> bool {
    if scalar_atom(ty, &["int", "float", "string", "bool", "false", "true", "null", "void",
        elephc_semantics::native_local_contract::RESOURCE_TYPE_SPEC]) { return true; }
    let Type::Atom(name) = ty else { return false; };
    name.trim_start_matches('\\').eq_ignore_ascii_case("Stringable")
        || context.class_method(name, "__toString").is_some()
        || context.native_method_signature(name, "__toString").is_some()
}

/// Checks one normalized argument before coercion, retiring shared facts changed by later operands.
pub(super) fn require_argument(expected: &EvalParameterType, source: Option<&ArgumentSource>,
    value: RuntimeCellHandle, shared: bool, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    require_argument_domain(expected, source, value, shared, false, context, values)
}

/// Checks structural container inputs whose elements never use PHP weak scalar parameter conversion.
pub(super) fn require_exact_argument(expected: &EvalParameterType, source: Option<&ArgumentSource>,
    value: RuntimeCellHandle, context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    require_argument_domain(expected, source, value, false, true, context, values)
}

/// Shares original packet and reference-epoch validation across PHP binding and exact structural inputs.
fn require_argument_domain(expected: &EvalParameterType, source: Option<&ArgumentSource>,
    value: RuntimeCellHandle, shared: bool, exact: bool, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    if !context.semantic_profile().contains(StrictFeature::Mixed) { return Ok(()); }
    let target = crate::eval_ir::generic_type(expected);
    if target == Type::atom("mixed") { return Ok(()); }
    let shared = shared || values.is_reference(value)?;
    let source = source.filter(|source| !shared || context.source_view_is_current(value, source.write_epoch));
    let actual = value_source(source.map(|source| crate::eval_ir::generic_type(&source.ty))
        .unwrap_or_else(|| Type::atom("mixed")));
    if !dynamic_source(&actual) || source_assignable(&actual, &target, exact, context, values)? { return Ok(()); }
    eval_throw_type_error(&format!("--strict-mixed: narrow {actual} before binding {target}; every alternative must support the operation"),
        context, values)
}

/// Validates canonical reference stamps even when a native adapter materialized the payload separately.
pub(super) fn require_argument_with_target(expected: &EvalParameterType,
    source: Option<&ArgumentSource>, value: RuntimeCellHandle, target: Option<&EvalReferenceTarget>,
    context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
) -> Result<(), EvalStatus> {
    let source = if let Some(EvalReferenceTarget::Cell { cell }) = target {
        source.filter(|source| context.source_view_is_current(*cell, source.write_epoch))
    } else { source };
    require_argument(expected, source, value, target.is_some(), context, values)
}

/// Shares finite alternative traversal with exact generic/nominal membership and weak scalar domains.
fn source_assignable(actual: &Type, target: &Type, exact: bool, context: &ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<bool, EvalStatus> {
    match actual {
        Type::Atom(name) if name == "mixed" => return Ok(false),
        Type::Atom(name) if name == "never" => return Ok(true),
        Type::Union(members) => {
            for member in members { if !source_assignable(member, target, exact, context, values)? { return Ok(false); } }
            return Ok(true);
        }
        Type::Nullable(inner) => return Ok(source_assignable(inner, target, exact, context, values)?
            && source_assignable(&Type::atom("null"), target, exact, context, values)?),
        _ => {}
    }
    if exact || context.semantic_profile().contains(StrictFeature::Coercions) {
        return generic_type_is_a(actual, target, context, values);
    }
    if let Some(kind) = scalar_source_kind(actual) {
        if elephc_semantics::weak_scalar_binding::supports_source_kind(scalar_target_kinds(target), kind) {
            return Ok(true);
        }
    }
    if scalar_target_kinds(target).allows(elephc_semantics::conversions::ValueKind::String)
        && scalar_source_kind(actual).is_none()
        && *actual != Type::atom(elephc_semantics::native_local_contract::RESOURCE_TYPE_SPEC)
        && string_context_source(actual, context) { return Ok(true); }
    generic_type_is_a(actual, target, context, values)
}

/// Projects scalar logical atoms without inspecting the candidate's current value.
fn scalar_source_kind(ty: &Type) -> Option<elephc_semantics::conversions::ValueKind> {
    use elephc_semantics::conversions::ValueKind as K;
    Some(match ty { Type::Atom(name) => match name.as_str() {
        "int" => K::Integer, "float" => K::Float, "string" => K::String,
        "bool" | "true" | "false" => K::Boolean, "null" | "void" => K::Null,
        _ => return None,
    }, _ => return None })
}

/// Keeps literal true/false constraints exact while collecting ordinary weak scalar alternatives.
fn scalar_target_kinds(ty: &Type) -> elephc_semantics::conversions::AllowedKinds {
    use elephc_semantics::conversions::{AllowedKinds as A, ValueKind as K};
    match ty {
        Type::Union(members) => members.iter().fold(A::EMPTY, |kinds, ty| kinds.union(scalar_target_kinds(ty))),
        Type::Nullable(inner) => scalar_target_kinds(inner).union(A::one(K::Null)),
        Type::Atom(name) if matches!(name.as_str(), "true" | "false") => A::EMPTY,
        _ => scalar_source_kind(ty).map_or(A::EMPTY, A::one),
    }
}

/// Applies the selected unary signature to logical kinds while preserving reviewed native handlers.
pub(super) fn unary_source_supported(op: EvalUnaryOp, ty: &Type, context: &ElephcEvalContext) -> bool {
    use elephc_semantics::{conversions::ValueKind as K, numeric_domains::NumericOperator as N};
    if op == EvalUnaryOp::BitNot && operator_class(ty, context)
        .and_then(|class| class.operators).is_some_and(|operators| operators.bit_not.is_some()) { return true; }
    if matches!(op, EvalUnaryOp::Plus | EvalUnaryOp::Negate)
        && object_pair_supported(if op == EvalUnaryOp::Plus { EvalBinOp::Add } else { EvalBinOp::Sub },
            &Type::atom("int"), ty, context) { return true; }
    let Some(kind) = scalar_source_kind(ty) else { return false; };
    let operator = if op == EvalUnaryOp::BitNot { N::BitNot } else { N::Unary };
    if let Some(rule) = elephc_semantics::numeric_domains::operand_rule(context.semantic_profile(), operator) {
        return rule.domain.allows(kind);
    }
    op != EvalUnaryOp::BitNot || matches!(kind, K::Integer | K::Float | K::String)
}
