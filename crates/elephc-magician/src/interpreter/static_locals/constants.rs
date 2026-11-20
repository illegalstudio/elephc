//! Purpose:
//! Prepares silent static constants without executing dynamic initializer syntax.
//!
//! Called from:
//! - Reflection static-local initialization before the first function invocation.
//!
//! Key details:
//! - Deferred expressions never publish a persistent value or a local origin.
//! - Every intermediate owner is rooted and retired on success, deferral and failure.

use super::*;
use crate::runtime_hooks::owner_frames::{RuntimeOwnerSlot, with_runtime_owners};

mod operators;
mod arrays;

/// Returns one owned constant, or no value when evaluation must wait for an invocation.
pub(super) fn prepare(expr: &EvalExpr, context: &mut ElephcEvalContext,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
    let mut reducer = Reducer { context, values, owners: Vec::new() };
    let prepared = reducer.reduce(expr, 0);
    let result = match prepared {
        Ok(Some(value)) => reducer.operation(|_, values| values.retain(value)).map(Some),
        other => other,
    };
    let result_owner = result.as_ref().ok().copied().flatten().map(RuntimeOwnerSlot::new);
    let mut cleanup = Ok(());
    with_runtime_owners(&(&result_owner, &reducer.owners), || {
        for owner in &reducer.owners {
            if let Err(status) = eval_release_value(reducer.context, reducer.values, owner.take()) {
                cleanup = Err(status);
            }
        }
    });
    match (result, cleanup) {
        (Err(status), _) => Err(status),
        (Ok(_), Err(status)) => {
            if let Some(owner) = result_owner { let _ = eval_release_value(reducer.context, reducer.values, owner.take()); }
            Err(status)
        }
        (Ok(_), Ok(())) => Ok(result_owner.map(|owner| owner.take())),
    }
}

/// Keeps proven constants separate from dynamic PHP execution and owns all preparation leases.
struct Reducer<'a, V: RuntimeValueOps> {
    context: &'a mut ElephcEvalContext,
    values: &'a mut V,
    owners: Vec<RuntimeOwnerSlot>,
}

