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
use crate::eval_ir::{generic_type, parameter_type, merge_array_element_sources};
use elephc_generics::Type;

/// Source alternatives of the final entries, independent from their boxed storage kinds.
#[derive(Default)]
pub(in crate::interpreter) struct LiteralSource {
    entries: Vec<(EvalArrayReferenceKey, Type)>,
}

impl LiteralSource {
    /// Records a successfully inserted element under the same normalized key as runtime storage.
    pub(super) fn record(&mut self, key: RuntimeCellHandle, source: Option<EvalParameterType>,
        value: RuntimeCellHandle, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
    ) -> Result<(), EvalStatus> {
        let Some(key) = eval_array_reference_key(key, context.semantic_profile(), values)? else { return Ok(()); };
        let ty = if let Some(source) = source { generic_type(&source) }
            else { runtime_generic_type(value, context, values, 0)? };
        let ty = local_binding_origins::element_source_type(ty,
            matches!(values.type_tag(value)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC));
        if let Some((_, previous)) = self.entries.iter_mut().find(|(previous, _)| previous == &key) {
            *previous = ty;
        } else { self.entries.push((key, ty)); }
        Ok(())
    }

    /// Describes final source entries, keeping heterogeneous inference distinct from explicit openness.
    pub(in crate::interpreter) fn finish(self) -> Type {
        let mut keys = Vec::new(); let mut element = None;
        for (key, ty) in self.entries {
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

/// Returns each final argument entry's own producer instead of inferring a homogeneous tuple.
pub(in crate::interpreter) fn evaluate_argument_literal(elements: &[EvalArrayElement],
    context: &mut ElephcEvalContext, scope: &mut ElephcEvalScope, values: &mut impl RuntimeValueOps,
) -> Result<(RuntimeCellHandle, Vec<(EvalArrayReferenceKey, EvalParameterType)>), EvalStatus> {
    let (value, source) = evaluate_literal_source(elements, context, scope, values)?;
    Ok((value, source.entries.into_iter().map(|(key, ty)| (key, parameter_type(&ty))).collect()))
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
pub(super) fn evaluate_value(expr: &EvalExpr, context: &mut ElephcEvalContext,
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
