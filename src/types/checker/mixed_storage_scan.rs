//! Purpose:
//! Syntactic per-body pre-scan that marks branch-divergently assigned locals as whole-frame
//! boxed `Mixed` storage, so `if (…) { $a = 0; } else { $a = "ciao"; }` compiles instead of
//! being rejected.
//!
//! Called from:
//! - `crate::types::checker::Checker::with_local_storage_context` (function/method/closure bodies)
//! - `crate::types::checker::Checker::check_top_level_program` (the top-level body)
//!
//! Key details:
//! - Runs BEFORE the body is statement-checked, because the decision has to be in place at the
//!   local's FIRST store — that is where the slot's type is fixed. The mark is not merely an
//!   initial type, though: `merge_local_assignment_type` re-asserts `PhpType::Mixed` for a marked
//!   name on EVERY assignment path, so neither the retype hook nor the "cannot reassign" error can
//!   fire for it even where flow narrowing has just re-typed the name for a guarded branch.
//! - Purely syntactic. It never consults the type environment, so it produces the same answer on
//!   every one of the checker's repeated walks over the same body (top level twice, method bodies
//!   to stability, function bodies once per call-site specialization).
//! - Disabled under `--strict-locals`: the scan returns without marking anything and a divergent
//!   assignment errors exactly as it does today.
//! - The same walk also answers a question that has nothing to do with marking: does this body
//!   call `eval()` at all? It is recorded in `Checker::body_contains_eval` and consulted by
//!   `Checker::local_binding_is_killable`. The walk is the natural place for it because it is
//!   the only per-body pass that runs BEFORE the first statement is checked (a point-in-time
//!   "have we crossed an eval yet" flag cannot see an eval BELOW the `unset` it has to veto),
//!   and because it runs in BOTH modes — the collect above is not gated on `--strict-locals`.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::errors::CompileWarning;
use crate::names::Name;
use crate::parser::ast::{
    is_compound_assignment_self_read, CallableTarget, CatchClause, Expr, ExprKind,
    InstanceOfTarget, Stmt, StmtKind,
};
use crate::span::Span;
use crate::types::PhpType;

use super::{infer_expr_type_syntactic, is_unset_call, Checker};

/// One statement-form assignment to a candidate local.
struct AssignSite {
    /// The value's syntactic type. Only recorded for values this scan can type EXACTLY
    /// (see [`has_exact_syntactic_type`]).
    ty: PhpType,
    /// Syntactic conditional nesting depth, mirroring `Checker::local_conditional_depth`.
    depth: u32,
    /// The assignment STATEMENT's span, which is what EIR lowering looks the decision up by.
    span: Span,
    /// The [`GuardRegion`] that GOVERNS this assignment: the INNERMOST still-open guard whose
    /// subject is this same name (`if (is_string($a)) { $a = "x"; }`), or `None` outside any.
    ///
    /// Innermost, because the checker's narrowings COMPOSE — `narrow_to` narrows from whatever the
    /// environment holds at that moment, so an inner `is_float($a)` re-narrows the `string` an
    /// outer `is_string($a)` had just installed, and it is the INNER target the assignment is
    /// merged against.
    ///
    /// Whether the site is actually skipped as evidence is a property of the whole region rather
    /// than of this one assignment — see [`GuardRegion`] and
    /// [`Checker::first_rejected_assignment`].
    guard_region: Option<usize>,
}

/// One entered guard: `if (is_string($a)) { … }` opens a region over its body, governing every
/// assignment to `$a` inside it for which no NESTED guard on `$a` is open.
///
/// The checker narrows the name to `target` on entry, carries each in-branch assignment forward to
/// the next statement of the same branch, and restores the pre-branch binding when the branch ends.
/// A region is therefore TRANSPARENT — invisible to the marking, because the checker accepts all of
/// it and leaves the outer binding untouched — exactly when replaying its own assignments in order,
/// starting from `target`, merges every time. If any merge fails the checker rejects inside the
/// region, and every assignment the region governs goes back to being ordinary evidence.
struct GuardRegion {
    /// The guard's subject. Regions live in one per-BODY list while `sites` indexes one per-NAME
    /// assignment list, so transparency must only ever be computed for the name that owns them.
    name: String,
    /// The type the guard narrows its subject to for the guarded branch.
    target: PhpType,
    /// Indices into the subject's [`NameFacts::assigns`], in source order, of the assignments this
    /// region governs. Not necessarily contiguous: a nested region on the same name takes the
    /// assignments written between them.
    sites: Vec<usize>,
}

/// Everything the scan learned about one name in one body.
#[derive(Default)]
struct NameFacts {
    /// Statement-form assignments, in source order.
    assigns: Vec<AssignSite>,
    /// Set by any write or aliasing shape that is not a plain statement-form assignment.
    disqualified: bool,
}

/// Everything one scan of one body collects.
#[derive(Default)]
struct Facts {
    /// Per-name evidence for the body being scanned, ordered so warnings are deterministic.
    names: BTreeMap<String, NameFacts>,
    /// Whether an `eval()` call appears ANYWHERE in this body, above or below any other
    /// statement. Body-scoped rather than point-in-time on purpose: see
    /// [`Checker::local_binding_is_killable`], which has to veto an `unset` that sits ABOVE the
    /// only `eval` in the body.
    ///
    /// A closure written inside this body does NOT contribute: [`collect_expr`] never descends
    /// into a closure body, which gets its own scan (and its own flag) at its own entry point.
    /// That is the same per-body rule every other field of the scan follows, and it is sound for
    /// the same reason: an `eval` inside the closure addresses the CLOSURE's scope by name, and
    /// the one shape that lets it reach an enclosing local — a `use (&$x)` capture — already
    /// makes `$x` reference-aliased and therefore neither killable nor re-bindable out here.
    contains_eval: bool,
    /// Plain local roots mentioned by `unset()` anywhere in this body.
    unset_names: HashSet<String>,
    /// Every guard region opened while walking this body, indexed by the id stored in
    /// [`AssignSite::guard_region`]. Kept after the region closes: transparency is decided later,
    /// once all of the region's assignments are known.
    guard_regions: Vec<GuardRegion>,
    /// Stack of `(guarded name, region id)` for the guards currently OPEN, innermost last. One name
    /// may appear more than once — nested guards on it — which is why this is a stack rather than a
    /// map, and why the lookup takes the last matching entry.
    open_guards: Vec<(String, usize)>,
}