impl<V: RuntimeValueOps> Reducer<'_, V> {
    /// Roots actual owner slots while a safe runtime operation allocates or collects.
    fn operation<T>(&mut self, consume: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<T, EvalStatus>)
        -> Result<T, EvalStatus> {
        with_runtime_owners(&self.owners, || consume(self.context, self.values))
    }

    /// Takes one runtime result into the preparation inventory, retaining borrowed metadata.
    fn own(&mut self, value: RuntimeCellHandle) -> Result<RuntimeCellHandle, EvalStatus> {
        let value = if value.is_borrowed() { self.operation(|_, values| values.retain(value))? } else { value };
        self.owners.push(RuntimeOwnerSlot::new(value));
        Ok(value.borrowed())
    }

    /// Materializes only a proven operation, never the original source expression.
    fn make(&mut self, consume: impl FnOnce(&mut ElephcEvalContext, &mut V) -> Result<RuntimeCellHandle, EvalStatus>)
        -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let value = self.operation(consume)?;
        self.own(value).map(Some)
    }

    /// Reduces reached constant syntax; dynamic calls and storage reads remain deferred.
    fn reduce(&mut self, expr: &EvalExpr, depth: usize) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if depth >= 256 { return Ok(None); }
        let depth = depth + 1;
        match expr {
            EvalExpr::Const(value) => self.make(|_, values| eval_const(value, values)),
            EvalExpr::Magic(value) => self.make(|context, values| eval_magic_const(value, context, values)),
            EvalExpr::ConstFetch(name) => {
                if constant_eval::eval_predefined_constant_value(name).is_none() { return Ok(None); }
                self.make(|context, values| eval_const_fetch(name, context, values))
            }
            EvalExpr::ClassNameFetch { class_name } => {
                if class_name.eq_ignore_ascii_case("static") { return Ok(None); }
                if resolve_eval_static_member_class_name(class_name, self.context).is_err() { return Ok(None); }
                self.make(|context, values| eval_class_name_fetch_result(class_name, context, values))
            }
            EvalExpr::ClassConstantFetch { class_name, constant } => self.class_constant(class_name, constant),
            EvalExpr::Array(elements) => self.array(elements, depth),
            EvalExpr::ArrayGet { array, index } => {
                let Some(array) = self.reduce(array, depth)? else { return Ok(None); };
                let Some(index) = self.reduce(index, depth)? else { return Ok(None); };
                self.array_get(array, index)
            }
            EvalExpr::Unary { op, expr } => {
                let Some(value) = self.reduce(expr, depth)? else { return Ok(None); };
                self.unary(*op, value)
            }
            EvalExpr::Cast { target, expr } => {
                let Some(value) = self.reduce(expr, depth)? else { return Ok(None); };
                self.cast(*target, value)
            }
            EvalExpr::NullCoalesce { value, default } => {
                let Some(value) = self.reduce(value, depth)? else { return Ok(None); };
                if self.values.type_tag(value)? != EVAL_TAG_NULL { Ok(Some(value)) }
                else { self.reduce(default, depth) }
            }
            EvalExpr::Ternary { condition, then_branch, else_branch } => {
                let Some(condition) = self.reduce(condition, depth)? else { return Ok(None); };
                let Some(truthy) = self.condition(condition)? else { return Ok(None); };
                if truthy {
                    match then_branch { Some(expr) => self.reduce(expr, depth), None => Ok(Some(condition)) }
                } else { self.reduce(else_branch, depth) }
            }
            EvalExpr::Binary { op, left, right } => {
                let Some(left) = self.reduce(left, depth)? else { return Ok(None); };
                if matches!(op, EvalBinOp::LogicalAnd | EvalBinOp::LogicalOr) {
                    let Some(truthy) = self.condition(left)? else { return Ok(None); };
                    if (*op == EvalBinOp::LogicalAnd && !truthy) || (*op == EvalBinOp::LogicalOr && truthy) {
                        return self.make(|_, values| values.bool_value(truthy));
                    }
                    let Some(right) = self.reduce(right, depth)? else { return Ok(None); };
                    let Some(truthy) = self.condition(right)? else { return Ok(None); };
                    return self.make(|_, values| values.bool_value(truthy));
                }
                let Some(right) = self.reduce(right, depth)? else { return Ok(None); };
                self.binary(*op, left, right)
            }
            _ => Ok(None),
        }
    }

    /// Reads already materialized class metadata without autoloading or accessing runtime defaults.
    fn class_constant(&mut self, class: &str, name: &str) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if class.eq_ignore_ascii_case("static") { return Ok(None); }
        let Ok(class) = resolve_eval_static_member_class_name(class, self.context) else { return Ok(None); };
        if let Some(value) = self.operation(|_, values| eval_builtin_reflection_class_constant(&class, name, values))? {
            return self.own(value).map(Some);
        }
        let value = if let Some(value) = self.context.enum_case(&class, name) { value }
        else if let Some((owner, constant)) = self.context.class_constant(&class, name) {
            if validate_eval_member_access(&owner, constant.visibility(), self.context).is_err() { return Ok(None); }
            match self.operation(|context, values|
                eval_class_constant_preview_state(&owner, name, context, values))? {
                EvalClassConstantPreview::Value(value, _) => value,
                EvalClassConstantPreview::Missing
                | EvalClassConstantPreview::Unavailable
                | EvalClassConstantPreview::Failed => return Ok(None),
            }
        } else { return Ok(None); };
        self.own(value).map(Some)
    }

    /// Excludes warning-producing or invalid keys before any runtime array operation.
    fn key(&mut self, value: RuntimeCellHandle) -> Result<Option<EvalArrayReferenceKey>, EvalStatus> {
        let tag = self.values.type_tag(value)?;
        if tag == EVAL_TAG_FLOAT {
            let value = f64::from_bits(self.values.raw_value_word(value)?);
            if elephc_semantics::numeric::checked_float_to_int(value).ok().is_none_or(|int| int as f64 != value) { return Ok(None); }
        }
        if !matches!(tag, EVAL_TAG_INT | EVAL_TAG_STRING | EVAL_TAG_NULL | EVAL_TAG_BOOL | EVAL_TAG_FLOAT) { return Ok(None); }
        eval_array_reference_key(value, self.context.semantic_profile(), self.values)
    }

    /// Constructs literal arrays with checked append keys and no reference entries.
    fn array(&mut self, elements: &[EvalArrayElement], depth: usize) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        let indexed = elements.iter().all(|element| matches!(element, EvalArrayElement::Value(_)));
        let Some(array) = self.make(|_, values| if indexed { values.array_new(elements.len()) } else { values.assoc_new(elements.len()) })? else { unreachable!(); };
        let mut next = None::<i64>;
        let mut exhausted = false;
        for element in elements {
            let (key, value) = match element {
                EvalArrayElement::Value(value) => {
                    if exhausted { return Ok(None); }
                    (EvalArrayReferenceKey::Int(next.unwrap_or(0)), value)
                }
                EvalArrayElement::KeyValue { key, value } => {
                    let Some(key) = self.reduce(key, depth)? else { return Ok(None); };
                    let Some(key) = self.key(key)? else { return Ok(None); };
                    (key, value)
                }
                _ => return Ok(None),
            };
            if let EvalArrayReferenceKey::Int(key) = &key {
                if next.is_none_or(|next| *key >= next) {
                    next = key.checked_add(1);
                    exhausted |= next.is_none();
                }
            }
            let Some(value) = self.reduce(value, depth)? else { return Ok(None); };
            let Some(key) = self.make(|_, values| match key {
                EvalArrayReferenceKey::Int(key) => values.int(key),
                EvalArrayReferenceKey::String(key) => values.string_bytes_value(&key),
            })? else { unreachable!(); };
            self.operation(|_, values| values.array_set(array, key, value))?;
        }
        Ok(Some(array))
    }

    /// Reads only existing literal-array entries so missing keys cannot emit diagnostics.
    fn array_get(&mut self, array: RuntimeCellHandle, index: RuntimeCellHandle) -> Result<Option<RuntimeCellHandle>, EvalStatus> {
        if self.values.type_tag(array)? == EVAL_TAG_STRING && self.values.type_tag(index)? == EVAL_TAG_INT {
            let bytes = self.values.string_bytes(array)?;
            let index = self.values.raw_value_word(index)? as i64;
            let index = if index < 0 { (bytes.len() as i64).checked_add(index) } else { Some(index) };
            let Some(byte) = index.and_then(|index| usize::try_from(index).ok()).and_then(|index| bytes.get(index)).copied() else { return Ok(None); };
            return self.make(|_, values| values.string_bytes_value(&[byte]));
        }
        if !matches!(self.values.type_tag(array)?, EVAL_TAG_ARRAY | EVAL_TAG_ASSOC) || self.key(index)?.is_none() { return Ok(None); }
        let Some(exists) = self.make(|_, values| values.array_key_exists(index, array))? else { unreachable!(); };
        if !self.values.truthy(exists)? { return Ok(None); }
        self.make(|_, values| values.array_get(array, index))
    }

    /// Excludes objects recursively before array operators can invoke comparison callbacks.
    fn plain_data(&mut self, value: RuntimeCellHandle, depth: usize) -> Result<bool, EvalStatus> {
        if depth >= 256 { return Ok(false); }
        match self.values.type_tag(value)? {
            EVAL_TAG_NULL | EVAL_TAG_BOOL | EVAL_TAG_INT | EVAL_TAG_STRING => Ok(true),
            EVAL_TAG_FLOAT => Ok(!f64::from_bits(self.values.raw_value_word(value)?).is_nan()),
            EVAL_TAG_ARRAY | EVAL_TAG_ASSOC => {
                for position in 0..self.values.array_len(value)? {
                    let Some(key) = self.make(|_, values| values.array_iter_key(value, position))? else { unreachable!(); };
                    let Some(item) = self.make(|_, values| values.array_get(value, key))? else { unreachable!(); };
                    if !self.plain_data(item, depth + 1)? { return Ok(false); }
                }
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}
