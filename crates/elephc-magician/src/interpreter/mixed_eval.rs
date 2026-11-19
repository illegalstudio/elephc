//! Purpose:
//! Evaluates operands with logical source snapshots at their actual source-order positions.
//!
//! Called from:
//! - Mixed-aware assignments and scalar expression consumers.
//!
//! Key details:
//! - Value owners remain in the existing runtime owner frames through effects and cleanup.
//! - Source metadata is returned with the result, never indexed by a runtime value handle.

use super::*;
use elephc_generics::Type;
use crate::eval_ir::generic_type;
use crate::runtime_hooks::owner_frames::with_runtime_owners;

/// Evaluates a source once, keeping declared returns independent from their concrete occupants.
pub(super) fn evaluate(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Type), EvalStatus> {
    match expr {
        EvalExpr::Binary { op, left, right } if !matches!(op, EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr) =>
            binary(*op, left, right, context, scope, values),
        EvalExpr::Unary { op, expr } => unary(*op, expr, context, scope, values),
        EvalExpr::Ternary { condition, then_branch: Some(then_branch), else_branch } => {
            let selected = if eval_condition(condition, context, scope, values)? { then_branch } else { else_branch };
            evaluate(selected, context, scope, values)
        }
        EvalExpr::Assign { name, .. } => {
            let value = eval_owned_expr(expr, context, scope, values)?;
            let source = source_type(&EvalExpr::LoadVar(name.clone()), context, scope);
            Ok((value, source))
        }
        _ => {
            let source = source_type(expr, context, scope);
            let value = eval_owned_expr(expr, context, scope, values)?;
            Ok((value, source))
        }
    }
}

/// Takes a producer snapshot without inferring a proof from its evaluated runtime tag.
fn source_type(expr: &EvalExpr, context: &ElephcEvalContext, scope: &ElephcEvalScope) -> Type {
    mixed_source_proofs::value_source(local_binding_origins::source_view(expr, context, scope)
        .as_ref().map(generic_type).unwrap_or_else(|| Type::atom("mixed")))
}

/// Roots each detached operand before evaluating the next source and preserves failure cleanup.
fn with_sources<V: RuntimeValueOps>(args: &[&EvalExpr], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut V,
    consume: impl FnOnce(&[RuntimeCellHandle], &[Type], &mut ElephcEvalContext,
        &mut ElephcEvalScope, &mut V) -> Result<(RuntimeCellHandle, Type), EvalStatus>,
) -> Result<(RuntimeCellHandle, Type), EvalStatus> {
    let mut owners = Vec::with_capacity(args.len()); let mut sources = Vec::with_capacity(args.len());
    let prepared = (|| {
        for arg in args {
            let (value, source) = with_runtime_owners(&owners, || evaluate(arg, context, scope, values))?;
            owners.push(value); sources.push(source);
        }
        Ok(())
    })();
    let result = prepared.and_then(|()| with_runtime_owners(&owners, ||
        consume(&owners, &sources, context, scope, values)));
    match result {
        Ok((value, source)) => finish_eval_argument_values(Ok(value), owners, context, values)
            .map(|value| (value, source)),
        Err(status) => finish_eval_argument_values(Err(status), owners, context, values)
            .map(|_| unreachable!("failed source evaluation cannot produce a value")),
    }
}

/// Validates complete source alternatives after both ordinary operands have run.
pub(super) fn binary(op: EvalBinOp, left: &EvalExpr, right: &EvalExpr,
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Type), EvalStatus> {
    with_sources(&[left, right], context, scope, values, |operands, sources, context, _, values| {
        mixed_source_proofs::require_binary(op, &sources[0], &sources[1], context, values)?;
        let value = eval_binary_result(op, operands[0], operands[1], context, values)?;
        Ok((value, binary_result_source(op, &sources[0], &sources[1], context)))
    })
}

/// Applies unary source proofs before delegating value semantics to the existing operation adapter.
pub(super) fn unary(op: EvalUnaryOp, expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Type), EvalStatus> {
    with_sources(&[expr], context, scope, values, |operands, sources, context, _, values| {
        if op != EvalUnaryOp::LogicalNot {
            mixed_source_proofs::require_source(&sources[0], "unary operations", context, values,
                |ty, _| matches!(ty, Type::Atom(name) if matches!(name.as_str(),
                    "int" | "float" | "string" | "bool" | "true" | "false" | "null" | "void")))?;
        }
        let value = eval_unary_value(op, operands[0], context, values)?;
        let source = if op == EvalUnaryOp::LogicalNot { Type::atom("bool") }
            else if op == EvalUnaryOp::BitNot && sources[0] == Type::atom("string") { Type::atom("string") }
            else if op == EvalUnaryOp::BitNot { Type::atom("int") } else { numeric_source() };
        Ok((value, source))
    })
}

/// Retains the closed logical numeric result instead of narrowing to the actual returned scalar.
fn numeric_source() -> Type { Type::Union(vec![Type::atom("int"), Type::atom("float")]) }

/// Selects source results without consulting the storage tag produced by the operation.
fn binary_result_source(op: EvalBinOp, left: &Type, right: &Type, context: &ElephcEvalContext) -> Type {
    match op {
        EvalBinOp::Concat => Type::atom("string"),
        EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr | EvalBinOp::LogicalXor | EvalBinOp::LooseEq
            | EvalBinOp::LooseNotEq | EvalBinOp::StrictEq | EvalBinOp::StrictNotEq
            | EvalBinOp::Lt | EvalBinOp::LtEq | EvalBinOp::Gt | EvalBinOp::GtEq => Type::atom("bool"),
        EvalBinOp::Spaceship | EvalBinOp::Mod | EvalBinOp::BitAnd | EvalBinOp::BitOr
            | EvalBinOp::BitXor | EvalBinOp::ShiftLeft | EvalBinOp::ShiftRight => Type::atom("int"),
        EvalBinOp::Div if context.semantic_profile().contains(elephc_semantics::StrictFeature::Arithmetic) => Type::atom("float"),
        EvalBinOp::Add if [left, right].iter().all(|ty|
            matches!(ty, Type::Atom(name) | Type::Apply(name, _) if name == "array")) => Type::atom("array"),
        _ => numeric_source(),
    }
}
