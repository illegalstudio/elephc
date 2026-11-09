//! Purpose:
//! Stores module-wide codegen artifacts that can be shared across function contexts.
//! Deduplicates runtime callable descriptors, wrappers, and invokers by semantic shape.
//!
//! Called from:
//! - `crate::codegen::block_emit::emit_module()` creates one state per generated module.
//! - `crate::codegen::lower_inst::callables` reuses emitted callable artifacts through it.
//!
//! Key details:
//! - Cached labels are global assembly entries emitted at their first call site.
//! - Descriptor invoker cache identity is the signature, the capture list and the
//!   string-result owner contract. Every invoker binds the same container layout, so two
//!   wrappers agreeing on those three can safely share one emitted body.
//! - Receiver-bearing descriptors cache only immutable templates; each call still captures its object.
//! - Owns the module-wide assembly label counter. It must not be per function: the readable part
//!   of a label is a lossy fragment of the PHP function/block name, so only a module-unique
//!   trailing id keeps two functions with similar names from emitting the same label.
//! - Indexes emitted instance and static method bodies once from immutable EIR module metadata.

// The emitted-method sets are probed at every method dispatch lowering, and a sample of the
// codegen phase caught SipHash at 7% of it. `fast_hash` exists for exactly these keys.
use crate::fast_hash::FastSet as HashSet;

use crate::codegen::callable_dispatch::{RuntimeCallableCase, RuntimeStaticMethodCallableCase};
use crate::ir::{CoreBuiltinOp, Function, Immediate, Module, Op, RuntimeCallTarget, RuntimeFnId};
use crate::names::php_symbol_key;
use crate::types::{FunctionSig, PhpType};

use super::shared_reflection::SharedReflectionState;

/// Cached string-callable cases: argument type, candidate names, strictness, and the cases.
/// `(source type, candidate names, strict, arity, cases)`: the arity is part of the key because an
/// arity-bounded table is a different one, and leaving it out of the cache rebuilt the whole
/// runtime-name ladder at every arity-specific site (a quarter of a Symfony build's codegen).
type StringCasesEntry = (Option<PhpType>, Option<Vec<String>>, bool, Option<usize>, Vec<RuntimeCallableCase>);

/// Cached static-method callable cases for one candidate-name set.
type StaticCasesEntry = (Option<Vec<String>>, Vec<RuntimeStaticMethodCallableCase>);

/// Receiver-captured descriptor templates, bucketed by `instance_descriptor_bucket`.
type InstanceDescriptorMap =
    crate::fast_hash::FastMap<u64, Vec<RuntimeInstanceMethodDescriptorCacheEntry>>;
/// Optional runtime families discovered while codegen lowers synthetic EIR functions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GeneratedRuntimeFeatures {
    pub(crate) handler_state: bool,
    pub(crate) object_clone: bool,
}

/// Module-wide artifacts emitted once and reused by every function lowering context.
pub(crate) struct SharedCodegenState {
    pub(super) reflection: SharedReflectionState,
    /// The module copy runtime call wrappers intern their missing data into; see
    /// `take_wrapper_module`.
    wrapper_module: Option<Box<Module>>,
    /// The scope-independent callable-array instance candidates; see
    /// `instance_method_target_base`.
    instance_method_target_base: std::cell::OnceCell<
        std::sync::Arc<Vec<super::lower_inst::callables::RuntimeArrayInstanceMethodTarget>>,
    >,
    /// The callable-array instance candidates of each class scope (`None` outside a class):
    /// the base filtered by the scope's visibility and private shadowing. A pure function of the
    /// module and the base, asked at every callable-array site, which each rebuilt and recloned.
    instance_method_targets_by_scope: std::cell::RefCell<
        crate::fast_hash::FastMap<Option<String>, std::sync::Arc<Vec<super::lower_inst::callables::RuntimeArrayInstanceMethodTarget>>>,
    >,
    emitted_instance_methods: EmittedMethods,
    emitted_static_methods: EmittedMethods,
    /// Memoized `method_dispatch::mixed_method_candidates` answers, keyed by the caller's class
    /// scope, the method key and the operand count. A pure function of the module, so no
    /// cache-touch accounting: any worker may build its own.
    mixed_method_candidates: std::cell::RefCell<crate::fast_hash::FastMap<MixedCandidatesKey, MixedCandidates>>,
    /// Addresses of the signatures in `instance_method_target_base`, which this state keeps
    /// alive: a target list made only of them is named by those addresses alone.
    base_target_signatures: std::cell::OnceCell<crate::fast_hash::FastSet<usize>>,
    /// Finished composite instance lookup tables, keyed by the base-signature addresses of
    /// their targets in order. Most callable-array sites of one class scope ask for the same
    /// table; each used to look every target's descriptor up and rebuild the table.
    instance_lookup_tables:
        std::cell::RefCell<crate::fast_hash::FastMap<Vec<usize>, crate::codegen_support::callable_lookup::StringLookupTable>>,
    /// Keys of the keyed helpers this state already emitted. Two cache entries can need one
    /// helper (an instance-method entry wrapper serves every receiver class of its implementing
    /// class), and a second copy in the same text is a duplicate definition: only the parallel
    /// merge drops copies, and the helpers emitted ahead of the bodies never pass through it.
    emitted_keyed_helpers: HashSet<String>,
    /// Helper-marker scopes (`{:012x}` of the key's label scope) already present in the merged
    /// text of a parallel pass. The compiling thread never emitted those helpers itself, so its
    /// `emitted_keyed_helpers` does not hold their keys; a pass that runs after the last
    /// de-duplication (the callable-argument normalizer) consults this instead.
    merged_helper_scopes: HashSet<String>,
    /// Descriptor argument adapters need the shared boxed-callable normalizer.
    pub(super) callable_argument_normalizer: bool,
    generated_runtime_features: GeneratedRuntimeFeatures,
    /// Declared parameter contracts cached independently of emitted method bodies.
    pub(super) literal_methods: super::literal_method_index::LiteralMethodIndex,
    runtime_string_descriptor_cases: Vec<StringCasesEntry>,
    runtime_static_method_descriptor_cases: Vec<StaticCasesEntry>,
    /// Static-method descriptor cases by php name: a `Vec` scanned per lookup was the top of
    /// stack of the Symfony demo's parallel codegen (thousands of cases, asked per target).
    runtime_static_method_descriptor_case_entries:
        crate::fast_hash::FastMap<String, RuntimeStaticMethodCallableCase>,
    /// Receiver-captured descriptor templates, bucketed by a hash of their three-part name key.
    ///
    /// This was a flat `Vec` scanned linearly, comparing three strings AND a whole `FunctionSig`
    /// per entry. The Symfony `--web` module fills it with thousands of templates and asks for
    /// one at every dynamic call site, so the scan is quadratic: a 40-second sample of the
    /// codegen phase put this one lookup at 4 404 of ~28 000 non-idle samples, 15.7%.
    ///
    /// The bucket key deliberately excludes the signature, which is the expensive part to
    /// compare: a bucket holds the handful of entries sharing a name, and the signature is
    /// compared only within it.
    runtime_instance_method_descriptors:
        crate::fast_hash::FastMap<u64, Vec<RuntimeInstanceMethodDescriptorCacheEntry>>,
    runtime_callable_invokers: Vec<RuntimeCallableInvokerCacheEntry>,
    eval_registration_helper: Option<String>,
    /// Set when the eval registration helper was emitted ahead of the bodies: its label is then
    /// a constant, and reading it is not a shared access a worker must avoid.
    eval_registration_preset: bool,
    /// What the helpers emitted ahead of the bodies cached, for a codegen worker's chunk. Looked
    /// up before the chunk's own caches: a hit there reads something emitted once, in the main
    /// text, so it is not a shared access; only a miss (which emits) can defer a body. Shared by
    /// reference, not copied, because every chunk of the parallel pass starts from it.
    preset: Option<std::sync::Arc<ModulePreset>>,
    runtime_builtin_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
    runtime_extern_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
    /// Memoized sharing decision for open Mixed callable dispatch by strictness profile.
    mixed_callable_sharing: super::shared_mixed_callable::MixedCallableSharing,
    /// The lookup table naming every non-public instance method, once built in this chunk's data
    /// section (`Some(None)`: the module has none). Every callable-array site that reports an
    /// unreachable method consults the same table, and building it is linear in the module's
    /// methods, so rebuilding it per site would be quadratic. It names a label in THIS data
    /// section only, which is why it needs no cross-chunk handling.
    inaccessible_method_table: Option<Option<crate::codegen_support::callable_lookup::StringLookupTable>>,
    /// Bumped by every shared-cache access, READ or write. `emit_module` samples it per body
    /// under `ELEPHC_CODEGEN_SHARED_TOUCHES` to learn how many bodies could be emitted without
    /// the shared state at all, which is what decides whether a parallel emission pass is worth
    /// it. A `Cell` because the lookups take `&self`, and a read that MISSES leads to emitting a
    /// helper, so a read counts exactly as much as a write.
    cache_touches: std::cell::Cell<usize>,
    /// Accesses to caches whose helper symbols are NOT yet derived from the cache key. A body
    /// that reaches one of those still has to be emitted serially: two workers would give the
    /// same helper two names and the merge could not collapse them.
    unkeyed_cache_touches: std::cell::Cell<usize>,
    /// Memoized "does this module share the Mixed string-context ladder", indexed by mode.
    ///
    /// The predicate counts sites across every body in the module and is consulted at EVERY
    /// string context, so computing it per site is quadratic in module size. That is not
    /// theoretical here: an `eval()` program emits close to a million lines of assembly, and
    /// its sites would each rescan the whole instruction stream.
    mixed_string_sharing: [Option<bool>; 2],
    /// Memoized "does this module share the `count()` countable guard", for the same reason.
    count_guard_sharing: Option<bool>,
    /// Memoized "does this module share the codegen-raised throwable", for the same reason.
    static_throw_sharing: Option<bool>,
    /// Class and interface names by their PHP case-insensitive key, built on first use.
    ///
    /// `class_info_by_name` scanned every class in the module and called `php_symbol_key` — which
    /// allocates a lowercased `String` — on EVERY entry, so one lookup cost thousands of
    /// allocations, and property lowering calls it per access and again for each parent hop. A
    /// sample of the codegen phase put the lookup at 11% and the allocator it feeds at 57%.
    ///
    /// `RefCell` because the lookup is reached through a shared `&FunctionContext`; the index is
    /// a pure function of the module, so filling it through a shared reference changes nothing an
    /// observer could see.
    class_key_index: std::cell::RefCell<Option<crate::fast_hash::FastMap<String, String>>>,
    /// Interface names by the same key, for the same reason.
    interface_key_index: std::cell::RefCell<Option<crate::fast_hash::FastMap<String, String>>>,
    /// Trait POSITIONS in `trait_table.names` by the same key; the table is a `Vec`, so the index
    /// stores where the name lives rather than a second copy of it.
    trait_key_index: std::cell::RefCell<Option<crate::fast_hash::FastMap<String, usize>>>,
    /// Enum names by the same key, for the same reason.
    enum_key_index: std::cell::RefCell<Option<crate::fast_hash::FastMap<String, String>>>,
    /// `object_type_is_a(source, target)` answers, keyed by `source \0 target` as spelled.
    ///
    /// A typed-property guard asks it for EVERY class of the program against each declared class,
    /// at every arm of every dynamic property write: on Laravel that walk -- lowercased keys and
    /// cloned names at each parent hop -- was most of the codegen phase. The relation is a pure
    /// function of the module, so remembering it changes nothing an observer can see.
    type_is_a_memo: std::cell::RefCell<crate::fast_hash::FastMap<String, crate::fast_hash::FastMap<String, bool>>>,
    /// `eval_sync_global_type` answers by global name: each one walks every instruction of every
    /// function and closure, and every dynamic include syncs every global, so Laravel paid
    /// globals x includes whole-program walks. A pure function of the module.
    eval_sync_global_type_memo:
        std::cell::RefCell<crate::fast_hash::FastMap<String, Option<crate::types::PhpType>>>,
    /// Runtime class ids satisfying a declared class list, keyed by the list as spelled.
    compatible_class_ids_memo:
        std::cell::RefCell<crate::fast_hash::FastMap<Vec<String>, std::rc::Rc<[u64]>>>,
    /// Every global needing shared reference-cell storage, collected in one walk of the module.
    ///
    /// `superglobals::uses_shared_ref_cell` answers for ONE name by walking every instruction of
    /// every body, and codegen asks it per scope entry at each `eval` site: a sample of the
    /// codegen phase found that chain holding its heaviest iterator.
    shared_ref_cell_globals: Option<crate::fast_hash::FastSet<String>>,
    /// Memoized "does this module call `pcntl_async_signals()`" and "… `pcntl_signal()`".
    ///
    /// These are the same trap as the two above, and they were paying it: `FunctionContext::new`
    /// asked BOTH on every body, and each answer walks every instruction of every function,
    /// method, closure, fiber wrapper, callback wrapper, extern trampoline and runtime invoker
    /// in the module. On the Symfony `--web` module that is 745 237 instructions revisited by
    /// each of 8 786 bodies, twice — and a 50-second sample of the codegen phase put
    /// `Chain::try_fold` at 22 916 of ~35 000 non-idle samples, about two thirds of it.
    pcntl_async_signals: Option<bool>,
    /// See `pcntl_async_signals`.
    pcntl_signal_handlers: Option<bool>,
    /// Memoized "did this module keep the `__elephc_shutdown_run` drain entry".
    ///
    /// Same trap, smaller walk — the module's FUNCTION LIST, not its instructions — but it is
    /// asked once per body and the Symfony module has 8 786 of them, so it is memoized with the
    /// rest rather than left as a linear scan repeated per body.
    php_shutdown_drain: Option<bool>,
    /// Armed by a codegen worker around one body: the `unkeyed_cache_touches` value the body
    /// started from. While it is set, the block walker stops the body at the first instruction
    /// that moved the count, instead of finishing a body the worker is going to discard.
    deferral_watermark: Option<usize>,
    /// `--counters`: every non-synthetic PHP function prologue increments a BSS slot.
    pub(super) counters: bool,
    /// `--instrument`: every non-synthetic PHP function calls
    /// `elephc_instr_enter(id)`/`_exit(id)` around its body for exact timing.
    pub(super) instrument: crate::codegen::Instrumentation,
    /// `--probe`: the embedded symbol table `(data label, entry count)` main's
    /// prologue hands to `elephc_probe_init`. `None` unless the probe is enabled.
    pub(super) probe_table: Option<(String, usize)>,
    /// Counted functions as `(display name, counter symbol)`, in emission order. Main's
    /// epilogue renders the exit dump from this list — main is emitted last, so the
    /// registry is complete by then.
    counter_registry: Vec<(String, String)>,
    /// Instrumented function names, in id order (id = index). Main emits the
    /// name table `elephc_instr_init` reads; main is emitted last, so it is
    /// complete by then.
    instr_registry: Vec<String>,
    /// Instruction sequences this worker has seen, for `op_outline`.
    op_outline: super::op_outline::OpOutlineState,
    /// Keys of the typed-property guard helpers this state has already requested.
    property_guard_requested: HashSet<String>,
    /// Requested guard helpers whose bodies have not been emitted yet. They are emitted right
    /// after the body that asked, never in the middle of it; see `drain_property_guard_helpers`.
    property_guard_pending: Vec<PendingPropertyGuard>,
    /// Keys of the runtime-name property helpers this state has already requested.
    dynamic_prop_requested: HashSet<String>,
    /// Requested runtime-name property helpers whose bodies are not emitted yet.
    dynamic_prop_pending: Vec<PendingDynamicPropHelper>,
    /// Helpers a parallel worker left to the compiling thread.
    postponed_helpers: Vec<PendingDynamicPropHelper>,
    /// The opcode and name immediate each named-read helper repeats, by helper key.
    pub(super) named_get_templates:
        crate::fast_hash::FastMap<String, (crate::ir::Op, Option<crate::ir::Immediate>)>,
    /// What each shared untyped-receiver method-call helper is built from, by helper key.
    pub(super) method_call_templates:
        crate::fast_hash::FastMap<String, MethodCallHelperTemplate>,
    /// What each shared owner-call helper cannot read off its class-erased parameters, by helper
    /// label: the argument-pack collector flags (they follow from the call site's receiver class)
    /// and whether the site publishes a lexical class through `_eval_owner_scope_*`.
    pub(super) owner_call_site_facts: crate::fast_hash::FastMap<String, (i64, bool)>,
}

