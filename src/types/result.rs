//! Purpose:
//! Defines the aggregate result returned by type checking to the pipeline.
//! Carries type environments, declarations, class metadata, warnings, FFI data, and required libraries forward.
//!
//! Called from:
//! - `crate::types::check()`
//! - `crate::pipeline::compile()`
//!
//! Key details:
//! - Fields are consumed by optimizer, codegen, and linker setup; keep additions explicit and phase-owned.

use std::collections::{HashMap, HashSet};

use crate::codegen::platform::Target;
use crate::errors::{CompileError, CompileWarning};
use crate::parser::ast::Program;
use crate::span::Span;

use super::{
    checker, AttrArgEntry, ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig,
    InterfaceInfo, PackedClassInfo, PhpType, ReturnAliasSummaries, TypeEnv,
};

/// Typed scalar packed locals whose storage may become a hash at run time, and the whole-value
/// reads the checker typed as boxed `array<mixed>` because of it.
///
/// A write whose integer key cannot be bounded keeps such a local typed (`array<int>`) instead of
/// widening every slot to `mixed`; codegen promotes the storage to a hash only when a key really
/// skips a slot. Element operations dispatch on the heap kind, so only whole-value reads (passing,
/// returning, copying, iterating the array) need the `array<mixed>` view that every boxed-array
/// consumer already accepts in either storage kind.
#[derive(Debug, Clone, Default)]
pub struct PromotableArrays {
    /// `(function-like scope, local name)` of every promotable local.
    pub locals: HashSet<(String, String)>,
    /// `(function-like scope, read span, local name)` of every whole-value read lowered through
    /// the kind-aware `ArrayToMixed` view.
    pub value_reads: HashSet<(String, Span, String)>,
    /// `(function-like scope, call span)` of every call to a function returning a promotable
    /// `array<int>` whose result is used as a whole value, lowered through the same view.
    pub call_views: HashSet<(String, Span)>,
    /// `(function-like scope, span)` of every promotable value a sink takes raw. They win over a
    /// view recorded for the same span by an earlier checker visit.
    pub raw_sites: HashSet<(String, Span)>,
}

/// Fixed-point loop storage contracts keyed by `(function-like scope, loop span)`.
pub(crate) type LoopStorageTypes = HashMap<(String, Span), TypeEnv>;

/// Value reads keyed by body scope, source span and local name; visits join conservatively.
pub(crate) type LocalValueReadTypes = HashMap<(String, Span, String), PhpType>;

/// The types the checker proved for values that become container elements.
///
/// EIR keeps a hash `foreach` binding, a guard-narrowed `mixed` local and a branch merge as
/// boxed cells, so the operand's storage alone cannot tell lowering which payload the
/// checker's container type promises. The key is the body scope, the span of the value and
/// the local the value reads directly, `None` for a branch merge. The local is part of the
/// key because a synthesized node can share its span with its first operand. The value is
/// `int`, `float`, `bool`, `string` or a loop binding's concrete array or hash; conflicting
/// visits hold `None`.
pub(crate) type ElementValueViews = HashMap<(String, Span, Option<String>), Option<PhpType>>;

/// A checked local write retains its value contract and the independently owning guarantee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalStoreContract {
    /// PHP-visible membership before storage adaptation.
    pub ty: PhpType,
    /// Restriction responsible for this store, independent of other selected guarantees.
    pub feature: elephc_semantics::StrictFeature,
}

/// Original producer facts retained even when optimization replaces a variable read with a constant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LocalCopyContract {
    pub source: String,
    pub contract: PhpType,
}

/// Source-local contracts and copy producers transported together through the checked pipeline.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocalStoreContracts {
    stores: HashMap<(String, Span, String), LocalStoreContract>,
    pub(crate) eval_sources: HashMap<(String, Span, String), (PhpType, bool)>,
    pub(crate) copies: HashMap<(String, Span, String), LocalCopyContract>,
    pub(crate) origins: HashMap<(String, Span, String), PhpType>,
    entry_origins: HashMap<(String, String), LocalEntryOrigin>,
    origin_bindings: HashSet<(String, String)>,
    origin_scopes: HashSet<String>,
}