impl Checker {
    /// Marks the locals of `body` that are assigned incompatible types across a branch, so their
    /// frame storage becomes boxed `Mixed` for the whole body.
    ///
    /// Fills `mixed_storage_locals` for this body (consumed by `merge_local_assignment_type`),
    /// records every marked name's assignment spans in `mixed_storage_store_sites` (consumed by
    /// EIR lowering), and files ONE warning per marked name. Under `--strict-locals` nothing is
    /// marked and nothing is recorded.
    ///
    /// `pre_bound_own_storage` maps each name the body's OWN frame already holds on entry to the
    /// type it arrives with: a closure's by-VALUE captures, and this body's parameters. It is NOT
    /// the body's incoming environment — a closure body starts from a clone of the whole enclosing
    /// scope, so keying on that silenced a FRESH closure local merely because some enclosing scope
    /// bound the same name (measured: the body then compiled with no diagnostic at all while
    /// `--strict-locals` rejected it).
    ///
    /// A name from that map is replayed from the type it arrives with, and that ONE replay answers
    /// both questions. If it rejects, the name is marked AND warned about: `--strict-locals` really
    /// would report `cannot reassign`, so the advice is true. If it merges clean, strict compiles
    /// the body clean and the advice would be false — so anything the ordinary unseeded replay
    /// still finds is marked SILENTLY. Marking is never withheld either way, because the boxed
    /// store type is what releases the value a capture slot arrived holding.
    ///
    /// PARAMETERS are excluded from marking altogether — typed or not, by reference or by value —
    /// through `local_binding_depth`; see [`Checker::first_rejected_assignment`] for why. They are
    /// still listed here so the contract reads as "the frame's own pre-bound storage" rather than
    /// "captures", which is what it means.
    pub(crate) fn run_mixed_storage_scan(
        &mut self,
        body: &[Stmt],
        pre_bound_own_storage: &HashMap<String, PhpType>,
    ) {
        // The marking describes ONE frame. The caller saves and restores the enclosing body's
        // set, but clear it here too so the scan owns the whole decision for this body.
        self.mixed_storage_locals.clear();

        let mut facts = Facts::default();
        collect_block(self, body, 0, &mut facts);

        // Recorded for EVERY body and in BOTH modes, because it does not gate marking: it gates
        // `Checker::local_binding_is_killable`, which the `unset` kill and the straight-line
        // retype both consult and which `--strict-locals` does not switch off (the kill is
        // mode-independent). Installed before the early return below for exactly that reason.
        self.body_contains_eval = facts.contains_eval;
        self.unset_mentioned_locals = facts.unset_names.clone();

        // This visit RE-DECIDES every assignment site in this body, so a decision a SUPERSEDED
        // walk recorded for one of them is dropped first. The checker walks a body more than
        // once and only the LAST walk's decisions may reach EIR lowering; a store site left
        // behind by an earlier walk that marked a name this walk does not would give lowering a
        // boxed slot the checker no longer types as `Mixed`. Mirrors the same re-decision in
        // `merge_local_assignment_type` and in the `unset` kill sites.
        //
        // Qualified by NAME as well as span, because a `Span` has no file identity: the same
        // (line, column) in an included file is an EQUAL span, and this walk must not drop the
        // decision recorded for that unrelated assignment. The map's value is a SET of names for
        // exactly that reason, so removing this body's name leaves any other body's name at the
        // same position untouched (before that, the insert of a second name silently evicted the
        // first and the compiler panicked on valid PHP — see the field's own documentation).
        //
        // The name is not enough on its own, though: the same NAME at the same POSITION in two
        // files is still one key, so this loop can drop a decision another body recorded and
        // leave `reject_ambiguous_local_binding_decisions` with nothing to reject. Every removed
        // key is therefore retired into `retired_mixed_storage_store_sites`, which that pass
        // checks against the node tally exactly as it checks a live one — see the field's own
        // documentation for why the kill and retype maps do not need the same treatment.
        for (name, name_facts) in &facts.names {
            for site in &name_facts.assigns {
                let Some(recorded) = self.mixed_storage_store_sites.get_mut(&site.span) else {
                    continue;
                };
                if !recorded.remove(name) {
                    continue;
                }
                // A span whose last decision just left carries none at all: keep it out of the
                // map so `local_binding_decision_spans` and lowering's consult never see an
                // empty entry.
                if recorded.is_empty() {
                    self.mixed_storage_store_sites.remove(&site.span);
                }
                self.retired_mixed_storage_store_sites
                    .insert((site.span, name.clone()));
                // The mark's WARNING is filed against one of these very site keys, so retiring
                // them retracts it. A walk that re-marks the name re-files it below; a walk that
                // does not leaves no warning for a decision that is no longer there. See
                // `Checker::binding_decision_warnings`.
                self.binding_decision_warnings
                    .remove(&(site.span, name.clone()));
            }
        }

        if self.strict_locals {
            return;
        }

        // Collected before pushing so the warnings come out in source order regardless of the
        // name ordering the evidence map imposes. The flag is "say it out loud".
        let mut marks: Vec<(Span, String, String, bool)> = Vec::new();
        for (name, name_facts) in &facts.names {
            // Eligibility first, so the transparency vector below — O(regions x sites) — is only
            // built for names that can still be marked.
            if !self.name_is_markable(name, name_facts) {
                continue;
            }
            let incoming = pre_bound_own_storage.get(name);
            // A guard region holding the name's FIRST in-body assignment is transparent for a
            // PRE-BOUND name and not for a fresh one, so the vector depends on which this is.
            let transparent_regions = self.transparent_guard_regions(
                name,
                name_facts,
                &facts.guard_regions,
                incoming.is_some(),
            );
            let (rejection, warns) = match incoming {
                // A name the frame already holds on entry is replayed from the type it ARRIVES
                // with, and never through the depth-0 retype arm: it has no binding depth this
                // body recorded, so `local_binding_is_killable` refuses it and every store has to
                // merge. That replay is exactly what `--strict-locals` does to such a name, which
                // makes it both the evidence AND the truth about the warning's advice.
                Some(incoming) => match self.first_rejected_assignment(
                    name,
                    name_facts,
                    &transparent_regions,
                    Some(incoming),
                ) {
                    Some(rejection) => (Some(rejection), true),
                    // Strict accepts the body, so anything marked here must be marked SILENTLY —
                    // the advice would be false. The unseeded replay still gets to ask whether the
                    // local needs boxed storage at all: a capture arriving as a union absorbs
                    // every store the body makes, yet its slot must still release the value it
                    // arrived holding.
                    None => (
                        self.first_rejected_assignment(name, name_facts, &transparent_regions, None),
                        false,
                    ),
                },
                None => (
                    self.first_rejected_assignment(name, name_facts, &transparent_regions, None),
                    true,
                ),
            };
            let Some((existing, site)) = rejection else {
                continue;
            };
            marks.push((
                site.span,
                name.clone(),
                format!(
                    "${} is assigned incompatible types ({} and {}); it is compiled as boxed \
                     mixed storage (compile with --strict-locals to make this an error)",
                    name, existing, site.ty
                ),
                warns,
            ));
        }
        // `sort_by` rather than `sort_by_key`: a key closure would have to CLONE the name to own
        // the tuple it returns, once per comparison.
        marks.sort_by(|(left_span, left_name, ..), (right_span, right_name, ..)| {
            (left_span.line, left_span.col, left_name).cmp(&(
                right_span.line,
                right_span.col,
                right_name,
            ))
        });
        for (span, name, message, warns) in marks {
            // A pre-bound name whose incoming type absorbs everything the body stores is marked
            // SILENTLY: the mark and the store sites are recorded exactly as for any other name,
            // but the warning — whose advice clause would claim a `--strict-locals` rejection that
            // will not happen — is withheld. See this method's own documentation.
            if warns {
                // Keyed by the DECISION, so a later walk that stops marking this name retracts the
                // warning with the store sites. `span` is one of the name's own store-site spans
                // (the first rejected assignment), which is what makes the retire loop above find
                // it.
                self.binding_decision_warnings
                    .insert((span, name.clone()), CompileWarning::new(span, &message));
            }
            for site in &facts.names[&name].assigns {
                self.mixed_storage_store_sites
                    .entry(site.span)
                    .or_default()
                    .insert(name.clone());
            }
            self.mixed_storage_locals.insert(name);
        }
    }