/// The instruction shape a shared untyped-receiver method-call helper repeats: the opcode, the
/// method-name immediate (with its strict-types profile) and the type of every operand and of
/// the result, which are all part of the helper's key.
#[derive(Clone, Debug)]
pub(crate) struct MethodCallHelperTemplate {
    pub(crate) op: crate::ir::Op,
    pub(crate) immediate: Option<crate::ir::Immediate>,
    pub(crate) operands: Vec<(crate::ir::IrType, PhpType)>,
    pub(crate) result_type: crate::ir::IrType,
    pub(crate) result_php_type: PhpType,
    /// Ownership of the result as the site records it.
    pub(crate) result_ownership: crate::ir::Ownership,
}

/// What a shared runtime-name property helper does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DynamicPropHelperKind {
    /// `$mixed->{$name}` read or probe against the whole module's classes.
    MixedGet(crate::ir::PropertyFetchMode),
    /// `$mixed->{$name} = $boxed`, emitted as a synthetic function.
    MixedSet,
    /// `unset($mixed->{$name})`, emitted as a synthetic function.
    MixedUnset,
    /// `$mixed->{$name}` read or probe from a body with a RUNTIME closure scope (see
    /// [`Self::MixedSetRuntimeScope`]).
    MixedGetRuntimeScope(crate::ir::PropertyFetchMode),
    /// `$mixed->{$name} = $boxed` from a body with a RUNTIME closure scope: the synthetic function
    /// also receives the caller's hidden scope capture, so its ladder and scope gate resolve as
    /// the caller's would.
    MixedSetRuntimeScope,
    /// `unset($mixed->{$name})` from a body with a runtime closure scope (see
    /// [`Self::MixedSetRuntimeScope`]).
    MixedUnsetRuntimeScope,
    /// `$mixed->name` read of one declared name, emitted as a synthetic function.
    /// `$mixed->name = $boxed` of one declared name, emitted as a synthetic function.
    NamedMixedSet { property: String },
    NamedMixedGet {
        property: String,
        mode: crate::ir::PropertyFetchMode,
    },
    /// `$mixed->method(...)` on an untyped receiver, emitted as a synthetic function.
    MixedMethodCall,
    /// A boxed callable resolved to its descriptor, emitted with a frame of its own.
    MixedCallableResolver(Box<CallableResolverSpec>),
    /// The search of one scope's callable-array targets over a selector on the caller's stack.
    MixedCallableArrayLookup(CallableResolverMiss),
}

/// Everything a shared callable resolver's code depends on, beside the module and the scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallableResolverSpec {
    /// The operation php-facing diagnostics and labels name.
    pub(crate) op_name: String,
    /// Whether a value that already is a descriptor comes back retained.
    pub(crate) retain_existing_descriptor: bool,
    /// Whether extension-only callables stay out of reach.
    pub(crate) strict_php: bool,
    /// The `TypeError` a string that names nothing raises, instead of the undefined fatal.
    pub(crate) string_type_error: Option<&'static str>,
    /// The `TypeError` any other refused shape raises, instead of the not-callable fatal.
    pub(crate) invalid_type_error: Option<&'static str>,
    /// The argument count, when the site narrows the string cases by it.
    pub(crate) arity: Option<usize>,
    /// How a method the scope cannot reach is reported; `None` derives it from the errors above.
    pub(crate) miss: Option<CallableResolverMiss>,
}