/// Incoming semantic evidence independent of whether its pointee belongs to this frame.
#[derive(Clone, Debug, PartialEq, Eq)]
struct LocalEntryOrigin {
    contract: PhpType,
    by_ref: bool,
    frozen_capture: bool,
}

impl LocalStoreContracts {
    /// Replaces incoming contracts when a frame is checked again with its final signature.
    pub(crate) fn clear_entry_origins(&mut self, scope: &str) {
        self.entry_origins.retain(|(owner, _), _| owner != scope);
    }

    /// Gives a recreatable owned parameter its declared incoming source contract.
    pub(crate) fn record_entry_origin(&mut self, scope: String, name: String, contract: PhpType) {
        self.entry_origins.insert((scope, name), LocalEntryOrigin { contract, by_ref: false, frozen_capture: false });
    }

    /// Keeps a foreign reference identity while giving its local name an incoming contract.
    pub(crate) fn record_reference_entry_origin(&mut self, scope: String, name: String, contract: PhpType) {
        self.entry_origins.insert((scope, name), LocalEntryOrigin { contract, by_ref: true, frozen_capture: false });
    }

    /// Keeps a descriptor selected when the closure captured its enclosing producer.
    pub(crate) fn record_captured_entry_origin(&mut self, scope: String, name: String, contract: PhpType, by_ref: bool) {
        self.entry_origins.insert((scope, name), LocalEntryOrigin { contract, by_ref, frozen_capture: true });
    }

    /// Selects hidden immutable metadata rather than the capture's current PHP value kind.
    pub(crate) fn entry_has_captured_contract(&self, scope: &str, name: &str) -> bool {
        self.entry_origins.get(&(scope.to_string(), name.to_string())).is_some_and(|origin| origin.frozen_capture)
    }

    /// Reads an incoming contract without conflating it with an assignment source position.
    pub(crate) fn entry_origin(&self, scope: &str, name: &str) -> Option<&PhpType> {
        self.entry_origins.get(&(scope.to_string(), name.to_string())).map(|origin| &origin.contract)
    }

    /// Prevents reference inputs from entering the by-value shadow-copy path.
    pub(crate) fn entry_is_reference(&self, scope: &str, name: &str) -> bool {
        self.entry_origins.get(&(scope.to_string(), name.to_string())).is_some_and(|origin| origin.by_ref)
    }

    /// Records immutable source metadata and indexes its body/name without scanning other stores.
    pub(crate) fn record_origin(&mut self, key: (String, Span, String), contract: PhpType) {
        self.track_binding_origin(&key.0, &key.2);
        self.origins.insert(key, contract);
    }

    /// Tracks logical binding ends even when reference formation supplies the only producer.
    pub(crate) fn track_binding_origin(&mut self, scope: &str, name: &str) {
        self.origin_bindings.insert((scope.into(), name.into()));
        self.origin_scopes.insert(scope.into());
    }

    /// Tests a binding's runtime-origin transport independently from its current source span.
    pub(crate) fn has_runtime_origin(&self, scope: &str, name: &str) -> bool {
        self.origin_bindings.contains(&(scope.to_string(), name.to_string()))
            || self.entry_origin(scope, name).is_some()
    }

    /// Includes lexical loop scopes belonging to the same native frame.
    pub(crate) fn has_runtime_origins_in_scope(&self, scope: &str) -> bool {
        self.origin_scopes.iter().any(|candidate| crate::types::loop_storage_scope_is_within(candidate, scope))
            || self.entry_origins.keys().any(|(candidate, _)| crate::types::loop_storage_scope_is_within(candidate, scope))
    }
}

impl std::ops::Deref for LocalStoreContracts {
    type Target = HashMap<(String, Span, String), LocalStoreContract>;

    /// Keeps existing store consumers focused on their authoritative membership map.
    fn deref(&self) -> &Self::Target { &self.stores }
}