    /// Returns the first assignment to `name` that the checker will REJECT, paired with the type
    /// the binding holds when it is reached — or `None` when the name is not markable at all.
    ///
    /// The assignments are replayed in source order through `merged_assignment_type`, exactly as
    /// `merge_local_assignment_type` will, instead of being compared pairwise. Pairwise comparison
    /// looks at conflicts the running binding has already resolved: in `$a = "s"; $a = 0; if (…) {
    /// $a = 1; }` the pair (`string`, `int` at depth 1) fails, but by the time the branch is
    /// reached `$a` has been re-bound to `int` by the depth-0 retype path and the program compiles
    /// today. Marking it would box a local for a conflict that no longer exists.
    ///
    /// The plan's "at least one member of the failing pair at depth > 0" rule falls out of the
    /// replay rather than being tested separately: a conflict where BOTH the binding and the new
    /// assignment sit at depth 0 is exactly the one `Checker::local_binding_is_killable` accepts,
    /// so it is left to the fresh-binding retype path — two unboxed slots, strictly better codegen.
    fn first_rejected_assignment<'a>(
        &self,
        name: &str,
        facts: &'a NameFacts,
        transparent_regions: &[bool],
        incoming: Option<&PhpType>,
    ) -> Option<(PhpType, &'a AssignSite)> {
        debug_assert!(
            self.name_is_markable(name, facts),
            "first_rejected_assignment must run behind name_is_markable",
        );
        self.replay_assignments(facts, transparent_regions, incoming)
    }

    /// Returns whether `name` is eligible for marking at all, before any evidence is weighed.
    ///
    /// Separated from the replay so the caller can answer it BEFORE building the per-name
    /// transparency vector, which walks every guard region in the body.
    fn name_is_markable(&self, name: &str, facts: &NameFacts) -> bool {
        if facts.disqualified || facts.assigns.len() < 2 {
            return false;
        }
        // A reference-aliased name is NEVER markable: the mark gives the local one boxed `Mixed`
        // slot, and for a `use (&$m)` capture that slot IS the caller's storage, so the boxed
        // pointer would be written straight through the alias into the enclosing frame. Measured
        // before this check: the caller's `var_dump($m)` printed a raw pointer (`int(4378264920)`)
        // where PHP prints `string(1) "s"`.
        //
        // `active_ref_params` is the set to consult rather than `ref_aliased_locals`, which
        // `enter_local_binding_scope` has just emptied (a name aliased in the CALLER says nothing
        // about this body). `with_local_storage_context` installs `active_ref_params` — by-ref
        // parameters AND the closure's by-ref captures — before running this scan, and it is the
        // same set `local_binding_is_killable` consults, so the two cannot drift. By-ref captures
        // of closures written INSIDE this body are handled separately, by `disqualify_root`.
        if self.active_ref_params.contains(name) {
            return false;
        }
        // EVERY parameter is excluded, typed or not, by reference or by value.
        //
        // The exclusion is load-bearing, not merely tidy. `merge_local_assignment_type` honours
        // the mark on EVERY assignment path (it used to consult it only on the fresh-insert
        // branch, which a parameter — bound before the first statement — could never take), so a
        // marked parameter would really be re-typed `Mixed` for the whole body and really have its
        // stores boxed. That is storage the parameter already has by another route: an untyped
        // by-value parameter called with two incompatible argument types is `mixed` on the
        // pre-existing specialization path, and a typed one is a contract. Marking it would take
        // the credit for storage this feature did not create, box a slot on the strength of
        // SYNTACTIC evidence where the specialization already has the real types, and file store
        // sites that block DCE tail-sinking and enter the binding-decision ambiguity tally for
        // nothing.
        //
        // `local_binding_depth` is exactly the parameter set here: `enter_local_binding_scope`
        // seeds it with every parameter at depth 0 and this scan runs before the first statement
        // is checked, so nothing else has been bound into it yet. It is also the only one of the
        // per-body sets that is authoritative at top level, where it is empty. By-reference and
        // type-hinted parameters were already excluded via `active_ref_params` /
        // `typed_local_names`; this subsumes both.
        if self.local_binding_depth.contains_key(name) {
            return false;
        }
        // Names the body's INCOMING environment already binds and whose storage the body does NOT
        // own: a superglobal (seeded into every scope, living in shared `_eir_global_*` storage),
        // and `$argc`/`$argv` plus every extern C global (seeded into the TOP-LEVEL environment by
        // `seed_global_env`). Marking one is wrong twice over — the warning credits this feature
        // for storage it never created, and a marked name is bound `PhpType::Mixed` at every
        // assignment, so the mark would BOX program-wide storage the rest of the compiler reaches
        // at its declared type. Measured before this exclusion: `$argv = 1; if (…) { $argv = "s"; }`
        // and the same shape on `$_SESSION` both warned and type-checked, where they are the
        // pre-existing hard error.
        //
        // By-VALUE closure captures are deliberately NOT excluded, even though they too are bound
        // on entry. A capture arrives as a hidden parameter and lives in a slot the CLOSURE's frame
        // owns, so the mark is load-bearing there rather than spurious: the boxed store type is
        // what releases the previous occupant of the capture slot
        // (`test_marked_local_captured_by_value_and_overwritten_in_a_closure`, measured leaking 48
        // bytes without it).
        if self.name_is_seeded_program_storage(name) {
            return false;
        }
        // A decision is only usable if EVERY one of the name's store sites can be named. A
        // `Span::dummy()` identifies no node — it is what every compiler-generated AST node
        // carries — so lowering would either miss the first store (leaving the slot unboxed
        // while the checker types the local `Mixed`) or box unrelated dummy-span assignments
        // across the whole program. Refusing to mark keeps the checker and lowering in
        // lock-step: the body simply reports today's error instead.
        facts
            .assigns
            .iter()
            .all(|site| site.span.identifies_a_node())
    }

    /// Replays a markable name's assignments in source order and returns the first one the checker
    /// will REJECT, paired with the type the binding holds when it is reached.
    ///
    /// `incoming` is `Some` for a name the frame already holds on entry (a by-value capture, a
    /// parameter). Such a name starts from that type instead of from its first store, and never
    /// takes the depth-0 retype arm below: `local_binding_is_killable` needs a binding depth THIS
    /// body recorded, which a capture has not got, so the checker rejects where an unseeded replay
    /// would have re-bound. Both shapes that exposed this hard-errored in permissive mode on code
    /// PHP runs — `use ($m)` then `$m = 1; $m = "s";` at depth 0, and `$m = null;` first (an
    /// unseeded replay starts at `Void`, which absorbs the later `string`, while the checker starts
    /// at the capture's `int`).
    fn replay_assignments<'a>(
        &self,
        facts: &'a NameFacts,
        transparent_regions: &[bool],
        incoming: Option<&PhpType>,
    ) -> Option<(PhpType, &'a AssignSite)> {
        // `(type the binding currently holds, conditional depth the binding was created at)`,
        // mirroring `Checker::local_binding_depth` for this one name. A pre-bound name is seeded
        // at depth 0 — it is bound before the body's first statement — which is also what stops
        // the depth-0 retype arm from applying, since that arm is guarded on eligibility this name
        // does not have.
        let mut binding: Option<(PhpType, u32)> = incoming.map(|ty| (ty.clone(), 0));
        for site in &facts.assigns {
            // An assignment governed by a TRANSPARENT guard region is not a conflict the checker
            // will ever report, and it does not move the binding either. Flow narrowing replaced
            // the name's type with the guard's TARGET for the guarded body (`control_flow` inserts
            // `guard.then_ty` before checking it) and restores the pre-branch binding when the
            // branch ends, so a region the checker accepts in full leaves nothing behind.
            //
            // Skipping those is what stops the scan predicting a rejection the checker never
            // makes: `$a = 1; if (is_string($a)) { $a = "x"; }` used to warn "compile with
            // --strict-locals to make this an error" while `--strict-locals` compiled it CLEAN,
            // and boxed the frame slot (plus blocked constant propagation for the name
            // program-wide) for nothing.
            //
            // Transparency is a property of the WHOLE region, decided by replaying its own
            // assignments from the guard target (see `guard_region_is_transparent`), because the
            // checker carries an in-branch assignment forward to the next statement of the same
            // branch. Judging each assignment against the target on its own was measured wrong
            // three ways, all of them hard errors in permissive mode on code PHP runs:
            // `if (is_string($a)) { $a = "x"; $a = 2; }`, the same across two arms of a nested
            // non-guard `if`, and `if (is_string($a)) { if (is_float($a)) { $a = "x"; } }` — the
            // last one because the acceptance test consulted an outer frame instead of the
            // innermost one, which `record_assign` now selects.
            //
            // The skip is deliberately conditioned on the name ALREADY having a binding: a guard
            // does not narrow a name that has none yet (`guard_narrowing` bails on an unbound
            // plain variable), so the FIRST assignment to a name still counts even when it sits
            // inside a guard naming it.
            //
            // Interplay with the mark's authority in `merge_local_assignment_type`: a name marked
            // on OTHER evidence stays marked, keeps every store site (this loop only decides
            // whether to mark, never which sites to record), and the mark then dominates the
            // narrowing at the guarded assignment too. So the two rules compose: marks win over
            // guards, and a name whose only conflicts are transparently guarded is never marked.
            if binding.is_some()
                && site
                    .guard_region
                    .is_some_and(|region| transparent_regions[region])
            {
                continue;
            }
            let Some((existing, binding_depth)) = binding else {
                binding = Some((site.ty.clone(), site.depth));
                continue;
            };
            if let Some(merged) = self.merged_assignment_type(&existing, &site.ty) {
                binding = Some((merged, binding_depth));
                continue;
            }
            // The plan asks for the merge to fail in BOTH directions before a conflict counts.
            // The checker only tries `existing -> new`, so a one-way failure is left alone: the
            // body keeps today's diagnostic instead of being quietly boxed.
            //
            // Over the types this scan can actually see, no pair reaches here: `record_assign`
            // admits only the EXACT syntactic types (`Int`, `Float`, `Str`, `Bool`, `Void`, the
            // scalar casts, and `Str` from a concatenation), and `merged_assignment_type` is
            // symmetric on every one of those. Kept as a defensive net rather than deleted,
            // because widening `has_exact_syntactic_type` is the natural next change and an
            // asymmetric pair would otherwise be marked on one direction's answer alone.
            if let Some(merged) = self.merged_assignment_type(&site.ty, &existing) {
                binding = Some((merged, binding_depth));
                continue;
            }
            if incoming.is_none() && site.depth == 0 && binding_depth == 0 {
                // `local_binding_is_killable` accepts this: the depth-0 retype path re-binds the
                // name to a fresh slot of the new type, and the replay follows it. Only for a name
                // this body CREATED — a pre-bound one has no binding depth recorded here, so the
                // predicate refuses it and the checker reports the conflict instead.
                binding = Some((site.ty.clone(), 0));
                continue;
            }
            return Some((existing, site));
        }
        None
    }

    /// Decides, for one name, which guard regions are transparent to it.
    ///
    /// Computed once per name and handed to both replays, because a region's verdict depends on
    /// assignments a sequential replay has not reached yet.
    fn transparent_guard_regions(
        &self,
        name: &str,
        facts: &NameFacts,
        guard_regions: &[GuardRegion],
        name_is_pre_bound: bool,
    ) -> Vec<bool> {
        guard_regions
            .iter()
            .map(|region| {
                self.guard_region_is_transparent(name, region, &facts.assigns, name_is_pre_bound)
            })
            .collect()
    }

    /// Returns whether a guard region is INVISIBLE to the marking: the checker accepts every
    /// assignment the region governs, and restores the pre-branch binding when the branch ends, so
    /// nothing inside it is evidence and nothing inside it moves the outer binding.
    ///
    /// The region's own assignments are replayed in source order starting from the guard's TARGET,
    /// which is what `control_flow` installs before checking the guarded body, and each merge
    /// carries forward — the checker does not undo an in-branch assignment until the branch is
    /// over. One failed merge is a rejection the checker really makes, and it disqualifies the
    /// WHOLE region: with the region opaque, its assignments are judged against the outer binding
    /// like any others, which is what produces the mark that makes such a body compile.
    ///
    /// A region containing the name's FIRST assignment is never transparent. The guard could not
    /// have narrowed a name that had no binding yet (`guard_narrowing` bails on an unbound plain
    /// variable), so `target` is not what the branch actually saw and the model does not apply.
    fn guard_region_is_transparent(
        &self,
        name: &str,
        region: &GuardRegion,
        assigns: &[AssignSite],
        name_is_pre_bound: bool,
    ) -> bool {
        // `guard_regions` is per-BODY while `sites` indexes one per-NAME assignment list, and
        // `transparent_guard_regions` maps the whole body list for each name in turn — so this is
        // reached on any body that guards one name and marks another, which is ordinary rather than
        // exotic (`$x` guarded by `is_string`, `$a` divergently assigned, in the same body). It is
        // load-bearing, not a belt-and-braces assertion: without it the replay below would index
        // `assigns` with another name's site numbers and panic out of range. Answering `false`
        // leans the safe way — no site of THIS name can point at a foreign region, so the answer is
        // never consulted, and "not transparent" only ever means "keep the evidence".
        if region.name != name {
            return false;
        }
        // A region holding the name's FIRST in-body assignment is transparent only for a name the
        // frame already holds. For one the body CREATES, the guard had nothing to narrow —
        // `guard_narrowing` bails on an unbound plain variable — so `target` is not what the branch
        // actually saw. A capture is bound on entry, the guard really does fire at its first
        // in-body store, and treating that region as opaque made the body WARN while
        // `--strict-locals` compiled it clean: the same false advice, arrived at from the other
        // direction.
        if !name_is_pre_bound && region.sites.contains(&0) {
            return false;
        }
        let mut current = region.target.clone();
        for &site in &region.sites {
            match self.merged_assignment_type(&current, &assigns[site].ty) {
                Some(merged) => current = merged,
                None => return false,
            }
        }
        true
    }
}

