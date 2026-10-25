//! Purpose:
//! Excludes direct and mutual recursion from small-function and instance-method inlining.
//!
//! Called from:
//! - `super::inline_small_functions` before mutating any host function.
//!
//! Key details:
//! - Resolves user calls, statically bound instance calls and explicit user static-method calls.
//! - Only method bodies reachable from the initial candidates enter the graph, keeping unused
//!   prelude methods out of the per-candidate reachability walks.

use std::collections::{HashMap, HashSet};

use crate::ir::{Function, Module};
use crate::names::php_symbol_key;

use super::{is_user_call_op, methods, CallTargetResolver};

/// Finds candidate callees that can reach themselves, including through non-inlined method bodies.
pub(super) fn compute_recursive_callees(
    module: &Module,
    name_to_idx: &HashMap<String, usize>,
    resolver: &CallTargetResolver,
    methods: &methods::MethodTargets,
) -> HashSet<String> {
    let mut nodes: Vec<&Function> = module.functions.iter().chain(methods.bodies().iter()).collect();
    let roots = nodes.len();
    let mut by_name: HashMap<String, usize> = nodes.iter().enumerate()
        .map(|(index, body)| (php_symbol_key(&body.name), index)).collect();
    let method_bodies: HashMap<String, &Function> = module.class_methods.iter()
        .map(|body| (php_symbol_key(&body.name), body)).collect();
    let mut adjacency = vec![Vec::new(); nodes.len()];
    let mut index = 0;
    while index < nodes.len() {
        let function = nodes[index];
        for inst in &function.instructions {
            let target = if is_user_call_op(inst.op) {
                resolver.resolve(inst).and_then(|name| name_to_idx.get(&name).copied())
            } else if let Some(method) = methods.resolve(function, inst, &module.data.strings) {
                Some(module.functions.len() + method)
            } else if let Some(name) = methods::explicit_static_method_name(module, inst)
                .or_else(|| methods::bound_method_name(module, function, inst))
            {
                let key = php_symbol_key(&name);
                if let Some(&target) = by_name.get(&key) {
                    Some(target)
                } else if let Some(&body) = method_bodies.get(&key) {
                    let target = nodes.len();
                    nodes.push(body);
                    adjacency.push(Vec::new());
                    by_name.insert(key, target);
                    Some(target)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(target) = target { adjacency[index].push(target); }
        }
        index += 1;
    }
    let mut recursive = HashSet::new();
    for start in 0..roots {
        let mut visited = vec![false; nodes.len()];
        let mut stack = adjacency[start].clone();
        while let Some(node) = stack.pop() {
            if node == start {
                recursive.insert(nodes[start].name.clone());
                break;
            }
            if visited[node] { continue; }
            visited[node] = true;
            stack.extend(adjacency[node].iter().copied());
        }
    }
    recursive
}