/// The frame-independent ways to report a callable method the scope cannot reach.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CallableResolverMiss {
    /// No dedicated diagnostic.
    Ignore,
    /// php's callback validation, with this prefix.
    InvalidCallback(String),
    /// php's direct call wording.
    DirectCall,
}

/// One runtime-name property helper waiting for its body to be emitted.
#[derive(Clone, Debug)]
pub(crate) struct PendingDynamicPropHelper {
    /// Cache key: everything the helper's code depends on.
    pub(crate) key: String,
    /// Entry symbol, derived from the key.
    pub(crate) label: String,
    pub(crate) kind: DynamicPropHelperKind,
    /// The php scope class the helper resolves visibility from.
    pub(crate) scope: Option<String>,
}

/// One typed-property guard helper waiting for its body to be emitted.
#[derive(Clone, Debug)]
pub(crate) struct PendingPropertyGuard {
    /// Cache key: everything the helper's code depends on.
    pub(crate) key: String,
    /// Entry symbol, derived from the key so every codegen worker spells it the same way.
    pub(crate) label: String,
    /// The declared property type the helper enforces.
    pub(crate) php_type: PhpType,
}

/// The shared caches as the helpers emitted ahead of the bodies left them; see
/// `SharedCodegenState::module_preset`.
#[derive(Default)]
pub(crate) struct ModulePreset {
    eval_registration_helper: Option<String>,
    string_cases: Vec<StringCasesEntry>,
    static_cases: Vec<StaticCasesEntry>,
    static_case_entries: crate::fast_hash::FastMap<String, RuntimeStaticMethodCallableCase>,
    instance_descriptors:
        crate::fast_hash::FastMap<u64, Vec<RuntimeInstanceMethodDescriptorCacheEntry>>,
    invokers: Vec<RuntimeCallableInvokerCacheEntry>,
    builtin_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
    extern_wrappers: Vec<RuntimeCallWrapperCacheEntry>,
    /// The module-wide open Mixed callable sharing decision per strictness profile. It is decided
    /// once, before any body, by counting the module's sites; a worker chunk that did not inherit
    /// it read "not decided" as "not shared" and inlined the whole dispatcher at every one of its
    /// sites (about 600 instructions each instead of a call).
    mixed_callable_sharing: super::shared_mixed_callable::MixedCallableSharing,
    /// Whole-module answers, decided once before any body (`context::prefill_module_facts`). Each
    /// walks every body of the program, and a worker chunk that had to ask again did so once per
    /// chunk: 256 scans of each, about 40% of the Symfony Demo's codegen.
    pcntl_async_signals: Option<bool>,
    pcntl_signal_handlers: Option<bool>,
    php_shutdown_drain: Option<bool>,
    mixed_string_sharing: [Option<bool>; 2],
    count_guard_sharing: Option<bool>,
    /// `compatible_class_ids_memo` across chunks: its keys are only known at the sites, so the
    /// first chunk that asks computes and every other one reads.
    compatible_class_ids:
        std::sync::RwLock<crate::fast_hash::FastMap<Vec<String>, std::sync::Arc<[u64]>>>,
}

/// Reusable static descriptor template for one public instance method.
#[derive(Clone)]
pub(super) struct RuntimeInstanceMethodDescriptorTemplate {
    pub(super) descriptor_label: String,
}

/// Cache key and emitted template for one receiver-class/method/signature shape.
#[derive(Clone)]
struct RuntimeInstanceMethodDescriptorCacheEntry {
    class_name: String,
    method_key: String,
    impl_class: String,
    signature: FunctionSig,
    template: RuntimeInstanceMethodDescriptorTemplate,
}

/// Cache key and label for one signature-compatible descriptor invoker body.
#[derive(Clone)]
struct RuntimeCallableInvokerCacheEntry {
    /// `invoker_fingerprint` of the key fields, compared before the full key: a lookup scans
    /// thousands of entries on Symfony and `FunctionSig` equality was the top codegen frame.
    fingerprint: u64,
    signature: FunctionSig,
    captures: Vec<(String, PhpType, bool)>,
    owns_string_return: bool,
    php_return_status: bool,
    /// The RESOLVED defaults the body materializes. Two signatures can be identical as ABI and
    /// still fold `self::`/`parent::` defaults differently in different class scopes, so the
    /// resolved values are part of the key rather than the declaring class name.
    defaults: crate::codegen::runtime_callable_invoker::InvokerDefaults,
    label: String,
}

/// Cache key and label for one synthetic builtin or extern entry wrapper.
#[derive(Clone)]
struct RuntimeCallWrapperCacheEntry {
    name: String,
    signature: FunctionSig,
    strict_php: bool,
    label: String,
}

/// Buckets a receiver-captured descriptor template by its name key, signature excluded.
fn instance_descriptor_bucket(class_name: &str, method_key: &str, impl_class: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    class_name.hash(&mut hasher);
    method_key.hash(&mut hasher);
    impl_class.hash(&mut hasher);
    hasher.finish()
}

/// `(caller class scope, method key, operand count)` of a `mixed_method_candidates` question.
pub(super) type MixedCandidatesKey = (Option<String>, String, usize);

/// A `mixed_method_candidates` answer.
pub(super) type MixedCandidates = std::sync::Arc<Vec<super::lower_inst::method_call_types::MixedMethodCandidate>>;

/// The `(class, canonical method key)` pairs of the emitted method bodies, grouped by class so a
/// probe borrows both names: a tuple set needed two fresh `String`s per question, and codegen asks
/// per candidate class of every method call.
#[derive(Debug, Default, Clone)]
pub(super) struct EmittedMethods(crate::fast_hash::FastMap<String, HashSet<String>>);

impl EmittedMethods {
    /// Records one emitted body.
    fn insert(&mut self, class_name: &str, method_key: String) {
        self.0.entry(class_name.to_string()).or_default().insert(method_key);
    }

    /// Whether `class_name` has an emitted body for `method_key`.
    pub(super) fn contains(&self, class_name: &str, method_key: &str) -> bool {
        self.0.get(class_name).is_some_and(|methods| methods.contains(method_key))
    }
}

impl SharedCodegenState {
    /// Builds module-wide caches and immutable method-membership indexes before emission starts.
    pub(super) fn for_module(module: &Module) -> Self {
        let mut state = Self::empty();
        state.reflection = SharedReflectionState::for_module(module);
        for function in &module.class_methods {
            let Some((class_name, method_name)) = function.name.rsplit_once("::") else {
                continue;
            };
            let methods = if function.flags.is_static {
                &mut state.emitted_static_methods
            } else {
                &mut state.emitted_instance_methods
            };
            methods.insert(class_name, php_symbol_key(method_name));
        }
        state
    }

    /// Hands out the module copy a runtime call wrapper interns its missing data entries into:
    /// the one an earlier wrapper left, or a fresh copy of `module` the first time.
    ///
    /// Codegen never changes the module, so a copy that earlier wrappers interned into is still
    /// the module plus those entries, and a later wrapper can be lowered and emitted against it.
    /// Copying the whole module (every body, every class) per wrapper, then dropping it, was a
    /// third of every wrapper's cost on a Symfony build. A wrapper emitted while another holds
    /// the copy (one wrapper reaching another) gets its own.
    pub(super) fn take_wrapper_module(&mut self, module: &Module) -> Box<Module> {
        self.wrapper_module.take().unwrap_or_else(|| Box::new(module.clone()))
    }

    /// The module's emitted instance methods as callable-array candidates, built once by `build`.
    ///
    /// A pure function of the module (no helper is emitted while building it), so it needs no
    /// cache-touch accounting: any codegen worker may build its own copy.
    /// The instance candidates of class scope `scope`, built by `build` on first request.
    pub(super) fn instance_method_targets_for_scope(
        &self,
        scope: Option<&str>,
        build: impl FnOnce() -> Vec<super::lower_inst::callables::RuntimeArrayInstanceMethodTarget>,
    ) -> std::sync::Arc<Vec<super::lower_inst::callables::RuntimeArrayInstanceMethodTarget>> {
        let key = scope.map(str::to_string);
        if let Some(found) = self.instance_method_targets_by_scope.borrow().get(&key) {
            return found.clone();
        }
        // Built outside the borrow: building asks this state for the base.
        let built = std::sync::Arc::new(build());
        self.instance_method_targets_by_scope.borrow_mut().insert(key, built.clone());
        built
    }

    pub(super) fn instance_method_target_base(
        &self,
        build: impl FnOnce() -> Vec<super::lower_inst::callables::RuntimeArrayInstanceMethodTarget>,
    ) -> std::sync::Arc<Vec<super::lower_inst::callables::RuntimeArrayInstanceMethodTarget>> {
        self.instance_method_target_base
            .get_or_init(|| std::sync::Arc::new(build()))
            .clone()
    }

    /// The signature addresses of the instance-method target base, collected once by `build`.
    pub(super) fn base_target_signatures(
        &self,
        build: impl FnOnce() -> crate::fast_hash::FastSet<usize>,
    ) -> &crate::fast_hash::FastSet<usize> {
        self.base_target_signatures.get_or_init(build)
    }

    /// A finished instance lookup table for targets named by `key`.
    ///
    /// Keyed like the descriptor templates the table is made of (every symbol in it comes from
    /// a key), so a worker may build its own.
    pub(super) fn instance_lookup_table(
        &self,
        key: &[usize],
    ) -> Option<crate::codegen_support::callable_lookup::StringLookupTable> {
        self.touch_cache_keyed();
        self.instance_lookup_tables.borrow().get(key).cloned()
    }

    /// Records a finished instance lookup table.
    pub(super) fn cache_instance_lookup_table(
        &self,
        key: Vec<usize>,
        table: crate::codegen_support::callable_lookup::StringLookupTable,
    ) {
        self.touch_cache_keyed();
        self.instance_lookup_tables.borrow_mut().insert(key, table);
    }

