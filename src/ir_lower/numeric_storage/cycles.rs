//! Purpose:
//! Finds every block with a nonempty explicit path back to itself.
//!
//! Called from:
//! - Numeric storage discovery, once per function.
//!
//! Key details:
//! - Iterative strongly connected components replace repeated per-block graph walks.
//! - Singleton self loops and disconnected components retain the original cycle predicate.

use std::collections::HashSet;

/// Uses iterative Kosaraju traversal, including self loops and unreachable components.
pub(super) fn cyclic_blocks(edges: &[Vec<usize>]) -> HashSet<usize> {
    let mut reverse = vec![Vec::new(); edges.len()];
    for (from, targets) in edges.iter().enumerate() {
        for &to in targets { reverse[to].push(from); }
    }
    let mut visited = vec![false; edges.len()];
    let mut order = Vec::with_capacity(edges.len());
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
    let mut cyclic = HashSet::new();
    for root in order.into_iter().rev() {
        if visited[root] { continue; }
        visited[root] = true;
        let mut component = Vec::new();
        let mut stack = vec![root];
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
            cyclic.extend(component);
        }
    }
    cyclic
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Preserves the previous independent nonempty-path predicate on every three-node graph.
    #[test]
    fn every_small_graph_matches_independent_searches() {
        for mask in 0..(1 << 9) {
            let edges: Vec<Vec<usize>> = (0..3).map(|from| {
                (0..3).filter(|to| mask & (1 << (from * 3 + to)) != 0).collect()
            }).collect();
            let expected: HashSet<_> = (0..3).filter(|&start| {
                let mut pending = edges[start].clone();
                let mut seen = HashSet::new();
                while let Some(block) = pending.pop() {
                    if block == start { return true; }
                    if seen.insert(block) { pending.extend(&edges[block]); }
                }
                false
            }).collect();
            assert_eq!(cyclic_blocks(&edges), expected, "graph {mask}");
        }
    }

    /// Deep acyclic paths stay acyclic without recursive traversal or repeated graph walks.
    #[test]
    fn deep_acyclic_graph_and_backedge_keep_exact_members() {
        let count = 20000;
        let mut edges: Vec<Vec<usize>> = (0..count).map(|node| {
            if node + 1 < count { vec![node + 1] } else { vec![] }
        }).collect();
        assert!(cyclic_blocks(&edges).is_empty());
        edges[count - 1].push(count - 3);
        assert_eq!(cyclic_blocks(&edges), (count - 3..count).collect());
    }

    /// Disconnected cycles, duplicate edges and singleton loops keep their original meaning.
    #[test]
    fn disconnected_components_and_self_loops_are_preserved() {
        let edges = vec![vec![1, 1], vec![], vec![3], vec![2], vec![4], vec![2, 4]];
        assert_eq!(cyclic_blocks(&edges), [2, 3, 4].into_iter().collect());
        assert!(cyclic_blocks(&[]).is_empty());
    }
}