impl std::ops::DerefMut for LocalStoreContracts {
    /// Allows checker store updates without separating immutable producer transport.
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.stores }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Describes a statically-known access violation that PHP raises as a catchable
/// exception at runtime instead of a compile-time rejection.
pub struct ThrowAccessInfo {
    /// Source span of the offending call/assignment, used as the lookup key.
    pub span: Span,
    /// The kind of violation, selecting the PHP error message template.
    pub kind: ThrowAccessKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// Categorizes statically-decided access violations lowered to runtime exception throws.
pub enum ThrowAccessKind {
    /// A private/protected method call from an inaccessible scope.
    PrivateMethod {
        /// Visibility label (`private` or `protected`).
        visibility: String,
        /// Class holding the method (e.g. `C`).
        class_name: String,
        /// Method name without the trailing parentheses (e.g. `secret`).
        method: String,
    },
    /// A private/protected constructor call from an inaccessible scope.
    PrivateConstructor {
        /// Visibility label (`private` or `protected`).
        visibility: String,
        /// Class declaring the constructor (e.g. `C`).
        class_name: String,
    },
    /// A call to a method PHP does not expose to user code even though the class declares
    /// a compiler-owned implementation of it (the internal URI classes' destructor).
    UndefinedMethod {
        /// Class holding the method (e.g. `Uri\\Rfc3986\\Uri`).
        class_name: String,
        /// Method name without the trailing parentheses (e.g. `__destruct`).
        method: String,
    },
    /// A readonly write requiring a runtime initialization check.
    ReadonlyProperty {
        /// Class holding the property (e.g. `Box`).
        class_name: String,
        /// Property name without the leading `$` (e.g. `x`).
        property: String,
        /// The setter-access Error for first initialization, or `None` when it is allowed.
        initialization_error: Option<String>,
    },
    /// A direct write to one of Reflection's internal public metadata mirrors.
    ReflectionReadOnlyProperty {
        /// Property name without the leading `$`.
        property: String,
    },
    /// A property argument whose declared storage cannot satisfy a typed by-reference
    /// parameter. PHP raises the call-site `TypeError` before the callee body runs; lowering
    /// reports this exact message and skips the callee.
    ReferenceArgumentType {
        /// The complete diagnostic prefix through `must be of type T, `; lowering appends
        /// the property's CURRENT runtime type and ` given`.
        prefix: String,
        /// The declared parameter type the property's current value must satisfy.
        expected: PhpType,
        /// Whether a current value could satisfy the parameter. A union/mixed property needs
        /// the runtime guard; a concrete disjoint type throws unconditionally.
        conditional: bool,
        /// Whether the place is an UNTYPED property. PHP weakly coerces its current value to
        /// a declared scalar by-reference parameter; a declared property (including `mixed`)
        /// is checked without coercion.
        coerces_weakly: bool,
        /// The declared property type's PHP name, used by the unconditional path.
        given: String,
    },
}

/// Checker-selected native property storage and any indirect-write refusal at one call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayPushPropertyPlan {
    /// The proven receiver class, or `None` when specialization requires dynamic property access.
    pub class_name: Option<String>,
    /// PHP's catchable Error at reference binding, before evaluating later arguments.
    pub rejection: Option<String>,
    /// Whether lowering may bind a managed live reference to this exact property slot.
    pub live_addressable: bool,
}

