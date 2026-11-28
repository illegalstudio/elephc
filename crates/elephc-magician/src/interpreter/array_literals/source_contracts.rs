//! Purpose:
//! Observes logical array literal producers at their actual source evaluation points.
//!
//! Called from:
//! - Indexed/hash literal builders and assignment RHS evaluation.
//!
//! Key details:
//! - Duplicate normalized keys keep the final producer without losing earlier effects.
//! - Metadata borrows no runtime cells and never becomes a global handle proof map.

use super::*;
use crate::eval_ir::{parameter_type, merge_array_element_sources};
use elephc_generics::Type;

/// Source alternatives of the final entries, independent from their boxed storage kinds.
#[derive(Default)]
pub(in crate::interpreter) struct LiteralSource {
    entries: Vec<(EvalArrayReferenceKey, Type, Option<ArgumentSource>)>,
}

impl LiteralSource {
    /// Records a successfully inserted element under the same normalized key as runtime storage.
    pub(super) fn record(&mut self, key: RuntimeCellHandle, source: Option<EvalParameterType>,
        proof: Option<ArgumentSource>, value: RuntimeCellHandle,
        context: &mut ElephcEvalContext, values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        let Some(key) = eval_array_reference_key(key, context.semantic_profile(), values)? else { return Ok(()); };
        let ty = local_binding_origins::element_value_source_type(source.as_ref(), value, context, values)?;
        if let Some((_, previous, source)) = self.entries.iter_mut().find(|(previous, _, _)| previous == &key) {
            *previous = ty; *source = proof;
        } else { self.entries.push((key, ty, proof)); }
        Ok(())
    }

    /// Describes final source entries, keeping heterogeneous inference distinct from explicit openness.
    pub(in crate::interpreter) fn finish(self) -> Type {
        if self.entries.is_empty() {
            return Type::Intersection(vec![Type::atom("array"), crate::eval_ir::pending_array_contents_type()]);
        }
        let mut keys = Vec::new(); let mut element = None;
        for (key, ty, _) in self.entries {
            let key = match key { EvalArrayReferenceKey::Int(_) => Type::atom("int"),
                EvalArrayReferenceKey::String(_) => Type::atom("string") };
            if !keys.contains(&key) { keys.push(key); }
            element = Some(match element { None => ty, Some(previous) => merge_array_element_sources(previous, ty) });
        }
        let key = match keys.len() { 0 => Type::atom("never"), 1 => keys.remove(0), _ => Type::Union(keys) };
        Type::Apply("array".to_string(), vec![key, element.unwrap_or_else(|| Type::atom("never"))])
    }
}

/// Evaluates a literal once and returns its logical producer alongside the owned array.
pub(in crate::interpreter) fn evaluate_literal(elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, EvalParameterType), EvalStatus> {
    let (value, source) = evaluate_literal_source(elements, context, scope, values)?;
    Ok((value, parameter_type(&source.finish())))
}

/// Captures tuple entry proofs separately from the schema used by inferred data-array constraints.
pub(in crate::interpreter) fn evaluate_argument_array(elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, ArgumentSource), EvalStatus> {
    let (value, source) = evaluate_literal_source(elements, context, scope, values)?;
    let entries = source.entries.iter().map(|(key, _, proof)| (key.clone(), proof.clone())).collect();
    let mut proof = ArgumentSource::capture(parameter_type(&source.finish()), context);
    proof.entries = entries;
    Ok((value, proof))
}

/// Shares source-aware construction without imposing a data-array contract on argument tuples.
fn evaluate_literal_source(elements: &[EvalArrayElement], context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, LiteralSource), EvalStatus> {
    let mut source = LiteralSource::default();
    let associative = elements.iter().any(|element| native_default_array_spread(element).is_some()
        || matches!(element, EvalArrayElement::KeyValue { .. } | EvalArrayElement::KeyReference { .. }));
    let value = if associative { eval_assoc_array(elements, context, scope, values, Some(&mut source))? }
        else { eval_indexed_array(elements, context, scope, values, Some(&mut source))? };
    Ok((value, source))
}

/// Captures a producer after earlier operands, recursively retaining nested literal alternatives.
pub(in crate::interpreter) fn evaluate_value(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalParameterType>), EvalStatus> {
    if let EvalExpr::Array(elements) = expr {
        let (value, source) = evaluate_literal(elements, context, scope, values)?;
        Ok((value, Some(source)))
    } else {
        let source = local_binding_origins::source_view(expr, context, scope);
        let value = eval_owned_expr(expr, context, scope, values)?;
        Ok((value, source))
    }
}

/// Captures nested tuple proofs independently from data inference, evaluating every expression once.
pub(super) fn evaluate_literal_value(expr: &EvalExpr, context: &mut ElephcEvalContext,
    scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Option<EvalParameterType>, Option<ArgumentSource>), EvalStatus> {
    if let EvalExpr::Array(elements) = expr {
        let (value, proof) = evaluate_argument_array(elements, context, scope, values)?;
        return Ok((value, Some(proof.ty.clone()), Some(proof)));
    }
    let schema = local_binding_origins::source_view(expr, context, scope);
    let packet = local_binding_origins::argument_source(expr, context, scope);
    if context.semantic_profile().contains(elephc_semantics::StrictFeature::Mixed) {
        let (value, source) = mixed_eval::evaluate(expr, context, scope, values)?;
        let proof = packet.or_else(|| source.as_ref().map(|ty| ArgumentSource::capture(parameter_type(ty), context)));
        Ok((value, schema, proof))
    } else {
        let value = eval_owned_expr(expr, context, scope, values)?;
        let proof = packet.or_else(|| schema.as_ref().map(|ty| ArgumentSource::capture(ty.clone(), context)));
        Ok((value, schema, proof))
    }
}