    /// A memoized `mixed_method_candidates` answer.
    pub(super) fn mixed_method_candidates(&self, key: &MixedCandidatesKey) -> Option<MixedCandidates> {
        self.mixed_method_candidates.borrow().get(key).cloned()
    }

    /// Records a `mixed_method_candidates` answer.
    pub(super) fn cache_mixed_method_candidates(&self, key: MixedCandidatesKey, candidates: MixedCandidates) {
        self.mixed_method_candidates.borrow_mut().insert(key, candidates);
    }

    /// Returns the wrapper module copy for the next wrapper.
    pub(super) fn return_wrapper_module(&mut self, copy: Box<Module>) {
        self.wrapper_module = Some(copy);
    }

    /// Creates private empty storage used only while constructing a module-populated state.
    fn empty() -> Self {
        Self {
            reflection: SharedReflectionState::empty(),
            wrapper_module: None,
            instance_method_target_base: std::cell::OnceCell::new(),
            instance_method_targets_by_scope: std::cell::RefCell::new(Default::default()),
            mixed_method_candidates: Default::default(),
            base_target_signatures: std::cell::OnceCell::new(),
            instance_lookup_tables: Default::default(),
            emitted_instance_methods: EmittedMethods::default(),
            emitted_static_methods: EmittedMethods::default(),
            emitted_keyed_helpers: HashSet::default(),
            merged_helper_scopes: HashSet::default(),
            callable_argument_normalizer: false,
            generated_runtime_features: GeneratedRuntimeFeatures::default(),
            runtime_string_descriptor_cases: Vec::new(),
            runtime_static_method_descriptor_cases: Vec::new(),
            runtime_static_method_descriptor_case_entries: Default::default(),
            runtime_instance_method_descriptors: crate::fast_hash::FastMap::default(),
            literal_methods: Default::default(),
            runtime_callable_invokers: Vec::new(),
            eval_registration_helper: None,
            eval_registration_preset: false,
            preset: None,
            runtime_builtin_wrappers: Vec::new(),
            runtime_extern_wrappers: Vec::new(),
            mixed_string_sharing: [None; 2],
            mixed_callable_sharing: Default::default(),
            inaccessible_method_table: None,
            cache_touches: std::cell::Cell::new(0),
            unkeyed_cache_touches: std::cell::Cell::new(0),
            count_guard_sharing: None,
            static_throw_sharing: None,
            class_key_index: std::cell::RefCell::new(None),
            interface_key_index: std::cell::RefCell::new(None),
            trait_key_index: std::cell::RefCell::new(None),
            enum_key_index: std::cell::RefCell::new(None),
            type_is_a_memo: std::cell::RefCell::new(Default::default()),
            eval_sync_global_type_memo: std::cell::RefCell::new(Default::default()),
            compatible_class_ids_memo: std::cell::RefCell::new(Default::default()),
            shared_ref_cell_globals: None,
            pcntl_async_signals: None,
            pcntl_signal_handlers: None,
            php_shutdown_drain: None,
            deferral_watermark: None,
            counters: false,
            instrument: crate::codegen::Instrumentation::default(),
            probe_table: None,
            counter_registry: Vec::new(),
            instr_registry: Vec::new(),
            op_outline: Default::default(),
            property_guard_requested: HashSet::default(),
            property_guard_pending: Vec::new(),
            dynamic_prop_requested: HashSet::default(),
            dynamic_prop_pending: Vec::new(),
            postponed_helpers: Vec::new(),
            named_get_templates: Default::default(),
            method_call_templates: Default::default(),
            owner_call_site_facts: Default::default(),
        }
    }

    /// Returns whether the immutable module inventory contains the requested method body.
    pub(super) fn emitted_method_contains(
        &self,
        class_name: &str,
        canonical_method_key: &str,
        is_static: bool,
    ) -> bool {
        let methods = if is_static {
            &self.emitted_static_methods
        } else {
            &self.emitted_instance_methods
        };
        methods.contains(class_name, canonical_method_key)
    }

    /// Borrows the immutable instance-method inventory for contains-only interface validation.
    pub(super) fn emitted_instance_method_keys(&self) -> &EmittedMethods {
        &self.emitted_instance_methods
    }

    /// Returns the memoized unreachable-method lookup table, if this chunk already built it.
    pub(super) fn inaccessible_method_table(
        &self,
    ) -> Option<Option<crate::codegen_support::callable_lookup::StringLookupTable>> {
        self.inaccessible_method_table.clone()
    }

    /// Memoizes the unreachable-method lookup table built for this chunk's data section.
    pub(super) fn cache_inaccessible_method_table(
        &mut self,
        table: Option<crate::codegen_support::callable_lookup::StringLookupTable>,
    ) {
        self.inaccessible_method_table = Some(table);
    }

    /// Returns the memoized Mixed callable sharing decision for one strictness profile.
    pub(super) fn mixed_callable_sharing(&self) -> &super::shared_mixed_callable::MixedCallableSharing {
        &self.mixed_callable_sharing
    }

    /// Stores the Mixed callable sharing decision for one strictness profile.
    pub(super) fn set_mixed_callable_sharing(
        &mut self,
        sharing: super::shared_mixed_callable::MixedCallableSharing,
    ) {
        self.mixed_callable_sharing = sharing;
    }

