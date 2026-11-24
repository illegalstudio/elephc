//! Purpose:
//! Joins the reads of a union-typed local across an `if`/`elseif` chain that has no `else`.
//!
//! Called from:
//! - `Checker::check_stmt` for `StmtKind::If`, in `stmt_check::control_flow`.
//!
//! Key details:
//! - The implicit fall-through of such a chain holds the complement of every guard rather than
//!   the type the local had on entry, so the join combines that complement with what each clause
//!   that falls out of the chain left behind (issue #2320).
//! - Only a join strictly narrower than the union is published, and only as a read view: the
//!   union stays the slot's storage contract, exactly as a narrowing store keeps it.
//! - A candidate is a stable local that a clause guards and that a falling-through clause wrote.
//!   Every other local keeps the ordinary conditional-flow join.

use std::collections::HashSet;

use crate::parser::ast::{Expr, ExprKind};
use crate::types::{PhpType, TypeEnv};

use super::super::loop_storage::visit_child_expressions;
use super::super::{Checker, GuardedLocalStorage};

/// A read view that is narrower than the union its local keeps as storage.
pub(super) struct NarrowedUnionJoin {
    /// The local the join belongs to.
    pub name: String,
    /// What every path reaching the code after the construct reads.
    pub read: PhpType,
    /// The union the local read as before the construct, kept as its storage contract.
    pub storage: PhpType,
}

/// One local an `elseif` chain may leave narrower than the union it holds.
struct Candidate {
    /// The local's name.
    name: String,
    /// The union the local read as where the chain started.
    entry: PhpType,
    /// What the local reads as where the current clause body starts, guards applied.
    clause_entry: Option<PhpType>,
    /// What each clause that falls out of the chain left, `None` where the fact was lost.
    exits: Vec<Option<PhpType>>,
    /// Whether a falling-through clause left something other than the view it started with.
    stored: bool,
}

/// The exits an `if`/`elseif` chain without `else` leaves in its union-typed locals.
pub(super) struct ChainJoin {
    /// The locals the chain's conditions name, in source order.
    candidates: Vec<Candidate>,
}

impl ChainJoin {
    /// Starts tracking the union-typed stable locals named by the chain's conditions.
    ///
    /// A single guarded `if` and a chain closed by `else` have their own joins, so `tracked`
    /// is false for them and nothing is collected.
    pub(super) fn begin<'a>(
        checker: &Checker, tracked: bool, conditions: impl Iterator<Item = &'a Expr>, env: &TypeEnv,
    ) -> Self {
        let mut candidates = Vec::new();
        if tracked {
            let mut seen = HashSet::new();
            for condition in conditions {
                collect_condition_locals(condition, &mut |name| {
                    if !seen.insert(name.to_string()) { return; }
                    let Some(entry @ PhpType::Union(_)) = env.get(name) else { return; };
                    // Only a boxed slot can be read as each of its members.
                    if !matches!(entry.codegen_repr(), PhpType::Mixed | PhpType::TaggedScalar)
                        || !checker.local_flow_read_is_stable(name)
                    {
                        return;
                    }
                    candidates.push(Candidate {
                        name: name.to_string(), entry: entry.clone(), clause_entry: None,
                        exits: Vec::new(), stored: false,
                    });
                });
            }
        }
        Self { candidates }
    }

    /// Remembers what each candidate reads as where a clause body starts, its guards applied.
    pub(super) fn enter_clause(&mut self, env: &TypeEnv) {
        for candidate in &mut self.candidates {
            candidate.clause_entry = env.get(&candidate.name).cloned();
        }
    }

    /// Records what a clause that falls out of the chain leaves in each candidate.
    pub(super) fn exit_clause(&mut self, env: &TypeEnv) {
        for candidate in &mut self.candidates {
            let exit = env.get(&candidate.name).cloned();
            candidate.stored |= exit != candidate.clause_entry;
            candidate.exits.push(exit);
        }
    }

    /// Joins every clause exit with the fall-through complement left in `env`.
    ///
    /// `guarded` lists the places the chain's guards narrowed with the types they had before
    /// their first guard. A candidate qualifies only when a guard narrowed the union it entered
    /// the chain with, so a condition that rebinds the local first keeps the ordinary join.
    pub(super) fn finish(
        self, checker: &Checker, guarded: &[(String, Option<PhpType>)], env: &TypeEnv,
    ) -> Vec<NarrowedUnionJoin> {
        self.candidates.into_iter().filter_map(|candidate| {
            if !candidate.stored || !checker.local_flow_read_is_stable(&candidate.name)
                || !guarded.iter().any(|(name, original)|
                    *name == candidate.name && original.as_ref() == Some(&candidate.entry))
            {
                return None;
            }
            let fall_through = env.get(&candidate.name).cloned();
            let mut read: Option<PhpType> = None;
            for exit in candidate.exits.iter().chain(std::iter::once(&fall_through)) {
                let exit = exit.as_ref()?;
                read = Some(read.map_or_else(|| exit.clone(),
                    |joined| checker.join_flow_views(&joined, exit)));
            }
            let read = read?;
            checker.union_join_is_narrower(&read, &candidate.entry).then(|| NarrowedUnionJoin {
                name: candidate.name, read, storage: candidate.entry,
            })
        }).collect()
    }
}

/// Reports every plain local a condition names, at any depth outside closure bodies.
fn collect_condition_locals(expr: &Expr, report: &mut dyn FnMut(&str)) {
    if let ExprKind::Variable(name) = &expr.kind {
        report(name);
    }
    visit_child_expressions(expr, &mut |child| collect_condition_locals(child, report));
}

impl Checker {
    /// Whether `read` holds strictly fewer values than `storage` while still fitting its slot.
    ///
    /// Containment is judged by value, so a reordered union (`null|Service` against
    /// `Service|null`) is not narrower, and neither is an `iterable` member that guards merely
    /// split into its `array` and `Traversable` halves.
    pub(super) fn union_join_is_narrower(&self, read: &PhpType, storage: &PhpType) -> bool {
        let members = match storage {
            PhpType::Union(members) => members.as_slice(),
            single => std::slice::from_ref(single),
        };
        let traversable = PhpType::Object("Traversable".to_string());
        let holds_every_member = members.iter().all(|member| match member {
            PhpType::Iterable => self.type_contains_values(read, member)
                || (self.type_contains_values(read, &PhpType::php_array())
                    && self.type_contains_values(read, &traversable)),
            _ => self.type_contains_values(read, member),
        });
        !holds_every_member && self.merged_assignment_type(storage, read).as_ref() == Some(storage)
    }

    /// Publishes a join that reads narrower than the union its slot keeps.
    ///
    /// Runs after the construct's conditional flow has been published. That step records the
    /// slot's physical contract when it can be wider than the union the construct started
    /// with (an enclosing guard or an earlier kept complement already narrowed the read), and
    /// such a contract has to survive: a later store of one of ITS members is still a store the
    /// slot holds. The construct's own entry union is recorded only when nothing wider is.
    pub(super) fn publish_narrowed_union_join(&mut self, join: NarrowedUnionJoin, env: &mut TypeEnv) {
        let retained = self.guarded_union_types.get(&join.name).is_some_and(|storage|
            self.merged_assignment_type(&storage.ty, &join.storage).as_ref() == Some(&storage.ty));
        if !retained {
            self.guarded_union_types.insert(join.name.clone(), GuardedLocalStorage {
                ty: join.storage, coalesced: false,
            });
        }
        env.insert(join.name, join.read);
    }
}
