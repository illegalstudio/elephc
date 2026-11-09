//! Purpose:
//! Indexes source declarations and computes their conservative fixed-point reachability.
//! Separates executable roots from dependency edges held by declaration bodies.
//!
//! Called from:
//! - `crate::optimize::reachability::prune_unreachable_declarations()`.
//!
//! Key details:
//! - Dynamic hazards widen roots only after their behaviorally reachable declaration is scanned.
//! - Method-name dispatch is deliberately conservative across all live class-like declarations.
//! - Scoped parent edges stay class-specific while preserving matching slots on the whole vtable lineage.
//! - Checker-injected interface contracts retain implementations even without source interface AST.

use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};

use crate::names::php_symbol_key;
use super::inventory::PreludeGroup;
use crate::parser::ast::Stmt;
use crate::types::CheckResult;

use super::usage::{self, Hazards, Usage};
use super::PruneOptions;

mod index;

/// Final declaration keep-sets produced by the fixed-point graph walk.
#[derive(Clone, Debug, Default)]
pub struct Reachability {
    pub functions: HashSet<String>,
    pub classes: HashSet<String>,
    pub methods: HashSet<(String, String, bool)>,
    pub externs: HashSet<String>,
    pub hazards: Hazards,
}

/// Indexed declarations and their deferred dependency summaries.
#[derive(Clone, Debug, Default)]
pub(crate) struct DeclarationIndex {
    pub(crate) functions: HashMap<String, Usage>,
    pub(crate) classes: HashMap<String, ClassNode>,
    pub(crate) checker_methods: HashMap<(String, String, bool), Usage>,
    pub(crate) externs: HashSet<String>,
    pub(crate) packed_classes: HashSet<String>,
    pub(crate) extern_classes: HashSet<String>,
    pub(crate) function_variants: HashMap<String, Vec<String>>,
    /// `class -> {(method, is_static)}` for every method that carries a PHP attribute.
    ///
    /// An attribute exists to be read by reflection, and the code that reads it is very often
    /// not part of the compiled world: symfony/routing's attribute loader runs in the
    /// interpreter, lists a controller's methods, and builds a route from each `#[Route]`. A
    /// method nothing calls statically was pruned with its metadata, and the console's route
    /// collection came out empty. Such methods are kept wherever their class is.
    pub(crate) attributed_methods: HashMap<String, HashSet<(String, bool)>>,
}

/// Indexed metadata for one source class, enum, interface, or trait.
#[derive(Clone, Debug)]
pub(crate) struct ClassNode {
    pub(crate) kind: ClassKind,
    pub(crate) usage: Usage,
    pub(crate) methods: HashMap<(String, bool), Usage>,
    pub(crate) parent: Option<String>,
    pub(crate) interfaces: Vec<String>,
    pub(crate) traits: Vec<String>,
}

/// Source class-like category used for conservative interface/trait method roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ClassKind {
    Class,
    Enum,
    Interface,
    Trait,
}

/// Declarations whose bodies can execute, rather than being retained only for structural metadata.
#[derive(Clone, Debug, Default)]
struct BehavioralReachability {
    functions: HashSet<String>,
    classes: HashSet<String>,
    methods: HashSet<(String, String, bool)>,
    referenced_methods: HashSet<(String, bool)>,
    instantiated_classes: HashSet<String>,
}

thread_local! {
    /// Per-round totals for the five sub-steps of `seed_live_methods`, printed once at the end
    /// of the fixed point under `ELEPHC_DECL_REACH_TIMES=1`.
    static SEED_SPENT: std::cell::RefCell<[f64; 5]> = const { std::cell::RefCell::new([0.0; 5]) };
}

