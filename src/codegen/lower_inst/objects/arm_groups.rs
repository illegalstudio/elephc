//! Purpose:
//! Collapses the per-class arms of a runtime-name property ladder into per-subtree arms.
//!
//! Called from:
//! - The Mixed-receiver runtime-name ladders in `runtime_property_writes`, which used to emit one
//!   arm for every `(runtime class, property name)` pair of the whole program.
//!
//! Key details:
//! - A subclass inherits its parent's property slots at the same offsets, so most pairs of one name
//!   carry the SAME action down a class chain. Measured on the compiled Symfony app, the shared
//!   write helpers held ~7 000 arms each for ~2 000 distinct (name, declaring class, action)
//!   triples, and the arms were 60% of every helper (about 12% of all property-ladder text).
//! - Per property name, a class is a TOP when its state (the action key, or "absent" when it has no
//!   arm) differs from its parent's. Every other class has its parent's state, so the nearest top
//!   ancestor-or-self of a class decides its action. Arms are emitted deepest top first; an
//!   `instanceof`-style subtree test (or an exact class-id compare for a leaf) then selects the
//!   first matching top, which is exactly that nearest top. An "absent" top is the arm that hands
//!   a subclass that dropped the name back to the ladder's no-match path.
//! - The grouping key is supplied by the caller and must cover everything the arm's code depends
//!   on. An arm whose code names the runtime class (a visibility refusal, a dynamic-property
//!   notice) keys on the class id, so it is its own top and never merges.
//! - `ELEPHC_NO_ARM_GROUPS=1` keeps one exact arm per pair, for size and behaviour comparisons.

use super::*;
use std::collections::HashMap;

/// One arm of a grouped ladder.
pub(super) struct ArmGroup {
    /// Class id the arm tests: exactly (a leaf) or as the root of a subtree.
    pub class_id: u64,
    /// The property name the arm answers for.
    pub property: String,
    /// Index of the input entry whose action the arm runs, or `None` for the "absent" arm that
    /// hands the receiver back to the ladder's no-match path.
    pub representative: Option<usize>,
    /// Whether the class has subclasses, so the test must cover the whole subtree.
    pub subtree: bool,
}

/// Returns whether grouping is switched off by the environment.
fn grouping_disabled() -> bool {
    std::env::var_os("ELEPHC_NO_ARM_GROUPS").is_some()
}

/// Groups `(class id, property, action key)` entries into the arms that answer them.
///
/// The entries are in ladder order; a repeated `(class id, property)` keeps its first entry, the
/// one the exact ladder would have matched. The result lists, per property in order of first
/// appearance, its arms deepest top first.
pub(super) fn group_arms(
    ctx: &FunctionContext<'_>,
    entries: &[(u64, &str, String)],
) -> Vec<ArmGroup> {
    if grouping_disabled() {
        let mut seen = crate::fast_hash::FastSet::default();
        return entries
            .iter()
            .enumerate()
            .filter(|(_, (class_id, property, _))| seen.insert((*class_id, property.to_string())))
            .map(|(index, (class_id, property, _))| ArmGroup {
                class_id: *class_id,
                property: property.to_string(),
                representative: Some(index),
                subtree: false,
            })
            .collect();
    }
    let graph = ClassGraph::new(ctx);
    let mut property_order: Vec<&str> = Vec::new();
    let mut by_property: HashMap<&str, Vec<(u64, usize)>> = HashMap::new();
    for (index, (class_id, property, _)) in entries.iter().enumerate() {
        let list = by_property.entry(*property).or_insert_with(|| {
            property_order.push(*property);
            Vec::new()
        });
        if !list.iter().any(|(seen, _)| seen == class_id) {
            list.push((*class_id, index));
        }
    }
    let mut groups = Vec::new();
    for property in property_order {
        let list = &by_property[property];
        let state: HashMap<u64, usize> = list.iter().copied().collect();
        let mut tops: Vec<(usize, u64, Option<usize>)> = Vec::new();
        for &(class_id, index) in list {
            let key = &entries[index].2;
            let parent_state = graph
                .parent(class_id)
                .and_then(|parent| state.get(&parent))
                .map(|parent_index| &entries[*parent_index].2);
            if parent_state != Some(key) {
                tops.push((graph.depth(class_id), class_id, Some(index)));
            }
            for &child in graph.children(class_id) {
                if !state.contains_key(&child) {
                    tops.push((graph.depth(child), child, None));
                }
            }
        }
        tops.sort_by(|left, right| right.0.cmp(&left.0).then(left.1.cmp(&right.1)));
        for (_, class_id, representative) in tops {
            groups.push(ArmGroup {
                class_id,
                property: property.to_string(),
                representative,
                subtree: graph.has_children(class_id),
            });
        }
    }
    groups
}

/// The class inheritance graph of the module, by class id.
struct ClassGraph {
    parents: HashMap<u64, u64>,
    children: HashMap<u64, Vec<u64>>,
}

impl ClassGraph {
    /// Builds the graph from the module's class table.
    fn new(ctx: &FunctionContext<'_>) -> Self {
        let mut parents = HashMap::new();
        let mut children: HashMap<u64, Vec<u64>> = HashMap::new();
        for info in ctx.module.class_infos.values() {
            let Some(parent_name) = info.parent.as_deref() else {
                continue;
            };
            let Some(parent) = class_info_by_name(ctx, parent_name) else {
                continue;
            };
            if parent.class_id == info.class_id {
                continue;
            }
            parents.insert(info.class_id, parent.class_id);
            children.entry(parent.class_id).or_default().push(info.class_id);
        }
        for list in children.values_mut() {
            list.sort_unstable();
        }
        Self { parents, children }
    }

    /// The parent class id, when the class has a known parent.
    fn parent(&self, class_id: u64) -> Option<u64> {
        self.parents.get(&class_id).copied()
    }

    /// The direct subclasses, in class-id order.
    fn children(&self, class_id: u64) -> &[u64] {
        self.children.get(&class_id).map_or(&[], Vec::as_slice)
    }

    /// Whether any class extends this one.
    fn has_children(&self, class_id: u64) -> bool {
        self.children.contains_key(&class_id)
    }

    /// The number of ancestors, bounded so a malformed cycle cannot hang the build.
    fn depth(&self, class_id: u64) -> usize {
        let mut depth = 0;
        let mut current = class_id;
        while let Some(parent) = self.parent(current) {
            depth += 1;
            current = parent;
            if depth > 256 {
                break;
            }
        }
        depth
    }
}
