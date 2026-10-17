//! Purpose:
//! Recognizes the compiler-only argument collector around an otherwise scalar closure.
//!
//! Called from:
//! - The loop closure signature gate and numeric argument proof.
//!
//! Key details:
//! - Only the authoritative generated variadic name is hidden from public arity.
//! - Exact public arity leaves this collector empty or containing scalar count metadata.
//! - Array values qualify only within `privatize_container_param`'s owning-shadow prologue.
//! - Introspection reads, escapes, user variadics and changed prologue shapes fail closed.

use crate::ir::{Function, Immediate, IrHeapKind, IrType, Op, Terminator, ValueId};
use crate::types::{FunctionSig, PhpType};
use std::collections::HashSet;

/// Returns exact public arity, admitting only the compiler's surplus-argument collector.
pub(super) fn visible_arity(signature: &FunctionSig) -> Option<usize> {
    if signature.variadic.is_none() {
        return Some(signature.params.len());
    }
    if !crate::func_args::sig_collects_surplus_args(signature) {
        return None;
    }
    let regular = crate::types::call_args::regular_param_count(signature);
    let (name, ty) = signature.params.get(regular)?;
    (regular + 1 == signature.params.len()
        && name == crate::func_args::HIDDEN_ARGS_PARAM
        && *ty == PhpType::Array(Box::new(PhpType::Mixed)))
        .then_some(regular)
}

/// Proves that every collector-array value belongs only to the generated ownership prologue.
pub(super) fn prologue_values(
    function: &Function,
    signature: &FunctionSig,
) -> Option<HashSet<ValueId>> {
    let arity = visible_arity(signature)?;
    if signature.variadic.is_none() {
        return Some(HashSet::new());
    }
    let name = crate::func_args::HIDDEN_ARGS_PARAM;
    let array = PhpType::Array(Box::new(PhpType::Mixed));
    let ir = IrType::Heap(IrHeapKind::Array);
    let param = function.params.get(arity)?;
    if param.name != name || !param.variadic || param.by_ref
        || param.php_type != array || param.ir_type != ir
    {
        return None;
    }
    let source = function.locals.iter().find(|local| local.name.as_deref() == Some(name))?;
    let shadow_name = format!("{name}#cow");
    let shadow = function.locals.iter().find(|local| local.name.as_deref() == Some(shadow_name.as_str()))?;
    if source.php_type != array || shadow.php_type != array
        || source.ir_type != ir || shadow.ir_type != ir
    {
        return None;
    }
    let mut values = HashSet::new();
    let mut loaded = false;
    let mut stored = false;
    for inst in &function.instructions {
        let slot = match inst.immediate.as_ref() {
            Some(Immediate::LocalSlot(slot)) => Some(*slot),
            _ => None,
        };
        if slot == Some(source.id) && inst.op == Op::LoadLocal && inst.span.is_none() && !loaded {
            if inst.result_type != ir || inst.result_php_type != array { return None; }
            values.insert(inst.result?);
            loaded = true;
        } else if inst.op == Op::Acquire && inst.span.is_none()
            && inst.operands.len() == 1 && values.contains(&inst.operands[0])
        {
            if inst.result_type != ir || inst.result_php_type != array { return None; }
            values.insert(inst.result?);
        } else if slot == Some(shadow.id) && inst.op == Op::StoreLocal && inst.span.is_none()
            && inst.operands.len() == 1 && values.contains(&inst.operands[0]) && !stored
        {
            stored = true;
        } else if inst.op == Op::Release && inst.span.is_none()
            && inst.operands.len() == 1 && values.contains(&inst.operands[0])
        {
            // A lowering-time temporary may be retired after the owning shadow retains it.
        } else if slot == Some(source.id) || slot == Some(shadow.id)
            || inst.operands.iter().any(|value| values.contains(value))
        {
            return None;
        }
    }
    if !loaded || !stored || function.blocks.iter().any(|block| {
        matches!(&block.terminator, Some(Terminator::Return { value: Some(value) })
            if values.contains(value))
    }) {
        return None;
    }
    Some(values)
}
