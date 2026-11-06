//! Purpose:
//! Eliminates direct self and mutual tail recursion using intra-function EIR branches.
//!
//! Called from:
//! - `driver::optimize_module`, after ordinary optimization and before ownership finalization.
//!
//! Key details:
//! - Only proven scalar frames and exact argument/return representations qualify.
//! - Mutual components retain every public ABI entry, cloning each body once per entry.
//! - The transformation runs once; subsequent scalar passes optimize the resulting loops.

use std::collections::HashMap;

use crate::ir::{BlockId, Function, Immediate, InstId, Module, Op, Terminator};
use crate::names::php_symbol_key;

mod eligibility;
mod numeric;
mod splice;

/// Caps duplicated body size per mutual component entry, without limiting self recursion.
const MAX_COMPONENT_INSTRUCTIONS: usize = 4096;
/// Bounds quadratic duplication across unusually large mutually recursive components.
const MAX_COMPONENT_FUNCTIONS: usize = 16;

/// A fully checked tail transition in one source body.
#[derive(Clone)]
struct TailSite {
    block: BlockId,
    call: InstId,
    callee: usize,
    erase: Vec<InstId>,
}

/// Replaces recursive tail-call components with bounded-stack EIR loops.
pub(super) fn eliminate_tail_recursion(module: &mut Module) -> bool {
    if eligibility::activation_chain_is_observable(module) {
        return false;
    }
    let names: HashMap<_, _> = module.functions.iter().enumerate()
        .map(|(index, function)| (php_symbol_key(&function.name), index)).collect();
    let eligible: Vec<_> = module.functions.iter().map(|function| eligibility::frame_is_safe(function, module)).collect();
    let sites: Vec<Vec<TailSite>> = module.functions.iter().enumerate().map(|(index, function)| {
        if !eligible[index] { return Vec::new(); }
        find_sites(function, module, &names, &eligible)
    }).collect();
    let edges: Vec<Vec<_>> = sites.iter().map(|sites| sites.iter().map(|site| site.callee).collect()).collect();
    let components = recursive_components(&edges);
    if components.is_empty() { return false; }
    let snapshots = module.functions.clone();
    let mut changed = false;
    for component in components {
        if component.len() > 1
            && (component.len() > MAX_COMPONENT_FUNCTIONS
                || component.iter().map(|&i| snapshots[i].instructions.len()).sum::<usize>()
                    > MAX_COMPONENT_INSTRUCTIONS)
        {
            continue;
        }
        for &host in &component {
            splice::lower_component(&mut module.functions[host], host, &component, &snapshots, &sites);
            changed = true;
        }
    }
    changed
}

/// Finds resolved calls with only scalar forwarding and proven numeric cleanup before return.
fn find_sites(
    function: &Function,
    module: &Module,
    names: &HashMap<String, usize>,
    eligible: &[bool],
) -> Vec<TailSite> {
    let mut sites = Vec::new();
    let roots = numeric::balanced_roots(function);
    let uses = function.value_use_counts();
    for block in &function.blocks {
        let Some(&call_id) = block.instructions.iter().rev().find(|&&id| {
            function.instructions[id.as_raw() as usize].op == Op::Call
        }) else { continue; };
        let call = &function.instructions[call_id.as_raw() as usize];
        let returned = if function.return_type == crate::ir::IrType::Void
            && call.result_php_type == crate::types::PhpType::Void
            && call.result.is_none_or(|result| uses.get(&result).copied().unwrap_or(0) == 0)
        { None } else { call.result };
        let Some(erase) = result_is_returned(function, block.id, call_id, returned, &roots) else { continue; };
        let Some(Immediate::Data(name)) = call.immediate else { continue; };
        let Some(name) = module.data.function_names.get(name.as_raw() as usize) else { continue; };
        let Some(&callee) = names.get(&php_symbol_key(name.trim_start_matches('\\'))) else { continue; };
        if eligible[callee] && eligibility::call_can_rebind(function, call, &module.functions[callee]) {
            sites.push(TailSite { block: block.id, call: call_id, callee, erase });
        }
    }
    sites
}