/// Records one statement-form assignment to `name`.
fn record_assign(facts: &mut Facts, name: &str, value: &Expr, depth: u32, span: Span) {
    // The INNERMOST open guard on this name governs the assignment, because the checker's
    // narrowings compose: an inner `is_float($a)` re-narrows what an outer `is_string($a)` just
    // installed. `rev().find` takes that one; an `any` over the whole stack let an outer frame
    // answer for a branch an inner one owns.
    let guard_region = facts
        .open_guards
        .iter()
        .rev()
        .find(|(guarded, _)| guarded == name)
        .map(|(_, region)| *region);
    let entry = facts.names.entry(name.to_string()).or_default();
    // A value this scan cannot type EXACTLY is not evidence, it is a guess.
    // `infer_expr_type_syntactic` answers `Int` for everything it does not recognise — a plain
    // `$a = $b`, a user function call, a property read — so trusting it here would mark locals in
    // programs that type-check perfectly well today, and a marked local's storage is boxed. Any
    // inexact value therefore disqualifies the name outright, which keeps the marking confined to
    // programs the checker rejects without it.
    if !has_exact_syntactic_type(value) {
        entry.disqualified = true;
        return;
    }
    let site = entry.assigns.len();
    entry.assigns.push(AssignSite {
        ty: infer_expr_type_syntactic(value),
        depth,
        span,
        guard_region,
    });
    // The region has to know which of the name's assignments it governs, because transparency is
    // decided over the whole region once the walk has seen all of them.
    if let Some(region) = guard_region {
        facts.guard_regions[region].sites.push(site);
    }
}

/// Marks `name` as never eligible for mixed storage in this body.
fn disqualify(facts: &mut Facts, name: &str) {
    facts.names.entry(name.to_string()).or_default().disqualified = true;
}

