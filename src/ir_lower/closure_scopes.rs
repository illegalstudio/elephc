//! Purpose:
//! Plans native closure entries for runtime-selected closures with an explicit binding scope.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` after all closure bodies have been collected.
//!
//! Key details:
//! - Clones preserve capture layouts and signatures; only lexical member visibility changes.
//! - Rebinding an already specialized entry maps through the same original closure family.
//! - A trailing hidden runtime-scope capture (`CLOSURE_SCOPE_LOCAL`) is carried through unchanged.
//! - Mixed `$this` captures remain runtime-shaped when a different class supplies the scope.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{ClosureScopeVariant, Function, Immediate, Module, Op};
use crate::types::PhpType;

/// Most compile-time scope variants one closure gets; past it, binds use the run-time stamp.
const MAX_VARIANTS_PER_CLOSURE: usize = 32;

/// Adds the closed native entry mappings required by explicit-scope binding instructions.
///
/// A bind site whose closure operand traces to known `ClosureNew` entries asks variants of those
/// closures only. Cloning every `$this`-capturing closure for every scope any site names costs
/// closures x scopes bodies: Symfony's runtime-scope binds name every class (1296), which cloned
/// 55 closures into 71,280 bodies and 19 M instructions before codegen started.
pub(super) fn specialize_module(module: &mut Module) {
    // Scopes asked at sites whose closure cannot be traced: every closure gets them.
    let mut global_scopes = BTreeSet::new();
    // Scopes asked at sites whose closure is exactly known, per original closure entry.
    let mut closure_scopes: BTreeMap<String, BTreeSet<Option<u32>>> = BTreeMap::new();
    let mut capture_counts = BTreeMap::new();
    for function in bodies(module) {
        for instruction in &function.instructions {
            if instruction.op == Op::ClosureBind {
                let mut site = BTreeSet::new();
                if let Some(Immediate::ClosureBindScope(scope)) = instruction.immediate {
                    site.insert(scope);
                }
                if let Some(Immediate::ClosureBindObjectScope(base)) = instruction.immediate {
                    site.insert(None);
                    // A scope only known at run time (`base == None`: a string, a mixed value, an
                    // untyped object) asks for NO compile-time variant: the bind site falls back to
                    // the plain bind plus the run-time scope stamp, which serves every class.
                    // Variants are only made for the classes of a statically known family.
                    if base.is_some() {
                        for (name, class) in &module.class_infos {
                            if !class.is_internal && in_class_family(module, name, base) {
                                if let Ok(id) = u32::try_from(class.class_id) { site.insert(Some(id)); }
                            }
                        }
                    }
                }
                if !site.is_empty() {
                    let traced = instruction.operands.first()
                        .and_then(|operand| traced_closure_names(module, function, *operand, 0));
                    match traced {
                        Some(names) => {
                            for name in names {
                                closure_scopes.entry(name).or_default().extend(site.iter().copied());
                            }
                        }
                        None => global_scopes.extend(site),
                    }
                }
            }
            if instruction.op == Op::ClosureNew {
                if let Some(Immediate::Data(data)) = instruction.immediate {
                    if let Some(name) = module.data.strings.get(data.as_raw() as usize) {
                        capture_counts.insert(name.clone(), instruction.operands.len());
                    }
                }
            }
        }
    }
    if global_scopes.is_empty() && closure_scopes.is_empty() { return; }
    let originals = module.closures.clone();
    for (index, original) in originals.iter().enumerate() {
        let Some(&capture_count) = capture_counts.get(&original.name) else { continue; };
        let mut scopes = global_scopes.clone();
        if let Some(own) = closure_scopes.get(&original.name) { scopes.extend(own.iter().copied()); }
        // A closure asked for in more scopes than this keeps the plain bind + run-time stamp.
        if scopes.is_empty() || scopes.len() > MAX_VARIANTS_PER_CLOSURE { continue; }
        if rebindable_shape(original, capture_count).is_none() { continue; }
        let Some(this) = original.params.get(original.params.len() - capture_count) else { continue; };
        let mut family = vec![original.name.clone()];
        let mut variants = Vec::new();
        for (scope_index, &scope) in scopes.iter().enumerate() {
            let lexical_class = scope.and_then(|id| module.class_infos.iter()
                .find(|(_, class)| class.class_id == u64::from(id))
                .map(|(name, _)| name.clone()));
            if scope.is_some() && lexical_class.is_none() { continue; }
            // A raw object capture already compiled against another layout cannot be retargeted.
            if this.php_type.codegen_repr() != PhpType::Mixed
                && original.lexical_class != lexical_class
            {
                continue;
            }
            let name = crate::names::internal_generated_function_name(
                "closure_scope", &[index as u64, scope_index as u64],
            );
            let mut variant = original.clone();
            variant.source_name = Some(original.source_name.clone().unwrap_or_else(|| original.name.clone()));
            variant.name = name.clone();
            variant.lexical_class = lexical_class;
            module.add_closure(variant);
            family.push(name.clone());
            variants.push((scope, name));
        }
        for (scope, name) in variants {
            module.closure_scope_variants.push(ClosureScopeVariant {
                scope_class_id: scope,
                source_names: family.clone(),
                function_name: name,
                capture_count,
            });
        }
    }
}