#[cfg(test)]
thread_local! {
    /// Methods `walk_vtable_slot_families` walked incrementally, for the complexity test.
    pub(super) static VTABLE_WALKS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Computes reachable declarations from executable, export, forced-prelude, and hazard roots.
pub fn compute(
    program: &[Stmt],
    check_result: &CheckResult,
    options: &PruneOptions<'_>,
) -> Reachability {
    let call_signatures = usage::CallSignatureIndex::from_check_result(check_result)
        .with_declared_property_types(program);
    let index = DeclarationIndex::build_with_signatures(
        program,
        check_result,
        &call_signatures,
    );
    let static_class_alias_targets = index.class_alias_targets();
    let executable_usage = usage::scan_executable_program(program, &call_signatures);
    let mut state = GraphState::new(
        index,
        executable_usage.hazards,
        // The inventory hands back a std-hashed set; the graph keys everything with the fast
        // hasher, so it is rebuilt once here rather than left as the one SipHash set in the
        // fixed point's hot path.
        options
            .inventory
            .internal_callable_methods()
            .into_iter()
            .collect(),
        options
            .inventory
            .array_iteration_only_bodies()
            .into_iter()
            .collect(),
        options.inventory.groups.values().cloned().collect(),
        check_result,
    );
    state.apply_usage(executable_usage, true);
    for target in static_class_alias_targets {
        state.keep_class_alias_target_methods(&target, true);
    }
    // An extern class is C struct layout metadata with no emitted code. Besides `ptr_cast<T>`
    // (recorded by the usage scan) it is named through types the scan does not walk: `ptr<T>`
    // in extern function signatures and in fields of other extern classes. Keeping every
    // declared one costs nothing and keeps `$ptr->field` lowering able to find the layout.
    state.reach.classes.extend(state.index.extern_classes.iter().cloned());
    for name in options.exported_functions {
        let name = php_symbol_key(name);
        if let Some((class, method)) = name.split_once("::") {
            state.reach.classes.insert(class.to_string());
            state.behavioral.classes.insert(class.to_string());
            if let Some(node) = state.index.classes.get(class) {
                for (candidate, is_static) in node.methods.keys().filter(|(candidate, _)| candidate == method) {
                    let key = (class.to_string(), candidate.clone(), *is_static);
                    state.reach.methods.insert(key.clone());
                    state.behavioral.methods.insert(key);
                }
            }
            continue;
        }
        state.reach.functions.insert(name.clone());
        state.behavioral.functions.insert(name);
    }
    for group in options
        .inventory
        .groups
        .values()
        .filter(|group| options.forced_groups.contains(&group.id))
    {
        state.reach.functions.extend(group.functions.iter().cloned());
        state.reach.classes.extend(group.classes.iter().cloned());
        state.reach.methods.extend(group.methods.iter().cloned());
        state.reach.externs.extend(group.externs.iter().cloned());
        state.behavioral.functions.extend(group.functions.iter().cloned());
        state.behavioral.classes.extend(group.classes.iter().cloned());
        state.behavioral.methods.extend(group.methods.iter().cloned());
    }
    if options.eval_forced {
        state.keep_everything();
    }
    state.apply_global_hazards();
    state.fixed_point();
    let mut reach = state.reach;
    retain_embedded_packed_classes(&mut reach, check_result);
    reach
}

/// Keeps every packed class embedded in, or pointed to by, a field of a reachable packed class,
/// and every packed class a reachable extern function, an extern global or an extern class field
/// names as `ptr<T>`.
///
/// A nested packed field (`packed class Body { public Vec2 $pos; }`) or a typed pointer field
/// (`public ptr<Vec2> $p;`) names its record type only in the outer declaration, which carries no
/// code and no graph edges, so `Vec2` would be pruned while `Body` survives and `$b[0]->pos->x`
/// or `$b[0]->p->x` would lower against a missing layout. Extern declarations name pointer
/// targets the same way (`extern function make(): ptr<Vec2>;`, `extern global ptr<Vec2> $g;`,
/// `extern class Holder { public ptr<Vec2> $p; }`, kept whole by `compute`), so their packed
/// targets seed the closure too. Packed classes hold only POD fields, so closing
/// over their field types after the fixed point is complete.
fn retain_embedded_packed_classes(reach: &mut Reachability, check_result: &CheckResult) {
    let packed: HashMap<String, &crate::types::PackedClassInfo> = check_result
        .packed_classes
        .iter()
        .map(|(name, info)| (php_symbol_key(name), info))
        .collect();
    let mut pending: Vec<String> = reach
        .classes
        .iter()
        .filter(|key| packed.contains_key(*key))
        .cloned()
        .collect();
    let extern_types = check_result
        .extern_functions
        .iter()
        .filter(|(name, _)| reach.externs.contains(&php_symbol_key(name)))
        .flat_map(|(_, sig)| {
            sig.params.iter().map(|(_, ty)| ty).chain(std::iter::once(&sig.return_type))
        })
        .chain(check_result.extern_globals.values())
        .chain(
            check_result
                .extern_classes
                .values()
                .flat_map(|info| info.fields.iter().map(|field| &field.php_type)),
        );
    for ty in extern_types {
        let crate::types::PhpType::Pointer(Some(inner)) = ty else {
            continue;
        };
        let inner_key = php_symbol_key(crate::names::pointer_target_leaf(inner));
        if packed.contains_key(&inner_key) && reach.classes.insert(inner_key.clone()) {
            pending.push(inner_key);
        }
    }
    while let Some(key) = pending.pop() {
        let Some(info) = packed.get(&key) else {
            continue;
        };
        for field in &info.fields {
            let (crate::types::PhpType::Packed(inner)
            | crate::types::PhpType::Pointer(Some(inner))) = &field.php_type
            else {
                continue;
            };
            let inner_key = php_symbol_key(crate::names::pointer_target_leaf(inner));
            if packed.contains_key(&inner_key) && reach.classes.insert(inner_key.clone()) {
                pending.push(inner_key);
            }
        }
    }
}

/// Removes the by-name Iterator/IteratorAggregate wildcards a `foreach` over an untyped value adds.
///
/// Only called for compiler-owned bodies declared `array_iteration_only`, whose loops walk arrays.
fn drop_iterator_protocol_wildcards(usage: &mut Usage) {
    for method in ["getiterator", "rewind", "valid", "current", "key", "next"] {
        usage.wildcard_methods.remove(&(method.to_string(), false));
    }
}

struct GraphState {
    index: DeclarationIndex,
    reach: Reachability,
    behavioral: BehavioralReachability,
    structural_referenced_methods: HashSet<(String, bool)>,
    /// Behavioral method references whose receiver class the index declares, by that class:
    /// each is kept on the live classes that are one, see `seed_receiver_methods`.
    behavioral_receiver_methods: HashMap<String, HashSet<(String, bool)>>,
    /// Interface contracts by interface: structurally kept on the live classes implementing it.
    structural_receiver_methods: HashMap<String, HashSet<(String, bool)>>,
    scoped_methods: HashSet<(String, String, bool)>,
    behavioral_scoped_methods: HashSet<(String, String, bool)>,
    instantiated_classes: HashSet<String>,
    /// `seed_vtable_slot_families` between rounds: the vtable-slot methods it already walked
    /// (with whether they were behavioral then) for the live classes it walked them against.
    vtable_walked: VtableWalked,
    scanned_functions: HashSet<String>,
    behaviorally_scanned_functions: HashSet<String>,
    scanned_classes: HashSet<String>,
    behaviorally_scanned_classes: HashSet<String>,
    scanned_methods: HashSet<(String, String, bool)>,
    behaviorally_scanned_methods: HashSet<(String, String, bool)>,
    opaque_variables: HashSet<String>,
    all_variables_opaque: bool,
    behavioral_variable_methods: HashMap<String, HashSet<(String, bool)>>,
    internal_callable_methods: HashSet<(String, String, bool)>,
    /// Compiler-owned bodies (functions under an empty class key, and methods) whose `foreach`
    /// loops walk arrays only; see `PreludeGroup::array_iteration_only`.
    array_iteration_only: HashSet<(String, String, bool)>,
    checker_interface_methods: HashMap<String, HashSet<(String, bool)>>,
    checker_method_implementations: HashMap<(String, String, bool), String>,
    /// `checker_method_implementations` restricted to MAGIC methods and keyed by visible class.
    ///
    /// WHY IT IS PRECOMPUTED. `seed_instantiated_magic_methods` runs once per fixed-point round
    /// and, for every instantiated class, used to scan the WHOLE of
    /// `checker_method_implementations` filtering on `visible_class == &class`. That is
    /// O(instantiated_classes x every_method_implementation_in_the_program) per round, and both
    /// factors are large on any real program, which is part of why the prune phase dominates. The
    /// filter depends on nothing that changes during the fixed point — the map is built once in
    /// `new` and never mutated — so it is inverted here, once, and each round does a single
    /// lookup per class instead of a full pass.
    ///
    /// THE RETAINED SET CANNOT MOVE: the entries are exactly those the filter selected
    /// (`visible_class == class && is_magic_method(method)`), carrying the same
    /// `(visible_class, method, is_static)` and `owner` the loop rebuilt, and one source entry
    /// still yields one pair. Order within a class follows the source map's iteration order,
    /// which was already unordered, and every consumer inserts into `HashSet`s.
    ///
    /// Measured on a 300-class fixture whose classes each carry several magic and several
    /// ordinary methods: the prune phase went 249.68 ms to 207.57 ms (median of three, −16.9 %)
    /// with the emitted assembly byte-identical (sha256 76cf50e5…, 6,787,310 bytes both sides).
    checker_magic_methods_by_class: HashMap<String, Vec<(String, bool, String)>>,
    vtable_slots: HashSet<(String, String, bool)>,
    /// The classes owning at least one entry of `vtable_slots`.
    vtable_slot_classes: HashSet<String>,
    /// Every injected prelude group, rooted whole whenever a literal eval fragment runs. The
    /// interpreter conjures core classes without the fragment naming them, and the builtin
    /// method bodies the eval-only lowering surfaces materialize (`DateTimeZone::getLocation`
    /// calling the `elephc_tz_location` extern, `listIdentifiers` calling the list-id prelude
    /// function) reference prelude declarations no AST walk can see. The groups are only the
    /// preludes the program's text already caused to be injected, so this stays proportional.
    literal_eval_baseline: Vec<PreludeGroup>,
    /// The global hazards whose whole-program expansion `apply_global_hazards` already applied:
    /// `[enumerates_functions, dynamic_function, dynamic_class, literal_eval]`.
    ///
    /// The expansion adds every declaration of the (fixed) index to sets that only grow, so
    /// applying it again adds nothing; but it ran after every scanned body, cloning and hashing
    /// every function and class name each time (most of the prune phase on a Symfony build,
    /// whose eval bridge raises the dynamic hazards).
    applied_hazards: [bool; 4],
}

/// Rewrites a reused `(class, method, is_static)` key in place.
fn fill_method_key(key: &mut (String, String, bool), class: &str, method: &str, is_static: bool) {
    key.0.clear();
    key.0.push_str(class);
    key.1.clear();
    key.1.push_str(method);
    key.2 = is_static;
}

/// Inserts `key` unless present, allocating only for the one that stores.
fn insert_new_method(methods: &mut HashSet<(String, String, bool)>, key: &(String, String, bool)) {
    if !methods.contains(key) {
        methods.insert(key.clone());
    }
}

impl GraphState {
    /// Creates graph state with hazards found in executable roots.
    fn new(
        index: DeclarationIndex,
        hazards: Hazards,
        internal_callable_methods: HashSet<(String, String, bool)>,
        array_iteration_only: HashSet<(String, String, bool)>,
        literal_eval_baseline: Vec<PreludeGroup>,
        check_result: &CheckResult,
    ) -> Self {
        let checker_interface_methods = check_result
            .interfaces
            .iter()
            .map(|(name, info)| {
                let methods = info
                    .methods
                    .keys()
                    .map(|method| (method.clone(), false))
                    .chain(
                        info.static_methods
                            .keys()
                            .map(|method| (method.clone(), true)),
                    )
                    .collect();
                (php_symbol_key(name), methods)
            })
            .collect();
        let checker_method_implementations = check_result
            .classes
            .iter()
            .flat_map(|(class, info)| {
                let class = php_symbol_key(class);
                info.method_impl_classes
                    .iter()
                    .map({
                        let class = class.clone();
                        move |(method, owner)| {
                            (
                                (class.clone(), php_symbol_key(method), false),
                                php_symbol_key(owner),
                            )
                        }
                    })
                    .chain(info.static_method_impl_classes.iter().map({
                        let class = class.clone();
                        move |(method, owner)| {
                            (
                                (class.clone(), php_symbol_key(method), true),
                                php_symbol_key(owner),
                            )
                        }
                    }))
            })
            .collect();
        let vtable_slots: HashSet<(String, String, bool)> = check_result
            .classes
            .iter()
            .flat_map(|(class, info)| {
                let class = php_symbol_key(class);
                info.vtable_methods
                    .iter()
                    .map({
                        let class = class.clone();
                        move |method| (class.clone(), php_symbol_key(method), false)
                    })
                    .chain(info.static_vtable_methods.iter().map({
                        let class = class.clone();
                        move |method| (class.clone(), php_symbol_key(method), true)
                    }))
            })
            .collect();
        Self {
            index,
            reach: Reachability {
                hazards,
                ..Reachability::default()
            },
            behavioral: BehavioralReachability::default(),
            structural_referenced_methods: HashSet::default(),
            behavioral_receiver_methods: HashMap::default(),
            structural_receiver_methods: HashMap::default(),
            scoped_methods: HashSet::default(),
            behavioral_scoped_methods: HashSet::default(),
            instantiated_classes: HashSet::default(),
            vtable_walked: VtableWalked::default(),
            scanned_functions: HashSet::default(),
            behaviorally_scanned_functions: HashSet::default(),
            scanned_classes: HashSet::default(),
            behaviorally_scanned_classes: HashSet::default(),
            scanned_methods: HashSet::default(),
            behaviorally_scanned_methods: HashSet::default(),
            opaque_variables: HashSet::default(),
            all_variables_opaque: false,
            behavioral_variable_methods: HashMap::default(),
            internal_callable_methods,
            array_iteration_only,
            checker_magic_methods_by_class: magic_methods_by_visible_class(
                &checker_method_implementations,
            ),
            checker_interface_methods,
            checker_method_implementations,
            vtable_slot_classes: vtable_slots.iter().map(|(class, _, _)| class.clone()).collect(),
            vtable_slots,
            literal_eval_baseline,
            applied_hazards: [false; 4],
        }
    }

    /// Marks every indexed declaration reachable for explicit eval force-keep.
    fn keep_everything(&mut self) {
        self.reach.functions.extend(self.index.functions.keys().cloned());
        self.reach.classes.extend(self.index.classes.keys().cloned());
        self.reach.classes.extend(self.index.packed_classes.iter().cloned());
        self.reach.classes.extend(self.index.extern_classes.iter().cloned());
        self.reach.externs.extend(self.index.externs.iter().cloned());
        self.behavioral
            .functions
            .extend(self.index.functions.keys().cloned());
        self.behavioral
            .classes
            .extend(self.index.classes.keys().cloned());
        for (class, node) in &self.index.classes {
            for (method, is_static) in node.methods.keys() {
                let method = (class.clone(), method.clone(), *is_static);
                self.reach.methods.insert(method.clone());
                self.behavioral.methods.insert(method);
            }
        }
    }

    /// Applies dynamic hazard widening accumulated from executable and reachable bodies.
    fn apply_global_hazards(&mut self) {
        let hazards = &self.reach.hazards;
        let raised = [
            hazards.enumerates_functions,
            hazards.dynamic_function,
            hazards.dynamic_class,
            hazards.literal_eval,
        ];
        let fresh = [0, 1, 2, 3].map(|slot| raised[slot] && !self.applied_hazards[slot]);
        for slot in 0..4 {
            self.applied_hazards[slot] |= raised[slot];
        }
        if fresh[3] {
            for group in &self.literal_eval_baseline {
                self.reach.functions.extend(group.functions.iter().cloned());
                self.reach.classes.extend(group.classes.iter().cloned());
                self.reach.methods.extend(group.methods.iter().cloned());
                self.reach.externs.extend(group.externs.iter().cloned());
                self.behavioral.functions.extend(group.functions.iter().cloned());
                self.behavioral.classes.extend(group.classes.iter().cloned());
                self.behavioral.methods.extend(group.methods.iter().cloned());
            }
            self.reach.externs.extend(self.index.externs.iter().cloned());
        }
        if fresh[0] {
            self.reach.functions.extend(self.index.functions.keys().cloned());
        }
        if fresh[1] {
            self.reach.functions.extend(self.index.functions.keys().cloned());
            self.reach.externs.extend(self.index.externs.iter().cloned());
            self.behavioral
                .functions
                .extend(self.index.functions.keys().cloned());
        }
        if fresh[2] {
            self.reach.classes.extend(self.index.classes.keys().cloned());
            self.reach.classes.extend(self.index.packed_classes.iter().cloned());
            self.reach.classes.extend(self.index.extern_classes.iter().cloned());
            self.instantiated_classes
                .extend(self.index.classes.keys().cloned());
            self.behavioral
                .classes
                .extend(self.index.classes.keys().cloned());
            self.behavioral
                .instantiated_classes
                .extend(self.index.classes.keys().cloned());
        }
    }

    /// Repeats declaration-body scans until no keep-set grows.
    fn fixed_point(&mut self) {
        // `graph_compute` is the whole of this loop and the largest single step left in the
        // build, but it is FIVE sub-steps run to a fixed point: a total tells you nothing about
        // which one to attack, and how many rounds it takes decides whether a per-round scan or
        // a per-entity scan is the cost. `ELEPHC_DECL_REACH_TIMES=1` prints both.
        let trace = std::env::var("ELEPHC_DECL_REACH_TIMES").is_ok();
        let mut spent = [0f64; 5];
        let mut rounds = 0usize;
        loop {
            rounds += 1;
            let before = self.size();
            let mut mark = std::time::Instant::now();
            self.expand_function_variants();
            if trace {
                spent[0] += mark.elapsed().as_secs_f64();
                mark = std::time::Instant::now();
            }
            self.scan_new_functions();
            if trace {
                spent[1] += mark.elapsed().as_secs_f64();
                mark = std::time::Instant::now();
            }
            self.scan_new_classes();
            if trace {
                spent[2] += mark.elapsed().as_secs_f64();
                mark = std::time::Instant::now();
            }
            self.seed_live_methods();
            if trace {
                spent[3] += mark.elapsed().as_secs_f64();
                mark = std::time::Instant::now();
            }
            self.scan_new_methods();
            if trace {
                spent[4] += mark.elapsed().as_secs_f64();
            }
            if self.size() == before {
                break;
            }
        }
        if trace {
            eprintln!(
                "[elephc-graph] rounds={rounds} variants={:.2}s functions={:.2}s classes={:.2}s \
                 seed_methods={:.2}s methods={:.2}s",
                spent[0], spent[1], spent[2], spent[3], spent[4]
            );
            SEED_SPENT.with(|seed| {
                let seed = seed.borrow();
                eprintln!(
                    "[elephc-graph-seed] magic={:.2}s direct={:.2}s vtable_families={:.2}s \
                     explicit_impls={:.2}s inherited_impls={:.2}s",
                    seed[0], seed[1], seed[2], seed[3], seed[4]
                );
            });
        }
    }

    /// Expands reachable public function variant groups to their concrete declarations.
    fn expand_function_variants(&mut self) {
        for (group, variants) in &self.index.function_variants {
            if self.reach.functions.contains(group) {
                self.reach.functions.extend(variants.iter().cloned());
                if self.behavioral.functions.contains(group) {
                    self.behavioral.functions.extend(variants.iter().cloned());
                }
            }
        }
    }

    /// Scans new free functions and rescans structural survivors upgraded to behavioral roots.
    fn scan_new_functions(&mut self) {
        let names: Vec<_> = self
            .reach
            .functions
            .iter()
            .filter(|name| {
                !self.scanned_functions.contains(*name)
                    || (self.behavioral.functions.contains(*name)
                        && !self.behaviorally_scanned_functions.contains(*name))
            })
            .cloned()
            .collect();
        for name in names {
            let behavioral = self.behavioral.functions.contains(&name);
            self.scanned_functions.insert(name.clone());
            if behavioral {
                self.behaviorally_scanned_functions.insert(name.clone());
            }
            if let Some(mut usage) = self.index.functions.get(&name).cloned() {
                // See `PreludeInventory::record_internal_callable_function`.
                if behavioral
                    && self
                        .internal_callable_methods
                        .contains(&(String::new(), name.clone(), false))
                {
                    usage.hazards.dynamic_function = false;
                    usage.hazards.dynamic_method = false;
                }
                if self.array_iteration_only.contains(&(String::new(), name.clone(), false)) {
                    drop_iterator_protocol_wildcards(&mut usage);
                }
                self.apply_usage(usage, behavioral);
            }
        }
    }

    /// Scans class shells and rescans structural survivors upgraded to behavioral roots.
    fn scan_new_classes(&mut self) {
        let names: Vec<_> = self
            .reach
            .classes
            .iter()
            .filter(|name| {
                !self.scanned_classes.contains(*name)
                    || (self.behavioral.classes.contains(*name)
                        && !self.behaviorally_scanned_classes.contains(*name))
            })
            .cloned()
            .collect();
        for name in names {
            let behavioral = self.behavioral.classes.contains(&name);
            self.scanned_classes.insert(name.clone());
            if behavioral {
                self.behaviorally_scanned_classes.insert(name.clone());
            }
            // Only the fields this loop reads, not the whole node. `ClassNode.methods` maps
            // every method to its own `Usage`, so cloning the node cloned all of them -- and
            // `usage` was then cloned a SECOND time for `apply_usage`. The method keys are
            // wanted only for an interface, and only as keys.
            let Some((usage, parent, interfaces, traits, declared_methods)) =
                self.index.classes.get(&name).map(|node| {
                    (
                        node.usage.clone(),
                        node.parent.clone(),
                        node.interfaces.clone(),
                        node.traits.clone(),
                        match node.kind {
                            ClassKind::Interface => {
                                node.methods.keys().cloned().collect::<Vec<_>>()
                            }
                            _ => Vec::new(),
                        },
                    )
                })
            else {
                continue;
            };
            self.apply_usage(usage, behavioral);
            if let Some(parent) = &parent {
                self.reach.classes.insert(parent.clone());
                if behavioral {
                    self.behavioral.classes.insert(parent.clone());
                }
            }
            self.reach.classes.extend(interfaces.iter().cloned());
            self.reach.classes.extend(traits.iter().cloned());
            // MOVED out and put back, not cloned. The contract lookup below used to clone the
            // WHOLE interface node -- every method's `Usage` with it -- once per implementing
            // class, purely to end the immutable borrow of `self.index` before this insert.
            // Nothing between the take and the restore reads this set, so taking it cannot
            // lose an entry; `apply_usage` writes it, and has already returned.
            // An interface's contract binds the classes that implement it, so its methods are
            // kept on those (`seed_receiver_methods`), not on every class declaring the names.
            // An interface the index does not declare keeps its names everywhere, as before.
            if !declared_methods.is_empty() {
                self.structural_receiver_methods
                    .entry(name.clone())
                    .or_default()
                    .extend(declared_methods);
            }
            let mut referenced = std::mem::take(&mut self.structural_referenced_methods);
            for interface in &interfaces {
                if let Some(contract) = self.index.classes.get(interface) {
                    self.structural_receiver_methods
                        .entry(interface.clone())
                        .or_default()
                        .extend(contract.methods.keys().cloned());
                } else if let Some(methods) = self.checker_interface_methods.get(interface) {
                    referenced.extend(methods.iter().cloned());
                }
            }
            self.structural_referenced_methods = referenced;
        }
    }

    /// Seeds methods on live classes from direct names, hazards, magic hooks, and contracts.
    fn seed_live_methods(&mut self) {
        // This one step is 13.98s of graph_compute's 17.29s over five rounds, and it is five
        // sub-steps. `ELEPHC_DECL_REACH_TIMES=1` splits them; without the split the obvious
        // suspect (the direct loop below) is not in fact where the time goes.
        let trace = std::env::var("ELEPHC_DECL_REACH_TIMES").is_ok();
        let mut mark = std::time::Instant::now();
        let lap = |slot: usize, mark: &mut std::time::Instant| {
            if trace {
                SEED_SPENT.with(|spent| spent.borrow_mut()[slot] += mark.elapsed().as_secs_f64());
                *mark = std::time::Instant::now();
            }
        };
        self.seed_instantiated_magic_methods();
        lap(0, &mut mark);
        // MOVED out and put back, not cloned: the clone existed only to end the immutable borrow
        // before the inserts below, and it copied every live class name on every fixed-point
        // round -- thousands of `String` allocations per round on a framework-scale module, which
        // a sample of this phase saw as `Vec::from_iter`. Nothing in the loop inserts into
        // `reach.classes`, so taking it cannot lose an entry.
        let live_classes = std::mem::take(&mut self.reach.classes);
        // Reused by every probe below; see the comment in the inner loop.
        let mut probe: (String, bool) = (String::new(), false);
        // Reused too: the fixed point meets the same pairs every round, so almost every key is
        // already in the set and building one per pair copied 36 000 samples of a Demo build.
        let mut key: (String, String, bool) = (String::new(), String::new(), false);
        for class in &live_classes {
            let Some(node) = self.index.classes.get(class.as_str()) else {
                continue;
            };
            let operators = elephc_builtin_contract::lookup_class(&class)
                .and_then(|contract| contract.operators);
            let has_runtime_owned_parent = node
                .parent
                .as_ref()
                .is_some_and(|parent| !self.index.classes.contains_key(parent));
            for (method, is_static) in node.methods.keys() {
                // The two sets are keyed by `(String, bool)`, which cannot be probed with a
                // `&str`, so this used to clone the method name for the probe and the class and
                // method names for the key -- three allocations per pair, before knowing whether
                // anything would be inserted. On the Symfony `--web` module that is ~270 000
                // pairs per fixed-point round, and a sample of the prune phase put the allocator
                // at 71% of it. `probe` is reused, so the probes allocate nothing, and the key is
                // built only on the path that stores it.
                probe.0.clear();
                probe.0.push_str(method);
                probe.1 = *is_static;
                if self.reach.hazards.dynamic_method
                    || has_runtime_owned_parent
                    || operators.is_some_and(|operators| {
                        if *is_static {
                            operators.arithmetic.iter().any(|(_, adapter)| *adapter == method)
                                || operators.compare == Some(method.as_str())
                        } else { operators.boolean == Some(method.as_str()) }
                    })
                    || matches!(method.as_str(), "__call" | "__callstatic")
                    // A lazy static-default initializer is called by lowering, not by source.
                    || method.starts_with(crate::lazy_static_defaults::STATIC_INIT_PREFIX)
                    || self.behavioral.referenced_methods.contains(&probe)
                    || self
                        .index
                        .attributed_methods
                        .get(class.as_str())
                        .is_some_and(|methods| methods.contains(&probe))
                {
                    fill_method_key(&mut key, class, method, *is_static);
                    insert_new_method(&mut self.reach.methods, &key);
                    insert_new_method(&mut self.behavioral.methods, &key);
                } else if self.structural_referenced_methods.contains(&probe) {
                    fill_method_key(&mut key, class, method, *is_static);
                    insert_new_method(&mut self.reach.methods, &key);
                }
            }
        }
        self.reach.classes = live_classes;
        self.seed_receiver_methods();
        lap(1, &mut mark);
        self.seed_vtable_slot_families();
        lap(2, &mut mark);
        self.seed_explicit_method_implementations();
        lap(3, &mut mark);
        self.seed_inherited_implementations();
        lap(4, &mut mark);
    }

    /// Keeps each receiver-scoped method reference on every live class that is the receiver's
    /// class: the class itself, a subclass, an implementation of the interface or a user of the
    /// trait. `seed_explicit_method_implementations` then resolves an inherited method to the
    /// class that declares its body.
    fn seed_receiver_methods(&mut self) {
        if self.behavioral_receiver_methods.is_empty() && self.structural_receiver_methods.is_empty() {
            return;
        }
        let live_classes: Vec<String> = self.reach.classes.iter().cloned().collect();
        let mut key: (String, String, bool) = (String::new(), String::new(), false);
        for class in &live_classes {
            for family in self.class_families(class) {
                if let Some(methods) = self.behavioral_receiver_methods.get(&family) {
                    for (method, is_static) in methods {
                        fill_method_key(&mut key, class, method, *is_static);
                        insert_new_method(&mut self.reach.methods, &key);
                        insert_new_method(&mut self.behavioral.methods, &key);
                    }
                }
                if let Some(methods) = self.structural_receiver_methods.get(&family) {
                    for (method, is_static) in methods {
                        fill_method_key(&mut key, class, method, *is_static);
                        insert_new_method(&mut self.reach.methods, &key);
                    }
                }
            }
        }
    }

    /// Every class-like `class` is one of: itself, its ancestors, the interfaces they implement
    /// (and those interfaces extend) and the traits they use.
    fn class_families(&self, class: &str) -> Vec<String> {
        let mut families: Vec<String> = Vec::new();
        let mut pending = vec![class.to_string()];
        let mut seen: HashSet<String> = HashSet::default();
        while let Some(current) = pending.pop() {
            if !seen.insert(current.clone()) {
                continue;
            }
            if let Some(node) = self.index.classes.get(current.as_str()) {
                pending.extend(node.parent.iter().cloned());
                pending.extend(node.interfaces.iter().cloned());
                pending.extend(node.traits.iter().cloned());
            }
            families.push(current);
        }
        families
    }

    /// Keeps shared virtual slots on every live class in a lineage once any occupant survives.
    ///
    /// The inner test used to be `class_is_or_descends_from(candidate, lineage_root)` run for
    /// EVERY (kept method, live class) pair — O(kept_methods x live_classes x depth) — and each
    /// call allocated a fresh `String` per parent hop plus a `HashSet` for its cycle guard. That
    /// product is the prune phase's dominant cost on a large program, and nothing in the
    /// predicate depends on the method: it is a pure function of the declaration index's parent
    /// chains. So the chains are walked ONCE per fixed-point round here and inverted into
    /// `lineage root -> live classes that descend from it`.
    ///
    /// The retained set cannot change:
    /// * the inversion and the predicate share ONE walk (`walk_class_ancestors`), so they cut the
    ///   chain at the same first repeated name and compare a name before looking its node up —
    ///   "root is among the names visited from candidate" is exactly what the predicate answered;
    /// * `live_classes` is deduplicated (it comes from a `HashSet`) and a name is visited at most
    ///   once per chain, so each root lists each candidate at most once;
    /// * the lists are built in ascending `live_classes` position, so the inner loop still visits
    ///   candidates in the same order it did, inserting the same pairs;
    /// * every other test in the loop (`has_vtable_slot`, the behavioral lookup) is untouched.
    ///
    /// Incremental across rounds: the walk of a method depends only on the method, whether it
    /// is behavioral, and the live classes, and the live classes only grow. While their number
    /// is unchanged, a method walked in an earlier round (as behavioral, or still structural) is
    /// not walked again. On the Symfony demo the full walk was 16.1 s of the 17.7 s this
    /// fixed point spends seeding methods, every round redoing the previous one.
    /// `ELEPHC_REACH_VERIFY=1` walks everything again afterwards and panics if that finds more.
    fn seed_vtable_slot_families(&mut self) {
        let live_classes = classes_with_vtable_slots(&self.reach.classes, &self.vtable_slot_classes);
        if live_classes.len() != self.vtable_walked.live_classes {
            self.vtable_walked = VtableWalked {
                live_classes: live_classes.len(),
                methods: HashMap::default(),
            };
        }
        self.walk_vtable_slot_families(&live_classes, true);
        if reach_verify() {
            #[cfg(test)]
            let walks = VTABLE_WALKS.with(std::cell::Cell::get);
            let before = (self.reach.methods.len(), self.behavioral.methods.len());
            self.walk_vtable_slot_families(&live_classes, false);
            #[cfg(test)]
            VTABLE_WALKS.with(|counter| counter.set(walks));
            assert_eq!(
                before,
                (self.reach.methods.len(), self.behavioral.methods.len()),
                "ELEPHC_REACH_VERIFY: the incremental vtable walk missed methods"
            );
        }
    }

    /// One walk of [`Self::seed_vtable_slot_families`]; `incremental` skips what an earlier
    /// round walked against the same live classes.
    fn walk_vtable_slot_families(&mut self, live_classes: &[String], incremental: bool) {
        let descendants_by_root = live_descendants_by_root(&self.index, live_classes);
        // MOVED out, not cloned: this runs every fixed-point round over every kept method. The
        // pairs found go in afterwards, so the walk still sees the round's snapshot.
        let kept_methods = std::mem::take(&mut self.reach.methods);
        let mut found = Vec::new();
        let mut key: (String, String, bool) = (String::new(), String::new(), false);
        for visible in &kept_methods {
            if !self.vtable_slots.contains(visible) {
                continue;
            }
            let (class, method, is_static) = visible;
            let behavioral = self.behavioral.methods.contains(visible);
            if incremental {
                match self.vtable_walked.methods.get(visible) {
                    Some(true) => continue,
                    Some(false) if !behavioral => continue,
                    _ => {
                        self.vtable_walked.methods.insert(visible.clone(), behavioral);
                    }
                }
            }
            #[cfg(test)]
            VTABLE_WALKS.with(|walks| walks.set(walks.get() + 1));
            let lineage_root = self.vtable_lineage_root(class, method, *is_static);
            let Some(descendants) = descendants_by_root.get(&lineage_root) else {
                continue;
            };
            for position in descendants {
                fill_method_key(&mut key, &live_classes[*position], method, *is_static);
                if !self.vtable_slots.contains(&key) {
                    continue;
                }
                if !kept_methods.contains(&key) {
                    found.push(key.clone());
                }
                if behavioral {
                    insert_new_method(&mut self.behavioral.methods, &key);
                }
            }
        }
        self.reach.methods = kept_methods;
        self.reach.methods.extend(found);
    }

    /// Resolves class-qualified method edges to the checker-selected implementation body.
    fn seed_explicit_method_implementations(&mut self) {
        // MOVED out, not cloned, as in `seed_vtable_slot_families`.
        let methods = std::mem::take(&mut self.reach.methods);
        let mut found = Vec::new();
        let mut key: (String, String, bool) = (String::new(), String::new(), false);
        let mut probe: (String, bool) = (String::new(), false);
        let mut chain: Vec<&str> = Vec::new();
        for visible_method in &methods {
            let (class, method, is_static) = visible_method;
            let behavioral = self.behavioral.methods.contains(visible_method);
            let mut owner = self
                .checker_method_implementations
                .get(visible_method)
                .map(String::as_str);
            if owner.is_none() {
                probe.0.clear();
                probe.0.push_str(method);
                probe.1 = *is_static;
                chain.clear();
                let mut current = Some(class.as_str());
                while let Some(candidate) = current {
                    if chain.contains(&candidate) {
                        break;
                    }
                    chain.push(candidate);
                    let Some(node) = self.index.classes.get(candidate) else {
                        break;
                    };
                    if node.methods.contains_key(&probe) {
                        owner = Some(candidate);
                        break;
                    }
                    current = node.parent.as_deref();
                }
            }
            let Some(owner) = owner else {
                continue;
            };
            fill_method_key(&mut key, owner, method, *is_static);
            if !methods.contains(&key) {
                found.push(key.clone());
            }
            if behavioral {
                insert_new_method(&mut self.behavioral.methods, &key);
            }
        }
        self.reach.methods = methods;
        self.reach.methods.extend(found);
    }

    /// Keeps the first concrete implementation of each implicit magic hook on instantiated classes.
    fn seed_instantiated_magic_methods(&mut self) {
        let instantiated: Vec<_> = self.instantiated_classes.iter().cloned().collect();
        for class in instantiated {
            let behavioral = self.behavioral.instantiated_classes.contains(&class);
            // ONE LOOKUP, not a pass over every method implementation in the program. See
            // `checker_magic_methods_by_class` for why the inversion is safe: the filter this
            // replaces read only the map built in `new`, which never changes during the fixed
            // point, so the pairs produced here are the same pairs in the same shape.
            let checker_magic: Vec<_> = self
                .checker_magic_methods_by_class
                .get(&class)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|(method, is_static, owner)| {
                            (
                                (class.clone(), method.clone(), *is_static),
                                (owner.clone(), method.clone(), *is_static),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            for (visible_method, owner_method) in checker_magic {
                self.reach.methods.insert(visible_method.clone());
                self.reach.methods.insert(owner_method.clone());
                if behavioral {
                    self.behavioral.methods.insert(visible_method);
                    self.behavioral.methods.insert(owner_method);
                }
            }
            let mut current = Some(class.clone());
            let mut seen_classes = HashSet::default();
            let mut found_methods = HashSet::default();
            while let Some(owner) = current {
                if !seen_classes.insert(owner.clone()) {
                    break;
                }
                let Some(node) = self.index.classes.get(&owner) else {
                    break;
                };
                for (method, is_static) in node.methods.keys() {
                    if is_magic_method(method) && found_methods.insert(method.clone()) {
                        let owner_method = (owner.clone(), method.clone(), *is_static);
                        let visible_method = (class.clone(), method.clone(), *is_static);
                        self.reach.methods.insert(owner_method.clone());
                        self.reach.methods.insert(visible_method.clone());
                        if behavioral {
                            self.behavioral.methods.insert(owner_method);
                            self.behavioral.methods.insert(visible_method);
                        }
                    }
                }
                current = node.parent.clone();
            }
        }
    }

    /// Keeps parent implementations and descendant vtable entries for every referenced method.
    fn seed_inherited_implementations(&mut self) {
        let live_classes: Vec<_> = self.reach.classes.iter().cloned().collect();
        // Referenced method names mapped to whether the reference is behavioral. A name in both
        // sets collapses to `true`: the old loop visited it twice and the behavioral pass's
        // inserts are a superset of the structural pass's.
        let mut referenced: HashMap<(String, bool), bool> = HashMap::default();
        for method in &self.structural_referenced_methods {
            referenced.entry(method.clone()).or_insert(false);
        }
        for method in &self.behavioral.referenced_methods {
            referenced.insert(method.clone(), true);
        }
        // The checker's chosen implementations, indexed by the class they are registered for,
        // so a class can enumerate its own without scanning the whole map.
        let mut implementations: HashMap<&str, Vec<(&str, bool, &str)>> = HashMap::default();
        for ((class, method, is_static), owner) in &self.checker_method_implementations {
            implementations
                .entry(class.as_str())
                .or_default()
                .push((method.as_str(), *is_static, owner.as_str()));
        }
        // The chain does not depend on the method, so it is walked ONCE per class instead of
        // once per (class, referenced method) pair -- this loop is a cross product, and the old
        // walk allocated a `String` for the cycle guard, one for the probe and one for
        // `parent.clone()` at every hop of it.
        let mut chain: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::default();
        // Keys already resolved for the current class: the chain is walked from the class
        // upwards, so the first entry to declare a key is its owner.
        let mut done: HashSet<(&str, bool)> = HashSet::default();
        // Probes a map keyed by `(String, bool)`, which cannot be looked up with a `&str`.
        let mut probe: (String, bool) = (String::new(), false);
        // Reused too: the fixed point meets the same pairs every round, so almost every key is
        // already in the set and building one per pair copied 36 000 samples of a Demo build.
        let mut key: (String, String, bool) = (String::new(), String::new(), false);
        for class in &live_classes {
            chain.clear();
            seen.clear();
            done.clear();
            let mut current: Option<&str> = Some(class.as_str());
            while let Some(owner) = current {
                if !seen.insert(owner) {
                    break;
                }
                let Some(node) = self.index.classes.get(owner) else {
                    break;
                };
                chain.push(owner);
                current = node.parent.as_deref();
            }
            if let Some(entries) = implementations.get(class.as_str()) {
                for (method, is_static, owner) in entries {
                    probe.0.clear();
                    probe.0.push_str(method);
                    probe.1 = *is_static;
                    let Some(&behavioral) = referenced.get(&probe) else {
                        continue;
                    };
                    if !done.insert((*method, *is_static)) {
                        continue;
                    }
                    let owner_method = ((*owner).to_string(), (*method).to_string(), *is_static);
                    let visible_method = (class.clone(), (*method).to_string(), *is_static);
                    self.reach.methods.insert(owner_method.clone());
                    self.reach.methods.insert(visible_method.clone());
                    if behavioral {
                        self.behavioral.methods.insert(owner_method);
                        self.behavioral.methods.insert(visible_method);
                    }
                }
            }
            for owner in &chain {
                let Some(node) = self.index.classes.get(*owner) else {
                    continue;
                };
                for (method, is_static) in node.methods.keys() {
                    probe.0.clear();
                    probe.0.push_str(method);
                    probe.1 = *is_static;
                    let Some(&behavioral) = referenced.get(&probe) else {
                        continue;
                    };
                    if !done.insert((method.as_str(), *is_static)) {
                        continue;
                    }
                    fill_method_key(&mut key, owner, method, *is_static);
                    insert_new_method(&mut self.reach.methods, &key);
                    if behavioral {
                        insert_new_method(&mut self.behavioral.methods, &key);
                    }
                    fill_method_key(&mut key, class, method, *is_static);
                    insert_new_method(&mut self.reach.methods, &key);
                    if behavioral {
                        insert_new_method(&mut self.behavioral.methods, &key);
                    }
                }
            }
        }
    }

    /// Scans new methods and rescans structural survivors upgraded to behavioral roots.
    fn scan_new_methods(&mut self) {
        let methods: Vec<_> = self
            .reach
            .methods
            .iter()
            .filter(|method| {
                !self.scanned_methods.contains(*method)
                    || (self.behavioral.methods.contains(*method)
                        && !self.behaviorally_scanned_methods.contains(*method))
            })
            .cloned()
            .collect();
        // Reused across the loop: `ClassNode.methods` is keyed by `(String, bool)`, which cannot
        // be probed with a `&str`, so the probe used to clone the method name once per method.
        let mut probe: (String, bool) = (String::new(), false);
        for (class, method, is_static) in methods {
            let key = (class.clone(), method.clone(), is_static);
            let behavioral = self.behavioral.methods.contains(&key);
            self.scanned_methods.insert(key.clone());
            if behavioral {
                self.behaviorally_scanned_methods.insert(key.clone());
            }
            probe.0.clear();
            probe.0.push_str(&method);
            probe.1 = is_static;
            let mut usage = self
                .index
                .classes
                .get(&class)
                .and_then(|node| node.methods.get(&probe))
                .cloned()
                // `key` is already `(class, method, is_static)`; rebuilding it here cloned both
                // names a second time for a probe that is thrown away.
                .or_else(|| self.index.checker_methods.get(&key).cloned());
            if let Some(usage) = usage.as_mut() {
                if self.array_iteration_only.contains(&key) {
                    drop_iterator_protocol_wildcards(usage);
                }
                if behavioral && self.internal_callable_methods.contains(&key) {
                    usage.hazards.dynamic_function = false;
                    usage.hazards.dynamic_method = false;
                }
            }
            if let Some(usage) = usage {
                self.apply_usage(usage, behavioral);
            }
        }
    }

    /// Applies one usage summary, propagating hazards only from behaviorally reachable bodies.
    fn apply_usage(&mut self, usage: Usage, behavioral: bool) {
        for target in &usage.class_alias_targets {
            self.keep_class_alias_target_methods(target, behavioral);
        }
        for root in &usage.instantiated_subclass_roots {
            self.keep_instantiable_subclasses(root, behavioral);
        }
        for class in &usage.fragment_classes {
            self.keep_class_whole(class);
        }
        if behavioral {
            self.reach.hazards.enumerates_functions |= usage.hazards.enumerates_functions;
            self.reach.hazards.dynamic_function |= usage.hazards.dynamic_function;
            self.reach.hazards.dynamic_method |= usage.hazards.dynamic_method;
            self.reach.hazards.dynamic_class |= usage.hazards.dynamic_class;
            self.reach.hazards.literal_eval |= usage.hazards.literal_eval;
            self.behavioral
                .functions
                .extend(usage.functions.iter().cloned());
            self.behavioral
                .classes
                .extend(usage.classes.iter().cloned());
            self.behavioral
                .instantiated_classes
                .extend(usage.instantiated_classes.iter().cloned());
            self.opaque_variables
                .extend(usage.global_aliases.iter().cloned());
            self.all_variables_opaque |= usage.dynamic_global_alias;
            for (variable, methods) in &usage.variable_methods {
                self.behavioral_variable_methods
                    .entry(variable.clone())
                    .or_default()
                    .extend(methods.iter().cloned());
            }
            self.promote_opaque_variable_methods();
        }
        self.reach.functions.extend(usage.functions);
        self.reach.classes.extend(usage.classes);
        self.reach.externs.extend(usage.externs);
        self.instantiated_classes
            .extend(usage.instantiated_classes.iter().cloned());
        for (class, method, is_static) in usage.methods {
            self.reach.classes.insert(class.clone());
            let key = (class.clone(), method.clone(), is_static);
            self.reach.methods.insert(key.clone());
            if behavioral {
                self.behavioral.methods.insert(key);
                // A receiver the program declares dispatches within its own family: the method
                // is kept on every live class that is one (`seed_receiver_methods`), not on every
                // class that declares the name. A receiver the index does not know (a builtin
                // class, an unresolved name) keeps the name everywhere, as before, and so does a
                // generic instantiation: `Holder<Dog>` is a `Holder<Animal>` through the
                // variance of `+T`, which no parent, interface or trait edge records.
                if self.index.classes.contains_key(class.as_str()) && !class.contains('<') {
                    self.behavioral_receiver_methods
                        .entry(class.clone())
                        .or_default()
                        .insert((method, is_static));
                } else {
                    self.behavioral
                        .referenced_methods
                        .insert((method, is_static));
                }
                self.behavioral.classes.insert(class);
            } else {
                self.structural_referenced_methods
                    .insert((method, is_static));
            }
        }
        for (class, method, is_static) in usage.scoped_methods {
            self.reach.classes.insert(class.clone());
            let key = (class.clone(), method, is_static);
            self.reach.methods.insert(key.clone());
            self.scoped_methods.insert(key.clone());
            if behavioral {
                self.behavioral.classes.insert(class);
                self.behavioral.methods.insert(key.clone());
                self.behavioral_scoped_methods.insert(key);
            }
        }
        if behavioral {
            self.behavioral
                .referenced_methods
                .extend(usage.wildcard_methods);
        } else {
            self.structural_referenced_methods
                .extend(usage.wildcard_methods);
        }
        self.apply_global_hazards();
    }

    /// Retains a class a literal eval fragment names, with its ancestors and every method of
    /// each, as live and instantiated. The interpreter reads such a class from runtime
    /// metadata: it inherits constructors, destructors and clone hooks from it, checks its
    /// abstract and final methods when a fragment subclasses it, reads and writes its static
    /// properties, and refuses to redeclare it, none of which an AST member reference records.
    fn keep_class_whole(&mut self, class: &str) {
        let mut current = Some(php_symbol_key(class));
        let mut seen = HashSet::default();
        while let Some(name) = current {
            if !seen.insert(name.clone()) {
                break;
            }
            let Some(node) = self.index.classes.get(&name) else {
                break;
            };
            self.reach.classes.insert(name.clone());
            self.behavioral.classes.insert(name.clone());
            self.instantiated_classes.insert(name.clone());
            self.behavioral.instantiated_classes.insert(name.clone());
            for (method, is_static) in node.methods.keys() {
                let key = (name.clone(), method.clone(), *is_static);
                self.reach.methods.insert(key.clone());
                self.behavioral.methods.insert(key);
            }
            for interface in node.interfaces.iter().chain(node.traits.iter()) {
                let key = php_symbol_key(interface);
                self.reach.classes.insert(key.clone());
                self.behavioral.classes.insert(key);
            }
            current = node.parent.as_deref().map(php_symbol_key);
        }
    }

    /// Retains the complete callable hierarchy exposed through one PHP class alias.
    ///
    /// A class alias is an alternate class name, not a newly restricted subtype. Opaque runtime
    /// code can therefore invoke any inherited instance or static method through the alias, even
    /// when no statically visible call names that member. Keep the target hierarchy narrowly here
    /// instead of promoting the global dynamic-method hazard and retaining unrelated classes.
    fn keep_class_alias_target_methods(&mut self, target: &str, behavioral: bool) {
        let mut current = php_symbol_key(target);
        let mut seen = HashSet::default();
        loop {
            if !seen.insert(current.clone()) {
                return;
            }
            let Some(node) = self.index.classes.get(&current) else {
                return;
            };
            let methods = node.methods.keys().cloned().collect::<Vec<_>>();
            let parent = node.parent.clone();
            self.reach.classes.insert(current.clone());
            if behavioral {
                self.behavioral.classes.insert(current.clone());
            }
            for (method, is_static) in methods {
                let method = (current.clone(), method, is_static);
                self.reach.methods.insert(method.clone());
                if behavioral {
                    self.behavioral.methods.insert(method);
                }
            }
            let Some(parent) = parent else {
                return;
            };
            current = parent;
        }
    }

    /// Retains every indexed class that is equal to or descends from one runtime-selected base.
    fn keep_instantiable_subclasses(&mut self, root: &str, behavioral: bool) {
        let root = php_symbol_key(root);
        let classes: Vec<_> = self
            .index
            .classes
            .keys()
            .filter(|class| self.class_is_or_descends_from(class, &root))
            .cloned()
            .collect();
        self.reach.classes.extend(classes.iter().cloned());
        self.instantiated_classes.extend(classes.iter().cloned());
        if behavioral {
            self.behavioral.classes.extend(classes.iter().cloned());
            self.behavioral.instantiated_classes.extend(classes);
        }
    }

    /// Returns whether the checker assigned a virtual slot for this visible method.
    fn has_vtable_slot(&self, class: &str, method: &str, is_static: bool) -> bool {
        self.vtable_slots
            .contains(&(class.to_string(), method.to_string(), is_static))
    }

    /// Returns the oldest ancestor that still occupies the same virtual slot.
    fn vtable_lineage_root(&self, class: &str, method: &str, is_static: bool) -> String {
        // One key rewritten per hop: this runs per kept method per fixed-point round.
        let mut key = (String::new(), method.to_string(), is_static);
        let mut seen: Vec<&str> = Vec::new();
        let mut current = class;
        loop {
            if seen.contains(&current) {
                return current.to_string();
            }
            seen.push(current);
            let Some(parent) = self
                .index
                .classes
                .get(current)
                .and_then(|node| node.parent.as_deref())
            else {
                return current.to_string();
            };
            key.0.clear();
            key.0.push_str(parent);
            if !self.vtable_slots.contains(&key) {
                return current.to_string();
            }
            current = parent;
        }
    }

    /// Returns whether one indexed class is the named root or inherits from it.
    ///
    /// Delegates to `walk_class_ancestors` so this predicate and the `root -> descendants`
    /// inversion `seed_vtable_slot_families` builds can never disagree about what a chain is.
    fn class_is_or_descends_from(&self, class: &str, root: &str) -> bool {
        let mut found = false;
        walk_class_ancestors(&self.index, class, |candidate| {
            if candidate == root {
                found = true;
                return false;
            }
            true
        });
        found
    }

    /// Turns method calls on interprocedurally aliased variable names into wildcard edges.
    fn promote_opaque_variable_methods(&mut self) {
        if self.all_variables_opaque {
            self.behavioral.referenced_methods.extend(
                self.behavioral_variable_methods
                    .values()
                    .flatten()
                    .cloned(),
            );
            return;
        }
        for variable in &self.opaque_variables {
            if let Some(methods) = self.behavioral_variable_methods.get(variable) {
                self.behavioral
                    .referenced_methods
                    .extend(methods.iter().cloned());
            }
        }
    }

    /// Returns the total cardinality used to detect fixed-point convergence.
    fn size(&self) -> usize {
        self.reach.functions.len()
            + self.reach.classes.len()
            + self.reach.methods.len()
            + self.reach.externs.len()
            + self.behavioral.functions.len()
            + self.behavioral.classes.len()
            + self.behavioral.methods.len()
            + self.behavioral.referenced_methods.len()
            + self.behavioral.instantiated_classes.len()
            + self.structural_referenced_methods.len()
            + self
                .behavioral_receiver_methods
                .values()
                .map(|methods| methods.len())
                .sum::<usize>()
            + self
                .structural_receiver_methods
                .values()
                .map(|methods| methods.len())
                .sum::<usize>()
            + self.scoped_methods.len()
            + self.behavioral_scoped_methods.len()
            + self.instantiated_classes.len()
            + self.opaque_variables.len()
            + usize::from(self.all_variables_opaque)
            + self
                .behavioral_variable_methods
                .values()
                .map(HashSet::len)
                .sum::<usize>()
            + usize::from(self.reach.hazards.dynamic_function)
            + usize::from(self.reach.hazards.enumerates_functions)
            + usize::from(self.reach.hazards.dynamic_method)
            + usize::from(self.reach.hazards.dynamic_class)
            + usize::from(self.reach.hazards.literal_eval)
    }
}

/// Returns whether an instantiated class must retain the declared PHP magic method.
///
/// Property HOOK accessors count. The parser compiles `set { … }` into `__propset_<p>` and
/// `get { … }` into `__propget_<p>`, and the only thing that used to mark one reachable was a
/// literal `$obj->p = …` statement somewhere in the source. Every other route to the same hook
/// PHP 8.5's `clone($obj, ["p" => …])` above all, whose write is synthesized after this pass
/// runs, found the accessor pruned and silently wrote the BACKING SLOT instead, skipping the
/// hook body entirely. An instantiated class keeps its own hooks.
fn is_magic_method(method: &str) -> bool {
    if method.starts_with("__propget_") || method.starts_with("__propset_") {
        return true;
    }
    matches!(
        method,
        "__construct"
            | "__destruct"
            | "__clone"
            | "__tostring"
            | "__get"
            | "__set"
            | "__isset"
            | "__unset"
            | "__invoke"
            | "__serialize"
            | "__unserialize"
            | "serialize"
            | "unserialize"
            | "__sleep"
            | "__wakeup"
            | "__debuginfo"
            // A hosted extension wrapper's dump method, reached only through `_class_debuginfo_ptrs`.
            | "__elephcdebuginfo"
            | "__set_state"
            | "__call"
            | "__callstatic"
    )
}

/// Walks one class's ancestor chain and hands every name it reaches to `visit`, stopping when
/// `visit` returns `false` or when a name repeats.
///
/// This IS the walk `class_is_or_descends_from` performed, factored out so the single-root
/// predicate and the root-to-descendants inversion cannot drift apart: the cycle guard cuts the
/// chain at the first repeated name (a class in an `extends` cycle therefore descends only from
/// the names before the repeat), and a name is handed to `visit` BEFORE its node is looked up, so
/// a parent with no indexed node is still part of the chain and is its own lineage root.
fn walk_class_ancestors(
    index: &DeclarationIndex,
    class: &str,
    mut visit: impl FnMut(&str) -> bool,
) {
    let mut current = Some(class.to_string());
    let mut seen = HashSet::default();
    while let Some(candidate) = current {
        if !seen.insert(candidate.clone()) {
            return;
        }
        if !visit(&candidate) {
            return;
        }
        current = index
            .classes
            .get(&candidate)
            .and_then(|node| node.parent.clone());
    }
}

/// Groups the MAGIC checker method implementations by the class they are visible on.
///
/// `seed_instantiated_magic_methods` needs, for one class, the magic methods the checker records
/// against it and the owner that implements each. It used to obtain them by scanning the whole
/// implementation map once per instantiated class per fixed-point round — a product of two large
/// numbers on any real program. The map is built in `GraphState::new` and never mutated, and the
/// filter reads nothing else, so the answer is identical whether it is recomputed every round or
/// inverted once here.
///
/// The value carries `(method, is_static, owner)` because that is precisely what the caller
/// rebuilds its two triples from; nothing is dropped and nothing is deduplicated, so one source
/// entry still yields one pair.
fn magic_methods_by_visible_class(
    checker_method_implementations: &HashMap<(String, String, bool), String>,
) -> HashMap<String, Vec<(String, bool, String)>> {
    let mut by_class: HashMap<String, Vec<(String, bool, String)>> = HashMap::default();
    for ((visible_class, method, is_static), owner) in checker_method_implementations {
        if !is_magic_method(method) {
            continue;
        }
        by_class
            .entry(visible_class.clone())
            .or_default()
            .push((method.clone(), *is_static, owner.clone()));
    }
    by_class
}

/// Inverts the live classes' ancestor chains into `root -> positions in live_classes`.
///
/// `descendants[root]` holds the position of every live class for which
/// `class_is_or_descends_from(class, root)` answers `true`, and no other: both come from
/// `walk_class_ancestors`. Positions are pushed in ascending order, so iterating one root's list
/// visits candidates in the same order a scan of `live_classes` did.
/// The live classes a vtable family can reach: only they can pass `has_vtable_slot` below.
///
/// `reach.classes` also holds every name a string might denote, 278 392 entries on the Symfony
/// Demo against ~4 000 classes. Cloning them each fixed-point round was most of the prune phase.
/// What [`GraphState::seed_vtable_slot_families`] walked in earlier rounds.
#[derive(Default)]
struct VtableWalked {
    /// How many live vtable-slot classes the walks were made against.
    live_classes: usize,
    /// Each walked method, and whether it was behavioral when walked.
    methods: HashMap<(String, String, bool), bool>,
}

/// Whether `ELEPHC_REACH_VERIFY` asks incremental steps to be checked against a full one.
fn reach_verify() -> bool {
    if cfg!(test) {
        return true;
    }
    static VERIFY: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *VERIFY.get_or_init(|| std::env::var_os("ELEPHC_REACH_VERIFY").is_some())
}

fn classes_with_vtable_slots(
    live_classes: &HashSet<String>,
    vtable_slot_classes: &HashSet<String>,
) -> Vec<String> {
    live_classes
        .iter()
        .filter(|class| vtable_slot_classes.contains(*class))
        .cloned()
        .collect()
}

fn live_descendants_by_root(
    index: &DeclarationIndex,
    live_classes: &[String],
) -> HashMap<String, Vec<usize>> {
    let mut descendants: HashMap<String, Vec<usize>> = HashMap::default();
    for (position, class) in live_classes.iter().enumerate() {
        walk_class_ancestors(index, class, |candidate| {
            descendants
                .entry(candidate.to_string())
                .or_default()
                .push(position);
            true
        });
    }
    descendants
}

#[cfg(test)]
mod vtable_lineage_index_tests {
    use super::*;

    /// A name that owns no slot stays out of the list the seeding round clones and walks.
    #[test]
    fn vtable_seeding_only_lists_classes_owning_a_slot() {
        let live: HashSet<String> = ["base", "afar", "abkhaze"].iter().map(|n| n.to_string()).collect();
        let slots: HashSet<String> = ["base", "not_live"].iter().map(|n| n.to_string()).collect();
        assert_eq!(classes_with_vtable_slots(&live, &slots), vec!["base".to_string()]);
    }

    /// The pre-patch predicate, copied verbatim, so the index below is compared against the code
    /// it replaced rather than against a refactoring of itself.
    fn reference_class_is_or_descends_from(
        index: &DeclarationIndex,
        class: &str,
        root: &str,
    ) -> bool {
        let mut current = Some(class.to_string());
        let mut seen = HashSet::default();
        while let Some(candidate) = current {
            if !seen.insert(candidate.clone()) {
                return false;
            }
            if candidate == root {
                return true;
            }
            current = index
                .classes
                .get(&candidate)
                .and_then(|node| node.parent.clone());
        }
        false
    }

    /// Builds one indexed class node.
    fn node(
        kind: ClassKind,
        parent: Option<&str>,
        interfaces: &[&str],
        traits: &[&str],
    ) -> ClassNode {
        ClassNode {
            kind,
            usage: Usage::default(),
            methods: HashMap::default(),
            parent: parent.map(str::to_string),
            interfaces: interfaces.iter().map(|name| (*name).to_string()).collect(),
            traits: traits.iter().map(|name| (*name).to_string()).collect(),
        }
    }

    /// A hierarchy with an interface, a trait, an abstract-shaped parent, a class whose parent is
    /// not indexed at all, and an `extends` cycle.
    fn hierarchy() -> DeclarationIndex {
        let mut index = DeclarationIndex::default();
        for (name, class_node) in [
            ("contract", node(ClassKind::Interface, None, &[], &[])),
            ("helper", node(ClassKind::Trait, None, &[], &[])),
            (
                "base",
                node(ClassKind::Class, None, &["contract"], &["helper"]),
            ),
            (
                "middle",
                node(ClassKind::Class, Some("base"), &[], &["helper"]),
            ),
            ("leaf", node(ClassKind::Class, Some("middle"), &[], &[])),
            ("sibling", node(ClassKind::Class, Some("base"), &[], &[])),
            (
                "orphan",
                node(ClassKind::Class, Some("runtime_owned"), &[], &[]),
            ),
            ("cycle_a", node(ClassKind::Class, Some("cycle_b"), &[], &[])),
            ("cycle_b", node(ClassKind::Class, Some("cycle_a"), &[], &[])),
        ] {
            index.classes.insert(name.to_string(), class_node);
        }
        index
    }

    /// The live-class list the seeding loop would scan, plus names that only ever appear as
    /// parents, so roots outside the list are exercised too.
    fn live_classes() -> Vec<String> {
        [
            "contract",
            "helper",
            "base",
            "middle",
            "leaf",
            "sibling",
            "orphan",
            "runtime_owned",
            "cycle_a",
            "cycle_b",
        ]
        .iter()
        .map(|name| (*name).to_string())
        .collect()
    }

    /// The inverted index answers, for every root, exactly the set the chain predicate answers.
    #[test]
    fn descendant_index_agrees_with_the_chain_predicate() {
        let index = hierarchy();
        let live = live_classes();
        let descendants = live_descendants_by_root(&index, &live);
        let mut roots = live.clone();
        roots.push("never_declared".to_string());
        for root in roots {
            let from_index: HashSet<usize> = descendants
                .get(&root)
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .collect();
            let from_predicate: HashSet<usize> = live
                .iter()
                .enumerate()
                .filter(|(_, class)| reference_class_is_or_descends_from(&index, class, &root))
                .map(|(position, _)| position)
                .collect();
            assert_eq!(from_index, from_predicate, "root {root}");
        }
    }

    /// Each root's list is in ascending `live_classes` position and free of duplicates, which is
    /// what keeps the seeding loop's visit order identical to the scan it replaced.
    #[test]
    fn descendant_lists_preserve_scan_order() {
        let index = hierarchy();
        for (root, positions) in live_descendants_by_root(&index, &live_classes()) {
            let mut sorted = positions.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                positions, sorted,
                "root {root} must list positions in scan order"
            );
        }
    }

    /// Interfaces and traits are their own roots: `base` implements `contract` and uses `helper`,
    /// but neither is on its `extends` chain, so neither lists `base` as a descendant. This pins
    /// the relation the inversion inherits from the predicate instead of inventing a new one.
    #[test]
    fn interfaces_and_traits_are_not_extends_roots() {
        let index = hierarchy();
        let live = live_classes();
        let descendants = live_descendants_by_root(&index, &live);
        let at = |name: &str| live.iter().position(|entry| entry == name).unwrap();
        assert_eq!(descendants["contract"], vec![at("contract")]);
        assert_eq!(descendants["helper"], vec![at("helper")]);
        assert_eq!(
            descendants["base"],
            vec![at("base"), at("middle"), at("leaf"), at("sibling")],
        );
        assert_eq!(descendants["middle"], vec![at("middle"), at("leaf")]);
        // A parent with no indexed node is still a root, and is its own descendant when live.
        assert_eq!(
            descendants["runtime_owned"],
            vec![at("orphan"), at("runtime_owned")],
        );
        // An `extends` cycle stops at the first repeat instead of looping.
        assert_eq!(descendants["cycle_a"], vec![at("cycle_a"), at("cycle_b")]);
        assert_eq!(descendants["cycle_b"], vec![at("cycle_a"), at("cycle_b")]);
        // A name no live class reaches has no entry at all.
        assert!(!descendants.contains_key("never_declared"));
    }
}