#[derive(Debug)]
/// Aggregate result of type checking, carrying type environments, declarations,
/// class metadata, warnings, FFI data, and required libraries forward to optimizer,
/// codegen, and linker setup.
pub struct CheckResult {
    /// Effective semantic guarantees consumed by lowering and code generation.
    pub semantic_profile: elephc_semantics::SemanticProfile,
    /// Default expressions php-src evaluates when execution reaches a class or trait statement
    /// whose properties compatibly redeclare a used trait's, keyed by class or trait name. A
    /// trait has no `ClassInfo`, so lowering reads its entry here.
    pub trait_collision_defaults: HashMap<String, Vec<crate::types::TraitCollisionDefault>>,
    /// How each user class-like is bound (compile time or at its statement) and where its
    /// trait-imported properties come from.
    pub class_binding: crate::types::ClassBindingFacts,
    /// Every instantiation a call site asked for, as (template name, type arguments).
    ///
    /// The pipeline splices one declaration per entry into the program and re-checks, because
    /// lowering walks the AST rather than the checker's tables.
    pub requested_instantiations: Vec<(String, crate::generics::Bindings)>,
    /// Typed clones of user functions that direct calls handing on a promotable `array<int>`
    /// asked for; spliced like the instantiations above (`generics::promotable_clones`).
    pub requested_promotable_clones: Vec<crate::generics::promotable_clones::PromotableCloneRequest>,
    /// Call site -> the instantiated function that call resolved to.
    ///
    /// Keyed by `Span` exactly like `builtin_call_types`, and consumed by lowering the same
    /// way. See `Checker::generic_call_sites` for why lowering cannot re-derive it.
    pub generic_call_sites: HashMap<(String, Span), String>,
    /// Every generic CLASS instantiation a `new` asked for, as (template key, type arguments).
    pub requested_class_instantiations: Vec<(String, crate::generics::Bindings)>,
    /// The generic METHOD instantiations this round asked for.
    pub requested_method_instantiations: Vec<((String, String), crate::generics::Bindings)>,
    /// (enclosing body scope, call site) -> the instantiated method name that site selected.
    pub generic_method_sites: std::collections::HashMap<
        (String, crate::span::Span),
        std::collections::BTreeSet<String>,
    >,
    /// (enclosing body scope, `new` site) -> the instantiated class it resolved to, consumed by
    /// `generics::classes` as an AST rewrite. See `Checker::generic_new_sites` for the key.
    pub generic_new_sites: HashMap<(String, Span), std::collections::BTreeSet<String>>,
    /// Whether the checked source can call `eval()` anywhere, including code the optimizer may
    /// later remove. EIR lowering keeps an unresolved declared-initializer constant on the
    /// eval-aware runtime lookup instead of reading native global storage when this is set.
    pub program_contains_eval: bool,
    /// Literal names a compiled `define()` declares anywhere in the checked source, including
    /// calls inside functions. These names keep the native global lookup even in an eval-aware
    /// program, because the compiled call writes native storage rather than an eval context.
    pub native_defined_constants: HashSet<String>,
    pub global_env: TypeEnv,
    pub functions: HashMap<String, FunctionSig>,
    pub function_attribute_names: HashMap<String, Vec<String>>,
    pub function_attribute_args: HashMap<String, Vec<Option<Vec<AttrArgEntry>>>>,
    pub callable_param_sigs: HashMap<(String, String), FunctionSig>,
    /// Function names written as string literals where a declared `callable` keeps the
    /// string. Runtime string dispatch must reach each of them, including builtins it would
    /// not resolve from an arbitrary runtime name.
    pub declared_callable_names: std::collections::BTreeSet<String>,
    /// Proven return-to-parameter storage aliases for source-declared callables.
    pub(crate) return_alias_summaries: ReturnAliasSummaries,
    #[allow(dead_code)]
    pub callable_return_sigs: HashMap<String, FunctionSig>,
    #[allow(dead_code)]
    pub callable_array_return_sigs: HashMap<String, FunctionSig>,
    pub interfaces: HashMap<String, InterfaceInfo>,
    pub classes: HashMap<String, ClassInfo>,
    pub enums: HashMap<String, EnumInfo>,
    pub packed_classes: HashMap<String, PackedClassInfo>,
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    pub extern_classes: HashMap<String, ExternClassInfo>,
    pub extern_globals: HashMap<String, PhpType>,
    pub required_libraries: Vec<String>,
    /// Subset of `required_libraries` whose bridge runtime some call site may reach.
    ///
    /// A library missing here was required only for extension identity by calls that
    /// lower natively; EIR feature discovery remains authoritative for surviving calls.
    pub runtime_bridge_libraries: Vec<String>,
    /// Libraries required by builtins named as callable values (`name(...)`, a string
    /// callback, or a literal kept by a declared `callable`).
    ///
    /// The reachability usage scan sees only direct calls, so it cannot prove that a callable
    /// reference died with a pruned declaration; reconciliation keeps these libraries, and
    /// their bridge runtimes, linked.
    pub callable_builtin_libraries: Vec<String>,
    pub warnings: Vec<CompileWarning>,
    /// Statically-decided access violations lowered to runtime exception throws,
    /// keyed by the owning body scope and the source span of the offending
    /// call/assignment. The scope half is `current_loop_storage_scope`; a trait body is
    /// checked once per using class, so its spans alone cannot identify which class's
    /// property/method the site resolved against.
    pub throw_access_sites: HashMap<(String, Span), ThrowAccessInfo>,
    /// Authoritative checker result types for builtin calls, keyed by call span.
    pub builtin_call_types: HashMap<Span, PhpType>,
    /// Closure literals whose checked source bodies contain yield, keyed by source identity.
    pub closure_generator_spans: HashSet<Span>,
    /// Contextual PHP parameter types inferred for closure literals passed to array callbacks,
    /// keyed by the closure expression's span. Lowering replays these hints so the compiled
    /// closure parameter matches the element/accumulator type the checker already proved.
    pub closure_param_hints: HashMap<Span, Vec<PhpType>>,
    /// Original scalar closure result contracts by enclosing body and source expression span.
    /// Conflicting source visits retain canonical Mixed storage instead of choosing a narrower ABI.
    pub closure_result_types: HashMap<(String, Span), PhpType>,
    /// Native array_push property decisions, scoped to the body and original place span.
    pub array_push_property_plans: HashMap<(String, Span), ArrayPushPropertyPlan>,
    /// Checked member receiver types by body scope and source node; conflicting visits are None.
    pub member_receiver_types: HashMap<(String, Span), Option<PhpType>>,
    /// Source-point element proofs for declared-array appends; conflicts disable projection.
    pub array_append_element_types: HashMap<(String, Span), Option<PhpType>>,
    /// Checked scalar views of element-forming values; conflicting visits disable projection.
    pub element_value_views: ElementValueViews,
    /// Source-point value read contracts; these never change a local's reference storage.
    pub local_value_read_types: LocalValueReadTypes,
    /// Scalar contracts for writes and reference-promotion sites, independent of read narrowing.
    pub local_store_contracts: LocalStoreContracts,
    /// Checker-authorized call-argument sites that may widen one local to a boxed Mixed cell.
    pub boxed_reference_promotion_sites: HashMap<(String, Span), HashSet<String>>,
    /// Reads proven to use native buffers on every checker visit to the source span.
    /// Buffer bounds checks cannot invoke PHP warning handlers.
    pub buffer_read_sites: HashSet<Span>,
    /// See `Checker::scalar_guard_arg_sites`: arguments whose parameter binding, and returned
    /// expressions whose declared scalar return, can throw `TypeError` at run time, kept
    /// observable by the post-typecheck optimizer.
    pub scalar_guard_arg_sites: HashSet<Span>,
    /// See `Checker::first_class_builtin_call_types`.
    pub first_class_builtin_call_types: HashMap<Span, PhpType>,
    /// Fixed-point array-local storage contracts keyed by function-like scope and loop span.
    pub loop_storage_types: LoopStorageTypes,
    /// `(function-like scope, local name)` pairs for `string` locals that are a `++`/`--`
    /// target, so EIR lowering can give them boxed `Mixed` storage from their first store.
    pub string_incdec_locals: HashSet<(String, String)>,
    /// The `unset()` arguments whose local binding the checker killed, as span -> the SET of local
    /// NAMES killed at that position, so EIR lowering abandons the old frame slot (after releasing
    /// its value) instead of null-storing into it.
    ///
    /// The name is not decoration, it is half the key. A `Span` carries line/col and NOTHING
    /// about which file they are in, and include resolution splices every included file's AST
    /// into one program without rebasing line numbers, so line 4 column 1 of `main.php` and of
    /// `lib.php` are the SAME `Span`. Lowering must therefore check that the decision it found
    /// is about the variable in front of it; without that check a retype recorded in one file
    /// re-binds an unrelated same-position assignment in another (measured: a conditional
    /// `$w = …` in an included file lost its whole binding, printing `|s` where PHP prints
    /// `a1|s`).
    ///
    /// The value is a SET, exactly like [`CheckResult::mixed_storage_store_sites`], because two
    /// DIFFERENT locals can be killed at the same (line, column) in two different files. With one
    /// name per span the later insert silently EVICTED the earlier decision; the evicted site then
    /// fell back to the pre-feature null store, which widens the abandoned slot to boxed `Mixed`
    /// while every read already lowered above it keeps its narrower IR type — measured as `Fatal
    /// error: heap memory exhausted` on a loop of `string` reads where PHP prints its sum.
    ///
    /// The name does NOT make the key unique: the same NAME at the same line and column in two
    /// files is still one key for two sites, and that shape was measured printing `|s` where PHP
    /// prints `a1|5` — in a program that was a plain compile error before this feature existed.
    /// It is not left ambiguous. `checker::binding_decision_ambiguity` counts, per role, how many
    /// nodes each recorded key actually names and REJECTS the program when a key names more than
    /// one, so an ambiguous decision is a compile error and never a silent re-bind. Giving `Span`
    /// file identity would remove the ambiguity outright, and every span-keyed map here would
    /// want it.
    pub local_bind_kill_sites: HashMap<Span, HashSet<String>>,
    /// Unconditional statement-form unsets allowed to detach an ordinary promoted reference.
    /// Unlike binding kills, these preserve the checker's escaped-reference type restrictions.
    /// Lowering keeps the old capture storage type and retires only this binding's cell owner.
    pub local_ref_detach_sites: HashMap<Span, HashSet<String>>,
    /// The statement-form assignments the checker re-bound to a fresh binding of an incompatible
    /// type, as span -> the SET of local NAMES re-bound at that position, so EIR lowering mints a
    /// new slot there. Read by `crate::ir_lower` alongside `local_bind_kill_sites`, and keyed the
    /// same way for the same reasons — see that field.
    pub local_retype_sites: HashMap<Span, HashSet<String>>,
    /// Every statement-form assignment to a local the checker's syntactic pre-scan marked as
    /// whole-frame boxed `Mixed` storage (a branch-divergently assigned local), as span -> the SET
    /// of local NAMES boxed at that position, so EIR lowering declares the slot boxed before the
    /// FIRST store instead of inferring it from the first stored value.
    ///
    /// Keyed like the other two decision maps and for the same reason — see
    /// `local_bind_kill_sites` — and checked by `checker::binding_decision_ambiguity` against the
    /// same statement-form-assignment tally the retype decisions use, because both roles name
    /// exactly that node kind.
    ///
    /// The value is a SET because two DIFFERENT locals can be marked at the same (line, column) in
    /// two different files, and a `Span` cannot tell them apart. With one name per span the later
    /// body's decision evicted the earlier one silently — the evicted local dropped out of
    /// [`CheckResult::mixed_storage_local_names`] while the checker still typed it `Mixed`, and the
    /// compiler panicked (`strlen cannot lower checked operand type Int`) on valid PHP. Same-NAME
    /// collisions remain genuinely ambiguous and are still rejected by the ambiguity pass.
    pub mixed_storage_store_sites: HashMap<Span, HashSet<String>>,
    /// Local array writes promoted to hash storage because their integer key may skip a slot of
    /// a non-empty packed array, as span -> the SET of local NAMES written there. Keyed like
    /// `local_bind_kill_sites`; the decision is idempotent, so post-typecheck clones need no veto.
    pub sparse_key_write_sites: HashMap<Span, HashSet<String>>,
    /// Typed scalar packed locals that may be promoted to a hash at run time; see
    /// [`PromotableArrays`].
    pub promotable_arrays: PromotableArrays,
}