    /// Records dependencies introduced by a synthetic wrapper that was not present during the
    /// module's original EIR feature scan.
    pub(super) fn record_generated_runtime_features(&mut self, function: &Function) {
        for inst in &function.instructions {
            match inst.op {
                Op::RuntimeCall => {
                    let target = match inst.immediate {
                        Some(Immediate::RuntimeCall(RuntimeCallTarget::Function(target)))
                        | Some(Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction {
                            target,
                            ..
                        })) => Some(target),
                        _ => None,
                    };
                    self.generated_runtime_features.object_clone |=
                        target == Some(RuntimeFnId::CloneWith);
                }
                Op::CoreBuiltin => {
                    let operation = match inst.immediate {
                        Some(Immediate::I64(value)) => CoreBuiltinOp::from_i64(value),
                        _ => None,
                    };
                    self.generated_runtime_features.handler_state |= matches!(
                        operation,
                        Some(
                            CoreBuiltinOp::RestoreErrorHandler
                                | CoreBuiltinOp::RestoreExceptionHandler
                                | CoreBuiltinOp::SetErrorHandler
                                | CoreBuiltinOp::SetExceptionHandler
                                | CoreBuiltinOp::GetErrorHandler
                                | CoreBuiltinOp::GetExceptionHandler
                        )
                    );
                }
                _ => {}
            }
        }
    }

    /// Returns optional runtime families introduced after the module-level EIR scan.
    pub(crate) fn generated_runtime_features(&self) -> GeneratedRuntimeFeatures {
        self.generated_runtime_features
    }

    /// Reserves the next module-unique assembly label id.
    ///
    /// Every generated local label ends in `_<id>` taken from this counter. Because the id is a
    /// decimal run terminated by the preceding `_`, it is recoverable from the finished label,
    /// which makes the whole label unique no matter how ambiguous its readable prefix is.
    /// Returns how many shared-cache accesses have been made so far.
    ///
    /// Only meaningful as a DIFFERENCE across one body: a body whose count does not move could
    /// have been emitted against a read-only view of this state, on any thread.
    pub(super) fn cache_touches(&self) -> usize {
        // The reflection materializers are a cache of the same kind, living on their own struct:
        // a body that reaches one cannot be emitted against a private copy of this state either.
        self.cache_touches.get() + self.reflection.cache_touches()
    }

    // `checkpoint()`/`rollback_to()` used to live here, to undo a deferred body's cache
    // entries. They are gone on purpose: one instruction's lowering can emit thousands of
    // descriptors, so the worker's abort cannot land inside it, and the rollback then makes
    // every following body re-emit its helpers -- measured on the Symfony module, 2 101 439 143
    // discarded bytes became 29 397 554 561 and the worker pass 40s became 121s. What makes
    // their absence safe is that a body which TOUCHES a cache is always deferred, so a kept
    // body never references a cached helper.

    /// Records one access to a cache a worker may NOT reach, so its body is emitted serially.
    ///
    /// Two reasons a family stays here. Its helper's symbol may still be minted from the
    /// emitting body, so two workers would give one helper two names. Or — the one that is not
    /// obvious — the cache may be module-wide STATE rather than a lookup: the static-method
    /// descriptor cases accumulate a dispatch table, and splitting that table across per-worker
    /// states left the 404 page rendering a raw exception dump instead of the error page.
    /// Duplication across workers is safe; a partitioned table is not, and keying its labels
    /// would not have helped.
    fn touch_cache(&self) {
        self.cache_touches.set(self.cache_touches.get() + 1);
        self.unkeyed_cache_touches
            .set(self.unkeyed_cache_touches.get() + 1);
    }

    /// Records one access to a cache whose helper symbols are derived from the cache key.
    ///
    /// Such a helper can be emitted by any worker: every copy spells the same symbols, so the
    /// merge keeps one and drops the rest, and the body does not have to go serial. The callable
    /// invokers and the builtin/extern wrappers are those families.
    fn touch_cache_keyed(&self) {
        self.cache_touches.set(self.cache_touches.get() + 1);
    }

    /// Asks for the typed-property guard helper `key` to exist in the module.
    ///
    /// The first request queues the body for emission after the current function; later ones are
    /// free. The access is KEYED -- the entry symbol comes from the key -- so a codegen worker may
    /// queue it on its own and the merge keeps one copy, instead of the body going serial.
    pub(super) fn request_property_guard_helper(&mut self, key: &str, label: &str, php_type: &PhpType) {
        self.touch_cache_keyed();
        if self.property_guard_requested.insert(key.to_string()) {
            self.property_guard_pending.push(PendingPropertyGuard {
                key: key.to_string(),
                label: label.to_string(),
                php_type: php_type.clone(),
            });
        }
    }

    /// Asks for the runtime-name property helper `key` to exist in the module.
    ///
    /// Keyed like the typed-property guard: the entry symbol comes from the key, so a codegen
    /// worker may queue the body itself and the merge keeps one copy.
    pub(super) fn request_dynamic_prop_helper(
        &mut self,
        key: &str,
        label: &str,
        kind: DynamicPropHelperKind,
        scope: Option<&str>,
    ) {
        self.touch_cache_keyed();
        if self.dynamic_prop_requested.insert(key.to_string()) {
            self.dynamic_prop_pending.push(PendingDynamicPropHelper {
                key: key.to_string(),
                label: label.to_string(),
                kind,
                scope: scope.map(str::to_string),
            });
        }
    }

    /// Takes the runtime-name property helpers requested since the last drain.
    /// Whether a parallel worker must leave this helper to the compiling thread.
    ///
    /// The callable searches name descriptors and tables of every method a scope reaches, all
    /// interned into the worker's data section, where a deferred body rolls them back while the
    /// memos naming them stay. Their entry symbols come from their keys, so bodies call them
    /// wherever they end up, and the compiling thread emits each one once.
    pub(super) fn postpones_helper(&self, pending: &PendingDynamicPropHelper) -> bool {
        self.deferral_watermark.is_some()
            && matches!(
                pending.kind,
                DynamicPropHelperKind::MixedCallableResolver(_)
                    | DynamicPropHelperKind::MixedCallableArrayLookup(_)
            )
    }

    /// Keeps one helper for the compiling thread; see [`Self::postpones_helper`].
    pub(super) fn postpone_helper(&mut self, pending: PendingDynamicPropHelper) {
        self.postponed_helpers.push(pending);
    }

    /// Hands over the helpers this worker left to the compiling thread.
    pub(super) fn take_postponed_helpers(&mut self) -> Vec<PendingDynamicPropHelper> {
        std::mem::take(&mut self.postponed_helpers)
    }

    /// Whether this state already asked for the helper of `key`.
    pub(super) fn dynamic_prop_helper_requested(&self, key: &str) -> bool {
        self.dynamic_prop_requested.contains(key)
    }

    pub(super) fn take_pending_dynamic_prop_helpers(&mut self) -> Vec<PendingDynamicPropHelper> {
        std::mem::take(&mut self.dynamic_prop_pending)
    }

    /// The instruction sequences this worker has seen.
    pub(super) fn op_outline_state(&mut self) -> &mut super::op_outline::OpOutlineState {
        &mut self.op_outline
    }

    /// Takes the guard helpers requested since the last drain.
    pub(super) fn take_pending_property_guards(&mut self) -> Vec<PendingPropertyGuard> {
        std::mem::take(&mut self.property_guard_pending)
    }

    /// Returns the memoized string-context sharing decision for one mode, if already computed.
    pub(super) fn mixed_string_sharing(&self, mode_index: usize) -> Option<bool> {
        self.mixed_string_sharing[mode_index]
    }

    /// Records the string-context sharing decision so later sites reuse it.
    pub(super) fn set_mixed_string_sharing(&mut self, mode_index: usize, shares: bool) {
        self.mixed_string_sharing[mode_index] = Some(shares);
    }

    /// Returns the memoized `count()` guard sharing decision, if already computed.
    pub(super) fn count_guard_sharing(&self) -> Option<bool> {
        self.count_guard_sharing
    }

    /// Records the `count()` guard sharing decision so later sites reuse it.
    pub(super) fn set_count_guard_sharing(&mut self, shares: bool) {
        self.count_guard_sharing = Some(shares);
    }

    /// Returns the memoized codegen-raised throwable sharing decision, if already computed.
    pub(super) fn static_throw_sharing(&self) -> Option<bool> {
        self.static_throw_sharing
    }

    /// Records the codegen-raised throwable sharing decision so later raises reuse it.
    pub(super) fn set_static_throw_sharing(&mut self, shares: bool) {
        self.static_throw_sharing = Some(shares);
    }

    /// Returns a remembered `eval_sync_global_type` answer, computing it once through `compute`.
    pub(crate) fn eval_sync_global_type_memoized(
        &self,
        name: &str,
        compute: impl FnOnce() -> Option<crate::types::PhpType>,
    ) -> Option<crate::types::PhpType> {
        if let Some(answer) = self.eval_sync_global_type_memo.borrow().get(name) {
            return answer.clone();
        }
        let answer = compute();
        self.eval_sync_global_type_memo
            .borrow_mut()
            .insert(name.to_string(), answer.clone());
        answer
    }

    /// Returns a remembered `object_type_is_a` answer, computing it once through `compute`.
    pub(crate) fn type_is_a_memoized(
        &self,
        source: &str,
        target: &str,
        compute: impl FnOnce() -> bool,
    ) -> bool {
        // Keyed target, then source, so a hit looks both up in place: the `format!` key this
        // replaces allocated on every probe, and a guard probes every class of the program.
        if let Some(answer) = self
            .type_is_a_memo
            .borrow()
            .get(target)
            .and_then(|sources| sources.get(source))
        {
            return *answer;
        }
        let answer = compute();
        self.type_is_a_memo
            .borrow_mut()
            .entry(target.to_string())
            .or_default()
            .insert(source.to_string(), answer);
        answer
    }

    /// Returns remembered compatible class ids for a declared class list, computing them once.
    pub(crate) fn compatible_class_ids_memoized(
        &self,
        class_names: &[String],
        compute: impl FnOnce() -> Vec<u64>,
    ) -> std::rc::Rc<[u64]> {
        if let Some(ids) = self.compatible_class_ids_memo.borrow().get(class_names) {
            return ids.clone();
        }
        let shared = self.preset.as_ref().map(|preset| &preset.compatible_class_ids);
        let known = shared
            .and_then(|memo| memo.read().ok().and_then(|memo| memo.get(class_names).cloned()));
        let ids: std::rc::Rc<[u64]> = match known {
            Some(ids) => ids.as_ref().into(),
            None => {
                let ids: std::rc::Rc<[u64]> = compute().into();
                if let Some(mut memo) = shared.and_then(|memo| memo.write().ok()) {
                    memo.insert(class_names.to_vec(), ids.as_ref().into());
                }
                ids
            }
        };
        self.compatible_class_ids_memo
            .borrow_mut()
            .insert(class_names.to_vec(), ids.clone());
        ids
    }

    /// Returns the module's spelling of the class whose PHP key is `key`, indexing once.
    pub(crate) fn class_name_for_key(&self, module: &Module, key: &str) -> Option<String> {
        let mut index = self.class_key_index.borrow_mut();
        index
            .get_or_insert_with(|| {
                module
                    .class_infos
                    .keys()
                    .map(|name| {
                        (
                            php_symbol_key(name.trim_start_matches('\\')),
                            name.clone(),
                        )
                    })
                    .collect()
            })
            .get(key)
            .cloned()
    }

    /// Returns the module's spelling of the interface whose PHP key is `key`, indexing once.
    pub(crate) fn interface_name_for_key(&self, module: &Module, key: &str) -> Option<String> {
        let mut index = self.interface_key_index.borrow_mut();
        index
            .get_or_insert_with(|| {
                module
                    .interface_infos
                    .keys()
                    .map(|name| {
                        (
                            php_symbol_key(name.trim_start_matches('\\')),
                            name.clone(),
                        )
                    })
                    .collect()
            })
            .get(key)
            .cloned()
    }

    /// Returns where in `trait_table.names` the trait whose PHP key is `key` lives, indexing once.
    pub(crate) fn trait_position_for_key(&self, module: &Module, key: &str) -> Option<usize> {
        let mut index = self.trait_key_index.borrow_mut();
        index
            .get_or_insert_with(|| {
                module
                    .trait_table
                    .names
                    .iter()
                    .enumerate()
                    .map(|(position, name)| (php_symbol_key(name.trim_start_matches('\\')), position))
                    .collect()
            })
            .get(key)
            .copied()
    }

    /// Returns the module's spelling of the enum whose PHP key is `key`, indexing once.
    pub(crate) fn enum_name_for_key(&self, module: &Module, key: &str) -> Option<String> {
        let mut index = self.enum_key_index.borrow_mut();
        index
            .get_or_insert_with(|| {
                module
                    .enum_infos
                    .keys()
                    .map(|name| {
                        (
                            php_symbol_key(name.trim_start_matches('\\')),
                            name.clone(),
                        )
                    })
                    .collect()
            })
            .get(key)
            .cloned()
    }

    /// Returns whether `name` needs shared reference-cell storage, walking the module once.
    pub(crate) fn uses_shared_ref_cell(&mut self, module: &Module, name: &str) -> bool {
        self.shared_ref_cell_globals
            .get_or_insert_with(|| crate::superglobals::shared_ref_cell_globals(module))
            .contains(name)
    }

    /// Returns whether this module calls `pcntl_async_signals()`, computing it at most once.
    pub(super) fn pcntl_async_signals(&mut self, compute: impl FnOnce() -> bool) -> bool {
        *self.pcntl_async_signals.get_or_insert_with(compute)
    }

    /// Returns whether this module registers `pcntl_signal()` handlers, computed at most once.
    pub(super) fn pcntl_signal_handlers(&mut self, compute: impl FnOnce() -> bool) -> bool {
        *self.pcntl_signal_handlers.get_or_insert_with(compute)
    }

    /// Returns whether this module still declares the shutdown drain, computed at most once.
    pub(super) fn php_shutdown_drain(&mut self, compute: impl FnOnce() -> bool) -> bool {
        *self.php_shutdown_drain.get_or_insert_with(compute)
    }

    /// Asks for the shared callable-argument normalizer, emitted once after every body.
    ///
    /// The flag is module state a codegen worker cannot keep: a worker's chunk state is dropped
    /// after its text is merged, so a flag raised there never reached the emission decision and
    /// the program called an undefined `_eir_callable_argument_normalizer`. The first body that
    /// raises it therefore counts as a shared-cache touch and is re-emitted serially.
    pub(super) fn require_callable_argument_normalizer(&mut self) {
        if !self.callable_argument_normalizer {
            self.callable_argument_normalizer = true;
            self.touch_cache();
        }
    }

    /// Forgets the worker-built instance-method templates and keyed-helper emissions.
    ///
    /// A worker rolls a deferred body's text and data back, but its caches are not rolled back
    /// (see the note on `touch_cache`). A template or keyed helper that body built would then be
    /// reused by the next body with its wrapper text and descriptor data gone: the Symfony demo
    /// linked with undefined `_data_h*` descriptor symbols. Forgetting them on every deferral
    /// makes the next body rebuild them; the merge drops the copies.
    pub(super) fn forget_worker_templates(&mut self) {
        self.runtime_instance_method_descriptors.clear();
        self.emitted_keyed_helpers.clear();
        // Both memos name a table interned into the chunk's data section, and a deferred body's
        // data is rolled back. A table the deferred body built left its label behind with no
        // definition: the next body that hit the memo referenced a content-addressed
        // `_data_h…` the serial pass never defines (it spells its own tables by counter), and a
        // Symfony `--web` build without preload failed to link on three such labels.
        self.instance_lookup_tables.borrow_mut().clear();
        self.inaccessible_method_table = None;
    }

    /// Sizes of the append-only caches whose entries name out-of-line functions.
    pub(super) fn out_of_line_cache_mark(&self) -> (usize, usize, usize) {
        (
            self.runtime_callable_invokers.len(),
            self.runtime_builtin_wrappers.len(),
            self.runtime_extern_wrappers.len(),
        )
    }

    /// Forgets the invoker and wrapper entries added since `mark`.
    ///
    /// A worker discards a deferred body's out-of-line functions with its text; an entry that
    /// named one would send the next body to a definition that is gone.
    pub(super) fn forget_out_of_line_caches_since(&mut self, mark: (usize, usize, usize)) {
        self.runtime_callable_invokers.truncate(mark.0);
        self.runtime_builtin_wrappers.truncate(mark.1);
        self.runtime_extern_wrappers.truncate(mark.2);
    }

    /// Records that the keyed helper `key` is being emitted, returning false when this state
    /// already emitted it.
    pub(super) fn first_keyed_helper_emission(&mut self, key: &str) -> bool {
        self.touch_cache_keyed();
        if !self.merged_helper_scopes.is_empty() {
            let scope = format!("{:012x}", crate::codegen::context::body_label_scope(key));
            if self.merged_helper_scopes.contains(&scope) {
                self.emitted_keyed_helpers.insert(key.to_string());
                return false;
            }
        }
        self.emitted_keyed_helpers.insert(key.to_string())
    }

    /// Records the helper scopes a parallel pass's merged text already defines.
    pub(super) fn mark_merged_helper_scopes<'a>(&mut self, scopes: impl IntoIterator<Item = &'a String>) {
        self.merged_helper_scopes.extend(scopes.into_iter().cloned());
    }

    /// Starts watching this body for a cache a worker may not reach.
    pub(super) fn arm_deferral_watch(&mut self) {
        self.deferral_watermark = Some(self.unkeyed_cache_touches.get());
    }

    /// Stops watching, for the serial pass, which may reach anything.
    pub(super) fn disarm_deferral_watch(&mut self) {
        self.deferral_watermark = None;
    }

    /// Returns whether the body being emitted has reached a cache a worker may not reach.
    ///
    /// False whenever the watch is disarmed, which is every path but a worker's.
    pub(super) fn must_defer(&self) -> bool {
        self.deferral_watermark
            .is_some_and(|mark| self.unkeyed_cache_touches.get() != mark)
    }

    /// Records one counted function for the exit dump.
    pub(super) fn register_counter(&mut self, display_name: String, symbol: String) {
        self.counter_registry.push((display_name, symbol));
    }

    /// Returns the counted functions in emission order.
    pub(super) fn counter_registry(&self) -> &[(String, String)] {
        &self.counter_registry
    }

    /// Registers a PHP source function, reusing its id for private implementation variants.
    pub(super) fn register_instr(&mut self, display_name: String) -> usize {
        if let Some(id) = self.instr_registry.iter().position(|registered| *registered == display_name) {
            return id;
        }
        let id = self.instr_registry.len();
        self.instr_registry.push(display_name);
        id
    }

    /// Returns the instrumented function names in id order.
    pub(super) fn instr_registry(&self) -> &[String] {
        &self.instr_registry
    }

    /// Returns cached runtime string-callable cases for the requested specialization.
    pub(super) fn runtime_string_descriptor_cases(
        &self,
        source_arg_ty: Option<&PhpType>,
        candidate_names: Option<&[String]>,
        strict_php: bool,
        arity: Option<usize>,
    ) -> Option<Vec<RuntimeCallableCase>> {
        let matches = |(cached_ty, cached_names, cached_strict_php, cached_arity, _): &&StringCasesEntry| {
            cached_ty.as_ref() == source_arg_ty
                && cached_names.as_deref() == candidate_names
                && *cached_strict_php == strict_php
                && *cached_arity == arity
        };
        let preset = self.preset.as_ref();
        if let Some(entry) = preset.and_then(|preset| preset.string_cases.iter().find(matches)) {
            return Some(entry.4.clone());
        }
        self.touch_cache();
        self.runtime_string_descriptor_cases
            .iter()
            .find(matches)
            .map(|entry| entry.4.clone())
    }

    /// Stores runtime string-callable cases after their global wrappers are emitted.
    pub(super) fn cache_runtime_string_descriptor_cases(
        &mut self,
        source_arg_ty: Option<&PhpType>,
        candidate_names: Option<&[String]>,
        strict_php: bool,
        arity: Option<usize>,
        cases: &[RuntimeCallableCase],
    ) {
        self.touch_cache();
        self.runtime_string_descriptor_cases.push((
            source_arg_ty.cloned(),
            candidate_names.map(|names| names.to_vec()),
            strict_php,
            arity,
            cases.to_vec(),
        ));
    }

    /// Returns the module-wide public static-method descriptor cases, if emitted.
    pub(super) fn runtime_static_method_descriptor_cases(
        &self,
        candidate_names: Option<&[String]>,
    ) -> Option<Vec<RuntimeStaticMethodCallableCase>> {
        let matches =
            |(cached_names, _): &&StaticCasesEntry| cached_names.as_deref() == candidate_names;
        let preset = self.preset.as_ref();
        if let Some(entry) = preset.and_then(|preset| preset.static_cases.iter().find(matches)) {
            return Some(entry.1.clone());
        }
        self.touch_cache();
        self.runtime_static_method_descriptor_cases
            .iter()
            .find(matches)
            .map(|entry| entry.1.clone())
    }

    /// Stores public static-method descriptors for reuse by later call sites.
    pub(super) fn cache_runtime_static_method_descriptor_cases(
        &mut self,
        candidate_names: Option<&[String]>,
        cases: &[RuntimeStaticMethodCallableCase],
    ) {
        self.touch_cache();
        self.runtime_static_method_descriptor_cases.push((
            candidate_names.map(|names| names.to_vec()),
            cases.to_vec(),
        ));
    }

    /// Returns one static-method descriptor case already emitted for another target set.
    pub(super) fn runtime_static_method_descriptor_case(
        &self,
        php_name: &str,
    ) -> Option<RuntimeStaticMethodCallableCase> {
        if let Some(case) = self
            .preset
            .as_ref()
            .and_then(|preset| preset.static_case_entries.get(php_name))
        {
            return Some(case.clone());
        }
        self.touch_cache();
        self.runtime_static_method_descriptor_case_entries
            .get(php_name)
            .cloned()
    }

    /// Records one static-method descriptor case for reuse across candidate sets.
    pub(super) fn cache_runtime_static_method_descriptor_case(
        &mut self,
        case: &RuntimeStaticMethodCallableCase,
    ) {
        self.touch_cache();
        // Only a named case can be asked for again; the first one recorded keeps answering, as the
        // scan this replaced did.
        if let Some(php_name) = case.case.php_name.as_ref() {
            self.runtime_static_method_descriptor_case_entries
                .entry(php_name.clone())
                .or_insert_with(|| case.clone());
        }
    }

    /// Returns an emitted receiver-captured descriptor template for one method shape.
    pub(super) fn runtime_instance_method_descriptor(
        &self,
        class_name: &str,
        method_key: &str,
        impl_class: &str,
        signature: &FunctionSig,
    ) -> Option<RuntimeInstanceMethodDescriptorTemplate> {
        let bucket = instance_descriptor_bucket(class_name, method_key, impl_class);
        let find = |map: &InstanceDescriptorMap| {
            map.get(&bucket)?
                .iter()
                .find(|entry| {
                    entry.class_name == class_name
                        && entry.method_key == method_key
                        && entry.impl_class == impl_class
                        && entry.signature == *signature
                })
                .map(|entry| entry.template.clone())
        };
        let preset = self.preset.as_ref();
        if let Some(template) = preset.and_then(|preset| find(&preset.instance_descriptors)) {
            return Some(template);
        }
        // Keyed: the entry wrapper's symbol comes from its key, the invoker is shared by content
        // and the descriptor is plain data, so a worker may build its own template.
        self.touch_cache_keyed();
        find(&self.runtime_instance_method_descriptors)
    }

    /// Stores a receiver-captured descriptor template after first emission.
    pub(super) fn cache_runtime_instance_method_descriptor(
        &mut self,
        class_name: &str,
        method_key: &str,
        impl_class: &str,
        signature: &FunctionSig,
        template: RuntimeInstanceMethodDescriptorTemplate,
    ) {
        self.touch_cache_keyed();
        self.runtime_instance_method_descriptors
            .entry(instance_descriptor_bucket(class_name, method_key, impl_class))
            .or_default()
            .push(RuntimeInstanceMethodDescriptorCacheEntry {
                class_name: class_name.to_string(),
                method_key: method_key.to_string(),
                impl_class: impl_class.to_string(),
                signature: signature.clone(),
                template,
            });
    }

    /// Returns an already-emitted descriptor invoker with the same ABI shape.
    pub(super) fn runtime_callable_invoker(
        &self,
        signature: &FunctionSig,
        captures: &[(String, PhpType, bool)],
        owns_string_return: bool,
        php_return_status: bool,
        defaults: &[Option<crate::codegen::runtime_callable_invoker::InvokerDefaultValue>],
    ) -> Option<String> {
        let fingerprint = invoker_fingerprint(signature, captures, owns_string_return, php_return_status, defaults);
        let matches = |entry: &&RuntimeCallableInvokerCacheEntry| {
            entry.fingerprint == fingerprint
                && {
                    #[cfg(test)]
                    INVOKER_KEY_COMPARISONS.with(|count| count.set(count.get() + 1));
                    entry.signature == *signature
                }
                && entry.captures == captures
                && entry.owns_string_return == owns_string_return
                && entry.php_return_status == php_return_status
                && entry.defaults == defaults
        };
        let preset = self.preset.as_ref();
        if let Some(entry) = preset.and_then(|preset| preset.invokers.iter().find(matches)) {
            return Some(entry.label.clone());
        }
        self.touch_cache_keyed();
        self.runtime_callable_invokers
            .iter()
            .find(matches)
            .map(|entry| entry.label.clone())
    }

    /// Records a descriptor invoker body for module-wide signature reuse.
    pub(super) fn cache_runtime_callable_invoker(
        &mut self,
        signature: &FunctionSig,
        captures: &[(String, PhpType, bool)],
        owns_string_return: bool,
        php_return_status: bool,
        defaults: &[Option<crate::codegen::runtime_callable_invoker::InvokerDefaultValue>],
        label: &str,
    ) {
        self.touch_cache_keyed();
        self.runtime_callable_invokers
            .push(RuntimeCallableInvokerCacheEntry {
                fingerprint: invoker_fingerprint(
                    signature, captures, owns_string_return, php_return_status, defaults,
                ),
                signature: signature.clone(),
                captures: captures.to_vec(),
                owns_string_return,
                php_return_status,
                defaults: defaults.to_vec(),
                label: label.to_string(),
            });
    }

    /// Returns the module-wide eval metadata registration helper label, if emitted.
    pub(super) fn eval_registration_helper(&self) -> Option<String> {
        if self.eval_registration_preset {
            return self.eval_registration_helper.clone();
        }
        self.touch_cache();
        self.eval_registration_helper.clone()
    }

    /// Records the helper emitted ahead of the bodies; see `emit_shared_eval_registration_helper`.
    pub(super) fn preset_eval_registration_helper(&mut self, label: String) {
        self.eval_registration_helper = Some(label);
        self.eval_registration_preset = true;
    }

    /// Captures what the helpers emitted ahead of the bodies left in the caches.
    ///
    /// Taken once, after those helpers and before any body. Everything in it names text that is
    /// already in the main emitter -- a descriptor, an invoker, a wrapper -- so a worker that
    /// starts from it reuses those symbols instead of emitting its own copies. That matters for
    /// more than speed: a callable descriptor is compared by address at run time, so the
    /// module-wide static-method table must exist exactly once, which is why a worker could
    /// never build its own and had to hand every body that reached it back to the serial pass.
    pub(super) fn module_preset(&self) -> std::sync::Arc<ModulePreset> {
        std::sync::Arc::new(ModulePreset {
            eval_registration_helper: self
                .eval_registration_preset
                .then(|| self.eval_registration_helper.clone())
                .flatten(),
            string_cases: self.runtime_string_descriptor_cases.clone(),
            static_cases: self.runtime_static_method_descriptor_cases.clone(),
            static_case_entries: self.runtime_static_method_descriptor_case_entries.clone(),
            instance_descriptors: self.runtime_instance_method_descriptors.clone(),
            invokers: self.runtime_callable_invokers.clone(),
            builtin_wrappers: self.runtime_builtin_wrappers.clone(),
            extern_wrappers: self.runtime_extern_wrappers.clone(),
            mixed_callable_sharing: self.mixed_callable_sharing.clone(),
            pcntl_async_signals: self.pcntl_async_signals,
            pcntl_signal_handlers: self.pcntl_signal_handlers,
            php_shutdown_drain: self.php_shutdown_drain,
            mixed_string_sharing: self.mixed_string_sharing,
            count_guard_sharing: self.count_guard_sharing,
            compatible_class_ids: Default::default(),
        })
    }

    /// Starts a codegen worker's chunk from `preset`; see `module_preset`.
    pub(super) fn apply_module_preset(&mut self, preset: &std::sync::Arc<ModulePreset>) {
        if let Some(label) = &preset.eval_registration_helper {
            self.preset_eval_registration_helper(label.clone());
        }
        self.mixed_callable_sharing = preset.mixed_callable_sharing.clone();
        self.pcntl_async_signals = preset.pcntl_async_signals;
        self.pcntl_signal_handlers = preset.pcntl_signal_handlers;
        self.php_shutdown_drain = preset.php_shutdown_drain;
        self.mixed_string_sharing = preset.mixed_string_sharing;
        self.count_guard_sharing = preset.count_guard_sharing;
        self.preset = Some(std::sync::Arc::clone(preset));
    }

    /// Publishes the module-wide eval metadata registration helper label.
    pub(super) fn cache_eval_registration_helper(&mut self, label: String) {
        self.touch_cache();
        debug_assert!(self.eval_registration_helper.is_none());
        self.eval_registration_helper = Some(label);
    }

    /// Returns a previously emitted synthetic builtin wrapper for the same signature.
    pub(super) fn runtime_builtin_wrapper(
        &self,
        name: &str,
        signature: &FunctionSig,
        strict_php: bool,
    ) -> Option<String> {
        if let Some(label) = self.preset.as_ref().and_then(|preset| {
            cached_runtime_call_wrapper(&preset.builtin_wrappers, name, signature, strict_php)
        }) {
            return Some(label);
        }
        self.touch_cache_keyed();
        cached_runtime_call_wrapper(
            &self.runtime_builtin_wrappers,
            name,
            signature,
            strict_php,
        )
    }

    /// Records a synthetic builtin wrapper for module-wide reuse.
    pub(super) fn cache_runtime_builtin_wrapper(
        &mut self,
        name: &str,
        signature: &FunctionSig,
        strict_php: bool,
        label: &str,
    ) {
        self.touch_cache_keyed();
        cache_runtime_call_wrapper(
            &mut self.runtime_builtin_wrappers,
            name,
            signature,
            strict_php,
            label,
        );
    }

    /// Returns a previously emitted synthetic extern wrapper for the same signature.
    pub(super) fn runtime_extern_wrapper(
        &self,
        name: &str,
        signature: &FunctionSig,
    ) -> Option<String> {
        if let Some(label) = self.preset.as_ref().and_then(|preset| {
            cached_runtime_call_wrapper(&preset.extern_wrappers, name, signature, false)
        }) {
            return Some(label);
        }
        self.touch_cache_keyed();
        cached_runtime_call_wrapper(&self.runtime_extern_wrappers, name, signature, false)
    }

    /// Records a synthetic extern wrapper for module-wide reuse.
    pub(super) fn cache_runtime_extern_wrapper(
        &mut self,
        name: &str,
        signature: &FunctionSig,
        label: &str,
    ) {
        self.touch_cache_keyed();
        cache_runtime_call_wrapper(
            &mut self.runtime_extern_wrappers,
            name,
            signature,
            false,
            label,
        );
    }
}