/// Includes every concrete runtime scope compatible with an object's declared class.
fn in_class_family(module: &Module, name: &str, base: Option<u32>) -> bool {
    let Some(base) = base else { return true; };
    let mut current = Some(name);
    while let Some(name) = current {
        let Some(class) = module.class_infos.get(name) else { return false; };
        if class.class_id == u64::from(base) { return true; }
        current = class.parent.as_deref();
    }
    false
}

/// Iterates every body that can construct or bind a closure, including generated wrappers.
fn bodies(module: &Module) -> impl Iterator<Item = &Function> {
    module.functions.iter().chain(&module.class_methods).chain(&module.closures)
        .chain(&module.fiber_wrappers).chain(&module.callback_wrappers)
        .chain(&module.extern_callback_trampolines).chain(&module.runtime_callable_invokers)
}

/// Names the `ClosureNew` entries a bind site's closure operand can only be, or `None` when it
/// may be anything else (a parameter, a call result, a captured or referenced variable).
///
/// Follows `Acquire` and the local slots the function itself fills: a slot qualifies only when
/// every instruction touching it is a plain load, store or release, it is no parameter, and every
/// stored value traces to a closure literal in turn.
fn traced_closure_names(
    module: &Module,
    function: &Function,
    value: crate::ir::ValueId,
    depth: usize,
) -> Option<BTreeSet<String>> {
    if depth > 8 { return None; }
    let crate::ir::ValueDef::Instruction { inst, .. } = function.value(value)?.def else { return None; };
    let instruction = function.instruction(inst)?;
    match instruction.op {
        Op::ClosureNew => {
            let Some(Immediate::Data(data)) = instruction.immediate else { return None; };
            let name = module.data.strings.get(data.as_raw() as usize)?;
            Some(BTreeSet::from([name.clone()]))
        }
        // A closure value normalized for a call, or rebound, is the same closure family.
        Op::Acquire | Op::NormalizeCallable | Op::ClosureBind => traced_closure_names(module, function, *instruction.operands.first()?, depth + 1),
        Op::LoadLocal => {
            let Some(Immediate::LocalSlot(slot)) = instruction.immediate else { return None; };
            let local = function.locals.get(slot.as_raw() as usize)?;
            if !matches!(
                local.kind,
                crate::ir::LocalKind::PhpLocal | crate::ir::LocalKind::HiddenTemp
                    | crate::ir::LocalKind::OwnedTemp | crate::ir::LocalKind::BorrowedTemp
            ) {
                return None;
            }
            if local.name.as_ref().is_some_and(|name| function.params.iter().any(|param| &param.name == name)) {
                return None;
            }
            let mut names = BTreeSet::new();
            let mut stored = false;
            for other in &function.instructions {
                let touches = match other.immediate {
                    Some(Immediate::LocalSlot(s)) => s == slot,
                    Some(Immediate::LocalSlotPair { first, second }) => first == slot || second == slot,
                    _ => false,
                };
                if !touches { continue; }
                match other.op {
                    // `PushCallOperandOwner` only roots the slot for call-unwind cleanup.
                    Op::LoadLocal | Op::UnsetLocal | Op::ZeroLocalSlot | Op::ReleaseLocalSlot
                    | Op::PushCallOperandOwner | Op::PopCallOperandOwner => {}
                    Op::StoreLocal => {
                        stored = true;
                        names.extend(traced_closure_names(
                            module, function, *other.operands.first()?, depth + 1,
                        )?);
                    }
                    _ => return None,
                }
            }
            stored.then_some(names)
        }
        _ => None,
    }
}

/// The capture shape a closure needs to be re-scoped: a by-value `$this`, optionally followed by
/// the called-class id, with the hidden runtime-scope capture last.
fn rebindable_shape(original: &Function, capture_count: usize) -> Option<usize> {
    let start = original.params.len().checked_sub(capture_count)?;
    let mut captures = &original.params[start..];
    let mut shape_count = capture_count;
    if captures.last().is_some_and(|last| last.name == crate::names::CLOSURE_SCOPE_LOCAL) {
        captures = &captures[..captures.len() - 1];
        shape_count -= 1;
    }
    let this = captures.first()?;
    if this.name != "this" || this.by_ref || !(1..=2).contains(&shape_count)
        || (shape_count == 2 && captures[1].name != crate::names::CALLED_CLASS_ID_LOCAL)
    {
        return None;
    }
    Some(shape_count)
}