/// Follows scalar result forwarding through local slots, block arguments and return joins.
/// Only forwarding instructions in the call block are erased; shared return joins remain intact.
fn result_is_returned(
    function: &Function,
    start: BlockId,
    call: InstId,
    value: Option<crate::ir::ValueId>,
    roots: &std::collections::HashSet<crate::ir::LocalSlotId>,
) -> Option<Vec<InstId>> {
    let mut block_id = start;
    let mut values: std::collections::HashSet<_> = value.into_iter().collect();
    let mut slots = std::collections::HashSet::new();
    let mut visited = std::collections::HashSet::new();
    let mut erase = Vec::new();
    while visited.insert(block_id) {
        let block = &function.blocks[block_id.as_raw() as usize];
        let first = if block_id == start {
            block.instructions.iter().position(|&id| id == call)? + 1
        } else { 0 };
        for &id in &block.instructions[first..] {
            let inst = &function.instructions[id.as_raw() as usize];
            if inst.op == Op::Nop { continue; }
            if inst.op == Op::ConcatReset {
                if block_id != start { return None; }
                continue;
            }
            let Some(Immediate::LocalSlot(slot)) = inst.immediate else { return None; };
            if roots.contains(&slot)
                && matches!(inst.op, Op::PopCallOperandOwner | Op::ReleaseLocalSlot | Op::UnsetLocal)
            {
                // A direct branch bypasses shared return blocks, so cleanup must
                // already execute in the call block before its replacement edge.
                if block_id != start { return None; }
                continue;
            }
            match inst.op {
                Op::StoreLocal if inst.operands.len() == 1 && values.contains(&inst.operands[0]) => {
                    slots.insert(slot);
                }
                Op::LoadLocal if slots.contains(&slot) => { values.insert(inst.result?); }
                Op::UnsetLocal if slots.remove(&slot) => {}
                _ => return None,
            }
            if block_id == start { erase.push(id); }
        }
        match block.terminator.as_ref()? {
            Terminator::Return { value: returned } => {
                let matches = returned.map_or(value.is_none(), |returned| values.contains(&returned));
                return matches.then_some(erase);
            }
            Terminator::Br { target, args } => {
                let next = &function.blocks[target.as_raw() as usize];
                let forwarded: Vec<_> = args.iter().zip(&next.params).filter_map(|(arg, param)| {
                    values.contains(arg).then_some(*param)
                }).collect();
                values.extend(forwarded);
                block_id = *target;
            }
            _ => return None,
        }
    }
    None
}

/// Finds cyclic strongly connected components with iterative Kosaraju traversal.
/// Iterative traversal also keeps compiler stack use bounded for deep call graphs.
fn recursive_components(edges: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let mut reverse = vec![Vec::new(); edges.len()];
    for (from, targets) in edges.iter().enumerate() {
        for &to in targets { reverse[to].push(from); }
    }
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::new();
    for root in 0..edges.len() {
        if visited[root] { continue; }
        visited[root] = true;
        let mut stack = vec![(root, 0)];
        while let Some((node, next)) = stack.last_mut() {
            if let Some(&target) = edges[*node].get(*next) {
                *next += 1;
                if !visited[target] {
                    visited[target] = true;
                    stack.push((target, 0));
                }
            } else {
                order.push(*node);
                stack.pop();
            }
        }
    }
    visited.fill(false);
    let mut components = Vec::new();
    for root in order.into_iter().rev() {
        if visited[root] { continue; }
        let mut component = Vec::new();
        let mut stack = vec![root];
        visited[root] = true;
        while let Some(node) = stack.pop() {
            component.push(node);
            for &target in &reverse[node] {
                if !visited[target] {
                    visited[target] = true;
                    stack.push(target);
                }
            }
        }
        if component.len() > 1 || edges[root].contains(&root) {
            component.sort_unstable();
            components.push(component);
        }
    }
    components
}