impl CheckResult {
    /// Every span carrying a local-binding decision: kills, reference detaches, retypes and stores.
    ///
    /// Handed to the post-typecheck optimizer. These decisions are keyed BY SPAN and EIR lowering
    /// consults them by span, so any pass that CLONES an AST node would hand one decision to two
    /// syntactic sites — and abandoning a binding is not idempotent. The invariant is therefore on
    /// the OPTIMIZER as a whole: no pass may clone a node carrying one of these spans, and a pass
    /// that clones must consult this set and veto itself. Two passes clone today and both are
    /// guarded through `optimize::control::binding_decisions` — DCE's tail-sinking
    /// (`optimize::control::dce`) and the single-case switch rewrite reached from the prune and
    /// normalize phases (`optimize::control::switch::prune_switch_stmt`), which materializes a
    /// `switch` default body into both branches of a synthesized `if`.
    ///
    /// The mixed-storage store sites are included even though their consumer — declaring the slot
    /// `Mixed` when the local has none yet, and storing at `Mixed` rather than at the value's own
    /// type — IS idempotent under duplication (the second clone finds the slot already declared,
    /// and substitutes the same type). They are kept singular anyway because
    /// the ambiguity check that runs in the checker counts the nodes of the ORIGINAL program: a
    /// pass that cloned one of these statements afterwards would create a second site for a key
    /// that check already cleared, and "the decision names exactly one node" is the invariant the
    /// two halves of this feature are built on. The cost is one lost tail-sink per boxed local,
    /// plus one un-rewritten single-case switch per boxed local assigned in a `default` body.
    pub fn local_binding_decision_spans(&self) -> HashSet<Span> {
        self.local_bind_kill_sites
            .keys()
            .chain(self.local_ref_detach_sites.keys())
            .chain(self.local_retype_sites.keys())
            .chain(self.mixed_storage_store_sites.keys())
            .chain(self.local_store_contracts.origins.keys().map(|(_, span, _)| span))
            .chain(self.local_store_contracts.eval_sources.keys().map(|(_, span, _)| span))
            .chain(
                self.boxed_reference_promotion_sites
                    .keys()
                    .map(|(_, span)| span),
            )
            .copied()
            .collect()
    }