/// Disqualifies the local at the root of an lvalue/reference access chain.
///
/// Mirrors `Checker::record_reference_alias_root`: a write to (or reference into) `$a[0]` or
/// `$o->p` reaches `$a` / `$o` too.
fn disqualify_root(facts: &mut Facts, expr: &Expr) {
    let mut current = expr;
    loop {
        match &current.kind {
            ExprKind::Variable(name) => {
                disqualify(facts, name);
                return;
            }
            ExprKind::ArrayAccess { array: base, .. }
            | ExprKind::PropertyAccess { object: base, .. }
            | ExprKind::NullsafePropertyAccess { object: base, .. }
            | ExprKind::DynamicPropertyAccess { object: base, .. }
            | ExprKind::NullsafeDynamicPropertyAccess { object: base, .. }
            | ExprKind::Spread(base)
            | ExprKind::ErrorSuppress(base)
            | ExprKind::NamedArg { value: base, .. } => current = base,
            _ => return,
        }
    }
}

/// Returns whether `expr`'s syntactic type is EXACT rather than a fallback guess.
///
/// Only shapes whose type is fixed by the SHAPE ITSELF qualify: literals, scalar casts and
/// string concatenation. For those the checker's own inference agrees by construction, so a pair
/// this scan judges incompatible is a pair the checker would reject. Everything else —
/// variables, calls, array literals, arithmetic — is either an approximation or depends on facts
/// only the typed walk has, and is treated as unknown.
fn has_exact_syntactic_type(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null => true,
        // An `(array)` cast's ELEMENT type is not knowable syntactically, so it is excluded.
        ExprKind::Cast { target, .. } => !matches!(target, crate::parser::ast::CastType::Array),
        // `.` casts BOTH operands to string and yields a string for every operand pair, so its
        // result type is a property of the operator rather than of what it is applied to: the
        // operands need no exactness of their own, and neither layer ever answers anything else
        // (`infer_expr_type_syntactic` and `Checker::binary_op_type` both return `Str`
        // unconditionally). Without this arm the commonest heterogeneous shape there is —
        // `$a = 0; … $a = "s" . $i;` — stayed a hard "cannot reassign" error.
        ExprKind::BinaryOp {
            op: crate::parser::ast::BinOp::Concat,
            ..
        } => true,
        _ => false,
    }
}

/// Walks a straight-line block of statements at `depth`.
fn collect_block(checker: &Checker, stmts: &[Stmt], depth: u32, facts: &mut Facts) {
    for stmt in stmts {
        collect_stmt(checker, stmt, depth, facts);
    }
}

/// Walks the body a `condition` GUARDS, remembering the local that condition narrows (if any) for
/// the whole of it.
///
/// An assignment to that local inside the body is merged by the checker against the guard's TARGET
/// type rather than against the running binding, and is undone when the construct restores the
/// pre-branch narrowing — so it is neither conflict evidence nor a change to the binding. See
/// [`Checker::first_rejected_assignment`], which acts on the flag this records.
fn collect_type_guarded_block(
    checker: &Checker,
    condition: &Expr,
    body: &[Stmt],
    depth: u32,
    facts: &mut Facts,
) {
    let guarded = type_guard_subject(condition);
    if let Some((name, target)) = &guarded {
        let region = facts.guard_regions.len();
        facts.guard_regions.push(GuardRegion {
            name: (*name).to_string(),
            target: target.clone(),
            sites: Vec::new(),
        });
        facts.open_guards.push(((*name).to_string(), region));
    }
    collect_block(checker, body, depth, facts);
    if guarded.is_some() {
        // Only the OPEN-guard stack is popped. The region itself outlives the walk of its body:
        // `first_rejected_assignment` replays it to decide transparency.
        facts.open_guards.pop();
    }
}

/// Returns the LOCAL a condition narrows in the branch it guards, and the type it narrows it TO.
///
/// Mirrors `checker::stmt_check::narrowing::guard_receiver_and_target` — the checker's own guard
/// recogniser — narrowed to the cases where the guarded branch really sees the guard's TARGET:
/// - the receiver must be a plain `Variable`, the only receiver shape `guard_env_key` keys as a
///   local (property places live under a `\x01` sigil and are not locals at all);
/// - the guard must not be negated. A leading `!`, a `!==` comparison and `isset(…)` all give the
///   guarded branch the guard's COMPLEMENT, which for a concrete type is that type unchanged — so
///   the checker really does reject an incompatible assignment there and the marking is right;
/// - the target must be stateable as one `PhpType`. `is_array($x)` narrows to the element-agnostic
///   array FAMILY (`GuardTarget::AnyArray`), and is modelled by that target's own fallback,
///   `Mixed` — see the arm itself for why that is exact for every name this scan can mark.
///
/// `isset($x)`, `!==` and a leading `!` are the checker's SELF-NEGATING guards: their branch sees
/// the COMPLEMENT, which for a concrete type is that type unchanged, so the checker really does
/// reject an incompatible store there and the mark is right. `is_object` is not a narrowing
/// predicate the checker supports at all. All four are correctly absent.
///
/// The type is returned because the caller has to ask whether the guard's target ACCEPTS what the
/// guarded branch assigns: `is_string($a)` followed by `$a = "x"` is accepted and is not evidence,
/// while `is_float($a)` followed by `$a = "s"` is rejected by the checker and is.
///
/// Answering `None` for a shape the checker does narrow costs only a spurious warning (the
/// pre-existing behaviour); answering `Some` for one it does not would suppress a mark the body
/// needs, so this stays strictly narrower than the recogniser it mirrors.
fn type_guard_subject(condition: &Expr) -> Option<(&str, PhpType)> {
    match &condition.kind {
        ExprKind::FunctionCall { name, args } if args.len() == 1 => {
            // The same predicate spellings `guard_receiver_and_target` accepts, and the same
            // targets: `is_null` narrows to `Void`, which is how elephc models a value's null.
            let target = match crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str()
            {
                "is_int" | "is_integer" | "is_long" => PhpType::Int,
                "is_float" | "is_double" | "is_real" => PhpType::Float,
                "is_string" => PhpType::Str,
                "is_bool" => PhpType::Bool,
                "is_null" => PhpType::Void,
                "is_callable" => PhpType::Callable,
                // `is_array` narrows to the element-agnostic array FAMILY, which has no single
                // `PhpType`. `Mixed` is what `GuardTarget::AnyArray::fallback_type` yields, and
                // `narrow_to` reaches for that fallback whenever the guarded name is not ALREADY
                // array-typed — which a name this scan can mark never is, because
                // `has_exact_syntactic_type` refuses array literals and `(array)` casts outright.
                // Leaving it unrecognised was the one remaining source of false advice:
                // `$a = 1; if (is_array($a)) { $a = "x"; }` warned that `--strict-locals` would
                // reject the body while strict compiled it clean.
                "is_array" => PhpType::Mixed,
                _ => return None,
            };
            Some((guarded_variable_name(&args[0])?, target))
        }
        ExprKind::InstanceOf { value, target } => match target {
            InstanceOfTarget::Name(class) => Some((
                guarded_variable_name(value)?,
                PhpType::Object(class.as_str().to_string()),
            )),
            InstanceOfTarget::Expr(_) => None,
        },
        // `$x === null` / `$x === false` (either operand order). `!==` is the negated form and is
        // deliberately not recognised.
        ExprKind::BinaryOp {
            left,
            op: crate::parser::ast::BinOp::StrictEq,
            right,
        } => {
            let (receiver, literal) = match guarded_variable_name(left) {
                Some(receiver) => (receiver, &right.kind),
                None => (guarded_variable_name(right)?, &left.kind),
            };
            match literal {
                ExprKind::Null => Some((receiver, PhpType::Void)),
                ExprKind::BoolLiteral(false) => Some((receiver, PhpType::False)),
                _ => None,
            }
        }
        _ => None,
    }
}

