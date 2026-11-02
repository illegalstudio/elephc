//! Purpose:
//! Specializes already-direct numeric closure calls without boxing scalar arguments.
//!
//! Called from:
//! - The module fixed-point driver after scalar counter proofs can expose exact argument types.
//!
//! Key details:
//! - Closure identity and call-site admission remain lowering decisions.
//! - Original descriptors and Mixed entry points are retained for every other call site.
//! - Only closed, straight-line numeric bodies with scalar by-value arguments are cloned.

mod template;

use std::collections::{HashMap, HashSet};
use crate::ir::{Function, Immediate, IrHeapKind, IrType, Module, Op};
use crate::types::PhpType;
use template::Template;

/// Rewrites compatible call sites to at most sixteen private numeric variants per module round.
pub(super) fn specialize(module: &mut Module) -> bool {
    let mut names: HashSet<_> = module.functions.iter().chain(&module.closures)
        .map(|function| crate::names::php_symbol_key(function.name.trim_start_matches('\\'))).collect();
    let mut templates = HashMap::new();
    let mut resolved: HashSet<_> = module.functions.iter()
        .map(|function| crate::names::php_symbol_key(function.name.trim_start_matches('\\'))).collect();
    for closure in &module.closures {
        let key = closure.name.trim_start_matches('\\').to_owned();
        // Physical closure labels retain case even when PHP name reservation does not.
        if resolved.insert(crate::names::php_symbol_key(&key)) {
            if let Some(template) = Template::analyze(closure) { templates.insert(key, template); }
        }
    }
    if templates.is_empty() { return false; }
    let mut variants = HashMap::new();
    let mut first_names: HashSet<_> = module.functions.iter()
        .map(|function| crate::names::php_symbol_key(function.name.trim_start_matches('\\'))).collect();
    let mut existing = Vec::new();
    for function in &module.closures {
        if !first_names.insert(crate::names::php_symbol_key(function.name.trim_start_matches('\\'))) {
            continue;
        }
        for (key, template) in &templates {
            if let Some(tag) = template.reusable_tag(function) {
                existing.push(((key.clone(), tag), function.name.clone()));
            }
        }
    }
    for (key, name) in existing {
        variants.entry(key).or_insert_with(|| module.data.intern_function_name(&name));
    }
    let mut generated = Vec::new();
    let mut changed = false;
    for function in module.functions.iter_mut()
        .chain(&mut module.class_methods)
        .chain(&mut module.closures)
        .chain(&mut module.fiber_wrappers)
        .chain(&mut module.callback_wrappers)
        .chain(&mut module.extern_callback_trampolines)
        .chain(&mut module.runtime_callable_invokers)
    {
        for index in 0..function.instructions.len() {
            let call = &function.instructions[index];
            if call.op != Op::Call || call.result_type != IrType::Heap(IrHeapKind::Mixed)
                || call.result_php_type != PhpType::Mixed
            {
                continue;
            }
            let Some(Immediate::Data(target)) = call.immediate else { continue; };
            let Some(name) = module.data.function_names.get(target.as_raw() as usize) else { continue; };
            let key = name.trim_start_matches('\\').to_owned();
            let Some(template) = templates.get(&key) else { continue; };
            let Some(types) = argument_types(function, index, template) else { continue; };
            let tag: String = types.iter().map(|ty| if *ty == PhpType::Int { 'i' } else { 'f' }).collect();
            let variant_key = (key, tag.clone());
            let target = if let Some(&target) = variants.get(&variant_key) { target } else {
                if generated.len() == 16 { continue; }
                let mut suffix = 0;
                let name = loop {
                    let candidate = format!("__eir_numeric_closure_{}_{}_{}", template.index, tag, suffix);
                    if names.insert(crate::names::php_symbol_key(&candidate)) { break candidate; }
                    suffix += 1;
                };
                let clone = template.emit(&name, &types);
                let target = module.data.intern_function_name(&name);
                generated.push(clone);
                variants.insert(variant_key, target);
                target
            };
            function.instructions[index].immediate = Some(Immediate::Data(target));
            changed = true;
        }
    }
    for clone in generated { module.add_closure(clone); }
    changed
}

/// Requires each actual physical argument to be exactly int or float, including hidden captures.
fn argument_types(function: &Function, call_index: usize, template: &Template) -> Option<Vec<PhpType>> {
    let call = &function.instructions[call_index];
    if call.operands.len() != template.params.len() { return None; }
    call.operands.iter().zip(&template.params).map(|(&argument, parameter)| {
        let value = function.value(argument)?;
        if !matches!((&value.ir_type, &value.php_type),
            (IrType::I64, PhpType::Int) | (IrType::F64, PhpType::Float))
            || (*parameter != PhpType::Mixed && *parameter != value.php_type)
        {
            return None;
        }
        Some(value.php_type.clone())
    }).collect()
}