    /// Every local NAME the mixed-storage marking compiled as boxed `mixed` frame storage.
    ///
    /// Handed to the post-typecheck constant propagator, which must not replace a read of one of
    /// these with a literal: the local is bound `mixed` for its whole frame and EIR lowering
    /// stores and reads it through that one type, so a literal of a concrete type hands lowering a
    /// view the checker never approved. See `optimize::binding_decisions`.
    ///
    /// This is the NAMES, not the spans, because the consumer is name-keyed. It is also a
    /// program-wide union rather than per-body: the AST passes have no body identity to key on,
    /// and over-approximating only costs constant propagation on a name some other body boxed.
    pub fn mixed_storage_local_names(&self) -> HashSet<String> {
        self.mixed_storage_store_sites
            .values()
            .flatten()
            .chain(self.local_store_contracts.origins.keys().map(|(_, _, name)| name))
            .chain(self.local_store_contracts.eval_sources.keys().map(|(_, _, name)| name))
            .cloned()
            .collect()
    }
}

/// Runs type checking using the host platform (auto-detected from the build environment).
/// Returns `Ok(CheckResult)` on success, or `Err(CompileError)` if type checking fails.
/// The `CheckResult` carries the resolved type environment, function signatures, class
/// metadata, warnings, and any required native libraries for linking.
/// Delegates to `check_with_options` with default `CheckOptions`.
#[allow(dead_code)]
pub fn check(program: &Program) -> Result<CheckResult, CompileError> {
    check_with_options(program, checker::CheckOptions::default())
}