/// Returns the name behind a plain `$var` guard receiver, and `None` for every other shape.
fn guarded_variable_name(expr: &Expr) -> Option<&str> {
    match &expr.kind {
        ExprKind::Variable(name) => Some(name.as_str()),
        _ => None,
    }
}

/// Walks one statement.
///
/// Exhaustive over `StmtKind` on purpose: a new statement form that can write a local must be
/// classified here rather than silently defaulting to "reads nothing", which would let the scan
/// mark a name some other statement also writes.
///
/// `depth` mirrors `Checker::local_conditional_depth`, which `check_stmt` raises for the WHOLE of
/// `If`/`IfDef`/`Switch`/`While`/`DoWhile`/`For`/`Foreach`/`Try`/`Throw`/`IncludeOnceGuard`
/// (conditions, `for` init/update and loop subjects included).
fn collect_stmt(checker: &Checker, stmt: &Stmt, depth: u32, facts: &mut Facts) {
    match &stmt.kind {
        StmtKind::Assign { name, value } => {
            // `$x .= "a"` / `$x ??= 1` reach the checker as a plain `Assign` whose value is a
            // synthesized self-read at the statement's own span. A compound assignment is a
            // read-modify-write, not the clean whole-value store the mixed contract needs.
            if is_compound_assignment_self_read(value, name, stmt.span) {
                disqualify(facts, name);
            } else {
                record_assign(facts, name, value, depth, stmt.span);
            }
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::TypedAssign { type_expr: _, name, value } => {
            // A declared type is a programmer contract and stays strict in both modes.
            disqualify(facts, name);
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::RefAssign { target, source } => {
            disqualify(facts, target);
            disqualify_root(facts, source);
            collect_expr(checker, source, depth, facts);
        }
        StmtKind::ArrayAssign { array, index, value } => {
            disqualify(facts, array);
            collect_expr(checker, index, depth, facts);
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::ArrayPush { array, value } => {
            disqualify(facts, array);
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::NestedArrayAssign { target, value } => {
            disqualify_root(facts, target);
            collect_expr(checker, target, depth, facts);
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::ListUnpack { vars, value } => {
            for var in vars {
                disqualify(facts, var);
            }
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::Global { vars } => {
            for var in vars {
                disqualify(facts, var);
            }
        }
        StmtKind::StaticVar { name, init } => {
            // A `static` local's storage outlives the call, so the body does not own its layout.
            disqualify(facts, name);
            collect_expr(checker, init, depth, facts);
        }
        StmtKind::Foreach { array, key_var, value_var, value_by_ref, body } => {
            if let Some(key_var) = key_var {
                disqualify(facts, key_var);
            }
            // The value variable is disqualified for BOTH forms, and the by-REFERENCE form needs
            // it: `foreach ($arr as &$v)` leaves `$v` bound to the last element's storage and
            // lowering ref-binds its slot, so a mark would promise whole-frame boxed `Mixed` for a
            // name whose value does not live in that slot. The checker's twin of this exclusion is
            // the permanent `ref_aliased_locals` marking in `stmt_check::control_flow`; the two
            // are independent, so a pre-bound `$v` with branch-divergent assignments is refused
            // here whether or not the alias marking is consulted.
            disqualify(facts, value_var);
            if *value_by_ref {
                // `foreach ($arr as &$v)` takes references INTO `$arr`'s elements.
                disqualify_root(facts, array);
            }
            collect_expr(checker, array, depth + 1, facts);
            collect_block(checker, body, depth + 1, facts);
        }
        StmtKind::Echo(expr) | StmtKind::ExprStmt(expr) => collect_expr(checker, expr, depth, facts),
        // `check_stmt` puts `throw` in a conditional scope, so its operand is one level deeper.
        StmtKind::Throw(expr) => collect_expr(checker, expr, depth + 1, facts),
        StmtKind::Return(value) => {
            if let Some(value) = value {
                collect_expr(checker, value, depth, facts);
            }
        }
        StmtKind::ConstDecl { name: _, value } => collect_expr(checker, value, depth, facts),
        StmtKind::Include { path, .. } => collect_expr(checker, path, depth, facts),
        StmtKind::PropertyAssign { object, property: _, value }
        | StmtKind::PropertyArrayPush { object, property: _, value } => {
            collect_expr(checker, object, depth, facts);
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::DynamicPropertyArrayPush {
            object,
            property,
            value,
        } => {
            collect_expr(checker, object, depth, facts);
            collect_expr(checker, property, depth, facts);
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::PropertyArrayAssign { object, property: _, index, value } => {
            collect_expr(checker, object, depth, facts);
            collect_expr(checker, index, depth, facts);
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::StaticPropertyAssign { receiver: _, property: _, value }
        | StmtKind::StaticPropertyArrayPush { receiver: _, property: _, value } => {
            collect_expr(checker, value, depth, facts);
        }
        StmtKind::StaticPropertyArrayAssign { receiver: _, property: _, index, value } => {
            collect_expr(checker, index, depth, facts);
            collect_expr(checker, value, depth, facts);
        }
        // Straight-line groupings: the statements inside run exactly when the group does.
        StmtKind::Synthetic(body) | StmtKind::NamespaceBlock { name: _, body } => {
            collect_block(checker, body, depth, facts)
        }
        // Conditional groups. The checker raises its depth for the whole statement, condition
        // included, so this does the same.
        StmtKind::IncludeOnceGuard { label: _, body } => {
            collect_block(checker, body, depth + 1, facts)
        }
        StmtKind::If { condition, then_body, elseif_clauses, else_body } => {
            collect_expr(checker, condition, depth + 1, facts);
            collect_type_guarded_block(checker, condition, then_body, depth + 1, facts);
            for (condition, body) in elseif_clauses {
                collect_expr(checker, condition, depth + 1, facts);
                collect_type_guarded_block(checker, condition, body, depth + 1, facts);
            }
            // The ELSE body sees the guard's COMPLEMENT, not its target, so an assignment there is
            // merged against the running binding exactly as an unguarded one is.
            if let Some(else_body) = else_body {
                collect_block(checker, else_body, depth + 1, facts);
            }
        }
        StmtKind::IfDef { symbol: _, then_body, else_body } => {
            collect_block(checker, then_body, depth + 1, facts);
            if let Some(else_body) = else_body {
                collect_block(checker, else_body, depth + 1, facts);
            }
        }
        // `check_control_flow_stmt` narrows a `while` CONDITION's guard over the loop body (the
        // condition is re-evaluated before every iteration, so it holds on entry to each one).
        StmtKind::While { condition, body } => {
            collect_expr(checker, condition, depth + 1, facts);
            collect_type_guarded_block(checker, condition, body, depth + 1, facts);
        }
        // A `do`/`while` body runs BEFORE its condition is ever evaluated, so the checker applies
        // no narrowing to it and neither does this.
        StmtKind::DoWhile { body, condition } => {
            collect_expr(checker, condition, depth + 1, facts);
            collect_block(checker, body, depth + 1, facts);
        }
        StmtKind::For { init, condition, update, body } => {
            // `check_stmt` puts the ENTIRE `for` — init and update included — in one conditional
            // scope, so an assignment in the header is at the body's depth, not the outer one.
            if let Some(init) = init {
                collect_stmt(checker, init, depth + 1, facts);
            }
            if let Some(condition) = condition {
                collect_expr(checker, condition, depth + 1, facts);
            }
            if let Some(update) = update {
                collect_stmt(checker, update, depth + 1, facts);
            }
            collect_block(checker, body, depth + 1, facts);
        }
        StmtKind::Switch { subject, cases, default } => {
            collect_expr(checker, subject, depth + 1, facts);
            for (patterns, body) in cases {
                for pattern in patterns {
                    collect_expr(checker, pattern, depth + 1, facts);
                }
                collect_block(checker, body, depth + 1, facts);
            }
            if let Some(default) = default {
                collect_block(checker, default, depth + 1, facts);
            }
        }
        StmtKind::Try { try_body, catches, finally_body } => {
            collect_block(checker, try_body, depth + 1, facts);
            for CatchClause { exception_types: _, variable, body } in catches {
                // The catch variable is bound by the clause, not by an assignment statement.
                if let Some(variable) = variable {
                    disqualify(facts, variable);
                }
                collect_block(checker, body, depth + 1, facts);
            }
            if let Some(finally_body) = finally_body {
                collect_block(checker, finally_body, depth + 1, facts);
            }
        }
        // Declarations own SEPARATE bodies, each of which gets its own scan at its own entry
        // point. Descending into them here would attribute a callee's assignments to this frame.
        StmtKind::FunctionDecl { .. }
        | StmtKind::ClassDecl { .. }
        | StmtKind::EnumDecl { .. }
        | StmtKind::InterfaceDecl { .. }
        | StmtKind::TraitDecl { .. }
        | StmtKind::PackedClassDecl { .. }
        | StmtKind::ExternFunctionDecl { .. }
        | StmtKind::ExternClassDecl { .. }
        | StmtKind::ExternGlobalDecl { .. }
        // Leaves: no sub-statements and no sub-expressions.
        | StmtKind::Break(_)
        | StmtKind::Continue(_)
        | StmtKind::IncludeOnceMark { .. }
        | StmtKind::NamespaceDecl { .. }
        | StmtKind::UseDecl { .. }
        | StmtKind::FunctionVariantGroup { .. }
        | StmtKind::FunctionVariantMark { .. } => {}
    }
}

/// Walks one expression.
///
/// Exhaustive over `ExprKind`: every shape that can WRITE a local (an expression-form assignment,
/// `++`/`--`, a by-reference call argument, a by-reference capture) disqualifies the name it
/// reaches, and every other shape recurses into its children.
fn collect_expr(checker: &Checker, expr: &Expr, depth: u32, facts: &mut Facts) {
    match &expr.kind {
        // An expression-form assignment yields the stored value to its enclosing expression, so
        // re-binding it to boxed storage has no single well-defined result to hand back.
        ExprKind::Assignment { target, value, result_target, prelude, conditional_value_temp: _ } => {
            disqualify_root(facts, target);
            if let Some(result_target) = result_target {
                disqualify_root(facts, result_target);
                collect_expr(checker, result_target, depth, facts);
            }
            collect_expr(checker, target, depth, facts);
            collect_expr(checker, value, depth, facts);
            // The prelude runs where the enclosing expression does, so it inherits its depth.
            collect_block(checker, prelude, depth, facts);
        }
        // `++`/`--` is a read-modify-write with its own storage contract (`string_incdec_locals`).
        ExprKind::PreIncrement(name)
        | ExprKind::PostIncrement(name)
        | ExprKind::PreDecrement(name)
        | ExprKind::PostDecrement(name) => disqualify(facts, name),
        ExprKind::FunctionCall { name, args } => {
            if is_eval_call(name.as_str()) {
                // Recorded from ANYWHERE in the body — nested inside a condition, an argument,
                // a loop, whatever — because the fact it feeds is body-scoped. `eval` is a
                // language construct in PHP's grammar, so it only ever reaches the checker as
                // this call shape (it cannot be a variable function or a callable string).
                facts.contains_eval = true;
            }
            if is_unset_call(name.as_str()) {
                // A name mentioned in ANY `unset` is left to the kill path (or to today's error).
                for arg in args {
                    disqualify_root(facts, arg);
                    if let Some(name) = root_local_name(arg) {
                        facts.unset_names.insert(name.to_string());
                    }
                }
            } else if callee_may_bind_arguments_by_ref(checker, name) {
                disqualify_call_arguments(facts, args);
            }
            collect_exprs(checker, args, depth, facts);
        }
        // Every other call shape resolves its callee from a value, not from a name the scan can
        // look up, so its arguments are disqualified conservatively: a by-reference parameter
        // anywhere behind them would alias the local for the rest of the body.
        ExprKind::MethodCall { object, method: _, args }
        | ExprKind::NullsafeMethodCall { object, method: _, args } => {
            disqualify_call_arguments(facts, args);
            collect_expr(checker, object, depth, facts);
            collect_exprs(checker, args, depth, facts);
        }
        ExprKind::NullsafeDynamicMethodCall { object, method, args } => {
            disqualify_call_arguments(facts, args);
            collect_expr(checker, object, depth, facts);
            collect_expr(checker, method, depth, facts);
            collect_exprs(checker, args, depth, facts);
        }
        ExprKind::StaticMethodCall { receiver: _, method: _, args }
        | ExprKind::NewScopedObject { receiver: _, args }
        | ExprKind::NewObject { class_name: _, args }
        | ExprKind::ClosureCall { var: _, args } => {
            disqualify_call_arguments(facts, args);
            collect_exprs(checker, args, depth, facts);
        }
        ExprKind::ExprCall { callee, args } => {
            disqualify_call_arguments(facts, args);
            collect_expr(checker, callee, depth, facts);
            collect_exprs(checker, args, depth, facts);
        }
        ExprKind::NewDynamic { name_expr, args } => {
            disqualify_call_arguments(facts, args);
            collect_expr(checker, name_expr, depth, facts);
            collect_exprs(checker, args, depth, facts);
        }
        ExprKind::NewDynamicObject { class_name, fallback_class: _, required_parent: _, args } => {
            disqualify_call_arguments(facts, args);
            collect_expr(checker, class_name, depth, facts);
            collect_exprs(checker, args, depth, facts);
        }
        // `$v |> $f` passes `$v` as `$f`'s single argument, so the piped value is judged by the
        // rule the ordinary call arm applies to an argument — through the SAME helper, so the two
        // arms cannot drift: a callee that may bind it by reference disqualifies it, a known
        // by-value callee leaves it an ordinary read that is still walked for nested writes.
        // Deciding this by call SYNTAX instead cost `$v |> strval(...)` the mark that the identical
        // `strval($v)` keeps.
        //
        // The only pipe target this scan can resolve is a first-class callable over a plain
        // FUNCTION name: `fn_decls` and the builtin registry are keyed by name, so a callable
        // variable (no signature until inference), a closure literal and a method/static-method
        // first-class callable all stay conservative.
        //
        // Symmetric with the kill/retype side for the `Function(name)` class this arm resolves:
        // both sides read the same `ref_params` source, so a by-value function target marks AND
        // stays killable/retypeable while a by-ref one refuses both. The other resolvable classes
        // (`Method`/`StaticMethod` first-class callables, closures with inferred signatures) are
        // NOT symmetric yet: `infer_pipe_type` resolves them through
        // `check_pipe_known_callable_call` (kill/retype permissive, by-ref rejected per the RFC)
        // while this arm keeps them conservative — the scan has no inference-time state to
        // resolve them. Extending this match the way `ops.rs` resolves those targets is the
        // natural closure of that residual gap; until then the cost is only a withheld mark.
        ExprKind::Pipe { value, callable } => {
            let target_is_known_by_value = match &callable.kind {
                ExprKind::FirstClassCallable(CallableTarget::Function(name)) => {
                    !callee_may_bind_arguments_by_ref(checker, name)
                }
                _ => false,
            };
            if !target_is_known_by_value {
                disqualify_root(facts, value);
            }
            collect_expr(checker, value, depth, facts);
            collect_expr(checker, callable, depth, facts);
        }
        ExprKind::Closure { params: _, body: _, capture_refs, .. } => {
            // A `use (&$x)` capture aliases the enclosing body's local and can outlive this
            // statement. The closure's own body is a SEPARATE frame with its own scan, and its
            // parameter defaults are evaluated there, so neither is walked here.
            for capture in capture_refs {
                disqualify(facts, capture);
            }
        }
        ExprKind::IncludeValue { path, once: _, required: _ } => collect_expr(checker, path, depth, facts),
        ExprKind::Yield { key, value } => {
            if let Some(key) = key {
                collect_expr(checker, key, depth, facts);
            }
            if let Some(value) = value {
                collect_expr(checker, value, depth, facts);
            }
        }
        ExprKind::BinaryOp { left, op: _, right } => {
            collect_expr(checker, left, depth, facts);
            collect_expr(checker, right, depth, facts);
        }
        ExprKind::InstanceOf { value, target } => {
            collect_expr(checker, value, depth, facts);
            if let InstanceOfTarget::Expr(target) = target {
                collect_expr(checker, target, depth, facts);
            }
        }
        ExprKind::YieldFrom(inner)
        | ExprKind::Clone(inner)
        | ExprKind::Negate(inner)
        | ExprKind::Not(inner)
        | ExprKind::BitNot(inner)
        | ExprKind::Throw(inner)
        | ExprKind::ErrorSuppress(inner)
        | ExprKind::Print(inner)
        | ExprKind::Spread(inner)
        | ExprKind::Cast { target: _, expr: inner }
        | ExprKind::PtrCast { target_type: _, expr: inner }
        | ExprKind::NamedArg { name: _, value: inner }
        | ExprKind::BufferNew { element_type: _, len: inner } => collect_expr(checker, inner, depth, facts),
        ExprKind::NullCoalesce { value, default } | ExprKind::ShortTernary { value, default } => {
            collect_expr(checker, value, depth, facts);
            collect_expr(checker, default, depth, facts);
        }
        ExprKind::Ternary { condition, then_expr, else_expr } => {
            collect_expr(checker, condition, depth, facts);
            collect_expr(checker, then_expr, depth, facts);
            collect_expr(checker, else_expr, depth, facts);
        }
        ExprKind::Match { subject, arms, default } => {
            collect_expr(checker, subject, depth, facts);
            for (patterns, body) in arms {
                collect_exprs(checker, patterns, depth, facts);
                collect_expr(checker, body, depth, facts);
            }
            if let Some(default) = default {
                collect_expr(checker, default, depth, facts);
            }
        }
        ExprKind::ArrayLiteral(items) => collect_exprs(checker, items, depth, facts),
        ExprKind::ArrayLiteralAssoc(items) => {
            for (key, value) in items {
                collect_expr(checker, key, depth, facts);
                collect_expr(checker, value, depth, facts);
            }
        }
        ExprKind::ArrayAccess { array, index } => {
            collect_expr(checker, array, depth, facts);
            collect_expr(checker, index, depth, facts);
        }
        ExprKind::PropertyAccess { object, property: _ }
        | ExprKind::NullsafePropertyAccess { object, property: _ }
        | ExprKind::ObjectClassName { object } => collect_expr(checker, object, depth, facts),
        ExprKind::DynamicPropertyAccess { object, property }
        | ExprKind::NullsafeDynamicPropertyAccess { object, property } => {
            collect_expr(checker, object, depth, facts);
            collect_expr(checker, property, depth, facts);
        }
        ExprKind::FirstClassCallable(target) => {
            if let CallableTarget::Method { object, method: _ } = target {
                collect_expr(checker, object, depth, facts);
            }
        }
        // Leaves. A plain `Variable` is a READ, which never disqualifies.
        ExprKind::Variable(_)
        | ExprKind::StringLiteral(_)
        | ExprKind::IntLiteral(_)
        | ExprKind::FloatLiteral(_)
        | ExprKind::BoolLiteral(_)
        | ExprKind::Null
        | ExprKind::ConstRef(_)
        | ExprKind::This
        | ExprKind::StaticPropertyAccess { .. }
        | ExprKind::ClassConstant { .. }
        | ExprKind::ScopedConstantAccess { .. }
        | ExprKind::MagicConstant(_) => {}
    }
}

/// Returns the plain local at the root of an lvalue/reference chain, if any.
fn root_local_name(expr: &Expr) -> Option<&str> {
    let mut current = expr;
    loop {
        match &current.kind {
            ExprKind::Variable(name) => return Some(name),
            ExprKind::ArrayAccess { array: base, .. }
            | ExprKind::PropertyAccess { object: base, .. }
            | ExprKind::NullsafePropertyAccess { object: base, .. }
            | ExprKind::DynamicPropertyAccess { object: base, .. }
            | ExprKind::NullsafeDynamicPropertyAccess { object: base, .. }
            | ExprKind::Spread(base)
            | ExprKind::ErrorSuppress(base)
            | ExprKind::NamedArg { value: base, .. } => current = base,
            _ => return None,
        }
    }
}

/// Walks a list of expressions.
fn collect_exprs(checker: &Checker, exprs: &[Expr], depth: u32, facts: &mut Facts) {
    for expr in exprs {
        collect_expr(checker, expr, depth, facts);
    }
}

/// Disqualifies the local at the root of every argument of a call that may bind one by reference.
///
/// Applied per CALL rather than per parameter index: named arguments and spreads make the
/// argument-to-parameter mapping unreliable here, and a call is only reached by this helper when
/// its callee declares a by-reference parameter at all (or cannot be resolved), which is rare.
fn disqualify_call_arguments(facts: &mut Facts, args: &[Expr]) {
    for arg in args {
        disqualify_root(facts, arg);
    }
}

/// Returns whether a called NAME may bind one of its arguments by reference.
///
/// Answers `true` whenever the callee cannot be resolved statically: an unknown callee could hold
/// a reference to the local for the rest of the body, and over-disqualifying only narrows the
/// feature, while under-disqualifying would box a local a reference still reaches.
fn callee_may_bind_arguments_by_ref(checker: &Checker, name: &Name) -> bool {
    let raw = name.as_str();
    let trimmed = raw.trim_start_matches('\\');
    // Language constructs the registry does not carry, all of which only READ their operand.
    //
    // Compared case-insensitively rather than through `php_symbol_key`, which is an ASCII
    // lowercase and would allocate a `String` at every call node this walk visits for the same
    // answer.
    if ["isset", "empty", "exit", "die"]
        .iter()
        .any(|construct| trimmed.eq_ignore_ascii_case(construct))
    {
        return false;
    }
    if let Some(decl) = checker
        .fn_decls
        .get(raw)
        .or_else(|| checker.fn_decls.get(trimmed))
    {
        // `ref_params` already carries the variadic's flag in its last slot.
        return decl.ref_params.iter().any(|by_ref| *by_ref) || decl.variadic_by_ref;
    }
    if let Some(def) = crate::builtins::registry::lookup(trimmed) {
        return def.ref_params.iter().any(|by_ref| *by_ref);
    }
    true
}

/// Returns whether a called name is PHP's `eval`.
///
/// Answers exactly what the compiler's other `eval` recognisers answer
/// (`ir_lower::expr::closures::is_eval_call_name`, `ir_lower::program::eval_aot`), which match
/// through `php_symbol_key` — an ASCII lowercase, so a case-insensitive compare is the same test
/// without the `String` this walk would allocate at EVERY call node in the body.
fn is_eval_call(name: &str) -> bool {
    name.trim_start_matches('\\').eq_ignore_ascii_case("eval")
}