/// Looks up a cached synthetic call wrapper by PHP name and ABI signature.
fn cached_runtime_call_wrapper(
    entries: &[RuntimeCallWrapperCacheEntry],
    name: &str,
    signature: &FunctionSig,
    strict_php: bool,
) -> Option<String> {
    entries
        .iter()
        .find(|entry| {
            entry.name == name
                && entry.signature == *signature
                && entry.strict_php == strict_php
        })
        .map(|entry| entry.label.clone())
}

/// Adds one synthetic call wrapper to its module-wide cache.
fn cache_runtime_call_wrapper(
    entries: &mut Vec<RuntimeCallWrapperCacheEntry>,
    name: &str,
    signature: &FunctionSig,
    strict_php: bool,
    label: &str,
) {
    entries.push(RuntimeCallWrapperCacheEntry {
        name: name.to_string(),
        signature: signature.clone(),
        strict_php,
        label: label.to_string(),
    });
}


#[cfg(test)]
std::thread_local! {
    /// Full invoker-key comparisons the cache lookup made after the fingerprint matched.
    static INVOKER_KEY_COMPARISONS: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
mod invoker_fingerprint_tests {
    use super::*;
    use crate::codegen::platform::Target;

    /// `count` distinct one-parameter signatures, `$p0: int` .. `$p{count-1}: int`.
    fn signatures(count: usize) -> Vec<FunctionSig> {
        (0..count)
            .map(|index| {
                FunctionSig {
                    params: vec![(format!("p{index}"), PhpType::Int)],
                    param_type_exprs: vec![None],
                    param_attributes: vec![Vec::new()],
                    defaults: vec![None],
                    return_type: PhpType::Void,
                    declared_return: false,
                    by_ref_return: false,
                    ref_params: vec![false],
                    declared_params: vec![true],
                    variadic: None,
                    deprecation: None,
                    is_generator: false,
                }
            })
            .collect()
    }

    /// A lookup compares full keys only with the entries whose fingerprint matches, not with
    /// every cached invoker: Symfony caches thousands, and comparing each signature per lookup
    /// was the top codegen frame. It still finds the entry and still misses an absent key.
    #[test]
    fn invoker_lookup_compares_only_fingerprint_matches() {
        let module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let mut shared = SharedCodegenState::for_module(&module);
        let sigs = signatures(200);
        for (index, sig) in sigs.iter().enumerate() {
            shared.cache_runtime_callable_invoker(sig, &[], false, false, &[], &format!("_inv_{index}"));
        }
        INVOKER_KEY_COMPARISONS.with(|count| count.set(0));
        let found = shared.runtime_callable_invoker(&sigs[150], &[], false, false, &[]);
        assert_eq!(found.as_deref(), Some("_inv_150"));
        assert_eq!(shared.runtime_callable_invoker(&sigs[150], &[], true, false, &[]), None);
        let compared = INVOKER_KEY_COMPARISONS.with(|count| count.get());
        assert!(compared <= 2, "{compared} full key comparisons for two lookups among 200 entries");
    }
}

#[cfg(test)]
mod numeric_closure_identity_tests {
    use super::SharedCodegenState;
    use crate::codegen::platform::Target;
    use crate::ir::Module;

    /// One source function keeps one profiling id across multiple native entry variants.
    #[test]
    fn numeric_closure_calls_share_instrumentation_source_id() {
        let module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let mut shared = SharedCodegenState::for_module(&module);
        assert_eq!(shared.register_instr("closure".into()), 0);
        assert_eq!(shared.register_instr("other".into()), 1);
        assert_eq!(shared.register_instr("closure".into()), 0);
        assert_eq!(shared.instr_registry(), ["closure", "other"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::ir::{Function, IrType};

    /// Creates a minimal EIR method body with the requested raw name and static flag.
    fn method(name: &str, is_static: bool) -> Function {
        let mut function = Function::new(name.to_string(), IrType::Void, PhpType::Void);
        function.flags.is_static = is_static;
        function
    }

    /// Reproduces the pre-index linear membership predicate for differential assertions.
    fn old_scan_contains(
        module: &Module,
        class_name: &str,
        method_key: &str,
        is_static: bool,
    ) -> bool {
        module.class_methods.iter().any(|function| {
            function.flags.is_static == is_static
                && function
                    .name
                    .rsplit_once("::")
                    .is_some_and(|(candidate_class, candidate_method)| {
                        candidate_class == class_name
                            && php_symbol_key(candidate_method) == method_key
                    })
        })
    }

    /// A worker chunk inherits every whole-module answer: 256 chunks each walking all the bodies
    /// was 40% of the Symfony Demo's codegen. A second chunk reuses the class lists of the first.
    #[test]
    fn a_worker_chunk_recomputes_no_whole_module_fact() {
        let module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let mut serial = SharedCodegenState::for_module(&module);
        crate::codegen::context::prefill_module_facts(&module, &mut serial);
        let preset = serial.module_preset();

        let mut chunk = SharedCodegenState::for_module(&module);
        chunk.apply_module_preset(&preset);
        chunk.pcntl_async_signals(|| panic!("async signals recomputed"));
        chunk.pcntl_signal_handlers(|| panic!("signal handlers recomputed"));
        chunk.php_shutdown_drain(|| panic!("shutdown drain recomputed"));
        assert!(chunk.mixed_string_sharing(0).is_some() && chunk.mixed_string_sharing(1).is_some());
        assert!(chunk.count_guard_sharing().is_some());

        let list = ["App\\Base".to_string()];
        assert_eq!(&*chunk.compatible_class_ids_memoized(&list, || vec![7]), &[7]);
        let mut other = SharedCodegenState::for_module(&module);
        other.apply_module_preset(&preset);
        let ids = other.compatible_class_ids_memoized(&list, || panic!("class list recomputed"));
        assert_eq!(&*ids, &[7]);
    }

    /// Proves the immutable index preserves raw classes and method-only canonicalization.
    #[test]
    fn emitted_method_index_matches_previous_scan_semantics() {
        let mut module = Module::new(Target::detect_host());
        module.class_methods = vec![
            method("Ns\\Thing::DoWork", false),
            method("Ns\\Thing::StaticWork", true),
            method("Ns\\Thing::Dual", false),
            method("Ns\\Thing::Dual", true),
            method("Outer::Inner::MiXeD", false),
            method("\\Ns\\Thing::Leading", false),
            method("missing_delimiter", false),
        ];
        let state = SharedCodegenState::for_module(&module);
        let queries = [
            ("Ns\\Thing", "dowork", false),
            ("Ns\\Thing", "DOWORK", false),
            ("Ns\\Thing", "staticwork", true),
            ("Ns\\Thing", "staticwork", false),
            ("Ns\\Thing", "dual", false),
            ("Ns\\Thing", "dual", true),
            ("Outer::Inner", "mixed", false),
            ("Outer", "inner::mixed", false),
            ("\\Ns\\Thing", "leading", false),
            ("Ns\\Thing", "leading", false),
            ("", "missing_delimiter", false),
        ];

        for (class_name, method_key, is_static) in queries {
            assert_eq!(
                state.emitted_method_contains(class_name, method_key, is_static),
                old_scan_contains(&module, class_name, method_key, is_static),
                "membership mismatch for {class_name}::{method_key}, static={is_static}"
            );
        }
    }

    /// Runtime call wrappers share one module copy: a wrapper that hands it back lets the next
    /// one reuse it (with whatever was interned into it) instead of copying the module again,
    /// and a wrapper reached while another holds the copy gets a fresh one.
    #[test]
    fn wrapper_module_copy_is_reused_across_wrappers() {
        let module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let mut state = SharedCodegenState::for_module(&module);
        let mut first = state.take_wrapper_module(&module);
        first.data.intern_string("interned by the first wrapper");
        let first_address = &*first as *const Module;
        let nested = state.take_wrapper_module(&module);
        assert_ne!(&*nested as *const Module, first_address, "a nested wrapper gets its own copy");
        state.return_wrapper_module(first);
        let second = state.take_wrapper_module(&module);
        assert_eq!(&*second as *const Module, first_address, "the next wrapper reuses the copy");
        assert!(second.data.string_id("interned by the first wrapper").is_some());
    }

    /// Finished instance lookup tables and mixed method candidates are answered from memory
    /// once recorded, and neither counts as an UNKEYED touch: a codegen worker never has to
    /// hand a body using them back to the serial pass.
    #[test]
    fn lookup_table_and_candidate_memos_are_keyed() {
        let module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let state = SharedCodegenState::for_module(&module);
        let unkeyed_before = state.unkeyed_cache_touches.get();
        assert!(state.instance_lookup_table(&[1, 2]).is_none());
        let table = crate::codegen_support::callable_lookup::StringLookupTable::Linear {
            label: "_instance_table".to_string(),
            count: 2,
        };
        state.cache_instance_lookup_table(vec![1, 2], table);
        assert!(matches!(
            state.instance_lookup_table(&[1, 2]),
            Some(crate::codegen_support::callable_lookup::StringLookupTable::Linear { ref label, count: 2 })
                if label == "_instance_table"
        ));
        assert!(state.instance_lookup_table(&[2, 1]).is_none(), "order is part of the key");
        let key = (Some("App\\Scope".to_string()), "run".to_string(), 1);
        assert!(state.mixed_method_candidates(&key).is_none());
        state.cache_mixed_method_candidates(key.clone(), std::sync::Arc::new(Vec::new()));
        assert!(state.mixed_method_candidates(&key).is_some());
        assert!(state.mixed_method_candidates(&(None, "run".to_string(), 1)).is_none(), "scope is part of the key");
        assert_eq!(state.unkeyed_cache_touches.get(), unkeyed_before);
    }

    /// A worker that defers a body rolls its data back, so every memo naming a table in that
    /// data must go with it. The instance lookup tables and the inaccessible-method table used
    /// to survive the deferral, and the next body of the chunk referenced a `_data_h…` table
    /// the serial pass never defines (Symfony `--web` without preload failed to link).
    #[test]
    fn deferral_forgets_memos_naming_chunk_data() {
        let module = Module::new(Target::parse("macos-aarch64").expect("target"));
        let mut state = SharedCodegenState::for_module(&module);
        let table = |label: &str| crate::codegen_support::callable_lookup::StringLookupTable::Linear {
            label: label.to_string(),
            count: 1,
        };
        state.cache_instance_lookup_table(vec![7], table("_data_h0000000000000001"));
        state.cache_inaccessible_method_table(Some(table("_data_h0000000000000002")));
        state.cache_mixed_method_candidates(
            (None, "run".to_string(), 0),
            std::sync::Arc::new(Vec::new()),
        );
        state.forget_worker_templates();
        assert!(state.instance_lookup_table(&[7]).is_none(), "the instance table named rolled-back data");
        assert!(state.inaccessible_method_table().is_none(), "the inaccessible table named rolled-back data");
        assert!(
            state.mixed_method_candidates(&(None, "run".to_string(), 0)).is_some(),
            "a memo of pure module facts names no data and survives"
        );
    }
}

/// A cheap digest of an invoker cache key: equal keys always have equal fingerprints, so an
/// entry whose fingerprint differs is skipped without comparing signatures. Defaults, whose
/// values have no `Hash`, contribute their presence; the full comparison settles the rest.
fn invoker_fingerprint(
    signature: &FunctionSig,
    captures: &[(String, PhpType, bool)],
    owns_string_return: bool,
    php_return_status: bool,
    defaults: &[Option<crate::codegen::runtime_callable_invoker::InvokerDefaultValue>],
) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = rustc_hash::FxHasher::default();
    for (name, ty) in &signature.params {
        name.hash(&mut hasher);
        ty.hash(&mut hasher);
    }
    signature.return_type.hash(&mut hasher);
    signature.ref_params.hash(&mut hasher);
    signature.declared_params.hash(&mut hasher);
    signature.variadic.hash(&mut hasher);
    (signature.by_ref_return, signature.declared_return, signature.defaults.len()).hash(&mut hasher);
    for (name, ty, by_ref) in captures {
        name.hash(&mut hasher);
        ty.hash(&mut hasher);
        by_ref.hash(&mut hasher);
    }
    (owns_string_return, php_return_status).hash(&mut hasher);
    for default in defaults {
        default.is_some().hash(&mut hasher);
    }
    hasher.finish()
}