/// Runs type checking using the host platform with explicit `CheckOptions`
/// (e.g. `--strict-locals`). Returns `Ok(CheckResult)` on success, or
/// `Err(CompileError)` if type checking fails.
#[allow(dead_code)]
pub fn check_with_options(
    program: &Program,
    options: checker::CheckOptions,
) -> Result<CheckResult, CompileError> {
    check_with_options_and_bounds(program, options, &crate::generics::GenericContext::default())
}

/// Runs type checking on the host platform, answering the bound obligations that generic class
/// instantiation left behind.
///
/// The host-target counterpart of [`check_with_target_and_bounds`]; see it for why the
/// obligations travel separately from the program.
pub fn check_with_options_and_bounds(
    program: &Program,
    options: checker::CheckOptions,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    checker::check_types_with_options(program, Target::detect_host(), options, generics)
}

/// Runs type checking targeting a specific platform (e.g., Linux instead of the host macOS).
/// Returns `Ok(CheckResult)` on success, or `Err(CompileError)` if type checking fails.
/// The target affects FFI metadata, required library resolution, and platform-specific type behavior.
/// Delegates to `check_with_target_and_options` with default `CheckOptions`.
///
/// No longer called by the compile pipeline directly (it now threads `CheckOptions`
/// via `check_with_target_and_options`), so this is dead code outside of tests.
#[allow(dead_code)]
pub fn check_with_target(program: &Program, target: Target) -> Result<CheckResult, CompileError> {
    check_with_target_and_options(
        program,
        target,
        checker::CheckOptions::default(),
        &crate::generics::GenericContext::default(),
    )
}

