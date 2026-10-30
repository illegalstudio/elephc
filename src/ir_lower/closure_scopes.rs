//! Purpose:
//! Plans native closure entries for runtime-selected closures with an explicit binding scope.
//!
//! Called from:
//! - `crate::ir_lower::program::lower()` after all closure bodies have been collected.
//!
//! Key details:
//! - Clones preserve capture layouts and signatures; only lexical member visibility changes.
//! - Rebinding an already specialized entry maps through the same original closure family.
//! - Mixed `$this` captures remain runtime-shaped when a different class supplies the scope.

use std::collections::{BTreeMap, BTreeSet};

use crate::ir::{ClosureScopeVariant, Function, Immediate, Module, Op};
use crate::types::PhpType;

/// Adds the closed native entry mappings required by explicit-scope binding instructions.
pub(super) fn specialize_module(module: &mut Module) {
    let mut scopes = BTreeSet::new();
    let mut capture_counts = BTreeMap::new();
    for function in bodies(module) {
        for instruction in &function.instructions {
            if instruction.op == Op::ClosureBind {
                if let Some(Immediate::ClosureBindScope(scope)) = instruction.immediate {
                    scopes.insert(scope);
                }
                if let Some(Immediate::ClosureBindObjectScope(base)) = instruction.immediate {
                    scopes.insert(None);
                    for (name, class) in &module.class_infos {
                        if !class.is_internal && in_class_family(module, name, base) {
                            if let Ok(id) = u32::try_from(class.class_id) { scopes.insert(Some(id)); }
                        }
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
    if scopes.is_empty() { return; }
    let originals = module.closures.clone();
    for (index, original) in originals.iter().enumerate() {
        let Some(&capture_count) = capture_counts.get(&original.name) else { continue; };
        let Some(start) = original.params.len().checked_sub(capture_count) else { continue; };
        let captures = &original.params[start..];
        let Some(this) = captures.first() else { continue; };
        if this.name != "this" || this.by_ref || !(1..=2).contains(&capture_count)
            || (capture_count == 2 && captures[1].name != crate::names::CALLED_CLASS_ID_LOCAL)
        {
            continue;
        }
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