/// Runs type checking targeting a specific platform, answering the bound obligations that
/// generic class instantiation left behind.
///
/// The entry point for a caller driving [`crate::generics::monomorphize`] by hand: instantiating
/// `Box<User>` is syntax, but deciding that `User` satisfies `T : Entity` needs this checker's
/// class table, so the obligations travel from the one pass to the other.
#[allow(dead_code)]
pub fn check_with_target_and_bounds(
    program: &Program,
    target: Target,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    check_with_target_and_options(
        program,
        target,
        checker::CheckOptions::default(),
        generics,
    )
}

/// Runs type checking targeting a specific platform with explicit `CheckOptions`
/// (e.g. `--strict-locals`). Returns `Ok(CheckResult)` on success, or
/// `Err(CompileError)` if type checking fails.
pub fn check_with_target_and_options(
    program: &Program,
    target: Target,
    options: checker::CheckOptions,
    generics: &crate::generics::GenericContext,
) -> Result<CheckResult, CompileError> {
    checker::check_types_with_options(program, target, options, generics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen_support::platform::Platform;
    use crate::codegen::platform::{Arch, Target};

    /// Parses PHP source text into an AST for use in tests.
    fn parse_program(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("tokenize failed");
        crate::parser::parse(&tokens).expect("parse failed")
    }

    /// Verifies that hashing builtins (here `md5`) require the pure-Rust
    /// `elephc_crypto` bridge on EVERY target after the Phase 2 migration off
    /// CommonCrypto/libcrypto — not the old linux-only system `crypto` library
    /// (which previously left macOS with no required library).
    #[test]
    fn test_hash_builtin_requires_elephc_crypto_on_all_targets() {
        let program = parse_program("<?php echo md5(\"abc\");");

        let linux = check_with_target(&program, Target::new(Platform::Linux, Arch::AArch64))
            .expect("linux type check failed");
        assert_eq!(linux.required_libraries, vec!["elephc_crypto"]);

        let mac = check_with_target(&program, Target::new(Platform::MacOS, Arch::AArch64))
            .expect("mac type check failed");
        assert_eq!(mac.required_libraries, vec!["elephc_crypto"]);
    }

    /// Verifies enum class metadata preserves flattened trait relation data for runtime reflection.
    #[test]
    fn test_enum_class_info_preserves_trait_metadata() {
        let program = parse_program(
            r#"<?php
trait EnumMetaTrait {
    public function original() {}
}
enum EnumMetaTarget {
    use EnumMetaTrait {
        original as aliasOriginal;
    }
    case Ready;
}
"#,
        );

        let result = check(&program).expect("type check failed");
        let enum_class = result
            .classes
            .get("EnumMetaTarget")
            .expect("missing enum class metadata");

        assert_eq!(enum_class.used_traits, vec!["EnumMetaTrait"]);
        assert_eq!(
            enum_class.trait_aliases,
            vec![(
                "aliasOriginal".to_string(),
                "EnumMetaTrait::original".to_string()
            )]
        );
    }
}
