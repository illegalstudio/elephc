//! Purpose:
//! Holds per-function AST-to-EIR lowering state: builder cursor, local slots,
//! local type facts, data interning, and active loop targets.
//!
//! Called from:
//! - `crate::ir_lower::function`, `crate::ir_lower::stmt`, and `crate::ir_lower::expr`.
//!
//! Key details:
//! - PHP locals remain addressable slots in this initial lowering pass. SSA
//!   values represent loads, stores, and operation results around those slots.
//! - Control-flow joins can reload locals from slots, so Phase 03 does not need
//!   to synthesize block-parameter phis for every PHP variable yet.

mod string_append;

use std::cell::Cell;
use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ir::{
    BlockId, Builder, DataId, DataPool, Effects, Function, Immediate, IrType, LocalKind,
    LocalSlotId, Op, Ownership, ValueId,
};
use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, TypeExpr};
use crate::span::Span;
use crate::types::{
    array_storage_conversion, join_array_storage_conversion, ClassInfo, EnumInfo,
    ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo, PhpType,
    ReturnAliasSummaries, ReturnArgAlias, ThrowAccessInfo, TypeEnv,
};

/// Value returned by expression lowering with its PHP metadata.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoweredValue {
    pub value: ValueId,
    pub ir_type: IrType,
}

/// Loop-control target pair for `break` and `continue`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoopFrame {
    pub break_block: BlockId,
    pub continue_block: BlockId,
    pub cleanup: Option<LoopCleanup>,
    /// Published owner for a foreach source that can run user code while the
    /// iterator is initialized. It protects temporary sources through catches.
    pub source_owner: Option<(LocalSlotId, Span)>,
    /// Lifetime reference a by-reference `foreach` took on a borrowed element or property source,
    /// so the loop keeps iterating live storage even if the body drops the parent that owned it
    /// (issues #580 and #642). Released on every exit that skips the loop's own exit block,
    /// exactly like `cleanup`.
    pub source_pin: Option<LoopCleanup>,
    /// Mixed slot owning a `getIterator()` result produced inside `Op::IterStart`.
    ///
    /// Published before `IterStart` and retired with `PopCallOperandOwner` then
    /// `ReleaseLocalSlot` on every exit that skips the loop's own exit block.
    /// Innermost `break` and `continue` must not retire it here: they keep using
    /// the iterator, and the exit block owns the normal-completion retire.
    pub iterator_owner: Option<(LocalSlotId, Span)>,
    /// Addressable iterator state whose owned relocation anchors need `IterEnd` cleanup.
    pub iterator_cleanup: Option<(LocalSlotId, Span)>,
    /// Reference a by-reference `foreach` holds on the OBJECT whose property slot owns the
    /// borrowed container, when that object is a temporary the loop reached through -- an array
    /// element, a call result (issue #690). Released on every exit, exactly like `source_pin`.
    pub receiver_pin: Option<LoopCleanup>,
}

/// Cleanup that must run when control leaves a loop without visiting its exit block.
#[derive(Debug, Clone, Copy)]
pub(crate) struct LoopCleanup {
    pub value: LoweredValue,
    pub span: Span,
}

/// Active `finally` body that must run before selected control-flow exits.
#[derive(Debug, Clone)]
pub(crate) struct FinallyFrame {
    pub body: Vec<Stmt>,
    pub run_on_throw: bool,
    pub handler_cleanup: Option<(i64, Span)>,
    /// Loop-stack length when the frame was pushed, so the loops enclosing the protected region
    /// have indices `0..loop_depth` and the ones opened inside it start at `loop_depth`. A
    /// `break`/`continue` whose target loop INDEX is strictly below `loop_depth` leaves the
    /// region and must run the frame; one whose target index is `loop_depth` or more stays
    /// inside and must not (`run_innermost_finally` skips when `loop_depth <= target`).
    pub loop_depth: usize,
}

/// Active `try` handler that must be unlinked when control leaves its protected body.
#[derive(Debug, Clone, Copy)]
pub(crate) struct TryHandlerFrame {
    pub handler_token: i64,
    pub span: Span,
    /// Number of surrounding loop targets when the protected body began.
    pub loop_depth: usize,
}

/// Compile-time callable target tracked for straight-line local FCC calls.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum StaticCallableBinding {
    UserFunction(String),
    ExternFunction(String),
    Builtin(String),
    Closure {
        name: String,
        signature: FunctionSig,
        captures: Vec<ClosureCapture>,
        return_alias: ReturnArgAlias,
        /// Requires scalar/nonthrowing call gates for newly preserved loop facts.
        loop_guarded: bool,
        /// Allows direct calls only at spans admitted by the numeric loop proof.
        numeric_loop_guarded: bool,
    },
    StaticMethod {
        receiver: StaticReceiver,
        method: String,
    },
    StaticMethodDescriptor {
        receiver: StaticReceiver,
        method: String,
    },
    InstanceMethod {
        object: Box<Expr>,
        method: String,
        signature: FunctionSig,
        direct_call: bool,
    },
}

/// Captured closure value recorded at closure creation time for static calls.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ClosureCapture {
    pub value: ValueId,
    /// Caller local whose reference cell this capture exposes to the closure body.
    pub by_ref_local: Option<String>,
}

/// Records one temporary owner published while a call argument list is evaluated.
#[derive(Debug, Clone)]
pub(crate) struct CallArgumentEvaluationOwner {
    pub value: Option<ValueId>,
    pub borrow: Option<ValueId>,
    pub temp_name: String,
    pub slot: LocalSlotId,
    pub span: Span,
    /// Payload type when this slot owns a managed reference cell instead of a value.
    pub ref_cell_payload: Option<PhpType>,
    /// Whether this is an intermediate receiver alias created for one nested ref argument.
    pub scoped_receiver_alias: bool,
    /// Whether the pinned source was a borrow the expression did not own (a loaded parameter
    /// or local): a user call then keeps borrowing it and the pin retires after the call.
    pub pins_borrowed_source: bool,
}

/// Keeps one call's evaluation owners distinct from nested expression lowering.
#[derive(Debug, Clone)]
pub(crate) struct CallArgumentEvaluationScope {
    pub expression_depth: usize,
    pub owners: Vec<CallArgumentEvaluationOwner>,
}

/// Staging for one `$target = &call()`, recorded while its source expression is lowered.
///
/// The hidden local and its ref-cell owner slot are declared, and that owner is published in
/// the unwind chain, BEFORE the source expression runs. A selected by-reference lowering then
/// adopts the transferred cell straight into this staging, so the lease is covered by a live
/// cleanup record from before the arguments were evaluated until the alias is bound.
#[derive(Debug, Clone)]
pub(crate) struct ReferenceCallContext {
    /// Expression depth of the call that may transfer a cell, one level inside the assignment.
    pub(crate) depth: usize,
    /// Hidden local that adopts the transferred cell.
    pub(crate) staged: String,
    /// Whether a selected lowering actually adopted a cell into `staged`.
    pub(crate) adopted: bool,
}

/// A local bound to a pointer by-reference parameter, with the parameter's declared type.
struct PointerReferenceBinding {
    slot: LocalSlotId,
    name: String,
    span: Span,
    param_ty: PhpType,
}

/// The program's global constants as one body sees them: the prescanned table, borrowed, plus
/// the constants this body's own `define()` calls registered.
///
/// Every body used to start from a CLONE of the whole table so that it could add to it, and every
/// speculative snapshot cloned it again; on the Symfony build that copying was about 5% of
/// lowering. A `define()` never replaces a prescanned constant (`register` keeps the first
/// value), so the two maps never share a key and a lookup may try them in either order.
#[derive(Clone)]
pub(crate) struct ConstantScope<'m> {
    base: &'m HashMap<String, (ExprKind, PhpType)>,
    added: HashMap<String, (ExprKind, PhpType)>,
}

impl<'m> ConstantScope<'m> {
    fn new(base: &'m HashMap<String, (ExprKind, PhpType)>) -> Self {
        Self {
            base,
            added: HashMap::new(),
        }
    }

    fn get(&self, name: &str) -> Option<&(ExprKind, PhpType)> {
        self.base.get(name).or_else(|| self.added.get(name))
    }

    fn register(&mut self, name: String, value: (ExprKind, PhpType)) {
        if !self.base.contains_key(&name) {
            self.added.entry(name).or_insert(value);
        }
    }

    /// The table a nested body starts from: this one's base when nothing was added, else a
    /// merged copy for the caller to lend.
    pub(crate) fn nested(&self) -> Option<HashMap<String, (ExprKind, PhpType)>> {
        (!self.added.is_empty()).then(|| {
            let mut merged = self.base.clone();
            merged.extend(self.added.iter().map(|(name, value)| (name.clone(), value.clone())));
            merged
        })
    }

    pub(crate) fn base(&self) -> &'m HashMap<String, (ExprKind, PhpType)> {
        self.base
    }
}

/// Rollback point for a speculative statement lowering.
#[derive(Clone)]
pub(crate) struct LoweringSnapshot {
    function: Function,
    insertion_block: Option<BlockId>,
    data: DataPoolLengths,
    local_slots: HashMap<String, LocalSlotId>,
    local_kinds: HashMap<String, LocalKind>,
    local_types: TypeEnv,
    initialized_slots: HashSet<LocalSlotId>,
    /// Only this body's `define()` additions; the prescanned table is borrowed and never changes.
    constants: HashMap<String, (ExprKind, PhpType)>,
    loop_stack: Vec<LoopFrame>,
    loop_carried_missing_strings: Vec<HashSet<String>>,
    finally_stack: Vec<FinallyFrame>,
    try_handler_stack: Vec<TryHandlerFrame>,
    try_loop_depths: Vec<usize>,
    static_callable_locals: HashMap<String, StaticCallableBinding>,
    static_callable_local_epochs: HashMap<String, u64>,
    reflection_class_locals: HashMap<String, String>,
    reflection_function_locals: HashMap<String, String>,
    reflection_property_locals: HashMap<String, (String, String)>,
    reflection_method_locals: HashMap<String, (String, String)>,
    reflection_arg_array_locals: HashMap<String, Vec<Expr>>,
    fiber_start_sigs: HashMap<String, FunctionSig>,
    ref_bound_locals: HashSet<String>,
    interior_ref_locals: HashMap<String, PhpType>,
    capture_bound_locals: HashSet<String>,
    pending_ref_bridges: Vec<(crate::ir::ValueId, String, String)>,
    borrowed_element_ref_locals: HashSet<String>,
    ref_cell_owner_locals: HashMap<String, LocalSlotId>,
    foreach_int_key_locals: HashSet<String>,
    storage_widened_locals: HashSet<String>,
    array_conversions: HashMap<String, PhpType>,
    speculating: bool,
    php_strict_types: bool,
    closure_count: usize,
    bound_closure_this_class: Option<String>,
    bound_closure_this_gradual: bool,
    expression_depth: usize,
    reference_call_context: Option<ReferenceCallContext>,
    refusals: usize,
    call_argument_evaluation_scopes: Vec<CallArgumentEvaluationScope>,
    borrowed_string_call_operands: HashSet<ValueId>,
    borrowed_call_results: HashSet<ValueId>,
    /// Provisional offset receiver reads belonging to this speculative function state.
    string_offset_receivers: Vec<(Span, ValueId)>,
    borrowed_string_calls_enabled: bool,
    /// Replay set of locals whose string cast must copy. See `owned_string_cast_locals`.
    owned_string_cast_locals: HashSet<String>,
    /// Casts that borrowed a concrete string local inside this speculative state.
    provisional_borrowed_string_casts: HashSet<String>,
    append_update_cell_aliases: HashSet<String>,
    pending_static_callable_result: Option<StaticCallableBinding>,
    closure_counter: usize,
    hidden_temp_counter: usize,
    /// Maps captured call values to their active exception-cleanup tokens.
    argument_guards: HashMap<ValueId, ValueId>,
    /// Pending call scopes order capture cleanup by PHP parameter position.
    argument_guard_scopes: Vec<Vec<(usize, ValueId)>>,
    eval_barrier_active: bool,
    eval_executed: bool,
    consuming_string_append_enabled: bool,
    eval_scope_read_param: Option<String>,
    eval_scope_read_names: HashSet<String>,
    eval_scope_write_names: HashSet<String>,
    eval_scope_flush_names: BTreeSet<String>,
    numeric_closure_sites: std::collections::HashSet<Span>,
    numeric_tracked_locals: std::collections::HashSet<String>,
    final_mixed_locals: std::collections::HashMap<String, std::collections::HashSet<usize>>,
    integer_local_declarations: HashSet<String>,
    conditional_local_declarations: HashSet<String>,
    numeric_proof_sealed: bool,
    pointer_reference_bindings: usize,
}

/// Lengths of append-only intern pools captured for speculative rollback.
#[derive(Clone)]
struct DataPoolLengths {
    strings: usize,
    float_literals: usize,
    global_names: usize,
    function_names: usize,
    class_names: usize,
    method_names: usize,
    property_names: usize,
}

impl DataPoolLengths {
    /// Captures the current length of every intern pool.
    fn capture(data: &DataPool) -> Self {
        Self {
            strings: data.strings.len(),
            float_literals: data.float_literals.len(),
            global_names: data.global_names.len(),
            function_names: data.function_names.len(),
            class_names: data.class_names.len(),
            method_names: data.method_names.len(),
            property_names: data.property_names.len(),
        }
    }

    /// Removes entries interned after the snapshot.
    fn truncate(&self, data: &mut DataPool) {
        data.strings.truncate(self.strings);
        data.float_literals.truncate(self.float_literals);
        data.global_names.truncate(self.global_names);
        data.function_names.truncate(self.function_names);
        data.class_names.truncate(self.class_names);
        data.method_names.truncate(self.method_names);
        data.property_names.truncate(self.property_names);
    }
}

const EVAL_CONTEXT_LOCAL_NAME: &str = "__eir_eval_context";
const EVAL_SCOPE_LOCAL_NAME: &str = "__eir_eval_scope";
const EVAL_GLOBAL_SCOPE_LOCAL_NAME: &str = "__eir_eval_global_scope";
const EVAL_ARGC_LOCAL_NAME: &str = "argc";
const EVAL_ARGV_LOCAL_NAME: &str = "argv";

thread_local! {
    static RUNTIME_DYNAMIC_FUNCTION_RESOLUTION: Cell<bool> = const { Cell::new(false) };
    /// Whether the program being lowered can load PHP at run time: an include the resolver could
    /// not expand, or an `eval()`.
    static PROGRAM_LOADS_CODE_AT_RUN_TIME: Cell<bool> = const { Cell::new(false) };
    /// `CheckResult::ref_param_escaping_callees` for the program being lowered.
    static REF_PARAM_ESCAPING_CALLEES: std::cell::RefCell<HashSet<String>> =
        std::cell::RefCell::new(HashSet::new());
}

/// Restores the enclosing lowering invocation's escaping-callee set on drop.
pub(crate) struct RefParamEscapingCalleesGuard {
    previous: HashSet<String>,
}

impl Drop for RefParamEscapingCalleesGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        REF_PARAM_ESCAPING_CALLEES.with(|set| *set.borrow_mut() = previous);
    }
}

/// Publishes, for one program-lowering invocation, the callees whose by-reference parameter a
/// closure may keep past the call (thread-local for the same reason as the dynamic-resolution
/// setting above).
pub(crate) fn set_ref_param_escaping_callees(
    callees: HashSet<String>,
) -> RefParamEscapingCalleesGuard {
    let previous = REF_PARAM_ESCAPING_CALLEES.with(|set| std::mem::replace(&mut *set.borrow_mut(), callees));
    RefParamEscapingCalleesGuard { previous }
}

thread_local! {
    /// `CheckResult::runtime_define_names` for the program being lowered.
    static RUNTIME_DEFINE_NAMES: std::cell::RefCell<HashSet<String>> =
        std::cell::RefCell::new(HashSet::new());
}

/// Restores the enclosing lowering invocation's define-name set on drop.
pub(crate) struct RuntimeDefineNamesGuard {
    previous: HashSet<String>,
}

impl Drop for RuntimeDefineNamesGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        RUNTIME_DEFINE_NAMES.with(|set| *set.borrow_mut() = previous);
    }
}

/// Publishes, for one program-lowering invocation, the names some checked `define()` creates.
pub(crate) fn set_runtime_define_names(names: HashSet<String>) -> RuntimeDefineNamesGuard {
    let previous = RUNTIME_DEFINE_NAMES.with(|set| std::mem::replace(&mut *set.borrow_mut(), names));
    RuntimeDefineNamesGuard { previous }
}

/// Whether a checked `define()` creates `name` (given without a leading `\`).
pub(crate) fn is_runtime_define_name(name: &str) -> bool {
    RUNTIME_DEFINE_NAMES.with(|set| set.borrow().contains(name))
}

thread_local! {
    /// `CheckResult::property_bound_promotion_sites` for the program being lowered.
    static PROPERTY_BOUND_PROMOTION_SITES: std::cell::RefCell<HashSet<(String, Span)>> =
        std::cell::RefCell::new(HashSet::new());
}

/// Restores the enclosing lowering invocation's property-bound promotion sites on drop.
pub(crate) struct PropertyBoundPromotionSitesGuard {
    previous: HashSet<(String, Span)>,
}

impl Drop for PropertyBoundPromotionSitesGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        PROPERTY_BOUND_PROMOTION_SITES.with(|set| *set.borrow_mut() = previous);
    }
}

/// Publishes, for one program-lowering invocation, the call-site locals the checker found passed
/// to a property-bound by-reference parameter (thread-local for the same reason as the
/// escaping-callee set).
pub(crate) fn set_property_bound_promotion_sites(
    sites: HashSet<(String, Span)>,
) -> PropertyBoundPromotionSitesGuard {
    let previous =
        PROPERTY_BOUND_PROMOTION_SITES.with(|set| std::mem::replace(&mut *set.borrow_mut(), sites));
    PropertyBoundPromotionSitesGuard { previous }
}

thread_local! {
    /// `CheckResult::by_ref_call_result_sites` for the program being lowered.
    static BY_REF_CALL_RESULT_SITES: std::cell::RefCell<HashSet<Span>> =
        std::cell::RefCell::new(HashSet::new());
}

/// Restores the enclosing lowering invocation's by-reference call-result sites on drop.
pub(crate) struct ByRefCallResultSitesGuard {
    previous: HashSet<Span>,
}

impl Drop for ByRefCallResultSitesGuard {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        BY_REF_CALL_RESULT_SITES.with(|set| *set.borrow_mut() = previous);
    }
}

/// Publishes, for one program-lowering invocation, the call results the checker found passed to
/// a by-reference parameter.
pub(crate) fn set_by_ref_call_result_sites(sites: HashSet<Span>) -> ByRefCallResultSitesGuard {
    let previous =
        BY_REF_CALL_RESULT_SITES.with(|set| std::mem::replace(&mut *set.borrow_mut(), sites));
    ByRefCallResultSitesGuard { previous }
}

/// Whether the expression at `span` is a call result bound to a by-reference parameter.
pub(crate) fn is_by_ref_call_result_site(span: Span) -> bool {
    BY_REF_CALL_RESULT_SITES.with(|set| set.borrow().contains(&span))
}

/// Whether a closure may keep a by-reference parameter of the function or method named `callee`
/// past the call (matched on the bare lowercase name, like the backend's lifetime choice).
pub(crate) fn ref_param_may_escape(callee: &str) -> bool {
    let bare = callee.rsplit_once("::").map_or(callee, |(_, method)| method);
    let key = crate::names::php_symbol_key(bare);
    REF_PARAM_ESCAPING_CALLEES.with(|set| set.borrow().contains(&key))
}

/// Scoped lowering setting for programs that preserve runtime include/eval execution.
///
/// A checked program can contain an AOT method whose function call is declared only after a
/// dynamic include runs in another frame. The setting is thread-local because lowering runs in
/// parallel test threads; its guard restores the previous nested-lowering value on drop.
pub(crate) struct RuntimeDynamicFunctionResolutionGuard {
    previous: bool,
}

impl Drop for RuntimeDynamicFunctionResolutionGuard {
    /// Restores the enclosing lowering invocation's dynamic-function policy.
    fn drop(&mut self) {
        RUNTIME_DYNAMIC_FUNCTION_RESOLUTION.with(|enabled| enabled.set(self.previous));
    }
}

/// Restores the enclosing lowering invocation's run-time code loading answer on drop.
pub(crate) struct ProgramLoadsCodeGuard {
    previous: bool,
}

impl Drop for ProgramLoadsCodeGuard {
    fn drop(&mut self) {
        PROGRAM_LOADS_CODE_AT_RUN_TIME.with(|loads| loads.set(self.previous));
    }
}

/// Records, for one complete program-lowering invocation, whether the program can load PHP at
/// run time; see [`LoweringContext::program_loads_code_at_run_time`].
pub(crate) fn set_program_loads_code_at_run_time(loads: bool) -> ProgramLoadsCodeGuard {
    let previous = PROGRAM_LOADS_CODE_AT_RUN_TIME.with(|current| current.replace(loads));
    ProgramLoadsCodeGuard { previous }
}

/// Enables dynamic function-owner fallback for one complete program-lowering invocation.
pub(crate) fn enable_runtime_dynamic_function_resolution(
    enabled: bool,
) -> RuntimeDynamicFunctionResolutionGuard {
    let previous = RUNTIME_DYNAMIC_FUNCTION_RESOLUTION.with(|current| {
        let previous = current.get();
        current.set(enabled);
        previous
    });
    RuntimeDynamicFunctionResolutionGuard { previous }
}

/// Mutable state for one function body while it is lowered.
pub(crate) struct LoweringContext<'m, 'f> {
    pub builder: Builder<'f>,
    pub data: &'m mut DataPool,
    pub source_catalog: Option<std::sync::Arc<crate::ir::SourceCatalog>>,
    pub runtime_bound_functions: std::sync::Arc<std::collections::HashSet<String>>,
    /// True when the body being lowered calls `eval()` anywhere, before or after the current
    /// point. Body-scoped, unlike `eval_barrier_active`, which only turns on once an eval was
    /// lowered: an `unset` ABOVE the eval must already leave the slot in the shape the fragment
    /// reads by name (see `store_unset_local`).
    pub body_contains_eval: bool,
    pub local_slots: HashMap<String, LocalSlotId>,
    pub local_kinds: HashMap<String, LocalKind>,
    pub local_types: TypeEnv,
    /// Number of synthetic include frames this function body has emitted so far; each
    /// `IncludeOnceGuard` body gets its own 48-byte activation slot.
    pub include_frame_slots: u32,
    /// Nested expression depth separates a reference-assignment call from its argument calls.
    pub(crate) expression_depth: usize,
    /// Printed callable description (`Class::method`) for argument `TypeError` messages.
    pub(crate) call_description: Option<String>,
    /// Staging published by the enclosing reference assignment, if one is being lowered.
    ///
    /// `finish_reference_return_call` adopts the transferred lease into that staging immediately
    /// after the call, before any caller-side argument or receiver cleanup can throw, and marks
    /// it adopted. The enclosing reference assignment then transfers out of it. Staging that was
    /// never adopted means no selected lowering produced a transferable cell, which is a refused
    /// reference assignment.
    pub(crate) reference_call_context: Option<ReferenceCallContext>,
    /// Nested owner ledgers for source-order call argument evaluation.
    pub(crate) call_argument_evaluation_scopes: Vec<CallArgumentEvaluationScope>,
    /// String subjects borrowed through read-only builtin calls whose result must be detached.
    pub(crate) borrowed_string_call_operands: HashSet<ValueId>,
    /// User call results that hand back an argument the caller only lent; see
    /// `value_is_borrowed_user_call_result`.
    pub(crate) borrowed_call_results: HashSet<ValueId>,
    pub(crate) borrowed_string_calls_enabled: bool,
    /// PHP locals whose `(string)` cast must copy: the finished frame slot frees the load.
    ///
    /// Filled after the first body lower and applied on the string-storage replay.
    /// A concrete string slot stays off this set, so the cast keeps borrowing it.
    pub(crate) owned_string_cast_locals: HashSet<String>,
    /// Locals a `(string)` cast borrowed because the slot was still a concrete string.
    pub(crate) provisional_borrowed_string_casts: HashSet<String>,
    /// Whole-body proof that an instance method cannot widen its immutable this slot.
    /// Set before body lowering and preserved across speculative snapshots.
    pub(crate) this_storage_stays_concrete: bool,
    /// Raw offset receiver reads checked against final frame storage before replay.
    pub(crate) string_offset_receivers: Vec<(Span, ValueId)>,
    /// Offset sites whose final local storage requires an independent, guarded receiver.
    /// This sealed replay proof survives speculative rollback, unlike provisional value IDs.
    pub(crate) owned_string_offset_sites: HashSet<Span>,
    pub(crate) append_update_cell_aliases: HashSet<String>,
    initialized_slots: HashSet<LocalSlotId>,
    /// Slots that have received a string still living in the shared concat scratch buffer.
    ///
    /// Kept as a MAY set that only ever grows: a slot that once held scratch-backed bytes is
    /// treated as scratch-backed from then on. Escape points (`return`) consult it, because a
    /// LOAD of such a slot has `Op::LoadLocal` as its defining op and would otherwise look
    /// persistent to every check keyed on the defining opcode alone.
    scratch_backed_slots: HashSet<LocalSlotId>,
    pub functions: &'m HashMap<String, FunctionSig>,
    pub extern_functions: &'m HashMap<String, ExternFunctionSig>,
    pub extern_globals: &'m HashMap<String, PhpType>,
    pub callable_param_sigs: &'m HashMap<(String, String), FunctionSig>,
    pub(crate) return_alias_summaries: &'m ReturnAliasSummaries,
    pub(crate) fiber_return_sigs: &'m HashMap<String, FunctionSig>,
    pub classes: &'m crate::fast_hash::FastMap<String, ClassInfo>,
    /// `names_an_operator_class` over `classes`, answered once per context.
    operator_classes: std::cell::OnceCell<bool>,
    pub enums: &'m HashMap<String, EnumInfo>,
    pub interfaces: &'m HashMap<String, InterfaceInfo>,
    pub declared_trait_names: &'m [String],
    pub declared_trait_methods:
        &'m HashMap<String, HashMap<String, crate::ir::TraitMethodInfo>>,
    pub declared_trait_properties: &'m HashMap<String, Vec<crate::parser::ast::ClassProperty>>,
    pub packed_classes: &'m HashMap<String, PackedClassInfo>,
    /// C struct layouts of the declared extern classes, read by `ptr<Class>` field accesses.
    pub extern_classes: &'m HashMap<String, ExternClassInfo>,
    /// Statically-decided access violations lowered to runtime `Error` throws, keyed by
    /// function-like scope and the source span of the offending call/assignment.
    pub throw_access_sites: &'m HashMap<(String, Span), ThrowAccessInfo>,
    /// Authoritative checker result types for builtin calls, scoped so equal source coordinates
    /// in separate loaded files cannot collide.
    pub builtin_call_types: &'m HashMap<(String, Span), PhpType>,
    pub closure_generator_spans: &'m HashSet<Span>,
    /// Checked property classes and write refusals before opaque calls erase flow facts.
    pub array_push_property_plans: &'m HashMap<(String, Span), crate::types::ArrayPushPropertyPlan>,
    /// Checked receiver views at property reads and method calls, scoped to the current body.
    pub member_receiver_types: &'m HashMap<(String, Span), Option<PhpType>>,
    /// Checker result types for property reads through flow-narrowed receivers.
    pub flow_typed_property_accesses: &'m HashMap<(String, Span), PhpType>,
    /// Checker-authorized argument sites that may widen an ordinary local to boxed Mixed.
    pub boxed_reference_promotion_sites: &'m HashMap<(String, Span), HashSet<String>>,
    pub first_class_builtin_call_types: &'m HashMap<Span, PhpType>,
    /// Which monomorphic instantiation each generic call site resolved to, keyed by the
    /// enclosing body's loop-storage scope and the call position.
    ///
    /// Carried from the checker rather than re-derived: choosing the instantiation needs the
    /// argument TYPES, and `lower_args_with_signature` needs the callee signature before it
    /// lowers the arguments, so lowering cannot ask the question itself.
    pub generic_call_sites: &'m HashMap<(String, Span), String>,
    /// Checker-computed fixed-point storage contracts for loop-carried array locals.
    pub loop_storage_types: &'m crate::types::LoopStorageTypes,
    /// Checker-recorded `(scope, local)` pairs for `string` locals used as a `++`/`--`
    /// target. Those locals get boxed `Mixed` frame storage from their first store, so
    /// every read of the slot is already a boxed load instead of an owned string detach.
    pub string_incdec_locals: &'m HashSet<(String, String)>,
    /// Checker-selected boxed storage contracts for caller locals whose by-reference
    /// parameter may replace their value with any member of a Mixed-represented type.
    pub by_ref_local_storage_types: &'m HashMap<(String, String), PhpType>,
    pub dynamic_ref_local_types: &'m HashMap<(String, String), PhpType>,
    /// Spans of the `unset()` ARGUMENTS whose local binding the CHECKER decided to kill
    /// (`CheckResult::local_bind_kill_sites`), each mapped to the SET of locals killed at that
    /// position. At one of these spans `unset_local` abandons the frame slot after releasing its
    /// value, so the next `declare_local` mints a fresh one. The checker is the single decision
    /// point: nothing here re-derives eligibility. The value is a set because a `Span` names no
    /// file, so two different killed locals can share one position.
    pub bind_kill_sites: &'m HashMap<Span, HashSet<String>>,
    /// Checker-authorized reference detaches that preserve already lowered capture storage.
    pub ref_detach_sites: &'m HashMap<Span, HashSet<String>>,
    /// Spans of statement-form assignments the CHECKER re-bound to a fresh binding of an
    /// incompatible type (`CheckResult::local_retype_sites`), each mapped to the SET of locals
    /// re-bound at that position. Carried alongside `bind_kill_sites` so both travel together;
    /// consumed by the retype lowering.
    pub retype_sites: &'m HashMap<Span, HashSet<String>>,
    /// Spans of the statement-form assignments to a local the CHECKER marked as branch-divergently
    /// assigned (`CheckResult::mixed_storage_store_sites`), each mapped to the SET of locals boxed
    /// at that position. At one of these spans `lower_assign` declares the slot `PhpType::Mixed`
    /// BEFORE the store and stores at `Mixed`, so both dynamic outcomes live in one boxed slot.
    /// The third of the checker's local-binding decision maps, keyed and consulted exactly like the
    /// other two — the value is a set because a `Span` names no file, so two different marked
    /// locals can share one position.
    pub mixed_storage_store_sites: &'m HashMap<Span, HashSet<String>>,
    /// Function-like scope key paired with loop spans for storage-contract lookup.
    pub loop_storage_scope: String,
    /// Scope key a closure's signature is being derived under, when that differs from this
    /// (the enclosing) body's own. The checker records builtin result types under the closure's
    /// scope, and the closure's return type is inferred from the PARENT context.
    pub(crate) closure_signature_scope: Option<String>,
    /// Statement spans whose boxed-container offset write is being lowered as the array arm of the
    /// run-time tag dispatch, so the statement does not dispatch on the tag a second time.
    pub offset_dispatch_bypass: HashSet<Span>,
    /// `static::method()` calls whose late-static dispatch ladder is already built: the copy that
    /// ends the ladder (the `__callStatic` fallback) must lower as an ordinary call. One-shot.
    pub late_static_ladder_fallbacks: HashSet<Span>,
    /// `->bindTo()` calls already split on the receiver being a closure at run time.
    pub closure_bind_splits: HashSet<Span>,
    /// Class whose implementation the NEXT instance `MethodCall` must run without virtual
    /// dispatch (`ReflectionMethod::invoke()` binds the reflected declaring class's method, not
    /// the receiver's override). Set and cleared around one call by the reflected-invoke lowering;
    /// `lower_method_call_with_receiver` takes it before it lowers any argument.
    pub direct_method_call_class: Option<String>,
    pub constants: ConstantScope<'m>,
    pub top_level_env: TypeEnv,
    pub current_class: Option<String>,
    pub loop_stack: Vec<LoopFrame>,
    /// Locals each enclosing loop assigns a value that may still be the missing-string
    /// sentinel. A cast inside the loop must see those stores even when they are lowered
    /// later in the body, because the back-edge runs them before the next iteration.
    /// A `string` parameter is absent unless such a store actually reaches it.
    pub loop_carried_missing_strings: Vec<HashSet<String>>,
    pub finally_stack: Vec<FinallyFrame>,
    pub try_handler_stack: Vec<TryHandlerFrame>,
    /// Loop-stack depth at each active `try` handler push; see
    /// `LoweringContext::loops_a_throw_would_leave`.
    pub try_loop_depths: Vec<usize>,
    /// Exceptions a finalizer took from the in-flight cell and has not rethrown yet: the hidden
    /// temp holding each, and the loop-stack and finally-stack depths its `finally` copy began
    /// at. A jump that leaves the copy DISCARDS the exception, so `control_exit` releases it
    /// there — see `stmt::exceptions::lower_catch_dispatch_with_finally`.
    pub taken_finally_exceptions: Vec<(String, usize, usize)>,
    /// Loop-stack indices whose cleanup the CURRENT exit path already emitted, so no later step
    /// of the same path — an enclosing `finally` lowered inline, with its own exit — emits it
    /// again. Loops inside a `finally` copy are cleaned before its exception is discarded; see
    /// `stmt::control_exit::release_taken_exceptions`, the only writer.
    pub exit_cleaned_loops: Vec<usize>,
    static_callable_locals: HashMap<String, StaticCallableBinding>,
    /// Per-local mutation generations used to distinguish a control-flow fact clear from an
    /// actual reassignment while lowering a try/catch region.
    static_callable_local_epochs: HashMap<String, u64>,
    /// Program-global names directly reachable from each statically declared function.
    function_global_names: HashMap<String, Option<HashSet<String>>>,
    reflection_class_locals: HashMap<String, String>,
    reflection_function_locals: HashMap<String, String>,
    reflection_property_locals: HashMap<String, (String, String)>,
    reflection_method_locals: HashMap<String, (String, String)>,
    reflection_arg_array_locals: HashMap<String, Vec<Expr>>,
    fiber_start_sigs: HashMap<String, FunctionSig>,
    ref_bound_locals: HashSet<String>,
    interior_ref_locals: HashMap<String, PhpType>,
    capture_bound_locals: HashSet<String>,
    pending_ref_bridges: Vec<(crate::ir::ValueId, String, String)>,
    /// Reference-bound locals whose cell address points INSIDE another allocation, currently
    /// an indexed-array element slot. `__rt_reference_cell_owner` deliberately answers zero for
    /// such an address, so the alias can never transfer an owner and must not escape the frame
    /// that keeps the container alive.
    borrowed_element_ref_locals: HashSet<String>,
    ref_cell_owner_locals: HashMap<String, LocalSlotId>,
    /// foreach loop-key locals whose source is a concretely-indexed array
    /// (`Array` of a non-Mixed element type), so the runtime key is always an
    /// integer even though `Op::IterCurrentKey` lowers it as Mixed. Used by
    /// `lower_array_assign` to avoid promoting a `$dst[$key] = ...` write to the
    /// hash path (and coercing the key to int) for these int-valued keys, while
    /// still promoting for keys that may be strings (generic `Array(Mixed)`,
    /// `AssocArray`, `Mixed`, `Union` sources).
    foreach_int_key_locals: HashSet<String>,
    /// Locals whose frame slot a store WIDENED to a boxed representation, so the slot holds
    /// several runtime shapes for the REST of the frame and every later read of the name has to
    /// be a boxed read. Filled by `store_local_impl`; see the comment at the widening there.
    storage_widened_locals: HashSet<String>,
    /// Joined storage representation conversions observed while lowering this function.
    array_conversions: HashMap<String, PhpType>,
    /// Whether the current statement lowering is a disposable discovery pass.
    speculating: bool,
    /// Whether the closed-world `class_exists()` retry rewrite is lowering its own native probe.
    autoload_hook_active: bool,
    /// PHP scalar-parameter coercion mode at the current call site.
    pub(crate) php_strict_types: bool,
    pub return_type: IrType,
    pub return_php_type: PhpType,
    /// `true` when the function SOURCE declares a return type, as opposed to one inferred
    /// from the body. Only a declared boundary carries PHP's coercive-mode verification
    /// (TypeError on a non-coercible runtime value); an inferred return must never throw.
    pub return_type_is_declared: bool,
    /// `true` when the function/closure being lowered returns by reference (`function &f()`),
    /// so a `return $obj->prop` yields the property's ref-cell pointer instead of a value copy.
    pub by_ref_return: bool,
    pub in_main: bool,
    pub all_global_var_names: HashSet<String>,
    /// Names whose local binding can switch to a global cell at runtime.
    pub(crate) runtime_global_bindings: HashSet<String>,
    /// This body's `static` locals used by reference (`crate::ir_lower::static_refs`): each is
    /// bound to a persistent reference cell, as php binds `static $x`, instead of a plain slot.
    pub(crate) reference_bound_statics: HashSet<String>,
    /// This body's `static` locals it also assigns as a whole (`static_refs::reassigned_statics`).
    pub(crate) reassigned_statics: HashSet<String>,
    /// `true` when lowering for a `--web` compile. Gates whether a bare
    /// request-superglobal name (`$_SERVER`/`$_SESSION`/…) is trusted to
    /// resolve to the fixed `AssocArray{Str, Mixed}` type: only `--web`
    /// builds pre-initialize that shared global storage, so a CLI build must
    /// fall back to the ordinary local/top-level type lookup (typically
    /// `Mixed`) instead of assuming a live Hash pointer. See `global_alias_type`.
    pub web: bool,
    owner_name: String,
    closures: Vec<Function>,
    /// Names this body writes through a non-integer array key, from `append_vivify`.
    ///
    /// Consulted only by [`Self::required_static_local_storage_type`], which must not widen an
    /// empty-array `static` that is really a hash. Empty for a body with no such write, which is
    /// the overwhelming majority.
    non_integer_keyed_locals: HashSet<String>,
    ref_capture_written_locals: HashSet<String>,
    /// Class of the object temporarily installed as `$this` while lowering a bound closure body.
    bound_closure_this_class: Option<String>,
    bound_closure_this_gradual: bool,
    pending_static_callable_result: Option<StaticCallableBinding>,
    closure_counter: usize,
    hidden_temp_counter: usize,
    /// Maps captured call values to their active exception-cleanup tokens.
    argument_guards: HashMap<ValueId, ValueId>,
    /// Pending call scopes order capture cleanup by PHP parameter position.
    argument_guard_scopes: Vec<Vec<(usize, ValueId)>>,
    /// Set while a container write BORROWS its value operand — the reference belongs to a
    /// hidden temporary that outlives the write. See `with_borrowed_write_operand`.
    write_operand_is_borrowed: bool,
    /// Set while a DIRECT call (`f(...$a)`, `$o->m(...$a)`) builds its descriptor argument
    /// container: php makes an element landing on a by-reference parameter a reference to the
    /// source slot, which `call_user_func()` (values only) must not do.
    pub(crate) direct_by_ref_unpack: bool,
    eval_barrier_active: bool,
    eval_executed: bool,
    consuming_string_append_enabled: bool,
    eval_scope_read_param: Option<String>,
    eval_scope_read_names: HashSet<String>,
    eval_scope_write_names: HashSet<String>,
    eval_scope_flush_names: BTreeSet<String>,
    /// Call spans whose arguments were proved `int|float` for a known closure.
    ///
    /// The proof is per span. A later call of the same local with an unproved
    /// argument does not inherit it.
    pub(crate) numeric_closure_sites: HashSet<Span>,
    /// Locals read or written by an admitted numeric proof.
    pub(crate) numeric_tracked_locals: HashSet<String>,
    /// Named storage epochs whose frame slot must be Mixed from the first store.
    ///
    /// Set on replay when a closure proof or numeric backedge requires stable runtime tags.
    /// Ordinals keep an unset/rebound variable independent from its old storage.
    /// This map fixes storage only; fast arithmetic needs its own numeric-domain proof.
    pub(crate) final_mixed_locals: HashMap<String, HashSet<usize>>,
    /// Explicit integer local declarations whose arithmetic writes are integer sinks.
    pub(crate) integer_local_declarations: HashSet<String>,
    /// Names whose conditional declarations prevent a path-independent integer write contract.
    pub(crate) conditional_local_declarations: HashSet<String>,
    /// When set, loop facts reuse `numeric_closure_sites` instead of seeding
    /// another proof from the now-Mixed slots.
    pub(crate) numeric_proof_sealed: bool,
    /// Locals whose loop-storage contract boxed a plain scalar entry (`$sum = 0` before
    /// `$sum += $f(1)`). Set by the loop being lowered just before its closure-fact check.
    pub(crate) loop_scalar_entry_boxed: HashSet<String>,
    /// Named storage epochs of counters whose checked-arithmetic seed (`$j = $i + 1`) the
    /// numeric storage proof showed always fits an integer, keyed like `final_mixed_locals`.
    ///
    /// Set on replay only (`numeric_storage::lower_body`); `lower_assign` then records such a seed
    /// as `int`, so the loop reads the counter exactly as after a `$j = 0` seed.
    pub(crate) int_seed_bindings: HashMap<String, HashSet<usize>>,
    /// Locals bound to a pointer by-reference parameter, checked against their final frame
    /// storage once the body is lowered (`refuse_pointer_reference_bindings`).
    pointer_reference_bindings: Vec<PointerReferenceBinding>,
    source_path: Option<String>,
    /// PHP-visible labels of closures lowered from this body, keyed by their EIR symbol name.
    closure_display_names: HashMap<String, String>,
}

impl<'m, 'f> LoweringContext<'m, 'f> {
    /// Whether a declared class has internal operator handlers, so an object operand may run one.
    pub(crate) fn program_has_operator_classes(&self) -> bool {
        *self.operator_classes
            .get_or_init(|| crate::types::object_operators::names_an_operator_class(self.classes.keys()))
    }

    /// Creates a lowering context over one function builder and shared module data.
    pub(crate) fn new(
        builder: Builder<'f>,
        data: &'m mut DataPool,
        env: TypeEnv,
        functions: &'m HashMap<String, FunctionSig>,
        extern_functions: &'m HashMap<String, ExternFunctionSig>,
        extern_globals: &'m HashMap<String, PhpType>,
        callable_param_sigs: &'m HashMap<(String, String), FunctionSig>,
        return_alias_summaries: &'m ReturnAliasSummaries,
        fiber_return_sigs: &'m HashMap<String, FunctionSig>,
        classes: &'m crate::fast_hash::FastMap<String, ClassInfo>,
        enums: &'m HashMap<String, EnumInfo>,
        interfaces: &'m HashMap<String, InterfaceInfo>,
        declared_trait_names: &'m [String],
        declared_trait_methods: &'m HashMap<String, HashMap<String, crate::ir::TraitMethodInfo>>,
        declared_trait_properties: &'m HashMap<String, Vec<crate::parser::ast::ClassProperty>>,
        packed_classes: &'m HashMap<String, PackedClassInfo>,
        extern_classes: &'m HashMap<String, ExternClassInfo>,
        throw_access_sites: &'m HashMap<(String, Span), ThrowAccessInfo>,
        builtin_call_types: &'m HashMap<(String, Span), PhpType>,
        closure_generator_spans: &'m HashSet<Span>,
        array_push_property_plans: &'m HashMap<(String, Span), crate::types::ArrayPushPropertyPlan>,
        member_receiver_types: &'m HashMap<(String, Span), Option<PhpType>>,
        flow_typed_property_accesses: &'m HashMap<(String, Span), PhpType>,
        boxed_reference_promotion_sites: &'m HashMap<(String, Span), HashSet<String>>,
        first_class_builtin_call_types: &'m HashMap<Span, PhpType>,
        generic_call_sites: &'m HashMap<(String, Span), String>,
        loop_storage_types: &'m crate::types::LoopStorageTypes,
        string_incdec_locals: &'m HashSet<(String, String)>,
        by_ref_local_storage_types: &'m HashMap<(String, String), PhpType>,
        dynamic_ref_local_types: &'m HashMap<(String, String), PhpType>,
        bind_kill_sites: &'m HashMap<Span, HashSet<String>>,
        ref_detach_sites: &'m HashMap<Span, HashSet<String>>,
        retype_sites: &'m HashMap<Span, HashSet<String>>,
        mixed_storage_store_sites: &'m HashMap<Span, HashSet<String>>,
        loop_storage_scope: String,
        constants: &'m HashMap<String, (ExprKind, PhpType)>,
        top_level_env: TypeEnv,
        current_class: Option<String>,
        owner_name: String,
        return_php_type: PhpType,
        in_main: bool,
        all_global_var_names: HashSet<String>,
        source_path: Option<String>,
        web: bool,
    ) -> Self {
        // All decision maps are keyed BY SPAN and consulted at every `unset` argument and every
        // assignment. `Span::dummy()` identifies no node, so a decision filed under it would
        // answer for every compiler-generated node at once — abandoning a binding at each of the
        // hundreds of dummy-span assignments the synthetic-class and PDO/mysqli/curl preludes
        // contain. The checker refuses to record one (`Span::identifies_a_node`); this is the
        // cheap whole-map restatement of that invariant, and the consult sites re-check it.
        debug_assert!(
            !bind_kill_sites.contains_key(&Span::dummy()),
            "a local-binding kill was recorded at a span that names no node",
        );
        debug_assert!(
            !ref_detach_sites.contains_key(&Span::dummy()),
            "a reference detach was recorded at a span that names no node",
        );
        debug_assert!(
            !retype_sites.contains_key(&Span::dummy()),
            "a local-binding retype was recorded at a span that names no node",
        );
        debug_assert!(
            !mixed_storage_store_sites.contains_key(&Span::dummy()),
            "a mixed-storage store site was recorded at a span that names no node",
        );
        let return_type = return_ir_type(&return_php_type);
        Self {
            builder,
            source_catalog: None,
            runtime_bound_functions: Default::default(),
            data,
            local_slots: HashMap::new(),
            local_kinds: HashMap::new(),
            local_types: env,
            include_frame_slots: 0,
            initialized_slots: HashSet::new(),
            scratch_backed_slots: HashSet::new(),
            functions,
            extern_functions,
            extern_globals,
            callable_param_sigs,
            return_alias_summaries,
            fiber_return_sigs,
            classes,
            operator_classes: std::cell::OnceCell::new(),
            enums,
            interfaces,
            declared_trait_names,
            declared_trait_methods,
            declared_trait_properties,
            packed_classes,
            extern_classes,
            throw_access_sites,
            builtin_call_types,
            closure_generator_spans,
            array_push_property_plans,
            member_receiver_types,
            flow_typed_property_accesses,
            boxed_reference_promotion_sites,
            first_class_builtin_call_types,
            generic_call_sites,
            loop_storage_types,
            string_incdec_locals,
            by_ref_local_storage_types,
            dynamic_ref_local_types,
            bind_kill_sites,
            ref_detach_sites,
            retype_sites,
            mixed_storage_store_sites,
            loop_storage_scope,
            closure_signature_scope: None,
            offset_dispatch_bypass: HashSet::new(),
            late_static_ladder_fallbacks: HashSet::new(),
            closure_bind_splits: HashSet::new(),
            direct_method_call_class: None,
            constants: ConstantScope::new(constants),
            top_level_env,
            current_class,
            loop_stack: Vec::new(),
            loop_carried_missing_strings: Vec::new(),
            finally_stack: Vec::new(),
            try_handler_stack: Vec::new(),
            try_loop_depths: Vec::new(),
            taken_finally_exceptions: Vec::new(),
            exit_cleaned_loops: Vec::new(),
            static_callable_locals: HashMap::new(),
            static_callable_local_epochs: HashMap::new(),
            function_global_names: HashMap::new(),
            reflection_class_locals: HashMap::new(),
            reflection_function_locals: HashMap::new(),
            reflection_property_locals: HashMap::new(),
            reflection_method_locals: HashMap::new(),
            reflection_arg_array_locals: HashMap::new(),
            fiber_start_sigs: HashMap::new(),
            ref_bound_locals: HashSet::new(),
            interior_ref_locals: HashMap::new(),
            capture_bound_locals: HashSet::new(),
            pending_ref_bridges: Vec::new(),
            borrowed_element_ref_locals: HashSet::new(),
            ref_cell_owner_locals: HashMap::new(),
            foreach_int_key_locals: HashSet::new(),
            storage_widened_locals: HashSet::new(),
            array_conversions: HashMap::new(),
            speculating: false,
            autoload_hook_active: false,
            php_strict_types: false,
            return_type,
            return_php_type,
            return_type_is_declared: false,
            by_ref_return: false,
            in_main,
            all_global_var_names,
            runtime_global_bindings: HashSet::new(),
            reference_bound_statics: HashSet::new(),
            reassigned_statics: HashSet::new(),
            web,
            owner_name,
            closures: Vec::new(),
            non_integer_keyed_locals: HashSet::new(),
            ref_capture_written_locals: HashSet::new(),
            bound_closure_this_class: None,
            bound_closure_this_gradual: false,
            pending_static_callable_result: None,
            closure_counter: 0,
            hidden_temp_counter: 0,
            expression_depth: 0,
            reference_call_context: None,
            call_argument_evaluation_scopes: Vec::new(),
            borrowed_string_call_operands: HashSet::new(),
            borrowed_call_results: HashSet::new(),
            string_offset_receivers: Vec::new(),
            owned_string_offset_sites: HashSet::new(),
            borrowed_string_calls_enabled: true,
            owned_string_cast_locals: HashSet::new(),
            provisional_borrowed_string_casts: HashSet::new(),
            this_storage_stays_concrete: false,
            append_update_cell_aliases: HashSet::new(),
            argument_guards: HashMap::new(),
            argument_guard_scopes: Vec::new(),
            write_operand_is_borrowed: false,
            direct_by_ref_unpack: false,
            eval_barrier_active: false,
            body_contains_eval: false,
            eval_executed: false,
            consuming_string_append_enabled: true,
            eval_scope_read_param: None,
            eval_scope_read_names: HashSet::new(),
            eval_scope_write_names: HashSet::new(),
            eval_scope_flush_names: BTreeSet::new(),
            numeric_closure_sites: HashSet::new(),
            numeric_tracked_locals: HashSet::new(),
            final_mixed_locals: HashMap::new(),
            integer_local_declarations: HashSet::new(),
            conditional_local_declarations: HashSet::new(),
            numeric_proof_sealed: false,
            loop_scalar_entry_boxed: HashSet::new(),
            int_seed_bindings: HashMap::new(),
            pointer_reference_bindings: Vec::new(),
            call_description: None,
            source_path,
            closure_display_names: HashMap::new(),
        }
    }

    /// Captures every mutable field touched by speculative statement lowering.
    pub(crate) fn snapshot(&self) -> LoweringSnapshot {
        LoweringSnapshot {
            function: self.builder.snapshot_function(),
            insertion_block: self.builder.insertion_block(),
            data: DataPoolLengths::capture(self.data),
            local_slots: self.local_slots.clone(),
            local_kinds: self.local_kinds.clone(),
            local_types: self.local_types.clone(),
            initialized_slots: self.initialized_slots.clone(),
            constants: self.constants.added.clone(),
            loop_stack: self.loop_stack.clone(),
            loop_carried_missing_strings: self.loop_carried_missing_strings.clone(),
            finally_stack: self.finally_stack.clone(),
            try_handler_stack: self.try_handler_stack.clone(),
            try_loop_depths: self.try_loop_depths.clone(),
            static_callable_locals: self.static_callable_locals.clone(),
            static_callable_local_epochs: self.static_callable_local_epochs.clone(),
            reflection_class_locals: self.reflection_class_locals.clone(),
            reflection_function_locals: self.reflection_function_locals.clone(),
            reflection_property_locals: self.reflection_property_locals.clone(),
            reflection_method_locals: self.reflection_method_locals.clone(),
            reflection_arg_array_locals: self.reflection_arg_array_locals.clone(),
            fiber_start_sigs: self.fiber_start_sigs.clone(),
            ref_bound_locals: self.ref_bound_locals.clone(),
            interior_ref_locals: self.interior_ref_locals.clone(),
            capture_bound_locals: self.capture_bound_locals.clone(),
            pending_ref_bridges: self.pending_ref_bridges.clone(),
            borrowed_element_ref_locals: self.borrowed_element_ref_locals.clone(),
            ref_cell_owner_locals: self.ref_cell_owner_locals.clone(),
            foreach_int_key_locals: self.foreach_int_key_locals.clone(),
            storage_widened_locals: self.storage_widened_locals.clone(),
            array_conversions: self.array_conversions.clone(),
            speculating: self.speculating,
            php_strict_types: self.php_strict_types,
            closure_count: self.closures.len(),
            bound_closure_this_class: self.bound_closure_this_class.clone(),
            bound_closure_this_gradual: self.bound_closure_this_gradual,
            pending_static_callable_result: self.pending_static_callable_result.clone(),
            closure_counter: self.closure_counter,
            hidden_temp_counter: self.hidden_temp_counter,
            expression_depth: self.expression_depth,
            reference_call_context: self.reference_call_context.clone(),
            // Refusals recorded by a speculative region must not outlive it: the region is
            // always lowered again for real, and that pass records its refusals again.
            refusals: crate::ir_lower::diagnostics::mark(),
            call_argument_evaluation_scopes: self.call_argument_evaluation_scopes.clone(),
            borrowed_string_call_operands: self.borrowed_string_call_operands.clone(),
            borrowed_call_results: self.borrowed_call_results.clone(),
            string_offset_receivers: self.string_offset_receivers.clone(),
            borrowed_string_calls_enabled: self.borrowed_string_calls_enabled,
            owned_string_cast_locals: self.owned_string_cast_locals.clone(),
            provisional_borrowed_string_casts: self.provisional_borrowed_string_casts.clone(),
            append_update_cell_aliases: self.append_update_cell_aliases.clone(),
            argument_guards: self.argument_guards.clone(),
            argument_guard_scopes: self.argument_guard_scopes.clone(),
            eval_barrier_active: self.eval_barrier_active,
            eval_executed: self.eval_executed,
            consuming_string_append_enabled: self.consuming_string_append_enabled,
            eval_scope_read_param: self.eval_scope_read_param.clone(),
            eval_scope_read_names: self.eval_scope_read_names.clone(),
            eval_scope_write_names: self.eval_scope_write_names.clone(),
            eval_scope_flush_names: self.eval_scope_flush_names.clone(),
            numeric_closure_sites: self.numeric_closure_sites.clone(),
            numeric_tracked_locals: self.numeric_tracked_locals.clone(),
            final_mixed_locals: self.final_mixed_locals.clone(),
            integer_local_declarations: self.integer_local_declarations.clone(),
            conditional_local_declarations: self.conditional_local_declarations.clone(),
            numeric_proof_sealed: self.numeric_proof_sealed,
            pointer_reference_bindings: self.pointer_reference_bindings.len(),
        }
    }

    /// Records that a `try` handler is now active, with the loops it encloses.
    pub(crate) fn push_try_loop_depth(&mut self) {
        let depth = self.loop_stack.len();
        self.try_loop_depths.push(depth);
    }

    /// Records locals this loop assigns a missing-string sentinel, for casts lowered
    /// earlier in the body than the store.
    pub(crate) fn push_loop_carried_missing_strings(&mut self, names: HashSet<String>) {
        self.loop_carried_missing_strings.push(names);
    }

    /// Drops the missing-string names recorded for the loop that just finished lowering.
    pub(crate) fn pop_loop_carried_missing_strings(&mut self) {
        self.loop_carried_missing_strings.pop();
    }

    /// Returns whether an enclosing loop assigns `name` a value that may be the sentinel.
    ///
    /// Copy-on-write shadows keep the PHP name with a `#cow` suffix. The loop scan records
    /// the source name, so the suffix is ignored here.
    pub(crate) fn loop_assigns_missing_string(&self, name: &str) -> bool {
        let name = name.strip_suffix("#cow").unwrap_or(name);
        self.loop_carried_missing_strings
            .iter()
            .any(|names| names.contains(name))
    }

    /// Drops the innermost active `try` handler's record.
    pub(crate) fn pop_try_loop_depth(&mut self) {
        self.try_loop_depths.pop();
    }

    /// Returns how many loop frames a throw raised here would actually LEAVE.
    ///
    /// A throw caught by a `try` INSIDE a loop does not leave that loop: control resumes in the
    /// catch and the loop keeps running, so releasing what the loop holds would free the storage
    /// it is still iterating and release it a second time on normal termination. Only the loops
    /// opened INSIDE the innermost active `try` are left behind (issue #690).
    ///
    /// With no `try` active, a throw leaves every loop -- it is on its way out of the function.
    pub(crate) fn loops_a_throw_would_leave(&self) -> usize {
        let innermost_try = self.try_loop_depths.last().copied().unwrap_or(0);
        self.loop_stack.len().saturating_sub(innermost_try)
    }

    /// Restores state captured before a discarded speculative lowering.
    pub(crate) fn restore(&mut self, snapshot: LoweringSnapshot) {
        self.builder.restore_function(snapshot.function);
        self.builder.restore_insertion_cursor(snapshot.insertion_block);
        snapshot.data.truncate(self.data);
        self.local_slots = snapshot.local_slots;
        self.local_kinds = snapshot.local_kinds;
        self.local_types = snapshot.local_types;
        self.initialized_slots = snapshot.initialized_slots;
        self.constants.added = snapshot.constants;
        self.loop_stack = snapshot.loop_stack;
        self.loop_carried_missing_strings = snapshot.loop_carried_missing_strings;
        self.finally_stack = snapshot.finally_stack;
        self.try_handler_stack = snapshot.try_handler_stack;
        self.try_loop_depths = snapshot.try_loop_depths;
        self.static_callable_locals = snapshot.static_callable_locals;
        self.static_callable_local_epochs = snapshot.static_callable_local_epochs;
        self.reflection_class_locals = snapshot.reflection_class_locals;
        self.reflection_function_locals = snapshot.reflection_function_locals;
        self.reflection_property_locals = snapshot.reflection_property_locals;
        self.reflection_method_locals = snapshot.reflection_method_locals;
        self.reflection_arg_array_locals = snapshot.reflection_arg_array_locals;
        self.fiber_start_sigs = snapshot.fiber_start_sigs;
        self.ref_bound_locals = snapshot.ref_bound_locals;
        self.interior_ref_locals = snapshot.interior_ref_locals;
        self.capture_bound_locals = snapshot.capture_bound_locals;
        self.pending_ref_bridges = snapshot.pending_ref_bridges;
        self.borrowed_element_ref_locals = snapshot.borrowed_element_ref_locals;
        self.ref_cell_owner_locals = snapshot.ref_cell_owner_locals;
        self.foreach_int_key_locals = snapshot.foreach_int_key_locals;
        self.storage_widened_locals = snapshot.storage_widened_locals;
        self.array_conversions = snapshot.array_conversions;
        self.speculating = snapshot.speculating;
        self.php_strict_types = snapshot.php_strict_types;
        self.closures.truncate(snapshot.closure_count);
        self.bound_closure_this_class = snapshot.bound_closure_this_class;
        self.bound_closure_this_gradual = snapshot.bound_closure_this_gradual;
        self.pending_static_callable_result = snapshot.pending_static_callable_result;
        self.closure_counter = snapshot.closure_counter;
        self.hidden_temp_counter = snapshot.hidden_temp_counter;
        self.expression_depth = snapshot.expression_depth;
        self.reference_call_context = snapshot.reference_call_context;
        crate::ir_lower::diagnostics::rollback_to(snapshot.refusals);
        self.pointer_reference_bindings.truncate(snapshot.pointer_reference_bindings);
        self.call_argument_evaluation_scopes = snapshot.call_argument_evaluation_scopes;
        self.borrowed_string_call_operands = snapshot.borrowed_string_call_operands;
        self.borrowed_call_results = snapshot.borrowed_call_results;
        self.string_offset_receivers = snapshot.string_offset_receivers;
        self.borrowed_string_calls_enabled = snapshot.borrowed_string_calls_enabled;
        self.owned_string_cast_locals = snapshot.owned_string_cast_locals;
        self.provisional_borrowed_string_casts = snapshot.provisional_borrowed_string_casts;
        self.append_update_cell_aliases = snapshot.append_update_cell_aliases;
        self.argument_guards = snapshot.argument_guards;
        self.argument_guard_scopes = snapshot.argument_guard_scopes;
        self.eval_barrier_active = snapshot.eval_barrier_active;
        self.eval_executed = snapshot.eval_executed;
        self.consuming_string_append_enabled = snapshot.consuming_string_append_enabled;
        self.eval_scope_read_param = snapshot.eval_scope_read_param;
        self.eval_scope_read_names = snapshot.eval_scope_read_names;
        self.eval_scope_write_names = snapshot.eval_scope_write_names;
        self.eval_scope_flush_names = snapshot.eval_scope_flush_names;
        self.numeric_closure_sites = snapshot.numeric_closure_sites;
        self.numeric_tracked_locals = snapshot.numeric_tracked_locals;
        self.final_mixed_locals = snapshot.final_mixed_locals;
        self.integer_local_declarations = snapshot.integer_local_declarations;
        self.conditional_local_declarations = snapshot.conditional_local_declarations;
        self.numeric_proof_sealed = snapshot.numeric_proof_sealed;
    }

    /// Returns the canonical PHP source path associated with this lowered body, if known.
    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }


    /// Records PHP's `{closure:<context>:<line>}` label for the closure about to be lowered.
    ///
    /// The context is the file for top-level code, `name()` inside a function, `Class::name()`
    /// inside a method and the enclosing closure's own label inside a closure, the spelling PHP
    /// 8.4+ prints in traces and `__FUNCTION__`.
    pub(crate) fn register_closure_display_name(&mut self, closure_name: &str, line: u32) {
        let enclosing_closure = self
            .builder
            .function()
            .flags
            .is_closure
            .then(|| self.builder.function().display_name.clone())
            .flatten();
        let context = if let Some(enclosing) = enclosing_closure {
            enclosing
        } else if self.in_main {
            self.source_path.clone().unwrap_or_else(|| "unknown".to_string())
        } else {
            format!("{}()", self.owner_name)
        };
        self.closure_display_names
            .insert(closure_name.to_string(), format!("{{closure:{context}:{line}}}"));
    }

    /// Takes the label recorded by [`Self::register_closure_display_name`] for `closure_name`.
    pub(crate) fn take_closure_display_name(&mut self, closure_name: &str) -> Option<String> {
        self.closure_display_names.remove(closure_name)
    }

    /// Interns a string literal or metadata name in the module data pool.
    pub(crate) fn intern_string(&mut self, value: &str) -> DataId {
        self.data.intern_string(value)
    }

    /// Converts parsed type syntax into PHP metadata using known packed classes.
    pub(crate) fn type_expr_to_php_type_for_value(&self, type_expr: &TypeExpr) -> PhpType {
        match type_expr {
            TypeExpr::Named(name) => {
                let name = name.as_str().trim_start_matches('\\');
                let php_type = named_type_expr_to_php_type(name);
                if matches!(php_type, PhpType::Object(_)) && self.packed_classes.contains_key(name)
                {
                    PhpType::Packed(name.to_string())
                } else {
                    php_type
                }
            }
            TypeExpr::Buffer(inner) => PhpType::Buffer(Box::new(
                crate::types::width_type_from_type_expr(inner)
                    .unwrap_or_else(|| self.type_expr_to_php_type_for_value(inner)),
            )),
            TypeExpr::Array(inner) => {
                PhpType::Array(Box::new(self.type_expr_to_php_type_for_value(inner)))
            }
            TypeExpr::Nullable(inner) => PhpType::Union(vec![
                PhpType::Void,
                self.type_expr_to_php_type_for_value(inner),
            ]),
            TypeExpr::Union(members) => PhpType::Union(
                members
                    .iter()
                    .map(|member| self.type_expr_to_php_type_for_value(member))
                    .collect(),
            ),
            other => type_expr_to_php_type(other),
        }
    }

    /// Interns a global-name metadata string in the module data pool.
    pub(crate) fn intern_global_name(&mut self, value: &str) -> DataId {
        self.data.intern_global_name(value)
    }

    /// Interns the backing program-global name for a PHP variable or `$GLOBALS` alias.
    pub(crate) fn intern_global_storage_name(&mut self, name: &str) -> DataId {
        let storage = crate::globals_array::alias_target(name).unwrap_or(name);
        self.intern_global_name(storage)
    }

    /// Interns a function-name metadata string in the module data pool.
    pub(crate) fn intern_function_name(&mut self, value: &str) -> DataId {
        self.data.intern_function_name(value)
    }

    /// Interns a class-name metadata string in the module data pool.
    pub(crate) fn intern_class_name(&mut self, value: &str) -> DataId {
        self.data.intern_class_name(value)
    }

    /// Returns the current known PHP type for a local or `Mixed` when unknown.
    pub(crate) fn local_type(&self, name: &str) -> PhpType {
        self.local_types
            .get(name)
            .cloned()
            .unwrap_or(PhpType::Mixed)
    }

    /// Returns the frame representation allocated for a local, including whole-scope widening.
    pub(crate) fn local_storage_type(&self, name: &str) -> PhpType {
        self.local_slots
            .get(name)
            .map(|slot| self.builder.local_php_type(*slot))
            .unwrap_or_else(|| self.required_local_storage_type(name, self.local_type(name)))
    }

    /// Records a foreach loop-key local whose source is a concretely-indexed
    /// array, so its runtime key is always an integer (see `foreach_int_key_locals`).
    pub(crate) fn mark_foreach_int_key(&mut self, name: &str) {
        self.foreach_int_key_locals.insert(name.to_string());
    }

    /// Returns true when `name` is a foreach loop key known to hold an integer at
    /// runtime despite its Mixed EIR type, so an indexed write can safely coerce it
    /// to int instead of promoting the destination to a hash.
    pub(crate) fn is_foreach_int_key(&self, name: &str) -> bool {
        self.foreach_int_key_locals.contains(name)
    }

    /// Runs `f` with container-write value operands treated as BORROWED.
    ///
    /// The `??=` result temporary is written into the container AND handed to the expression's
    /// consumer. The write therefore only borrows it: `array_set` takes its own reference by
    /// retaining, and the slot keeps hers. Treating it as an owning temporary made the write
    /// consume the slot's reference, and the consumer's release then took the cell to zero
    /// while the container still pointed at it.
    pub(crate) fn with_borrowed_write_operand<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let previous = self.write_operand_is_borrowed;
        self.write_operand_is_borrowed = true;
        let result = f(self);
        self.write_operand_is_borrowed = previous;
        result
    }

    /// Classifies a receiver lease independently of the borrowed RHS of a container write.
    pub(crate) fn with_independent_write_receiver<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let previous = self.write_operand_is_borrowed;
        self.write_operand_is_borrowed = false;
        let result = f(self);
        self.write_operand_is_borrowed = previous;
        result
    }

    /// Makes a borrowed write operand explicit so backend consumers cannot adopt its temp slot.
    pub(crate) fn borrow_write_operand_if_needed(&mut self, value: LoweredValue, span: Span) -> LoweredValue {
        let ty = self.builder.value_php_type(value.value);
        if !self.write_operand_is_borrowed || !Ownership::php_type_needs_lifetime_tracking(&ty) {
            return value;
        }
        let borrowed = self.emit_value(Op::Borrow, vec![value.value], None, ty,
            Op::Borrow.default_effects(), Some(span));
        self.builder.set_value_ownership(borrowed.value, Ownership::Borrowed);
        borrowed
    }

    /// Returns the storage type for a `global` alias name.
    ///
    /// Under `--web`, request superglobals resolve to their fixed
    /// `AssocArray{Str, Mixed}` type directly: inside a function the
    /// `top_level_env` snapshot may not carry them, but their global slot
    /// must still be a Hash pointer (not a boxed Mixed cell) so the function
    /// read agrees with the prelude's StoreGlobal. Outside `--web` nothing
    /// pre-initializes that shared global storage, so trusting the fixed Hash
    /// type here would read a null/zeroed `.comm` slot as a live Hash pointer
    /// and crash; fall through to the ordinary env lookup (typically `Mixed`)
    /// instead. Ordinary PHP globals use boxed Mixed storage in every scope
    /// because a function declaring `global $x` may replace its runtime type.
    pub(crate) fn global_alias_type(&self, name: &str) -> PhpType {
        let name = crate::globals_array::alias_target(name).unwrap_or(name);
        if self.web && crate::superglobals::is_superglobal(name) {
            return crate::superglobals::superglobal_type();
        }
        PhpType::Mixed
    }

    /// Returns the prescanned value and PHP type for a global constant name.
    pub(crate) fn constant_value(&self, name: &str) -> Option<(ExprKind, PhpType)> {
        self.constants
            .get(name)
            .or_else(|| self.constants.get(elephc_builtin_contract::canonical_constant_name(name)))
            .cloned()
    }

    /// Returns a class or interface constant expression resolved with PHP lookup order.
    pub(crate) fn scoped_constant_value(
        &self,
        class_name: &str,
        const_name: &str,
    ) -> Option<crate::parser::ast::Expr> {
        let mut current = Some(class_name);
        while let Some(name) = current {
            if let Some(info) = self.classes.get(name) {
                if let Some(value) = info.constants.get(const_name) {
                    return Some(value.clone());
                }
                current = info.parent.as_deref();
            } else {
                current = None;
            }
        }
        if let Some(info) = self.classes.get(class_name) {
            for interface_name in &info.interfaces {
                if let Some(value) = self.interface_constant_value(interface_name, const_name) {
                    return Some(value);
                }
            }
        }
        self.interface_constant_value(class_name, const_name)
    }

    /// Returns an interface constant expression, including inherited parent interfaces.
    fn interface_constant_value(
        &self,
        interface_name: &str,
        const_name: &str,
    ) -> Option<crate::parser::ast::Expr> {
        let mut visited = HashSet::new();
        let mut queue = vec![interface_name.to_string()];
        while let Some(name) = queue.pop() {
            if !visited.insert(name.clone()) {
                continue;
            }
            if let Some(info) = self.interfaces.get(&name) {
                if let Some(value) = info.constants.get(const_name) {
                    return Some(value.clone());
                }
                queue.extend(info.parents.iter().cloned());
            }
        }
        None
    }

    /// Records a constant discovered while lowering source-order `define()` calls.
    pub(crate) fn register_constant(&mut self, name: String, value: ExprKind, ty: PhpType) {
        self.constants.register(name, (value, ty));
    }

    /// Updates the current known PHP type for a local.
    pub(crate) fn set_local_type(&mut self, name: &str, ty: PhpType) {
        self.set_local_type_impl(name, ty, true);
    }

    /// Updates a local type and optionally records a runtime array-layout conversion.
    fn set_local_type_impl(&mut self, name: &str, ty: PhpType, track_array_conversion: bool) {
        if let Some(slot) = self.local_slots.get(name).copied() {
            self.builder.widen_local_storage_type(slot, ty.clone());
        }
        if track_array_conversion {
            self.record_array_storage_conversion(name, &ty);
        }
        self.local_types.insert(name.to_string(), ty);
    }

    /// Records a runtime layout change independently of the frame slot's storage metadata.
    fn record_array_storage_conversion(&mut self, name: &str, ty: &PhpType) {
        if let Some(target) = array_storage_conversion(self.local_types.get(name), ty) {
            let joined = self.array_conversions.get(name).map_or(target.clone(), |previous| {
                join_array_storage_conversion(previous, &target)
            });
            self.array_conversions.insert(name.to_string(), joined);
        }
    }

    /// Returns the joined array representation conversion observed for a local.
    pub(crate) fn array_conversion(&self, name: &str) -> Option<&PhpType> {
        self.array_conversions.get(name)
    }

    /// Clears conversion facts for the candidate locals before a discovery pass.
    pub(crate) fn forget_array_conversions(&mut self, names: &[String]) {
        for name in names {
            self.array_conversions.remove(name);
        }
    }

    /// Returns whether lowering is currently running only to discover conversions.
    pub(crate) fn is_speculating(&self) -> bool {
        self.speculating
    }

    /// Sets speculative-lowering mode and returns its previous value.
    pub(crate) fn set_speculating(&mut self, speculating: bool) -> bool {
        std::mem::replace(&mut self.speculating, speculating)
    }

    /// Updates only the flow-sensitive PHP type fact for a local.
    pub(crate) fn set_local_logical_type(&mut self, name: &str, ty: PhpType) {
        self.local_types.insert(name.to_string(), ty);
    }

    /// Records a local bound to a pointer by-reference parameter, for the final storage check.
    pub(crate) fn record_pointer_reference_binding(
        &mut self,
        name: &str,
        span: Span,
        param_ty: &PhpType,
    ) {
        if let Some(slot) = self.local_slots.get(name).copied() {
            self.pointer_reference_bindings.push(PointerReferenceBinding {
                slot,
                name: name.to_string(),
                span,
                param_ty: param_ty.clone(),
            });
        }
    }

    /// Refuses each pointer by-reference binding whose local did not end up a pointer word.
    ///
    /// The callee reads and writes a raw pointer word through the reference. A frame slot is a
    /// pointer word when every store into it was `null`, a packed record address or a pointer
    /// (`pointer_word_storage` in `crate::ir::builder`; a `null`-only local bound here is retyped
    /// to the parameter's `?ptr<T>` first). A slot that a reference or `global` aliases, or that
    /// also holds another type, is boxed, so the caller would misread what the callee wrote.
    /// Final storage is known only once the whole body is lowered.
    pub(crate) fn refuse_pointer_reference_bindings(&self) {
        for binding in &self.pointer_reference_bindings {
            let storage = self.builder.local_php_type(binding.slot);
            if matches!(storage.codegen_repr(), PhpType::Pointer(_)) {
                continue;
            }
            let shown = crate::types::pointer_param_display(&binding.param_ty);
            crate::ir_lower::diagnostics::refuse(
                binding.span,
                &format!(
                    "${name} cannot bind to a `{shown}` by-reference parameter: the callee writes \
                     a pointer word back, but ${name} is not stored as one (a reference or a \
                     `global` statement aliases it, or it holds another type somewhere in this \
                     scope). Declare it as `{shown} ${name} = null;`",
                    name = binding.name,
                ),
            );
        }
    }

    /// Re-asserts the boxed frame-storage type of every local a store has WIDENED, after a
    /// snapshot restore put an older, narrower fact back.
    ///
    /// Only one construct needs it. A `while` exit deliberately restores the types as of the
    /// CONDITION rather than the body's, so a body-only narrowing does not leak past a false
    /// condition — but that restore also undoes a widening the body performed, and the loop exit
    /// is reached from iterations where the body DID run. `$b = \strlen('ab'); while (…) { $b =
    /// 'sb'; } echo $b;` read the slot as `int` and printed `0`. Every other construct keeps the
    /// fact the widening store installed and needs nothing here.
    pub(crate) fn reassert_widened_local_storage_types(&mut self) {
        let widened = self
            .storage_widened_locals
            .iter()
            .filter_map(|name| {
                let slot = *self.local_slots.get(name)?;
                Some((name.clone(), self.builder.local_php_type(slot)))
            })
            .collect::<Vec<(String, PhpType)>>();
        for (name, ty) in widened {
            self.set_local_logical_type(&name, ty);
        }
    }

    /// Returns `true` if a local slot has already been declared for `name`.
    pub(crate) fn has_local_slot(&self, name: &str) -> bool {
        self.local_slots.contains_key(name)
    }

    /// Declares a local slot if it does not already exist.
    pub(crate) fn declare_local(&mut self, name: &str, php_type: PhpType) -> LocalSlotId {
        let kind = if crate::globals_array::is_alias(name) {
            LocalKind::GlobalAlias
        } else {
            LocalKind::PhpLocal
        };
        self.declare_local_with_kind(name, php_type, kind)
    }

    /// Counts earlier storage bindings of one PHP name, independently of unrelated hidden slots.
    /// Parameter COW shadows are separate storage epochs and retain their generated suffix.
    pub(crate) fn local_binding_ordinal(&self, name: &str, slot: Option<LocalSlotId>) -> usize {
        let locals = &self.builder.function().locals;
        let limit = slot.map_or(locals.len(), |slot| slot.as_raw() as usize);
        locals.iter().take(limit).filter(|local| local.name.as_deref().is_some_and(|stored| {
            stored == name || stored.strip_suffix("#cow") == Some(name)
        })).count()
    }

    /// Applies a replay constraint only to its original binding, including before the first store.
    pub(crate) fn is_final_mixed_local(&self, name: &str) -> bool {
        self.final_mixed_locals.get(name).is_some_and(|bindings| {
            bindings.contains(&self.local_binding_ordinal(name, self.local_slots.get(name).copied()))
        })
    }

    /// Returns whether the current binding of `name` is a counter with a proven integer seed.
    ///
    /// Resolved like `is_final_mixed_local`, so the answer also holds before the binding's first
    /// store mints its slot.
    pub(crate) fn is_int_seed_binding(&self, name: &str) -> bool {
        self.int_seed_bindings.get(name).is_some_and(|bindings| {
            bindings.contains(&self.local_binding_ordinal(name, self.local_slots.get(name).copied()))
        })
    }

    /// The checker's result type for a builtin call at `span`, under the scope being lowered
    /// (a closure signature's own scope while one is derived from this parent context).
    pub(crate) fn builtin_call_type_at(&self, span: Span) -> Option<&PhpType> {
        let scope = self
            .closure_signature_scope
            .as_ref()
            .unwrap_or(&self.loop_storage_scope);
        self.builtin_call_types.get(&(scope.clone(), span))
    }

    /// Returns the frame storage type a local must use, boxing the locals PHP's `++`/`--`
    /// can retype.
    ///
    /// Two types qualify, for the same reason. `"9"++` is `int(10)`, so a string target
    /// cannot keep concrete `Str` storage; and `PHP_INT_MAX++` is a float, so an int target
    /// cannot keep a raw `I64` one. Widening the slot lazily at the increment is not enough:
    /// the slot type is a whole-frame property, so every OTHER concretely typed access to it
    /// reads the wrong representation. The string case leaked one heap block per read
    /// (`__rt_mixed_cast_string` detaching an owned copy); the int case silently TRUNCATED —
    /// the promoting `ichecked_add` produced a boxed float and the raw slot kept its low
    /// word, so `for ($i = PHP_INT_MAX - 1; …; $i++)` wrapped to the negative minimum where
    /// php prints the float.
    ///
    /// Boxing from the first store — including the incoming-parameter store — keeps every
    /// access on the ordinary boxed-Mixed path instead, and `checked_int_sink` narrows the
    /// slot back to `I64` wherever no access can observe the float, which is what keeps an
    /// ordinary counter loop as fast as it was.
    ///
    /// A name bound by a runtime `global` statement is boxed too, and a caller local the checker
    /// gave a by-reference storage contract takes that contract.
    fn required_local_storage_type(&self, name: &str, php_type: PhpType) -> PhpType {
        if self.is_final_mixed_local(name) {
            return PhpType::Mixed;
        }
        if self.runtime_global_bindings.contains(name)
            && !crate::superglobals::is_superglobal(name)
            && !self.extern_globals.contains_key(name)
        {
            return PhpType::Mixed;
        }
        let key = (self.loop_storage_scope.clone(), name.to_string());
        if let Some(required) = self.by_ref_local_storage_types.get(&key) {
            return required.clone();
        }
        if let Some(current) = self.local_types.get(name) {
            if current.codegen_repr() == PhpType::TaggedScalar
                && matches!(
                    php_type.codegen_repr(),
                    PhpType::Int
                        | PhpType::Bool
                        | PhpType::Void
                        | PhpType::Never
                        | PhpType::TaggedScalar
                )
            {
                // A conditional store into one branch of a nullable-scalar CFG must not make
                // later merged reads forget the tag word. Keep the original semantic union so
                // skipped branches still decode null rather than reading the payload as a scalar.
                return current.clone();
            }
        }
        if matches!(php_type.codegen_repr(), PhpType::Str)
            && self.string_incdec_locals.contains(&key)
        {
            return PhpType::Mixed;
        }
        php_type
    }

    /// Returns stable storage for a function static across all assignments in its body.
    ///
    /// The checker environment supplied to lowering describes the type after the whole body,
    /// whereas `php_type` describes only the declaration initializer. When those representations
    /// differ, or when the initializer is null, a concrete slot cannot represent every value the
    /// persistent variable may hold on later calls. A boxed Mixed slot preserves that evolution.
    ///
    /// An EMPTY ARRAY initializer is the same case one step in: `array<never>` is not a narrower
    /// array type, it is the claim that the slot has no elements, and the initializer that made
    /// that true ran on the first call only. Its element slots are ZERO WIDTH, and codegen acts
    /// on that — `array_shift`/`array_pop` lower the removed payload to a literal zero — so a
    /// static left at `array<never>` returned `NULL` for an element an earlier call pushed, and
    /// the same compaction walked a buffer `__rt_array_push_str` had already re-stamped to
    /// 16-byte string slots at a stride of 8. The decision lives HERE and not in the checker:
    /// the checker's environment type drives acceptance and narrowing rules that are written
    /// against the literal's own type, while this is the STORAGE the slot must have for the whole
    /// program, which is exactly what `array<never>` cannot describe.
    fn required_static_local_storage_type(&self, name: &str, php_type: PhpType) -> PhpType {
        let initial_repr = php_type.codegen_repr();
        let checked_repr = self.local_types.get(name).map(PhpType::codegen_repr);
        if php_type.is_empty_array_literal() && !self.non_integer_keyed_locals.contains(name) {
            // An empty-array initializer widens its ELEMENTS, not the slot. A boxed slot would
            // put a Mixed cell in the symbol, which a `mixed &$param` argument then destroys —
            // see `PhpType::gradual_empty_array_storage`. And only for a static this body writes
            // with integer keys: a string-keyed one is a hash whose slot type would go stale at
            // the runtime promotion (`append_vivify::locals_written_with_non_integer_keys`).
            return PhpType::gradual_empty_array_storage();
        }
        if php_type.is_empty_array_literal() {
            // The string-keyed case: a hash from the declaration on. Typed `array<never>`, the
            // slot kept an indexed header over the hash the first string-key write promoted it
            // to, and on the SAME call a read of `$cache[$class]` after `$cache[$class] = ...`
            // missed (`static $cache = []` is how Twig's `CoreExtension::getAttribute()` memoizes
            // an object's methods); a later call re-promoted the hash and duplicated its keys.
            // `lower_static_var` converts the initializer to match.
            return PhpType::AssocArray {
                key: Box::new(PhpType::Mixed),
                value: Box::new(PhpType::Mixed),
            };
        }
        if matches!(initial_repr, PhpType::Void | PhpType::Never)
            || checked_repr.is_some_and(|checked| checked != initial_repr)
        {
            PhpType::Mixed
        } else {
            php_type
        }
    }

    /// Declares a local slot with the requested role if it does not already exist.
    pub(crate) fn declare_local_with_kind(
        &mut self,
        name: &str,
        php_type: PhpType,
        kind: LocalKind,
    ) -> LocalSlotId {
        if let Some(slot) = self.local_slots.get(name) {
            return *slot;
        }
        let boxed_php_type = match kind {
            LocalKind::PhpLocal => self.required_local_storage_type(name, php_type.clone()),
            LocalKind::StaticLocal => {
                self.required_static_local_storage_type(name, php_type.clone())
            }
            _ => php_type.clone(),
        };
        // An incoming `string` parameter arrives with a `Str` entry already seeded from the
        // signature environment, so the boxed contract has to REPLACE that fact rather than
        // defer to it; otherwise every read of the parameter stays `Str`-typed against the
        // boxed slot the increment needs.
        let overrides_seeded_type = boxed_php_type != php_type;
        let php_type = boxed_php_type;
        let ir_type = value_ir_type(&php_type);
        let slot = self
            .builder
            .add_local(Some(name.to_string()), ir_type, php_type.clone(), kind);
        self.local_slots.insert(name.to_string(), slot);
        self.local_kinds.insert(name.to_string(), kind);
        if overrides_seeded_type {
            self.local_types.insert(name.to_string(), php_type);
        } else {
            // `or_insert`, so an ABANDONED name keeps the `Void` fact `abandon_local_binding`
            // deliberately left behind (that fact is what makes `isset`/`empty`/`??` on the
            // unbound name answer false instead of reading uninitialized storage) — this call
            // mints the fresh slot without overwriting it. The discipline that makes the stale
            // `Void` harmless belongs to the CALLER: every re-binding path pairs its
            // `declare_local` with a `store_local`, which ends by setting the name's type to the
            // stored one. A future caller that declares an abandoned name WITHOUT storing to it
            // would keep reading it as `Void`.
            self.local_types.entry(name.to_string()).or_insert(php_type);
        }
        slot
    }

    /// Rebinds a by-value container or Mixed parameter to an owning copy-on-write shadow slot.
    ///
    /// Call sites pass container pointers as borrows. Concrete containers are retained so their
    /// first mutation observes refcount two and splits. An exact PHP `array` arrives as a boxed
    /// Mixed cell, so its wrapper is cloned before the shadow is published; mutating that cell can
    /// then replace its payload without rewriting caller storage.
    pub(crate) fn privatize_container_param(
        &mut self,
        name: &str,
        php_type: &PhpType,
        span: Option<Span>,
    ) {
        let borrowed = self.load_local(name, span);
        let value = if php_type.codegen_repr() == PhpType::Mixed {
            self.emit_owned_value(
                Op::MixedClone, vec![borrowed.value], None, PhpType::Mixed,
                Op::MixedClone.default_effects(), span,
            )
        } else if crate::types::is_pointer_element_indexed_array(php_type) {
            // A runtime bridge (callable invoker, eval binding) can pass an array of boxed Mixed
            // cells; the conversion consumes this owned reference and returns raw pointer words.
            let owned = crate::ir_lower::ownership::acquire_if_refcounted(self, borrowed, span);
            self.emit_owned_value(
                Op::ArrayToPointerWords, vec![owned.value], None, php_type.clone(),
                Op::ArrayToPointerWords.default_effects(), span,
            )
        } else {
            borrowed
        };
        let shadow = self.builder.add_local(
            Some(format!("{}#cow", name)),
            value_ir_type(php_type),
            php_type.clone(),
            LocalKind::PhpLocal,
        );
        self.local_slots.insert(name.to_string(), shadow);
        self.local_kinds
            .insert(name.to_string(), LocalKind::PhpLocal);
        self.store_local(name, value, php_type.clone(), span);
    }

    /// Marks a local slot as initialized by caller or synthetic setup.
    pub(crate) fn mark_local_initialized(&mut self, name: &str) {
        if let Some(slot) = self.local_slots.get(name) {
            self.initialized_slots.insert(*slot);
        }
    }

    /// Captures the definitely-initialized local slots at a control-flow split.
    pub(crate) fn initialized_slots_snapshot(&self) -> HashSet<LocalSlotId> {
        self.initialized_slots.clone()
    }

    /// Returns whether a local slot is definitely initialized at this point.
    pub(crate) fn slot_is_initialized(&self, slot: LocalSlotId) -> bool {
        self.initialized_slots.contains(&slot)
    }

    /// Returns definitely visible PHP locals in declaration order for scope introspection.
    ///
    /// Compiler-generated frame and parser temporaries are `PhpLocal`s like any other by the
    /// time lowering sees them, so they are excluded by the one shared name predicate rather
    /// than by a `LocalKind` of their own: nothing about their ownership, initialization or
    /// store behaviour differs, only their visibility to PHP.
    ///
    /// The bound `$this` of a method frame is excluded for the separate, PHP-semantic reason
    /// spelled out at the filter below, and only here.
    ///
    /// PHP lists a variable at the position of its FIRST binding: reassigning it at another type,
    /// or unsetting and assigning it again, never moves it. Ending a binding gives the name a
    /// newer slot (a retype, an `unset` kill, a storage-only numeric re-bind), so the order key is
    /// the first slot that ever carried the name rather than the one it holds now.
    pub(crate) fn visible_local_names(&self) -> Vec<String> {
        let mut first_slots: HashMap<&str, u32> = HashMap::new();
        for local in &self.builder.function().locals {
            if let Some(name) = local.name.as_deref() {
                first_slots.entry(name).or_insert(local.id.as_raw());
            }
        }
        let mut locals = self
            .local_slots
            .iter()
            .filter_map(|(name, slot)| {
                if crate::names::is_generated_local_name(name.as_str()) {
                    return None;
                }
                // PHP never lists `$this` in `get_defined_vars()`. It is not a variable of the
                // frame: it is the bound object of the method call, which is why PHP also
                // refuses to assign to it. The slot keeps its name and its `PhpLocal` kind, so
                // eval scope synchronization still publishes `$this` into a fragment, where PHP
                // does make it readable; only this PHP-visible enumeration drops it.
                if name.as_str() == "this" {
                    return None;
                }
                let kind = self
                    .local_kinds
                    .get(name)
                    .copied()
                    .unwrap_or(LocalKind::PhpLocal);
                (kind == LocalKind::PhpLocal
                    && self.initialized_slots.contains(slot)
                    && !matches!(self.local_types.get(name), Some(PhpType::Void | PhpType::Never)))
                .then(|| {
                    let position = first_slots.get(name.as_str()).copied();
                    (position.unwrap_or(slot.as_raw()), name.clone())
                })
            })
            .collect::<Vec<_>>();
        locals.sort_by_key(|(slot, _)| *slot);
        locals.into_iter().map(|(_, name)| name).collect()
    }

    /// Whether a program function of this name is part of the module being lowered.
    pub(crate) fn module_declares_function(&self, name: &str) -> bool {
        self.functions.contains_key(name)
    }

    /// Whether the closed-world `class_exists()` retry rewrite is lowering its own native probe.
    pub(crate) fn autoload_hook_active(&self) -> bool {
        self.autoload_hook_active
    }

    /// Sets that flag and returns the previous value.
    pub(crate) fn set_autoload_hook_active(&mut self, active: bool) -> bool {
        std::mem::replace(&mut self.autoload_hook_active, active)
    }

    /// Names of the frame's PHP locals, initialized or not, in slot order (reference-bound ones excluded).
    ///
    /// These are the variables a native `extract()` can create or overwrite: the frame's locals
    /// are all compiled, so the symbol table `extract()` writes into is known.
    pub(crate) fn extractable_local_names(&self) -> Vec<String> {
        let mut locals = self
            .local_slots
            .iter()
            .filter(|(name, _)| {
                !crate::names::is_generated_local_name(name.as_str())
                    && name.as_str() != "this"
                    && self.local_kinds.get(*name).copied().unwrap_or(LocalKind::PhpLocal)
                        == LocalKind::PhpLocal
                    && !self.is_ref_bound_local(name.as_str())
            })
            .map(|(name, slot)| (slot.as_raw(), name.clone()))
            .collect::<Vec<_>>();
        locals.sort();
        locals.into_iter().map(|(_, name)| name).collect()
    }

    /// Replaces the definitely-initialized local set after branch lowering or merge analysis.
    pub(crate) fn restore_initialized_slots(&mut self, initialized_slots: HashSet<LocalSlotId>) {
        self.initialized_slots = initialized_slots;
    }

    /// Captures the flow-sensitive local-type facts at a control-flow split.
    pub(crate) fn local_types_snapshot(&self) -> TypeEnv {
        self.local_types.clone()
    }

    /// Restores only the flow-sensitive type facts at a branch split.
    ///
    /// This deliberately avoids `set_local_type`, which would also widen the shared frame slot;
    /// storage widening must survive while each branch gets its own logical type environment.
    pub(crate) fn restore_local_types(&mut self, types: TypeEnv) {
        self.local_types = types;
    }

    /// Returns whether a local is backed by program-global storage.
    pub(crate) fn local_uses_global_storage(&self, name: &str) -> bool {
        let kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        self.uses_global_storage(name, kind)
    }

    /// Returns whether a local is an ordinary frame slot of its own: a PHP local with a slot that
    /// is neither program-global storage nor bound to a reference cell.
    ///
    /// Only such a slot can take the pointer-word storage of a local that holds `null` and
    /// pointers (`pointer_word_storage` in `crate::ir::builder`) by a retype alone: a reference
    /// cell, a global or a static keeps the `null` it already holds in its own representation.
    pub(crate) fn local_is_plain_frame_slot(&self, name: &str) -> bool {
        self.local_kinds.get(name).copied() == Some(LocalKind::PhpLocal)
            && self.local_slots.contains_key(name)
            && !self.local_uses_global_storage(name)
            && !self.is_ref_bound_local(name)
    }

    /// Records that a local currently aliases by-reference storage.
    pub(crate) fn mark_ref_bound_local(&mut self, name: &str) {
        self.ref_bound_locals.insert(name.to_string());
        // A new binding says nothing yet about WHAT it aliases; `foreach` re-marks an interior
        // element right after this call, and every other binding aliases a managed cell.
        self.interior_ref_locals.remove(name);
        self.capture_bound_locals.remove(name);
    }

    /// Records that a local's only reference binding so far is a closure's `use (&$name)`.
    ///
    /// Such a cell belongs to this name alone, so a later capture that writes may still widen the
    /// name to `Mixed` (the slot and its owner widen together). A cell shared with another name, a
    /// caller or a global keeps the representation it was made with.
    pub(crate) fn mark_capture_bound_local(&mut self, name: &str) {
        self.capture_bound_locals.insert(name.to_string());
    }

    /// Returns true when the name's reference binding came only from closure captures.
    pub(crate) fn is_capture_bound_local(&self, name: &str) -> bool {
        self.capture_bound_locals.contains(name)
    }

    /// Returns true when a local may hold an interior container address.
    pub(crate) fn is_interior_ref_local(&self, name: &str) -> bool {
        self.interior_ref_locals.contains_key(name)
    }

    /// Clears the by-reference alias marker for a local after `unset()`.
    pub(crate) fn unmark_ref_bound_local(&mut self, name: &str) {
        self.ref_bound_locals.remove(name);
        self.interior_ref_locals.remove(name);
        self.capture_bound_locals.remove(name);
        self.forget_static_closures_capturing(name);
        self.borrowed_element_ref_locals.remove(name);
    }

    /// Drops the static bindings of closures that capture `name` by reference.
    ///
    /// A statically bound `$f()` passes a by-reference capture as the address of the variable's
    /// slot at the CALL, not the cell the closure captured. Once the name is unset or bound to
    /// another reference the two differ: `$f = function () use (&$s) {...}; unset($s);
    /// $s = "w"; $f();` showed the closure the new `$s`. The call then goes through the
    /// descriptor, which holds the captured cell.
    pub(crate) fn forget_static_closures_capturing(&mut self, name: &str) {
        self.static_callable_locals.retain(|_, binding| {
            !matches!(
                binding,
                StaticCallableBinding::Closure { captures, .. }
                    if captures.iter().any(|capture| capture.by_ref_local.as_deref() == Some(name))
            )
        });
    }

    /// Records that a local aliases an interior address borrowed from a live container.
    pub(crate) fn mark_borrowed_element_ref_local(&mut self, name: &str) {
        self.borrowed_element_ref_locals.insert(name.to_string());
    }

    /// Returns true when a local's cell address is borrowed from a container's payload.
    ///
    /// Such an address has no independent cell owner, so no exit path can hand it to a caller:
    /// the container may be released while the caller still holds the alias.
    ///
    /// This is a MAY analysis and deliberately only an early diagnostic. Provenance that no
    /// straight-line marker can settle, such as an alias relayed into a by-reference parameter
    /// from another function, is caught at run time by the owner-zero guard in
    /// `codegen::lower_inst::local_stores::lower_acquire_ref_cell`, which is what makes the
    /// boundary sound on every path.
    pub(crate) fn is_borrowed_element_ref_local(&self, name: &str) -> bool {
        self.borrowed_element_ref_locals.contains(name)
    }

    /// Captures the interior-alias markers so branch arms can be merged instead of overwritten.
    pub(crate) fn borrowed_element_ref_locals_snapshot(&self) -> HashSet<String> {
        self.borrowed_element_ref_locals.clone()
    }

    /// Restores the interior-alias markers captured at a branch split.
    pub(crate) fn restore_borrowed_element_ref_locals(&mut self, markers: HashSet<String>) {
        self.borrowed_element_ref_locals = markers;
    }

    /// Unions another arm's interior-alias markers into the current ones.
    ///
    /// A name bound to an array interior on ONE arm is still an interior alias after the merge,
    /// even when the other arm rebound the same name to a managed cell. Without the union the
    /// arm lowered last would silently decide the marker for both.
    pub(crate) fn merge_borrowed_element_ref_locals(&mut self, markers: &HashSet<String>) {
        for name in markers {
            self.borrowed_element_ref_locals.insert(name.clone());
        }
    }

    /// Returns whether `name` can be promoted to its own managed reference cell in place.
    ///
    /// Promotion rewrites the local's storage into a heap cell that keeps the variable's
    /// identity, which only works for an ordinary addressable PHP local of this frame.
    /// Globals, static locals, extern globals, eval-scope names and interior aliases keep
    /// storage this frame does not own.
    pub(crate) fn local_is_promotable_to_ref_cell(&self, name: &str) -> bool {
        if self.extern_global_type(name).is_some() || self.eval_scope_read_param.is_some() {
            return false;
        }
        let kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        if kind != LocalKind::PhpLocal || self.uses_global_storage(name, kind) {
            return false;
        }
        if self.is_borrowed_element_ref_local(name) {
            return false;
        }
        self.local_slots
            .get(name)
            .is_some_and(|slot| self.initialized_slots.contains(slot))
    }

    /// Returns whether type checking approved widening this exact call argument local to Mixed.
    pub(crate) fn boxed_reference_promotion_is_authorized(
        &self,
        name: &str,
        span: Span,
    ) -> bool {
        span.identifies_a_node()
            && self
                .boxed_reference_promotion_sites
                .get(&(self.loop_storage_scope.clone(), span))
                .is_some_and(|names| names.contains(name))
    }

    /// Whether the checker authorized boxing `name` for a by-reference argument anywhere in this
    /// body. The promotion widens the slot's storage to `Mixed` for the WHOLE frame, including
    /// code lowered before the promoting call, so an earlier by-reference use must already treat
    /// the slot as gradual storage.
    pub(crate) fn local_awaits_boxed_reference_promotion(&self, name: &str) -> bool {
        self.boxed_reference_promotion_sites
            .iter()
            .any(|((scope, _), names)| *scope == self.loop_storage_scope && names.contains(name))
    }

    /// Whether the checker found the local argument at `span` passed to a by-reference parameter
    /// its callee binds a property to (`Checker::property_bound_promotion_sites`).
    pub(crate) fn property_bound_promotion_is_authorized(&self, span: Span) -> bool {
        span.identifies_a_node()
            && PROPERTY_BOUND_PROMOTION_SITES
                .with(|set| set.borrow().contains(&(self.loop_storage_scope.clone(), span)))
    }

    /// Returns true when a local is currently modeled as a by-reference alias.
    pub(crate) fn is_ref_bound_local(&self, name: &str) -> bool {
        self.ref_bound_locals.contains(name)
    }

    /// Returns the hidden internal-array-pointer cursor slot for `variable`, declaring it
    /// on first use.
    ///
    /// PHP keeps the internal pointer on the hashtable itself. elephc's array and hash
    /// headers have no room for it — widening either would shift every offset in every
    /// runtime helper and inline lowering — so the cursor lives in a hidden `Int` frame
    /// slot beside the array local, named after that local so repeated calls on the same
    /// variable share one cursor.
    ///
    /// The slot is seeded with `0` at the END of the entry block, which is where PHP's
    /// freshly-created array starts its pointer. Appending there (rather than at the call
    /// site) is what makes `while (current($a) !== false) { next($a); }` work: a call-site
    /// seed inside a loop body would rewind the cursor on every iteration. The entry block
    /// stores its terminator separately from its instruction list, so appending after it
    /// has been terminated still lands before the branch and dominates every use.
    pub(crate) fn array_pointer_cursor_slot(&mut self, variable: &str) -> LocalSlotId {
        let name = Self::array_pointer_cursor_name(variable);
        if let Some(slot) = self.local_slots.get(&name) {
            return *slot;
        }
        let slot = self.declare_local_with_kind(&name, PhpType::Int, LocalKind::HiddenTemp);
        self.builder.seed_entry_int_local(slot, 0);
        self.initialized_slots.insert(slot);
        slot
    }

    /// Rewinds a local's internal-array-pointer cursor to the first element, if it has one.
    ///
    /// PHP resets the internal pointer whenever the variable is bound to a different array
    /// (`$a = [1,2,3]; next($a); $a = [4,5,6];` leaves `key($a)` at `0`), because the new
    /// value carries its own hashtable. Assignments lowered BEFORE the variable's first
    /// pointer call see no slot yet and skip this, which is harmless: the cursor is still
    /// sitting on its entry-block seed of `0` at that point.
    pub(crate) fn reset_array_pointer_cursor(&mut self, variable: &str) {
        let name = Self::array_pointer_cursor_name(variable);
        let Some(slot) = self.local_slots.get(&name).copied() else {
            return;
        };
        let zero = self.builder.emit_const_i64(0);
        self.builder.emit_store_local(slot, zero);
    }

    /// Builds the reserved frame-slot name holding one local's internal-array-pointer cursor.
    ///
    /// The `__eir_` prefix cannot appear in PHP source, so the cursor can never collide
    /// with a user variable.
    fn array_pointer_cursor_name(variable: &str) -> String {
        format!("__eir_aptr_{}", variable)
    }

    /// Declares a fresh hidden temporary slot and returns its synthetic name.
    pub(crate) fn declare_hidden_temp(&mut self, php_type: PhpType) -> String {
        let name = format!("__eir_tmp{}", self.hidden_temp_counter);
        self.hidden_temp_counter += 1;
        self.declare_local_with_kind(&name, php_type, LocalKind::HiddenTemp);
        name
    }

    /// Declares a fresh synthetic slot that follows ordinary PHP local-variable ownership.
    ///
    /// `declare_hidden_temp` uses `LocalKind::HiddenTemp`, whose store *moves* an already-owned
    /// expression result into the slot without retaining it. A lowering that desugars a PHP
    /// construct into `$tmp = <place>; ...; <place> = $tmp;` needs the opposite contract: the
    /// read may be a borrowed pointer (a static-property load carries no reference of its own),
    /// so the store must retain and function-exit cleanup must release. Declaring the slot as
    /// `LocalKind::PhpLocal` gives it exactly the ownership rules the equivalent user-written
    /// assignment would have. The `__eir_` prefix cannot appear in PHP source, so the name can
    /// never collide with a user variable.
    ///
    /// The name additionally carries `crate::names::GENERATED_LOCAL_MARKER`. The prefix alone is
    /// a convention the compiler trusts, not a rule PHP enforces, and this temporary is a
    /// `PhpLocal` precisely so its ownership matches a user assignment: without the marker it
    /// was enumerated as a PHP variable and `get_defined_vars()` reported `$__eir_place0` from
    /// any function that sorted a property. Only the name changes; the kind and every store,
    /// retain and release stay exactly as they were.
    pub(crate) fn declare_synthetic_php_local(&mut self, php_type: PhpType) -> String {
        let name = self.next_synthetic_place_local_name();
        self.declare_local_with_kind(&name, php_type, LocalKind::PhpLocal);
        name
    }

    /// Mints the next synthetic place temporary name from the shared frame counter.
    ///
    /// The single spelling site for both place temporaries, so the marker cannot be applied to
    /// one and forgotten on the other.
    pub(crate) fn next_synthetic_place_local_name(&mut self) -> String {
        let name = crate::names::synthetic_place_local_name(self.hidden_temp_counter);
        self.hidden_temp_counter += 1;
        name
    }

    /// Declares a one-shot hidden expression-result temporary.
    pub(crate) fn declare_owned_hidden_temp(&mut self, php_type: PhpType) -> String {
        let name = format!("__eir_tmp{}", self.hidden_temp_counter);
        self.hidden_temp_counter += 1;
        self.declare_local_with_kind(&name, php_type, LocalKind::OwnedTemp);
        name
    }

    /// Returns true when `IterStart` may call `IteratorAggregate::getIterator` on this source.
    ///
    /// An array source is excluded on purpose. The backend still routes a Mixed/Union-element
    /// array through its dynamic dispatch because runtime hash promotion can change the
    /// storage kind, but an array is never heap kind 4, so the aggregate case is unreachable
    /// there. The backend treats the resulting optional owner as authoritative because the
    /// operand's stored PHP type may be widened after this lowering decision.
    pub(crate) fn iter_start_needs_get_iterator_owner(&self, source_ty: &PhpType) -> bool {
        match source_ty.codegen_repr() {
            PhpType::Mixed | PhpType::Union(_) | PhpType::Iterable => true,
            PhpType::Object(name) => self.object_may_invoke_get_iterator(&name),
            _ => false,
        }
    }

    /// Publishes a zeroed Mixed owner for `getIterator()` results when the source can produce one.
    pub(crate) fn publish_iter_start_owner(
        &mut self,
        source_ty: &PhpType,
        span: Span,
    ) -> Option<LocalSlotId> {
        if !self.iter_start_needs_get_iterator_owner(source_ty) {
            return None;
        }
        let name = self.declare_owned_hidden_temp(PhpType::Mixed);
        let slot = self.local_slots[&name];
        self.emit_void(
            Op::PushCallOperandOwner,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::PushCallOperandOwner.default_effects(),
            Some(span),
        );
        Some(slot)
    }

    /// Emits `iter_start` for a source that never republishes a relocated container.
    pub(crate) fn emit_iter_start(
        &mut self,
        source: LoweredValue,
        by_ref: bool,
        span: Span,
    ) -> (LoweredValue, Option<LocalSlotId>, LocalSlotId) {
        self.emit_iter_start_with_origin(source, by_ref, None, span)
    }

    /// Emits `iter_start` after publishing its optional `getIterator()` owner.
    ///
    /// `origin` names the local slot a by-reference simple-variable source was promoted into.
    /// Recording it lets `IterNext` reload the live container after the loop body grew it or
    /// split it through copy-on-write, instead of walking the table the start captured.
    pub(crate) fn emit_iter_start_with_origin(
        &mut self,
        source: LoweredValue,
        by_ref: bool,
        origin: Option<LocalSlotId>,
        span: Span,
    ) -> (LoweredValue, Option<LocalSlotId>, LocalSlotId) {
        let source_ty = self.builder.value_php_type(source.value);
        let owner = self.publish_iter_start_owner(&source_ty, span);
        let state = self.declare_iterator_state();
        let origin = if by_ref { origin } else { None };
        let iterator = self.emit_value(
            Op::IterStart,
            vec![source.value],
            Some(Immediate::IterStart(crate::ir::IterStartMetadata::new(
                state, by_ref, owner, origin,
            ))),
            PhpType::Iterable,
            Op::IterStart.default_effects(),
            Some(span),
        );
        (iterator, owner, state)
    }

    /// Declares one addressable stack record for an iterator's cursor and private anchors.
    pub(crate) fn declare_iterator_state(&mut self) -> LocalSlotId {
        let name = format!("__eir_iterator_state_{}", self.hidden_temp_counter);
        self.hidden_temp_counter += 1;
        self.builder.add_local(
            Some(name),
            IrType::Heap(crate::ir::IrHeapKind::Iterable),
            PhpType::Iterable,
            LocalKind::IteratorState,
        )
    }

    /// Unlinks and releases a published `IterStart` `getIterator()` owner.
    pub(crate) fn retire_iter_start_owner(&mut self, slot: LocalSlotId, span: Span) {
        self.emit_void(
            Op::PopCallOperandOwner,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::PopCallOperandOwner.default_effects(),
            Some(span),
        );
        self.emit_void(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::ReleaseLocalSlot.default_effects(),
            Some(span),
        );
    }

    /// Returns true when a named object or interface type can invoke `getIterator()`.
    ///
    /// Direct `Iterator` implementations (including `Generator`) take precedence and
    /// never call `getIterator()`, so they do not need an owner slot.
    fn object_may_invoke_get_iterator(&self, type_name: &str) -> bool {
        let name = type_name.trim_start_matches('\\');
        if self.type_is_or_implements_interface(name, "Iterator") {
            return false;
        }
        self.type_is_or_implements_interface(name, "IteratorAggregate")
            || self.type_is_or_implements_interface(name, "Traversable")
    }

    /// Returns true when `type_name` is `interface_name` or implements/extends it.
    fn type_is_or_implements_interface(&self, type_name: &str, interface_name: &str) -> bool {
        let type_key = php_symbol_key(type_name);
        let interface_key = php_symbol_key(interface_name);
        if type_key == interface_key {
            return true;
        }
        if self.interfaces.contains_key(type_name) {
            return self.interface_extends(type_name, interface_name);
        }
        let mut current = Some(type_name);
        while let Some(candidate) = current {
            let Some(info) = self.classes.get(candidate) else {
                return false;
            };
            if info.interfaces.iter().any(|implemented| {
                let implemented = implemented.trim_start_matches('\\');
                php_symbol_key(implemented) == interface_key
                    || self.interface_extends(implemented, interface_name)
            }) {
                return true;
            }
            current = info
                .parent
                .as_deref()
                .map(|parent| parent.trim_start_matches('\\'));
        }
        false
    }

    /// Returns true when `interface_name` is `ancestor_name` or extends it.
    fn interface_extends(&self, interface_name: &str, ancestor_name: &str) -> bool {
        let interface_key = php_symbol_key(interface_name.trim_start_matches('\\'));
        let ancestor_key = php_symbol_key(ancestor_name.trim_start_matches('\\'));
        if interface_key == ancestor_key {
            return true;
        }
        let Some(info) = self.interfaces.get(interface_name.trim_start_matches('\\')) else {
            return false;
        };
        info.parents.iter().any(|parent| {
            let parent = parent.trim_start_matches('\\');
            php_symbol_key(parent) == ancestor_key || self.interface_extends(parent, ancestor_name)
        })
    }

    /// Ensures this function has a persistent eval context handle slot.
    ///
    /// Under `ELEPHC_EXPLAIN_EVAL_BRIDGE` each first declaration in a function names the lowering
    /// site that asked for it, since an eval context links the interpreter into the program.
    #[track_caller]
    pub(crate) fn declare_eval_context_local(&mut self) -> LocalSlotId {
        if !self.local_slots.contains_key(EVAL_CONTEXT_LOCAL_NAME)
            && std::env::var_os("ELEPHC_EXPLAIN_EVAL_BRIDGE").is_some()
        {
            let caller = std::panic::Location::caller();
            eprintln!(
                "eval-context: {} from {}:{}",
                self.owner_name(),
                caller.file(),
                caller.line()
            );
        }
        self.declare_local_with_kind(
            EVAL_CONTEXT_LOCAL_NAME,
            PhpType::Int,
            LocalKind::EvalContext,
        )
    }

    /// Ensures this function has a persistent eval scope handle slot.
    pub(crate) fn declare_eval_scope_local(&mut self) -> LocalSlotId {
        self.declare_local_with_kind(EVAL_SCOPE_LOCAL_NAME, PhpType::Int, LocalKind::EvalScope)
    }

    /// Ensures this function has a persistent eval global-scope handle slot.
    pub(crate) fn declare_eval_global_scope_local(&mut self) -> LocalSlotId {
        self.declare_local_with_kind(
            EVAL_GLOBAL_SCOPE_LOCAL_NAME,
            PhpType::Int,
            LocalKind::EvalGlobalScope,
        )
    }

    /// Applies the static part of the eval barrier to visible PHP local storage.
    #[track_caller]
    pub(crate) fn apply_eval_barrier(&mut self) {
        self.eval_barrier_active = true;
        self.invalidate_static_callable_locals();
        self.declare_eval_context_local();
        self.declare_eval_scope_local();
        self.declare_eval_global_scope_local();
        self.declare_eval_main_superglobals();
        let local_names = self
            .local_slots
            .iter()
            .filter_map(|(name, slot)| {
                let kind = self
                    .local_kinds
                    .get(name)
                    .copied()
                    .unwrap_or(LocalKind::PhpLocal);
                // `$this` cannot be rebound by eval or an include, so it keeps its class.
                (kind == LocalKind::PhpLocal
                    && name != "this"
                    && !crate::names::is_generated_local_name(name))
                .then_some((name.clone(), *slot))
            })
            .collect::<Vec<_>>();
        for (name, slot) in local_names {
            if eval_barrier_can_widen(&self.builder.local_php_type(slot)) {
                self.set_local_type(&name, PhpType::Mixed);
            }
        }
        for (name, ty) in self.local_types.clone() {
            let kind = self
                .local_kinds
                .get(&name)
                .copied()
                .unwrap_or(LocalKind::PhpLocal);
            if kind == LocalKind::PhpLocal
                && name != "this"
                && !crate::names::is_generated_local_name(&name)
                && eval_barrier_can_widen(&ty)
            {
                self.local_types.insert(name, PhpType::Mixed);
            }
        }
    }

    /// Enables direct eval-scope reads for selected variable names in an AOT eval body.
    pub(crate) fn enable_eval_scope_access(
        &mut self,
        scope_param: String,
        read_names: HashSet<String>,
        write_names: HashSet<String>,
        flush_names: BTreeSet<String>,
    ) {
        self.eval_scope_read_param = Some(scope_param);
        self.eval_scope_read_names = read_names;
        self.eval_scope_write_names = write_names;
        self.eval_scope_flush_names = flush_names;
    }

    /// Flushes selected local slots back into the eval scope before function exit.
    pub(crate) fn emit_eval_scope_finalizer(&mut self, span: Option<Span>) {
        let Some(scope_param) = self.eval_scope_read_param.clone() else {
            return;
        };
        let names = self
            .eval_scope_flush_names
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        for name in names {
            if !self.local_slots.contains_key(&name) {
                continue;
            }
            let scope = self.load_local(&scope_param, span);
            let value = self.load_local(&name, span);
            let name_data = self.intern_global_name(&name);
            self.emit_void(
                Op::EvalScopeSet,
                vec![scope.value, value.value],
                Some(Immediate::GlobalName(name_data)),
                Op::EvalScopeSet.default_effects(),
                span,
            );
        }
    }

    /// Applies the materialized local scope and widens caller slots written by EIR eval AOT.
    pub(crate) fn apply_eval_scope_barrier(&mut self, write_names: &BTreeSet<String>) {
        self.eval_barrier_active = true;
        for name in write_names {
            self.clear_static_callable_local(name);
            self.clear_reflection_class_local(name);
            self.clear_reflection_function_local(name);
            self.clear_reflection_property_local(name);
            self.clear_reflection_method_local(name);
            self.clear_reflection_arg_array_local(name);
            self.clear_fiber_start_sig(name);
        }
        self.declare_eval_scope_local();
        // Scope-sync codegen paths flush program globals into the local scope,
        // so the global-scope handle slot must exist alongside the scope slot.
        self.declare_eval_global_scope_local();
        let widen_names = write_names
            .iter()
            .filter(|name| {
                self.local_kinds.get(*name).copied() == Some(LocalKind::PhpLocal)
                    && self
                        .local_slots
                        .get(*name)
                        .is_some_and(|slot| eval_barrier_can_widen(&self.builder.local_php_type(*slot)))
            })
            .cloned()
            .collect::<Vec<_>>();
        for name in widen_names {
            self.set_local_type(&name, PhpType::Mixed);
        }
    }

    /// Ensures top-level eval fragments can see `$argc` and `$argv` by name.
    fn declare_eval_main_superglobals(&mut self) {
        if !self.in_main {
            return;
        }
        self.declare_local(EVAL_ARGC_LOCAL_NAME, PhpType::Int);
        self.mark_local_initialized(EVAL_ARGC_LOCAL_NAME);
        self.declare_local(EVAL_ARGV_LOCAL_NAME, PhpType::Array(Box::new(PhpType::Str)));
        self.mark_local_initialized(EVAL_ARGV_LOCAL_NAME);
    }

    /// Returns true after this function has lowered an `eval()` call.
    pub(crate) const fn has_eval_barrier(&self) -> bool {
        self.eval_barrier_active
    }

    /// Returns whether another dynamic include/eval frame may declare a callable function.
    pub(crate) fn can_resolve_runtime_dynamic_functions(&self) -> bool {
        RUNTIME_DYNAMIC_FUNCTION_RESOLUTION.with(Cell::get)
    }

    /// Whether the program can load PHP at run time (an unexpanded include, an `eval()`), and so
    /// meet a class or constant the closed world never saw. Code that asks the eval bridge about
    /// such names links the interpreter; a program that cannot load any keeps php's plain answer.
    pub(crate) fn program_loads_code_at_run_time(&self) -> bool {
        PROGRAM_LOADS_CODE_AT_RUN_TIME.with(Cell::get)
    }

    /// Records that an `eval()` call was lowered, even when its fragment
    /// compiled through a barrier-free AOT path.
    pub(crate) fn mark_eval_executed(&mut self) {
        self.eval_executed = true;
    }

    /// Returns true when any `eval()` call was lowered in this function.
    /// Unlike `has_eval_barrier`, this also covers barrier-free AOT evals:
    /// dynamic constant probes must consult the eval registry either way.
    pub(crate) const fn eval_executed(&self) -> bool {
        self.eval_executed
    }

    /// Returns the reusable hidden owner slot for a promoted local, declaring it if needed.
    pub(crate) fn declare_ref_cell_owner(&mut self, variable: &str, php_type: PhpType) -> LocalSlotId {
        if let Some(slot) = self.ref_cell_owner_locals.get(variable).copied() {
            self.builder.widen_local_storage_type(slot, php_type);
            return slot;
        }
        let name = format!("__eir_ref_owner{}_{}", self.hidden_temp_counter, variable);
        self.hidden_temp_counter += 1;
        let slot = self.declare_local_with_kind(&name, php_type, LocalKind::RefCell);
        self.ref_cell_owner_locals
            .insert(variable.to_string(), slot);
        slot
    }

    /// Returns the hidden owner slot for a promoted local ref-cell, if any.
    pub(crate) fn ref_cell_owner_slot(&self, variable: &str) -> Option<LocalSlotId> {
        self.ref_cell_owner_locals.get(variable).copied()
    }

    /// Returns the physical representation of a local's referenced storage.
    ///
    /// A reference may be semantically narrowed after it aliases a `Mixed` array element (notably
    /// a recursive closure assigned through `$array[]`). The hidden owner slot retains the cell's
    /// original storage contract; using the current local type would write a raw callable pointer
    /// into a cell that readers correctly interpret as a boxed Mixed value.
    fn ref_cell_storage_type(&self, variable: &str, fallback: &PhpType) -> PhpType {
        self.ref_cell_owner_slot(variable)
            .map(|slot| self.builder.local_php_type(slot))
            .unwrap_or_else(|| fallback.clone())
    }

    /// Returns the physical value representation a closure must use for a by-reference capture.
    ///
    /// A recursive closure benefits from a callable *logical* binding for call resolution, but a
    /// captured array element can still be a `Mixed` ref-cell. The hidden closure ABI carries the
    /// latter representation because its loads, stores, and cleanup operate directly on that cell.
    pub(crate) fn ref_cell_capture_storage_type(&self, variable: &str) -> PhpType {
        let fallback = self
            .local_slots
            .get(variable)
            .map(|slot| self.builder.local_php_type(*slot))
            .unwrap_or_else(|| self.local_type(variable));
        self.ref_cell_storage_type(variable, &fallback)
    }

    /// PHP-visible name of the body being lowered (`"Class::method"` for methods), as
    /// runtime error messages spell it.
    pub(crate) fn owner_name(&self) -> &str {
        &self.owner_name
    }

    /// Returns a deterministic EIR function name for the next closure literal in this body.
    pub(crate) fn next_closure_name(&mut self) -> String {
        let name = format!(
            "__eir_closure_{}_{}",
            closure_name_fragment(&self.owner_name),
            self.closure_counter
        );
        self.closure_counter += 1;
        name
    }

    /// Returns true when the body being lowered is the get or set hook accessor for `property`.
    ///
    /// `owner_name` is `"Class::method"` for a method body, so this compares the method part against
    /// the synthetic accessor names. Inside a property's own accessor, `$this->property` must read or
    /// write the raw backing slot rather than re-entering the accessor (which would recurse).
    pub(crate) fn in_own_property_accessor(&self, property: &str) -> bool {
        let Some((_, method)) = self.owner_name.split_once("::") else {
            return false;
        };
        method == property_hook_get_method(property) || method == property_hook_set_method(property)
    }

    /// Appends closure functions discovered while lowering expressions in this body.
    pub(crate) fn extend_closures(&mut self, closures: impl IntoIterator<Item = Function>) {
        self.closures.extend(closures);
    }

    /// Records where the closure literal lowered as `name` sits: this body and its span.
    pub(crate) fn set_closure_origin(&mut self, name: &str, span: Span) {
        let owner = self.owner_name.clone();
        if let Some(closure) = self.closures.iter_mut().find(|closure| closure.name == name) {
            closure.closure_origin = Some(crate::ir::ClosureOrigin { owner, span });
        }
    }

    /// Returns a closure function accumulated while lowering this body.
    ///
    /// Static `Closure::bind` specialization clones the function so an explicit PHP scope can
    /// change member visibility without mutating the original closure or weakening property
    /// access for unrelated calls.
    pub(crate) fn closure_function(&self, name: &str) -> Option<&Function> {
        self.closures.iter().find(|closure| closure.name == name)
    }

    /// Returns closure functions accumulated in this body once lowering has finished.
    pub(crate) fn into_closures(self) -> Vec<Function> {
        self.closures
    }

    /// Records that the expression just lowered produced a statically known callable.
    pub(crate) fn set_pending_static_callable_result(&mut self, target: StaticCallableBinding) {
        self.pending_static_callable_result = Some(target);
    }

    /// Takes any statically known callable result recorded by the last direct expression.
    pub(crate) fn take_pending_static_callable_result(&mut self) -> Option<StaticCallableBinding> {
        self.pending_static_callable_result.take()
    }

    /// Records the names this body writes through a non-integer array key.
    ///
    /// Called once, before the body's statements are lowered, because
    /// [`Self::required_static_local_storage_type`] has to answer at the `static` DECLARATION —
    /// the point where the slot's type is fixed for the whole program.
    pub(crate) fn record_non_integer_keyed_locals(&mut self, body: &[crate::parser::ast::Stmt]) {
        self.non_integer_keyed_locals =
            crate::append_vivify::locals_written_with_non_integer_keys(body);
        self.ref_capture_written_locals = crate::ast_usage::ref_capture_writes(body);
    }

    /// Returns whether some closure in this body captures `name` by reference and writes it.
    pub(crate) fn is_ref_capture_written_local(&self, name: &str) -> bool {
        self.ref_capture_written_locals.contains(name)
    }

    /// Records that a bound closure's `$this` is a receiver this build cannot name.
    ///
    /// The closure still captures the enclosing `$this` value — the runtime bind overwrites it —
    /// but the capture is typed `Mixed` so the body dispatches members at run time, exactly as a
    /// top-level closure's null `$this` capture already does.
    pub(crate) fn set_bound_closure_this_gradual(&mut self) {
        self.bound_closure_this_gradual = true;
    }

    /// Takes the gradual-`$this` flag recorded for the bound closure being lowered.
    pub(crate) fn take_bound_closure_this_gradual(&mut self) -> bool {
        std::mem::take(&mut self.bound_closure_this_gradual)
    }

    /// Records the class of the receiver that a bound closure will expose as `$this`.
    pub(crate) fn set_bound_closure_this_class(&mut self, class_name: String) {
        self.bound_closure_this_class = Some(class_name);
    }

    /// Takes and clears the pending bound-closure receiver class so it cannot leak to another body.
    pub(crate) fn take_bound_closure_this_class(&mut self) -> Option<String> {
        self.bound_closure_this_class.take()
    }

    /// Clears stale callable-result metadata before lowering a new independent expression.
    pub(crate) fn clear_pending_static_callable_result(&mut self) {
        self.pending_static_callable_result = None;
    }

    /// Emits a load from a PHP local slot.
    /// Loads a PHP local using its current semantic type and the slot's stable frame contract.
    /// Returns the IR type `load_local(name)` would produce now, without emitting the load.
    ///
    /// A path choice taken from a real load leaves that load behind once the chosen path loads
    /// the container again, and a load that unboxes a Mixed-storage slot holds a reference of
    /// its own. Callers that only need the representation peek instead. A by-reference
    /// `callable` cell read as Mixed is not modelled: no array path asks about callables.
    pub(crate) fn peek_local_load_ir_type(&self, name: &str) -> IrType {
        if let Some(php_type) = self.extern_global_type(name) {
            return value_ir_type(&php_type);
        }
        if self.should_load_from_eval_scope(name) {
            return IrType::Heap(crate::ir::IrHeapKind::Mixed);
        }
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, kind);
        let php_type = if uses_global {
            self.global_alias_type(name)
        } else {
            self.local_type(name)
        };
        let php_type = match (kind, self.local_slots.get(name)) {
            (LocalKind::StaticLocal, Some(slot)) if !uses_global => {
                let slot_type = self.builder.local_php_type(*slot);
                if slot_type.codegen_repr() != php_type.codegen_repr() {
                    slot_type
                } else {
                    php_type
                }
            }
            _ => php_type,
        };
        if !uses_global && matches!(php_type.codegen_repr(), PhpType::Void) {
            return IrType::I64;
        }
        value_ir_type(&php_type)
    }

    pub(crate) fn load_local(&mut self, name: &str, span: Option<Span>) -> LoweredValue {
        if let Some(php_type) = self.extern_global_type(name) {
            return self.load_extern_global(name, php_type, span);
        }
        if self.should_load_from_eval_scope(name) {
            return self.load_eval_scope_name(name, span);
        }
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, kind);
        let php_type = self.slot_load_php_type(name, uses_global);
        // A static's slot is ONE storage for the whole function, reached from every path: a
        // body lowered again as a `goto` target starts with no flow facts, and read the hash
        // behind `static $cache = []` as a Mixed cell (Twig's `CoreExtension::getAttribute()`
        // jumps to `methodCheck:` and memoizes there). The slot's own type is the one fact that
        // holds on every path.
        let php_type = match (kind, self.local_slots.get(name)) {
            (LocalKind::StaticLocal, Some(slot)) if !uses_global => {
                let slot_type = self.builder.local_php_type(*slot);
                if slot_type.codegen_repr() != php_type.codegen_repr() {
                    slot_type
                } else {
                    php_type
                }
            }
            _ => php_type,
        };
        if !uses_global && php_type == PhpType::Void && !self.local_slots.contains_key(name) {
            // Probing an abandoned binding must not allocate a null slot that later widens
            // a fresh string capture to Mixed. The absent local is already known to be null.
            return self.emit_value(
                Op::ConstNull, Vec::new(), None, PhpType::Void,
                Op::ConstNull.default_effects(), span,
            );
        }
        let slot = self.declare_local(name, php_type.clone());
        let is_ref_bound = self.is_ref_bound_local(name) && !uses_global && kind == LocalKind::PhpLocal;
        // Read a by-reference `callable` through its cell's REPRESENTATION, the same one
        // `store_local` writes. A closure capturing `use (&$c)` before `$c` is assigned boxes the
        // cell; loading it back under the flow type `callable` read the box pointer as a
        // descriptor. A call on the Mixed value dispatches dynamically, as the closure body's own
        // read of the capture already does.
        let php_type = if is_ref_bound
            && !self.is_interior_ref_local(name)
            && matches!(php_type.codegen_repr(), PhpType::Callable)
            && matches!(
                self.ref_cell_storage_type(name, &self.builder.local_php_type(slot))
                    .codegen_repr(),
                PhpType::Mixed
            ) {
            PhpType::Mixed
        } else {
            php_type
        };
        let ir_type = value_ir_type(&php_type);
        let op = match (is_ref_bound, uses_global, kind) {
            (true, _, _) => Op::LoadRefCell,
            (false, true, _) => Op::LoadGlobal,
            (false, false, LocalKind::StaticLocal) => Op::LoadStaticLocal,
            _ => Op::LoadLocal,
        };
        // Reference-cell reads borrow their pointee. Append normalization chooses the
        // final boxed representation without changing other reference payload contracts.
        let ownership = if op == Op::LoadRefCell
            && Ownership::php_type_needs_lifetime_tracking(&php_type)
        {
            Ownership::Borrowed
        } else {
            Ownership::for_php_type(&php_type)
        };
        let immediate = if uses_global {
            Some(Immediate::GlobalName(self.intern_global_storage_name(name)))
        } else {
            Some(Immediate::LocalSlot(slot))
        };
        let value = self
            .builder
            .emit_with_effects(
                op,
                Vec::new(),
                immediate,
                ir_type,
                php_type,
                ownership,
                op.default_effects(),
                span,
            )
            .expect("load_local produces a value");
        LoweredValue { value, ir_type }
    }

    /// Returns the PHP type `load_local` stamps on a read of `name`, without emitting the read.
    ///
    /// `None` means the read is not a frame, reference-cell or global slot load (an extern
    /// global or an eval-scope name). A lowering that picks its shape from the loaded
    /// representation before it evaluates anything asks here, so its choice is the type of the
    /// load that follows.
    pub(crate) fn local_load_php_type(&self, name: &str) -> Option<PhpType> {
        if self.extern_global_type(name).is_some() || self.should_load_from_eval_scope(name) {
            return None;
        }
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        Some(self.slot_load_php_type(name, self.uses_global_storage(name, kind)))
    }

    /// Selects the representation of a slot or global load for `load_local` and its probe.
    fn slot_load_php_type(&self, name: &str, uses_global: bool) -> PhpType {
        // Under `--web`, superglobals carry a fixed `AssocArray{Str, Mixed}` type
        // in every scope. Ordinary globals remain boxed Mixed cells.
        if uses_global {
            self.global_alias_type(name)
        } else if self.is_final_mixed_local(name) && !self.is_ref_bound_local(name) {
            // Replay fixed the final representation. Preserve its actual runtime tag
            // instead of truncating a promoted float to the entry integer type.
            PhpType::Mixed
        } else {
            self.local_type(name)
        }
    }

    /// Returns true when a variable read should be sourced from the eval scope handle.
    fn should_load_from_eval_scope(&self, name: &str) -> bool {
        let Some(scope_param) = &self.eval_scope_read_param else {
            return false;
        };
        name != scope_param
            && !crate::globals_array::is_alias(name)
            && (self.eval_scope_read_names.contains(name)
                || self.eval_scope_write_names.contains(name))
    }

    /// Emits an `EvalScopeGet` for a selected eval-scope variable read.
    fn load_eval_scope_name(&mut self, name: &str, span: Option<Span>) -> LoweredValue {
        let scope_param = self
            .eval_scope_read_param
            .clone()
            .expect("eval scope read mode has a scope parameter");
        let scope = self.load_local(&scope_param, span);
        let name_data = self.intern_global_name(name);
        let value = self
            .builder
            .emit_with_effects(
                Op::EvalScopeGet,
                vec![scope.value],
                Some(Immediate::GlobalName(name_data)),
                IrType::Heap(crate::ir::IrHeapKind::Mixed),
                PhpType::Mixed,
                Ownership::Borrowed,
                Op::EvalScopeGet.default_effects(),
                span,
            )
            .expect("eval_scope_get produces a Mixed value");
        LoweredValue {
            value,
            ir_type: IrType::Heap(crate::ir::IrHeapKind::Mixed),
        }
    }

    /// Returns true when a variable write should be stored into the eval scope handle.
    fn should_store_to_eval_scope(&self, name: &str) -> bool {
        let Some(scope_param) = &self.eval_scope_read_param else {
            return false;
        };
        name != scope_param && !crate::globals_array::is_alias(name)
            && self.eval_scope_write_names.contains(name)
    }

    /// Emits an `EvalScopeSet` for a selected eval-scope variable write.
    fn store_eval_scope_name(
        &mut self,
        name: &str,
        value: LoweredValue,
        span: Option<Span>,
    ) -> LoweredValue {
        let php_type = self.builder.value_php_type(value.value).codegen_repr();
        let previous_slot = self.local_slots.get(name).copied();
        let previous_kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        let scope_param = self
            .eval_scope_read_param
            .clone()
            .expect("eval scope write mode has a scope parameter");
        let scope = self.load_local(&scope_param, span);
        let name_data = self.intern_global_name(name);
        self.emit_void(
            Op::EvalScopeSet,
            vec![scope.value, value.value],
            Some(Immediate::GlobalName(name_data)),
            Op::EvalScopeSet.default_effects(),
            span,
        );
        let slot = self.declare_local(name, php_type.clone());
        self.builder
            .widen_local_storage_type(slot, php_type.clone());
        // Retain before cleanup because a borrowed result can alias the old slot.
        let stored = crate::ir_lower::ownership::acquire_if_refcounted(self, value, span);
        if local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_some_and(|slot| self.initialized_slots.contains(&slot))
        {
            self.release_stored_local_value(name, slot, span);
        }
        if local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_some_and(|slot| !self.initialized_slots.contains(&slot))
            && !self.loop_stack.is_empty()
        {
            self.release_stored_local_value(name, slot, span);
        }
        self.store_slot_with_op(slot, stored, Op::StoreLocal, span);
        self.set_local_type(name, php_type);
        if self.value_needs_release_after_retaining_store(value) {
            crate::ir_lower::ownership::release_if_owned(self, value, span);
        }
        stored
    }

    /// Emits a load using the local slot's concrete frame-storage type.
    ///
    /// This is for cleanup paths that must release the value already present in
    /// a slot. Normal expression reads should use `load_local`, which preserves
    /// the narrower logical type facts from the checker.
    fn load_local_storage(
        &mut self,
        name: &str,
        slot: LocalSlotId,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        let ir_type = value_ir_type(&php_type);
        // This load exists specifically to release the owner held by the slot;
        // mark it explicitly so finalization does not mistake it for a deferred
        // borrowed expression load when the storage stays concrete.
        let ownership = Ownership::Owned;
        let kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, kind);
        let is_ref_bound =
            self.is_ref_bound_local(name) && !uses_global && kind == LocalKind::PhpLocal;
        let op = match (is_ref_bound, uses_global, kind) {
            (true, _, _) => Op::LoadRefCell,
            (false, true, _) => Op::LoadGlobal,
            (false, false, LocalKind::StaticLocal) => Op::LoadStaticLocal,
            _ => Op::LoadLocal,
        };
        let immediate = if uses_global {
            Some(Immediate::GlobalName(self.intern_global_storage_name(name)))
        } else {
            Some(Immediate::LocalSlot(slot))
        };
        let value = self
            .builder
            .emit_with_effects(
                op,
                Vec::new(),
                immediate,
                ir_type,
                php_type,
                ownership,
                op.default_effects(),
                span,
            )
            .expect("storage-typed local load produces a value");
        LoweredValue { value, ir_type }
    }

    /// Releases the value currently stored in a local slot using frame-storage metadata.
    pub(crate) fn release_stored_local_value(
        &mut self,
        name: &str,
        slot: LocalSlotId,
        span: Option<Span>,
    ) {
        // Global storage is released by the store itself: `StoreGlobal` drops the symbol's
        // previous occupant before writing the new one. Releasing it here as well freed it twice
        // -- `unset($_ENV[$k])` loaded `$_ENV`, released it, then stored the edited copy, whose
        // store released the same array again. The freed block was then handed out anew while the
        // second release still wrote into it, and an unrelated live string took its place.
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        if self.uses_global_storage(name, kind) {
            return;
        }
        // A by-reference local can retain a narrower logical type after its backing cell was
        // promoted to Mixed storage. Cleanup must use that physical cell contract, just like
        // loads and stores, or it can pass a Mixed box to the callable/object destructor.
        let slot_type = self.builder.local_php_type(slot);
        let storage_type = self.ref_cell_storage_type(name, &slot_type);
        if !Ownership::php_type_needs_lifetime_tracking(&storage_type) {
            return;
        }
        let previous = self.load_local_storage(name, slot, storage_type, span);
        crate::ir_lower::ownership::release_if_owned(self, previous, span);
    }

    /// Releases the slot's occupant unless it is the very value the store is about to keep.
    ///
    /// Comparable single-pointer payloads only, the same restriction the argument side applies:
    /// a boxed Mixed that WRAPS a container holds a different pointer than the container, so
    /// comparing them would read "not aliased" for a value the store does keep.
    fn release_stored_local_value_unless_aliases(
        &mut self,
        name: &str,
        slot: LocalSlotId,
        stored: LoweredValue,
        span: Option<Span>,
    ) {
        let slot_type = self.builder.local_php_type(slot);
        let storage_type = self.ref_cell_storage_type(name, &slot_type);
        if !Ownership::php_type_needs_lifetime_tracking(&storage_type) {
            return;
        }
        let stored_repr = self.builder.value_php_type(stored.value).codegen_repr();
        let slot_repr = storage_type.codegen_repr();
        if !matches!(stored_repr, PhpType::Mixed | PhpType::Union(_))
            || !matches!(slot_repr, PhpType::Mixed | PhpType::Union(_))
        {
            self.release_stored_local_value(name, slot, span);
            return;
        }
        let previous = self.load_local_storage(name, slot, storage_type, span);
        self.emit_void(
            Op::ReleaseUnlessAliases,
            vec![previous.value, stored.value],
            None,
            Op::ReleaseUnlessAliases.default_effects(),
            span,
        );
    }

    /// Releases the previous occupant immediately before a retaining store overwrites it.
    ///
    /// The caller must first retain the incoming value because borrowing operations
    /// can return storage that aliases the previous occupant (for example,
    /// `$value = trim($value)`). When the slot's storage type already needs lifetime
    /// tracking this emits a slot retirement that clears the owner before release.
    /// This prevents exceptional frame cleanup from revisiting storage freed by a
    /// throwing destructor. When it does not, the slot can
    /// STILL be widened to refcounted storage by a store lowered later that reaches
    /// this one through a loop back-edge (e.g. an inner `for` counter re-initialized
    /// by the outer body but widened Int→Mixed by its checked-add update). An eval barrier
    /// can similarly restore a future local before its first syntactic store, whose type
    /// may be widened only by a later store. The storage type visible here is stale in
    /// either case, so a deferred `release_local_slot` is emitted: the backend releases
    /// the occupant using the final widened storage type, and
    /// `prune_untracked_release_local_slot_ops` erases the op when the slot never widens
    /// (issue #534: without this, the previous outer iteration's Mixed box leaked on every
    /// re-initialization).
    ///
    /// The deferred op also covers issue #479, which main fixed by deferring only inside loops
    /// for storage that can still widen: a slot typed `Object("Shape")` widened to `Mixed` by a
    /// later store of a different class (`$s = make(); $s = new Sq();`, or a `catch (\Throwable
    /// $e)` reassigned to a `TypeError`) made the eager, concretely typed load an
    /// `__rt_mixed_unbox` + retain whose release cancelled only that retain and never freed the
    /// BOX. Retiring the slot at its final storage type frees the box and the payload it pins,
    /// in and out of loops alike, which is why no loop or widenability guard is needed here.
    ///
    /// Returns whether a `release_local_slot` was emitted. Its backend retirement stores zero
    /// over a boxed `Mixed` slot before dropping the reference, which is what lets an abandoned
    /// numeric binding skip its separate `zero_local_slot` (see
    /// `release_and_abandon_local_binding`).
    fn release_stored_local_value_before_overwrite(
        &mut self,
        name: &str,
        slot: LocalSlotId,
        storage_type: &PhpType,
        span: Option<Span>,
    ) -> bool {
        // A ref-bound slot stores an alias, not the payload owner being replaced.
        // StoreRefCell publishes the replacement through that alias before it retires
        // the previous payload, so generic slot cleanup must never release it first.
        if self.is_ref_bound_local(name) {
            return false;
        }
        // Whether the eager form of this release applied: storage that is tracked, a body inside a
        // loop, or a slot an eval reload may have populated. The caller skips its own zeroing only
        // in that case.
        let tracked = Ownership::php_type_needs_lifetime_tracking(storage_type);
        let eval_may_have_reloaded_slot = self.eval_barrier_active
            && self.builder.local_kind(slot) == LocalKind::PhpLocal;
        let released_eagerly = tracked || !self.loop_stack.is_empty() || eval_may_have_reloaded_slot;
        // No early exit for storage that is untracked SO FAR, even outside loops: a LATER store in
        // straight-line code can still widen the slot (`$n = 0; $n += strlen($a); $n +=
        // strlen($b);` boxes `$n` only at the second `+=`), and the first `+=` then overwrote the
        // box `$n = 0` became without releasing it, one block per call.
        // `prune_untracked_release_local_slot_ops` erases the op when the slot never widens.
        self.emit_void(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::ReleaseLocalSlot.default_effects(),
            span,
        );
        released_eagerly
    }

    /// Releases the previous occupant before a store, when the stored value is known.
    ///
    /// `$acc = f($acc, ...)`: when the callee hands one of its parameters back, the value about
    /// to be stored IS the occupant being released. Releasing it deeply frees the payload the
    /// store is keeping (Symfony's `Config\Definition\Processor::process()` died in
    /// `_rt_mixed_free_deep` through `BaseNode::merge()`, whose tail is
    /// `return $this->mergeValues($l, $r);`), so that shape releases only unless the two alias.
    fn release_stored_local_value_before_overwrite_from(
        &mut self,
        name: &str,
        slot: LocalSlotId,
        storage_type: &PhpType,
        stored: LoweredValue,
        span: Option<Span>,
    ) {
        if Ownership::php_type_needs_lifetime_tracking(storage_type)
            && !self.is_ref_bound_local(name)
            && self
                .call_argument_read_from_slot(stored.value, slot)
                .is_some()
        {
            self.release_stored_local_value_unless_aliases(name, slot, stored, span);
            return;
        }
        self.release_stored_local_value_before_overwrite(name, slot, storage_type, span);
    }

    /// Emits a store to a PHP local slot, updates type facts, and returns the stored value.
    /// Stores an ordinary PHP value and widens the frame slot for every runtime representation
    /// the assignment can introduce.
    pub(crate) fn store_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        self.store_local_impl(name, value, php_type, span, false, true)
    }

    /// Stores the null written by `unset` without changing the slot contract used by earlier
    /// instructions in the same function.
    ///
    /// Except in a body that calls `eval()`: the fragment reads caller locals by name and boxes a
    /// concrete slot's payload, so a `Str` slot holding the unset sentinel reached it as `""` and
    /// `unset($a); eval('echo isset($a) ? "set" : "unset";')` printed `set`. There the null widens
    /// the slot to `Mixed`, as an ordinary null store does, and the fragment sees a real null.
    fn store_unset_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        span: Option<Span>,
    ) -> LoweredValue {
        let preserve_storage_type = !self.body_contains_eval;
        self.store_local_impl(name, value, PhpType::Void, span, preserve_storage_type, true)
    }

    /// Replaces a call argument's representation without resetting its PHP internal array pointer.
    /// Unlike consuming array conversions, boxing retains the old payload and needs normal cleanup.
    pub(crate) fn store_call_argument_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        self.store_local_impl(name, value, php_type, span, false, false)
    }

    /// Implements local stores. `preserve_storage_type` lets `unset` keep an already allocated frame
    /// representation (ordinary null assignments instead require discriminated Mixed storage);
    /// `reset_array_cursor` rewinds the hidden array cursor for a PHP-visible assignment.
    fn store_local_impl(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
        preserve_storage_type: bool,
        reset_array_cursor: bool,
    ) -> LoweredValue {
        if self.append_update_cell_aliases.contains(name) {
            return crate::ir_lower::expr::store_append_update_cell(self, name, value, span);
        }
        // A resolver-preserved include executes through Magician and always returns a boxed
        // runtime cell. The checker can infer an `array` from its known consumer, but storing
        // that cell in an array-shaped frame slot would make later lowering treat the Mixed
        // object pointer as a raw container pointer. Keep the value gradual until a concrete
        // call/property boundary emits its checked `MixedToHash`/unbox conversion.
        let php_type = if matches!(
            self.builder.value_defining_instruction(value.value)
                .and_then(|inst| inst.immediate.as_ref()),
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::DynamicInclude { .. }
            ))
        ) {
            PhpType::Mixed
        } else {
            php_type
        };
        self.clear_static_callable_local(name);
        self.clear_reflection_class_local(name);
        self.clear_reflection_function_local(name);
        self.clear_reflection_property_local(name);
        self.clear_reflection_method_local(name);
        self.clear_reflection_arg_array_local(name);
        self.clear_fiber_start_sig(name);
        if let Some(extern_type) = self.extern_global_type(name) {
            let release_source_after_store = self.value_is_owning_temporary(value);
            self.store_extern_global_name(name, value, span);
            self.set_local_type(name, extern_type);
            if release_source_after_store {
                crate::ir_lower::ownership::release_if_owned(self, value, span);
            }
            return value;
        }
        if self.should_store_to_eval_scope(name) {
            return self.store_eval_scope_name(name, value, span);
        }
        // Binding the variable to a different array gives it a different hashtable, and
        // PHP's internal pointer belongs to the hashtable: rewind the hidden cursor so
        // `$a = [1,2,3]; next($a); $a = [4,5,6];` leaves `key($a)` at `0` like PHP.
        if reset_array_cursor {
            self.reset_array_pointer_cursor(name);
        }
        let previous_slot = self.local_slots.get(name).copied();
        let previous_type = self.local_type(name);
        let previous_kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, previous_kind);
        let php_type = if uses_global {
            self.global_alias_type(name)
        } else if previous_kind == LocalKind::PhpLocal {
            // A `string` local PHP's `++`/`--` can retype uses boxed Mixed storage from its
            // FIRST store, so no read of the slot is ever typed `Str` against boxed storage.
            self.required_local_storage_type(name, php_type)
        } else {
            php_type
        };
        let mut local_type_after_store = if !preserve_storage_type
            && matches!(php_type.codegen_repr(), PhpType::Void)
        {
            // Preserve the runtime distinction between a concrete value and an ordinary null
            // assignment at later joins. `unset` deliberately remains semantically Void.
            PhpType::Mixed
        } else {
            php_type.clone()
        };
        let slot = self.declare_local(name, php_type.clone());
        let storage_before_store = self.builder.local_php_type(slot).codegen_repr();
        // Backend frame layout uses the final widened slot type for every load
        // and store, so cleanup loads must be typed after this store's widening.
        // For ref-bound locals, keep the existing slot type to avoid widening
        // Int→Mixed mid-function (which would break earlier loads that expect I64).
        // The codegen narrows Mixed→Int at the store point instead.
        let is_ref_bound = self.is_ref_bound_local(name);
        // The cell's REPRESENTATION, not the name's flow type. A local captured `use (&$c)` before
        // it is assigned has a boxed Mixed cell, and its flow type then says `callable` -- which
        // made `$c = static function () use (&$c) {…}` store the raw descriptor pointer into the
        // box. The enclosing function called it fine through the same flow type, and every reader
        // of the cell as a VALUE read the descriptor's first word as a type tag: inside a function
        // body, php 8.5.10 answers `object` for `gettype($c)` and `call_user_func($c, …)` works,
        // elephc answered `string` and `Call to undefined function <dynamic>()`. Symfony's
        // `UrlGenerator::doGenerate()` passes exactly such a `$caster` on to
        // `array_walk_recursive()`. At top level the same program worked, because a global's slot
        // is not ref-bound this way.
        // NOT for a `foreach` value reference: its slot may be typed Mixed while the cell it holds
        // points INTO an `array<int>`, whose element is a raw word (`is_interior_ref_local`).
        let slot_storage_type = self.builder.local_php_type(slot);
        let ref_cell_fallback = if let Some(element) = self.interior_ref_locals.get(name) {
            element.clone()
        } else if matches!(slot_storage_type.codegen_repr(), PhpType::Mixed) {
            slot_storage_type
        } else {
            previous_type.clone()
        };
        // An interior reference's element type is authoritative even over a ref-cell OWNER slot:
        // the owner describes a cell an earlier binding of the same name created, and a later
        // `foreach ($b as &$v)` rebinds `$v` into a different array's raw element.
        let ref_cell_storage_type = if let Some(element) = self.interior_ref_locals.get(name) {
            element.clone()
        } else if is_ref_bound {
            self.ref_cell_storage_type(name, &ref_cell_fallback)
        } else {
            previous_type.clone()
        };
        let widen_type = if is_ref_bound || preserve_storage_type {
            previous_type.clone()
        } else if matches!(php_type.codegen_repr(), PhpType::Void) {
            // A normal `$value = null` is observable on paths where the same slot holds a
            // concrete value. Unlike `unset`, it therefore needs a runtime tag rather than the
            // concrete representation's null-like payload sentinel.
            PhpType::Mixed
        } else {
            php_type.clone()
        };
        // What the slot will be typed AFTER this store: the release decisions below follow it,
        // because a slot becoming `Mixed` needs its cleanup even when this value is an int.
        let widened_storage_type = self.builder.widened_local_php_type(slot, &widen_type);
        let source = value;
        let source_is_owning_temporary = self.value_is_owning_temporary(value);
        let transfer_catch_source_to_store = matches!(
            self.builder.value_defining_op(value.value),
            Some(Op::CatchBind)
        ) && previous_kind != LocalKind::StaticLocal;
        let release_source_after_store =
            self.value_needs_release_after_retaining_store(value)
                && !matches!(previous_kind, LocalKind::HiddenTemp | LocalKind::OwnedTemp)
                && !transfer_catch_source_to_store;
        let transfer_callable_source_to_store = source_is_owning_temporary
            && matches!(php_type.codegen_repr(), PhpType::Callable);
        let transfer_source_to_store =
            transfer_callable_source_to_store || transfer_catch_source_to_store;
        // A `static` local outlives the frame, so a store into one must take its OWN
        // reference. The backend's `lower_store_static_local` already does that via
        // `emit_incref_if_refcounted`, but `PhpType::is_refcounted()` does NOT list
        // `PhpType::Str` — so that incref is a silent no-op for STRINGS specifically, while
        // `release_source_after_store` below still fires. `static $s = ""; $s = f($x);`
        // therefore stored the callee's buffer and then freed it, leaving the static
        // pointing at freed memory that rendered as whatever the allocator handed out next.
        //
        // Retain here for the string case ONLY. Widening this to every static would
        // DOUBLE-count Mixed/Array/Object statics, which the backend already increfs
        // (`static $n = 0; $n = $n + 1;` widens to Mixed through `ichecked_add` and
        // regressed `test_http_response_path_leaves_no_live_heap_blocks` that way).
        // The paired release of the PREVIOUS occupant is emitted below.
        let static_local_store_needs_string_retain = previous_kind == LocalKind::StaticLocal
            && matches!(previous_type.codegen_repr(), PhpType::Str)
            && matches!(
                self.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Str
            );
        let store_retains_value = uses_global
            || previous_kind == LocalKind::PhpLocal
            || static_local_store_needs_string_retain;
        // Retain before cleanup because a borrowed result can alias the old slot.
        let value = if store_retains_value
            && !transfer_source_to_store
            && !self.is_ref_bound_local(name)
        {
            crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
        } else if store_retains_value && !transfer_source_to_store {
            // For ref-bound locals, acquire only when NOT narrowing Mixed→Int.
            // When the source is Mixed and the ref cell's previous type is Int,
            // the ref cell store narrows via __rt_mixed_cast_int, consuming the
            // Mixed box. The release_if_owned at the end frees the original
            // without a paired incref, which is correct for narrowing.
            let source_is_mixed = matches!(
                self.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed
            );
            let target_is_int = matches!(ref_cell_storage_type.codegen_repr(), PhpType::Int);
            // The narrowing CONSUMES the box it converts, so it must be handed one it owns. An
            // owned temporary already is; a BORROWED read -- `$v = $tmp` from another local -- is
            // not, and consuming it freed the box that local still held: the next store to it
            // released it again. Acquiring first gives the narrowing its own share.
            if !(source_is_mixed && target_is_int) || !self.value_is_owning_temporary(value) {
                crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
            } else {
                value
            }
        } else {
            value
        };
        if !uses_global
            && local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_some_and(|slot| self.initialized_slots.contains(&slot))
        {
            self.release_stored_local_value_before_overwrite_from(name, slot, &widened_storage_type, source, span);
        }
        // A loop-carried slot can exist globally without being definitely initialized
        // on this CFG path. An earlier eval barrier can likewise reload a future PHP local
        // before lowering reaches its first syntactic store. Release either runtime occupant
        // before overwriting it.
        if !uses_global
            && local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_some_and(|slot| !self.initialized_slots.contains(&slot))
            && (!self.loop_stack.is_empty()
                || (previous_kind == LocalKind::PhpLocal && self.eval_barrier_active))
        {
            self.release_stored_local_value_before_overwrite_from(name, slot, &widened_storage_type, source, span);
        }
        // A first syntactic store inside a loop body (main or function) can still
        // overwrite a prior runtime iteration's value. The same is true after eval, whose
        // backend inventory can reload a future PHP local before this store exists in EIR.
        // Function cleanup locals (including returned slots) are zero-initialized in the
        // prologue, so paths with no prior runtime occupant safely release a null slot.
        if !uses_global
            && local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_none()
            && (!self.loop_stack.is_empty()
                || (previous_kind == LocalKind::PhpLocal && self.eval_barrier_active))
        {
            self.release_stored_local_value_before_overwrite_from(name, slot, &widened_storage_type, source, span);
        }
        // NO release of the previous occupant is emitted here for the string case: the
        // BACKEND already does it. `lower_store_static_local` calls
        // `emit_store_result_to_symbol(.., release_previous = true)`, whose `PhpType::Str`
        // arm loads the symbol's current payload and calls `__rt_heap_free_safe` before
        // overwriting it. Emitting a second release here is a DOUBLE FREE — it survived
        // macOS/aarch64 (where the redundant free was absorbed) and was caught by the
        // heap-debug double-free guard on linux-x86_64.
        //
        // So the two layers split cleanly for a static string store: this layer owns the
        // RETAIN (the backend's `emit_incref_if_refcounted` skips `Str`), the backend owns
        // the RELEASE of what it overwrites.
        //
        // The slot widens only AFTER the releases above were emitted, and the two sides read
        // different types on purpose. WHETHER a release is emitted follows the widened storage
        // type (`widened_storage_type` above): a slot that becomes `Mixed` needs its cleanup
        // even when this store's own value is an int. WHAT that release is analysed as follows
        // the slot's type at emission, which still describes what the slot HOLDS: a `string`
        // local being overwritten with `null` retires a string, not the `Mixed` the widened slot
        // will carry. Widening first made that release look like a possible destructor and
        // erased every global-backed closure fact in the frame. The backend lays the frame out
        // from the final widened type either way.
        self.builder.widen_local_storage_type(slot, widen_type);
        // A store that WIDENED an existing slot to a different runtime representation makes the
        // slot boxed for the rest of the frame, and the name's logical type has to say so from
        // here on.
        //
        // `$i = \strlen('ab'); if (…) { $i = 'si'; } echo $i;` is the shape. The store inside the
        // branch widens `$i`'s slot `int` -> boxed `Mixed`; leaving its LOGICAL type at the value's
        // own `string` (or, past a merge that restores an older snapshot, back at `int`) makes a
        // later read ask for a representation the slot no longer holds unconditionally, and
        // `__rt_mixed_cast_int` on the boxed `"si"` printed `0` where PHP prints `si`. The same
        // stale fact leaked out of every construct that has no type join of its own: a `try` whose
        // callee threw before the store, a zero-trip `for`/`foreach`, a ternary or `&&` arm.
        //
        // Recording the WIDENING itself, rather than patching each construct's exit, is what makes
        // the answer construct-independent: from this store onwards `$i` reads as the slot's
        // storage type wherever it is read, exactly like a name the checker's `mixed_storage_scan`
        // marked (whose contract is the same boxed slot, forced from its FIRST store) and like
        // `string_incdec_locals`. Narrowing back to a concrete value is still available where it is
        // sound — a guard's `is_string($i)` unboxes the LOADED value inside the branch, which is
        // where the runtime tag check belongs.
        //
        // Only a widening of an ALREADY DECLARED slot counts. The slot a store mints is at its own
        // value's type by construction and claims nothing about another path; and `unset`
        // (`preserve_storage_type`), a ref-bound name (its home is the cell) and a global-backed
        // name (`store_local` types it from the symbol) all keep the representations they have.
        //
        // And only a widening INTO a boxed representation counts. That is the one that makes the
        // slot hold several runtime shapes at once, so that the shape a read must assume stops
        // being a property of the last store. `Array(int)` -> `Array(string)` is not it: the slot
        // still holds one container, and reconciling those element types is the
        // `ArrayToMixed`/`ArrayToHash` machinery's job (`repr_fixpoint`, `arm_conversions`), which
        // this must not pre-empt. `TaggedScalar` is not it either — a nullable scalar's tag word
        // is the nullable lowering's own contract.
        let widened_storage = self.builder.local_php_type(slot);
        if previous_slot.is_some()
            && !preserve_storage_type
            && !is_ref_bound
            && !uses_global
            && previous_kind == LocalKind::PhpLocal
            && widened_storage.codegen_repr() != storage_before_store
            && matches!(
                widened_storage.codegen_repr(),
                PhpType::Mixed | PhpType::Union(_)
            )
        {
            self.storage_widened_locals.insert(name.to_string());
        }
        if self.storage_widened_locals.contains(name) && !preserve_storage_type && !is_ref_bound {
            local_type_after_store = self.builder.local_php_type(slot);
        }
        if uses_global {
            self.store_global_name(name, slot, value, span);
            self.set_local_type(name, local_type_after_store);
            if release_source_after_store && !transfer_source_to_store {
                crate::ir_lower::ownership::release_if_owned(self, source, span);
            }
            return value;
        }
        let is_ref_bound =
            self.is_ref_bound_local(name) && !uses_global && previous_kind == LocalKind::PhpLocal;
        let op = match (is_ref_bound, previous_kind) {
            (true, _) => Op::StoreRefCell,
            (false, LocalKind::StaticLocal) => Op::StoreStaticLocal,
            _ => Op::StoreLocal,
        };
        // Track whether the ref cell store narrows Mixed→Int and releases the
        // source Mixed box in the codegen, so we skip the release_if_owned below.
        let ref_cell_narrowed_mixed_to_int = is_ref_bound
            && matches!(
                self.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed
            )
            && matches!(ref_cell_storage_type.codegen_repr(), PhpType::Int);
        // A ref-cell store publishes its replacement before retiring the old pointee. That
        // retirement can run user destructors and throw, so an owned source whose ordinary
        // post-store release has not run yet needs an unwind-visible owner of its own. The
        // store's acquired value becomes the new pointee owner; this root owns only the source
        // expression lease that normal control flow retires immediately after publication.
        let ref_store_source_owner = if is_ref_bound
            && release_source_after_store
            && !transfer_source_to_store
            && !ref_cell_narrowed_mixed_to_int
        {
            let (_, owner) = crate::ir_lower::expr::root_owned_call_operand(
                self,
                source,
                span.unwrap_or_else(Span::dummy),
            );
            owner
        } else {
            None
        };
        if is_ref_bound {
            let value = self.box_typed_array_for_mixed_ref_cell(value, &ref_cell_storage_type, span);
            self.store_ref_cell_slot(slot, value, ref_cell_storage_type, span);
        } else {
            self.store_slot_with_op(slot, value, op, span);
        }
        if !is_ref_bound {
            self.set_local_type(name, local_type_after_store);
        }
        if let Some(owner) = ref_store_source_owner {
            crate::ir_lower::expr::retire_owned_call_operand(
                self,
                owner,
                span.unwrap_or_else(Span::dummy),
            );
        }
        if release_source_after_store
            && !transfer_source_to_store
            && !ref_cell_narrowed_mixed_to_int
            && ref_store_source_owner.is_none()
        {
            self.release_stored_source(source, slot, span);
        }
        value
    }

    /// Releases the value a retaining store consumed, deciding at RUNTIME when the callee only
    /// MIGHT have handed one of its parameters back.
    ///
    /// `$acc = f($acc, …)` is the shape that needs this. When `f` returns a fresh value the
    /// store's acquire/release pair nets zero and the slot owns the callee's `+1`. When `f`
    /// returns its own argument there is no such `+1`, so the unconditional release took the
    /// accumulator's count down one per iteration until the next read hit freed memory —
    /// php answered `2:a,b`, elephc answered `TypeError: Unsupported operand types`.
    fn release_stored_source(
        &mut self,
        source: LoweredValue,
        slot: LocalSlotId,
        span: Option<Span>,
    ) {
        let aliased_argument = self
            .call_argument_read_from_slot(source.value, slot)
            .or_else(|| self.user_call_result_may_alias_argument(source.value));
        if let Some(argument) = aliased_argument {
            // Only single-pointer payloads can be compared, the same restriction the argument
            // side already applies: a boxed Mixed that WRAPS a container holds a different
            // pointer than the container, so comparing them would read "not aliased" for a
            // value the result does own.
            let source_repr = self.builder.value_php_type(source.value).codegen_repr();
            let argument_repr = self.builder.value_php_type(argument).codegen_repr();
            if matches!(source_repr, PhpType::Mixed | PhpType::Union(_))
                && matches!(argument_repr, PhpType::Mixed | PhpType::Union(_))
            {
                self.emit_void(
                    Op::ReleaseUnlessAliases,
                    vec![source.value, argument],
                    None,
                    Op::ReleaseUnlessAliases.default_effects(),
                    span,
                );
                return;
            }
        }
        crate::ir_lower::ownership::release_if_owned(self, source, span);
    }

    /// Boxes a typed-array source to `Array(Mixed)` before it is stored through a reference
    /// cell whose element type is `Mixed`.
    ///
    /// `$ref = [1, 2]` where `$ref` aliases an object's `array` (Mixed-element) property stores
    /// the literal's pointer into the shared cell. Without conversion the cell would hold an
    /// `Array(Int)` payload but every read goes through the property's `Array(Mixed)` view, so
    /// element reads (`implode`, `$prop[0]`) would misinterpret the raw scalar slots. Converting
    /// with `ArrayToMixed` boxes each element so the stored array matches the cell's element
    /// type. Empty / `Never`-element sources are left untouched (no elements to box).
    fn box_typed_array_for_mixed_ref_cell(
        &mut self,
        value: LoweredValue,
        cell_ty: &PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        let value_ty = self.builder.value_php_type(value.value);
        if !ref_cell_needs_mixed_array_conversion(cell_ty, &value_ty) {
            return value;
        }
        self.emit_value(
            Op::ArrayToMixed,
            vec![value.value],
            None,
            PhpType::Array(Box::new(PhpType::Mixed)),
            Op::ArrayToMixed.default_effects(),
            span,
        )
    }

    /// Stores a synthetic foreach initializer in the local frame without eval-scope sync.
    ///
    /// Fresh `foreach` key/value locals need a concrete frame slot before the first
    /// iteration, but PHP must not observe that setup when the iterable is empty.
    /// Runtime eval-scope writes therefore use this path for the pre-loop null seed
    /// and keep normal `store_local` for values assigned inside the loop body.
    pub(crate) fn store_foreach_initializer_local_only(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        let previous_slot = self.local_slots.get(name).copied();
        let previous_kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        let slot = self.declare_local(name, php_type.clone());
        self.builder
            .widen_local_storage_type(slot, php_type.clone());
        let source = value;
        let release_source_after_store = self.value_needs_release_after_retaining_store(value);
        // Retain before cleanup because a borrowed result can alias the old slot.
        let stored = crate::ir_lower::ownership::acquire_if_refcounted(self, value, span);
        if local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_some_and(|slot| self.initialized_slots.contains(&slot))
        {
            self.release_stored_local_value(name, slot, span);
        }
        if local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_some_and(|slot| !self.initialized_slots.contains(&slot))
            && (!self.loop_stack.is_empty()
                || (previous_kind == LocalKind::PhpLocal && self.eval_barrier_active))
        {
            self.release_stored_local_value_before_overwrite(name, slot, &self.builder.local_php_type(slot), span);
        }
        if local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_none()
            && (!self.loop_stack.is_empty()
                || (previous_kind == LocalKind::PhpLocal && self.eval_barrier_active))
        {
            self.release_stored_local_value_before_overwrite(name, slot, &self.builder.local_php_type(slot), span);
        }
        self.store_slot_with_op(slot, stored, Op::StoreLocal, span);
        self.set_local_type(name, php_type);
        if release_source_after_store {
            crate::ir_lower::ownership::release_if_owned(self, source, span);
        }
        stored
    }

    /// Returns the declared PHP type for an extern global visible as a variable.
    fn extern_global_type(&self, name: &str) -> Option<PhpType> {
        self.extern_globals.get(name).cloned()
    }

    /// Emits a read from a C extern global symbol instead of a PHP local slot.
    fn load_extern_global(
        &mut self,
        name: &str,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        let data = self.intern_global_name(name);
        let ir_type = value_ir_type(&php_type);
        let ownership = Ownership::for_php_type(&php_type);
        let value = self
            .builder
            .emit_with_effects(
                Op::ExternGlobalLoad,
                Vec::new(),
                Some(Immediate::GlobalName(data)),
                ir_type,
                php_type,
                ownership,
                Op::ExternGlobalLoad.default_effects(),
                span,
            )
            .expect("extern_global_load produces a value");
        LoweredValue { value, ir_type }
    }

    /// Emits a write to a C extern global symbol using the already-lowered source value.
    fn store_extern_global_name(&mut self, name: &str, value: LoweredValue, span: Option<Span>) {
        let data = self.intern_global_name(name);
        self.builder.emit_with_effects(
            Op::ExternGlobalStore,
            vec![value.value],
            Some(Immediate::GlobalName(data)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::ExternGlobalStore.default_effects(),
            span,
        );
    }

    /// Releases the boxed local owner before a consuming mutation executes.
    ///
    /// The source load already owns an unboxed concrete reference. Widening the
    /// destination storage before emitting this cleanup makes the final Mixed frame
    /// representation visible, then dropping the old box transfers its payload owner
    /// to the source SSA value so COW sees only real aliases during the mutation.
    pub(crate) fn prepare_mutated_local_owner(
        &mut self,
        name: &str,
        source: LoweredValue,
        replacement_type: PhpType,
        span: Option<Span>,
    ) {
        self.prepare_mutated_local_owner_impl(name, source, replacement_type, span, true);
    }

    /// Widens the destination for a consuming mutation whose BACKEND retires the boxed owner.
    ///
    /// `Op::ArrayToHash` lowering drops the slot's Mixed box itself
    /// (`FunctionContext::release_mutated_source_local_owner`), because the slot can still widen
    /// after this point and only the backend sees the final storage. Emitting the release here as
    /// well dropped the same box twice whenever the storage was already Mixed at lowering time:
    /// `$a = []; $a["x"] = 1;` in a loop of a program that also calls `eval()` freed the box on
    /// the first write of every iteration after the first, silently on x86_64 (its heap-debug
    /// Mixed release never checks liveness) and as a bad refcount on aarch64. Use this for the
    /// conversions the backend retires for; `prepare_mutated_local_owner` stays for the ones it
    /// does not (`ArrayToMixed`, `HashToMixed`, the ensure-unique runtime calls).
    pub(crate) fn prepare_mutated_local_owner_for_backend_retire(
        &mut self,
        name: &str,
        source: LoweredValue,
        replacement_type: PhpType,
        span: Option<Span>,
    ) {
        self.prepare_mutated_local_owner_impl(name, source, replacement_type, span, false);
    }

    /// Shared body of the two consuming-mutation preludes above.
    ///
    /// Declares (or re-declares) the local at `replacement_type`, widens its frame storage so
    /// the final Mixed representation is visible, and, only when `release_box` is set, clears
    /// and retires the previous Mixed box so the source load becomes the container's sole owner
    /// for the mutation. Program-global names return early: their storage is the `_eir_global_*`
    /// symbol, not a frame slot. A ref-bound local never releases here either; its cell alias
    /// is retired by the `StoreRefCell` that publishes the replacement.
    fn prepare_mutated_local_owner_impl(
        &mut self,
        name: &str,
        source: LoweredValue,
        replacement_type: PhpType,
        span: Option<Span>,
        release_box: bool,
    ) {
        let previous_kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        if self.uses_global_storage(name, previous_kind) {
            return;
        }
        let slot = self.declare_local(name, replacement_type.clone());
        // A BOXED static differs from a boxed frame slot twice: loading its box takes no
        // reference, and its store releases the previous value itself. Releasing the "stored" box
        // here therefore freed the live static, and the store freed it again: `static $v = [];
        // $v[$name] = $mixed;` rebuilt its hash from freed storage on the second call. The unboxed
        // source load DOES hold its own reference, so the static is cleared through its own store
        // instead: that releases the box, the source becomes the payload's only owner, and the
        // mutation runs in place rather than cloning the container on every write.
        if previous_kind == LocalKind::StaticLocal {
            self.builder
                .widen_local_storage_type(slot, replacement_type);
            if self.static_local_is_boxed(slot) {
                let null = self
                    .builder
                    .emit_with_effects(
                        Op::ConstNull,
                        Vec::new(),
                        None,
                        IrType::I64,
                        PhpType::Void,
                        Ownership::NonHeap,
                        Op::ConstNull.default_effects(),
                        span,
                    )
                    .expect("const_null produces a value");
                self.store_slot_with_op(
                    slot,
                    LoweredValue {
                        value: null,
                        ir_type: IrType::I64,
                    },
                    Op::StoreStaticLocal,
                    span,
                );
            }
            return;
        }
        let is_ref_bound =
            self.is_ref_bound_local(name) && previous_kind == LocalKind::PhpLocal;
        let source_type = self.builder.value_php_type(source.value).codegen_repr();
        self.builder
            .widen_local_storage_type(slot, replacement_type);
        let storage_type = self.builder.local_php_type(slot).codegen_repr();
        if release_box
            && !is_ref_bound
            && matches!(storage_type, PhpType::Mixed | PhpType::Union(_))
            && !matches!(source_type, PhpType::Mixed | PhpType::Union(_))
        {
            // A later reference promotion can reach this prelude through a loop back-edge.
            // Retire and clear the actual runtime owner, rather than releasing a loaded box
            // that the reference-cell storeback would otherwise retire a second time.
            let _ = self.release_stored_local_value_before_overwrite(name, slot, &storage_type, span);
        }
    }

    /// Emits a consuming local storeback after a mutation or representation change.
    ///
    /// The mutation result already owns the reference that moves into the destination,
    /// so this deliberately skips assignment acquire/release. When the local's final
    /// frame representation is boxed Mixed but the mutation result is concrete, loading
    /// the old value produced a separate owned unboxed reference; release the previous
    /// Mixed box before replacing it so COW generations do not leak.
    pub(crate) fn store_mutated_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        self.store_mutated_local_impl(name, value, php_type, span, true, true, false)
    }

    /// Stores a one-word container replacement while changing only the local's logical type.
    ///
    /// Indexed-to-hash promotion keeps the same raw pointer slot. Widening the frame metadata from
    /// `Array` to `AssocArray` would instead choose boxed Mixed storage and invalidate earlier
    /// indexed loads, so this path preserves the established frame representation while making
    /// every subsequent load select associative lowering.
    pub(crate) fn store_retyped_container_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        self.record_array_storage_conversion(name, &php_type);
        self.store_mutated_local_impl(name, value, php_type, span, true, true, true)
    }

    /// Changes only the flow-sensitive view of a raw one-word container slot.
    pub(crate) fn set_retyped_container_local_type(&mut self, name: &str, php_type: PhpType) {
        self.local_types.insert(name.to_string(), php_type);
    }

    /// Stores a by-reference call's internal array normalization without hoisting it.
    ///
    /// Call lowering deliberately captures the operand's concrete representation before adapting
    /// an `array<mixed>` by-reference argument. Treating that adaptation as a statement-level
    /// conversion makes the fixed-point pass pre-widen the operand and breaks concrete builtin
    /// dispatch such as `sort()` and `array_pop()`.
    pub(crate) fn store_call_normalized_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        self.store_mutated_local_impl(name, value, php_type, span, true, false, false)
    }

    /// Stores a mutation result whose previous boxed local owner was released beforehand.
    pub(crate) fn store_prepared_mutated_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        self.store_mutated_local_impl(name, value, php_type, span, false, true, false)
    }

    /// Implements consuming local storeback with caller-selected cleanup timing.
    fn store_mutated_local_impl(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
        release_previous: bool,
        track_array_conversion: bool,
        preserve_frame_storage: bool,
    ) -> LoweredValue {
        self.clear_static_callable_local(name);
        self.clear_reflection_class_local(name);
        self.clear_reflection_function_local(name);
        self.clear_reflection_property_local(name);
        self.clear_reflection_method_local(name);
        self.clear_reflection_arg_array_local(name);
        self.clear_fiber_start_sig(name);
        let previous_kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, previous_kind);
        let slot = self.declare_local(name, php_type.clone());
        if uses_global {
            self.store_global_name(name, slot, value, span);
            if preserve_frame_storage {
                self.local_types.insert(name.to_string(), php_type);
            } else {
                self.set_local_type_impl(name, php_type, track_array_conversion);
            }
            return value;
        }
        let is_ref_bound = self.is_ref_bound_local(name) && previous_kind == LocalKind::PhpLocal;
        let value_type = self.builder.value_php_type(value.value).codegen_repr();
        if preserve_frame_storage {
            self.local_types.insert(name.to_string(), php_type.clone());
        } else {
            self.set_local_type_impl(name, php_type.clone(), track_array_conversion);
        }
        let storage_type = self.builder.local_php_type(slot).codegen_repr();
        // A boxed static's store releases its previous box and RETAINS what it boxes, unlike the
        // consuming frame store this path is written for: skip the frame-slot release (a second
        // release of the same box) and drop the mutation result's own reference once stored.
        if !is_ref_bound
            && previous_kind == LocalKind::StaticLocal
            && self.static_local_is_boxed(slot)
        {
            self.store_slot_with_op(slot, value, Op::StoreStaticLocal, span);
            crate::ir_lower::ownership::release_if_owned(self, value, span);
            return value;
        }
        if release_previous
            && !is_ref_bound
            && matches!(storage_type, PhpType::Mixed | PhpType::Union(_))
            && !matches!(value_type, PhpType::Mixed | PhpType::Union(_))
        {
            self.release_stored_local_value_before_overwrite(name, slot, &storage_type, span);
        }
        match (is_ref_bound, previous_kind) {
            (true, _) => self.store_ref_cell_slot(slot, value, php_type, span),
            (false, LocalKind::StaticLocal) => {
                self.store_slot_with_op(slot, value, Op::StoreStaticLocal, span);
            }
            _ => {
                self.store_slot_with_op(slot, value, Op::StoreLocal, span);
            }
        }
        value
    }

    /// Returns whether a static local's storage is a boxed `Mixed` cell.
    fn static_local_is_boxed(&self, slot: LocalSlotId) -> bool {
        matches!(
            self.builder.local_php_type(slot).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    }

    /// Emits `unset($local)`, breaking by-reference aliases without writing through them.
    ///
    /// At a span the CHECKER recorded in `bind_kill_sites` the binding is also ABANDONED: the
    /// name loses its slot mapping, so the next `declare_local` mints a FRESH slot at whatever
    /// type the checker approved for the re-binding assignment. Only the checker decides
    /// eligibility (`Checker::local_binding_is_killable`); the guard consulted here
    /// (`local_binding_slot_is_abandonable`) is about STORAGE, not eligibility — it refuses the
    /// storage shapes where "the name's slot" is not the value's home at all.
    ///
    /// The old value is released and the slot is left cleared either way. That is load-bearing:
    /// the frame epilogue still emits cleanup for every slot whose storage type is refcounted,
    /// including the abandoned one, so leaving the released pointer in place would be a double
    /// free.
    ///
    /// The two forms clear the slot differently, and only the NON-abandoning one can use the
    /// ordinary null store. There the name still resolves to the slot, so the store's widening to
    /// `Mixed` is what a later `isset`/`??` read needs in order to see PHP null. The abandoning
    /// form must NOT widen — every load already lowered above it would keep its narrower IR type
    /// against boxed storage and leak a detached copy per executed read — so it goes through
    /// `release_and_abandon_local_binding`, which zeroes the slot at its own storage type instead.
    ///
    /// The null-store form's `set_local_type(name, Void)` is not branch-sensitive, and in ONE shape
    /// that shows: `unset()` in EXPRESSION position, which is an elephc extension (PHP rejects
    /// `$b = $c ? unset($a) : 1;` as a parse error). Statement-level conditionals snapshot and
    /// restore `local_types` across their arms, so a plain `if ($c) { unset($a); } echo $a;` is
    /// correct; `lower_ternary` restores only `initialized_slots`, so an `unset` in an UNTAKEN
    /// ternary arm still leaves the name typed `Void` (and its slot widened) below the ternary,
    /// and the read prints nothing. That predates local-binding kills — a non-recorded `unset` here
    /// takes exactly the null-store path it took before they existed — and fixing it means giving
    /// expression-level branch merges the `local_types` treatment statement-level ones have, which
    /// would change every ternary arm that assigns, not just this one.
    pub(crate) fn unset_local(
        &mut self,
        name: &str,
        null: LoweredValue,
        span: Option<Span>,
    ) -> LoweredValue {
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        let global_name = crate::globals_array::alias_target(name).unwrap_or(name);
        if self.uses_global_storage(name, kind)
            && !crate::superglobals::is_superglobal(global_name)
            && !self.extern_globals.contains_key(global_name)
        {
            let data = self.intern_global_storage_name(name);
            self.emit_void(
                Op::UnsetGlobal, Vec::new(), Some(Immediate::GlobalName(data)),
                Op::UnsetGlobal.default_effects(), span,
            );
            return null;
        }
        let abandons_binding = span.is_some_and(|span| {
            span.identifies_a_node()
                && self
                    .bind_kill_sites
                    .get(&span)
                    .is_some_and(|killed| killed.contains(name))
        }) && self.local_binding_slot_is_abandonable(name);
        if !self.is_ref_bound_local(name) {
            if abandons_binding {
                self.release_and_abandon_local_binding(name, span);
                return null;
            }
            return self.store_unset_local(name, null, span);
        }
        // Reference detachment needs its own checker decision. Ordinary binding kills exclude
        // escaped references, and branch-local mapping changes cannot be applied unconditionally.
        let detaches_reference = span.is_some_and(|span| {
            span.identifies_a_node()
                && self.ref_detach_sites.get(&span).is_some_and(|names| names.contains(name))
        }) && self.local_kinds.get(name) == Some(&LocalKind::PhpLocal)
            && !self.local_uses_global_storage(name)
            && !self.extern_globals.contains_key(name)
            && !self.eval_barrier_active
            && self.eval_scope_read_param.is_none();
        self.clear_static_callable_local(name);
        self.clear_reflection_class_local(name);
        self.clear_reflection_function_local(name);
        self.clear_reflection_property_local(name);
        self.clear_reflection_method_local(name);
        self.clear_reflection_arg_array_local(name);
        self.clear_fiber_start_sig(name);
        let slot = self.declare_local(name, PhpType::Void);
        self.release_ref_cell_owner(name, span);
        self.emit_void(
            Op::UnsetLocal,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::UnsetLocal.default_effects(),
            span,
        );
        self.unmark_ref_bound_local(name);
        if detaches_reference {
            // UnsetLocal clears promotion state, but its null sentinel is not an inert raw
            // string pointer. Zero the old storage without widening the capture's payload ABI.
            self.emit_void(
                Op::ZeroLocalSlot,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                Op::ZeroLocalSlot.default_effects(),
                span,
            );
            self.reset_array_pointer_cursor(name);
            self.ref_cell_owner_locals.remove(name);
            self.initialized_slots.insert(slot);
            self.abandon_local_binding(name);
            return null;
        }
        self.set_local_type(name, PhpType::Void);
        self.initialized_slots.insert(slot);
        null
    }

    /// Returns whether a checker-recorded `unset` kill may abandon this name's frame slot.
    ///
    /// This is NOT a second eligibility judgement — the checker already made that one, and a
    /// name that reaches here with a recorded span is one it approved. What it refuses is every
    /// storage shape where the name's local slot is not the value's home, so abandoning the
    /// mapping would either strand storage the program still reaches by NAME or make the next
    /// `declare_local` mint a plain frame slot for something that lives elsewhere:
    ///
    /// - a superglobal (`$_SERVER`, …) or, inside `main`, any name some `global` statement
    ///   names: those live in `_eir_global_*` symbols, and the checker seeds superglobals into
    ///   every environment without a binding depth, so it can offer them here;
    /// - a `global`/`static` alias slot, a ref-cell, or any other non-`PhpLocal` role;
    /// - an extern C global, whose store path bypasses the slot entirely;
    /// - a name a promoted ref-cell owner still tracks (`ref_cell_owner_locals`), which would
    ///   be left pointing at the abandoned slot's cell;
    /// - anything in a body that runs `eval`, where the eval scope addresses locals BY NAME.
    fn local_binding_slot_is_abandonable(&self, name: &str) -> bool {
        let Some(kind) = self.local_kinds.get(name).copied() else {
            return false;
        };
        if kind != LocalKind::PhpLocal || !self.local_slots.contains_key(name) {
            return false;
        }
        if self.uses_global_storage(name, kind) || self.extern_globals.contains_key(name) {
            return false;
        }
        if self.ref_cell_owner_locals.contains_key(name) || self.is_ref_bound_local(name) {
            return false;
        }
        if self.eval_barrier_active || self.eval_scope_read_param.is_some() {
            return false;
        }
        true
    }

    /// Drops the name-to-slot binding after `unset` released and nulled the slot.
    ///
    /// The slot itself stays in the frame (it is still cleaned up, and any SSA value already
    /// loaded from it stays valid); only the NAME stops resolving to it, so the next
    /// `declare_local` allocates a fresh slot at the re-binding type instead of widening the
    /// old one. The per-name facts that describe the dead binding's storage go with it: a stale
    /// array-representation conversion or foreach-key fact would otherwise be applied to storage
    /// that no longer exists.
    ///
    /// The TYPE fact is reset to `Void` rather than dropped, which puts the name in exactly the
    /// state a never-assigned name has (the checker seeds those `Void` too). PHP still allows
    /// `isset($a)` / `empty($a)` / `$a ?? …` on an unbound name, and those lower as an ordinary
    /// load: with no type fact at all the load would mint a `Mixed` slot and read uninitialized
    /// storage as a pointer (`unset($a, $b); echo isset($a);` segfaulted, measured).
    /// `load_local` emits `ConstNull` for an absent `Void` binding without creating storage.
    /// The probe therefore answers false, and a later store creates a fresh slot at its own type.
    fn abandon_local_binding(&mut self, name: &str) {
        self.local_slots.remove(name);
        self.local_kinds.remove(name);
        self.local_types.insert(name.to_string(), PhpType::Void);
        self.array_conversions.remove(name);
        self.foreach_int_key_locals.remove(name);
    }

    /// Ends a local binding: releases the value the old slot owns, ZEROES the slot at its own
    /// storage type, and drops the name-to-slot mapping.
    ///
    /// The single mechanism behind BOTH ways a binding can end — an `unset` the checker recorded
    /// in `bind_kill_sites`, and an incompatible reassignment it recorded in `retype_sites`.
    ///
    /// The slot's storage type is NEVER touched here, and that is the whole design. A slot's
    /// storage type is a whole-FRAME property, so the obvious implementation — a `store_local` of
    /// null at `PhpType::Void` — widened the slot (`Str` joined with `Void` is `Mixed`), and every
    /// load the body had ALREADY lowered above this point kept its `Str` IR type against what was
    /// now boxed storage. Each such read became an unreleased `__rt_mixed_cast_string` detach: one
    /// leaked copy of the string per EXECUTED read, so
    /// `$a = "x" . $n; for (…200000…) { strlen($a); } $a = 5;` died with
    /// `Fatal error: heap memory exhausted` where PHP prints the sum.
    ///
    /// Nor can the null simply be stored at the slot's OWN type. `null` is materialized as an
    /// `I64`, so it lands in the int result register; a `Str` slot is two words written from the
    /// STRING result register pair instead — `(x1, x2)` on aarch64 against `x0` for the int,
    /// entirely disjoint — and nothing coerces an `Int`/`Void` source into that pair
    /// (`coerce_current_result_for_target_store` only handles a `TaggedScalar` target). That store
    /// wrote two STALE registers over the slot instead of clearing it, leaving a live pointer the
    /// frame epilogue's `__rt_heap_free_safe` then freed — a buffer another local still owned.
    /// Measured: `$q = "a" . $n; $r = $q; $q = 1; return $r . "|" . $q;` returned four NUL bytes,
    /// with allocations and frees still balanced because the freed block was a genuinely live one.
    /// (`$r = $q` does take its own copy — `acquire` on a `Str` lowers to `__rt_str_persist`, which
    /// duplicates every source except a `CONCAT_TEMP_HEAP_KIND` block — so the fault really was the
    /// uncleared slot, not a missing copy.)
    ///
    /// `Op::ZeroLocalSlot` is what closes that gap: it writes literal zeros over the slot's words
    /// at the slot's own storage type, so no register-shape coercion is involved and no widening is
    /// needed to make the clear land. Clearing the slot stays load-bearing — the frame epilogue
    /// still emits cleanup for every slot whose storage type is refcounted, the abandoned one
    /// included — and zero is precisely the state both prologues already leave cleanup-tracked
    /// slots in, so that cleanup walks past it.
    ///
    /// The release ordering and the name-keyed facts are the ones `store_local`'s overwrite path
    /// uses, replicated here because there is no value to store: the previous occupant is released
    /// when the slot is live on this path (definitely initialized, or reachable through a loop
    /// back-edge), and the per-name lowering facts that describe the dead binding are cleared.
    ///
    /// A numeric slot skips the zero where it is redundant (see `abandoned_slot_needs_zeroing`),
    /// whichever way its binding ends. Every numeric proof (checked-integer sinking, bounded
    /// counters, scalar promotion, numeric frame cells) refuses a slot that any other opcode
    /// names, so a needless `zero_local_slot` would keep the old binding boxed for the rest of
    /// the function. That matters most for the storage-only re-bind the checker also records in
    /// `retype_sites`: a compatible `int` / `int|float` / `float` / `mixed` reassignment, so two
    /// loops reusing one counter name keep two slots.
    fn release_and_abandon_local_binding(&mut self, name: &str, span: Option<Span>) {
        let Some(slot) = self.local_slots.get(name).copied() else {
            return;
        };
        self.clear_static_callable_local(name);
        self.clear_reflection_class_local(name);
        self.clear_reflection_function_local(name);
        self.clear_reflection_property_local(name);
        self.clear_reflection_method_local(name);
        self.clear_reflection_arg_array_local(name);
        self.clear_fiber_start_sig(name);
        self.reset_array_pointer_cursor(name);
        let storage = self.builder.local_php_type(slot);
        let function = self.builder.function();
        let has_store_owner = slot.as_raw() as usize >= function.params.len()
            && function.instructions.iter().any(|inst| {
                inst.op == Op::StoreLocal && inst.immediate == Some(Immediate::LocalSlot(slot))
            });
        // The same liveness test `store_local` applies before overwriting a slot: release the
        // occupant when this path definitely wrote it, and also when a loop back-edge can carry a
        // previous iteration's value into a slot straight-line flow has not initialized yet.
        // A conditional arm may also have written the owner. Non-parameter cleanup slots start
        // at zero, so retiring that owner is safe on the path where the arm never ran.
        let released = (self.initialized_slots.contains(&slot) || !self.loop_stack.is_empty() || has_store_owner)
            && self.release_stored_local_value_before_overwrite(name, slot, &storage, span);
        // The zero also carries the frame's OWNERSHIP claim on a slot no `store_local` writes
        // (`codegen::local_analysis` counts it as a store, so an incoming parameter's prologue
        // retains what the release drops). Only a released slot an ordinary store already
        // claims may go without it.
        let released_store_owner = released && has_store_owner;
        if abandoned_slot_needs_zeroing(&storage, released_store_owner) {
            self.emit_void(
                Op::ZeroLocalSlot,
                Vec::new(),
                Some(Immediate::LocalSlot(slot)),
                Op::ZeroLocalSlot.default_effects(),
                span,
            );
        }
        self.initialized_slots.insert(slot);
        self.abandon_local_binding(name);
    }

    /// Abandons `name`'s binding at a checker-recorded RETYPE site, so the assignment's own store
    /// mints a FRESH slot at the new type instead of widening the old one.
    ///
    /// Called from `lower_assign` AFTER the right-hand side is lowered and BEFORE the store: the
    /// RHS of `$a = "n=" . $a` — and of every compound assignment, which the parser hands over as
    /// an ordinary `Assign` whose value reads the target — must read the OLD binding. Nothing
    /// else may run in between, because the release emitted here frees whatever the old slot
    /// owns; the RHS value has already been computed and (where it captured the old value, as in
    /// `$a = [$a]`) already holds its own reference to it.
    ///
    /// The eligibility judgement is the checker's alone. The gate consulted here is the same
    /// STORAGE gate the `unset` kill uses (`local_binding_slot_is_abandonable`): where the name's
    /// slot is not the value's home, the assignment falls back to the storage-widening path it
    /// used before retypes were lowered, which is correct, just less precise.
    pub(crate) fn rebind_local_for_retype(&mut self, name: &str, span: Option<Span>) {
        if !self.local_binding_slot_is_abandonable(name) {
            return;
        }
        self.release_and_abandon_local_binding(name, span);
    }

    /// Returns whether the checker recorded a binding RETYPE of `name` at this assignment's span.
    ///
    /// Neither half of the key is redundant. `Span::dummy()` names no node, so a decision filed
    /// under it would answer for every compiler-generated assignment in the program; the checker
    /// refuses to record one, and this is the second half of that contract. And a `Span` carries
    /// no FILE, while include resolution splices every included file's statements into one
    /// program without rebasing line numbers — so line 4 column 1 of the main file and of an
    /// included one are equal spans, and only the local's name separates the decision recorded
    /// for one from the unrelated assignment at the other.
    pub(crate) fn is_recorded_retype_site(&self, span: Span, name: &str) -> bool {
        span.identifies_a_node()
            && self
                .retype_sites
                .get(&span)
                .is_some_and(|retyped| retyped.contains(name))
    }

    /// Returns whether the checker marked `name` as branch-divergently assigned and recorded THIS
    /// assignment as one of its store sites.
    ///
    /// Keyed by span AND name for the same two reasons as `is_recorded_retype_site`: a
    /// `Span::dummy()` names no node, and equal spans in different included files are
    /// indistinguishable without the local's name.
    ///
    /// The STORAGE gate is applied here rather than at the call site, so the one answer covers both
    /// halves of the mixed contract (pre-declaring the slot boxed, and forcing the store's PHP type
    /// to `Mixed`) — the same way `local_binding_slot_is_abandonable` covers both the kill and the
    /// retype.
    pub(crate) fn is_recorded_mixed_storage_site(&self, span: Span, name: &str) -> bool {
        span.identifies_a_node()
            && self
                .mixed_storage_store_sites
                .get(&span)
                .is_some_and(|boxed| boxed.contains(name))
            && self.local_binding_may_force_mixed_storage(name)
    }

    /// Returns whether `name`'s storage shape can carry the whole-frame boxed-`Mixed` contract.
    ///
    /// The `Mixed` sibling of `local_binding_slot_is_abandonable`, and it refuses the same shapes
    /// for the same reason: where the name's local slot is not the value's home, the frame-wide
    /// promise "every read of this name is a boxed read" is not this layer's to make. Every one of
    /// these already degrades harmlessly inside `store_local` (a global-backed name has its type
    /// overridden by `global_alias_type`, an extern global bypasses the slot, a ref-bound name
    /// stores through its cell at the cell's type) — stating the refusal here makes it a decision
    /// instead of a coincidence, and stops the pre-declare from minting a plain `PhpLocal` slot for
    /// a name whose value lives elsewhere.
    ///
    /// Two deliberate NON-refusals, both differences from the abandonable set rather than
    /// omissions:
    ///
    /// - a name with NO slot yet is exactly what the pre-declare is for, so "no `local_kinds`
    ///   entry" is a pass here where it is a refusal there;
    /// - a body that CALLS `eval` (`eval_barrier_active`) keeps its marks. The kill/retype must
    ///   stand down there because the eval scope addresses locals BY NAME and abandoning a slot
    ///   moves the name's storage out from under it — but boxed `Mixed` is the representation that
    ///   scope wants in the first place, and a marked local the checker already bound `Mixed` would
    ///   miscompile if its slot stayed concrete (`test_branch_divergent_local_survives_an_eval_body`).
    ///
    /// The `eval_scope_read_param` test refuses the SCOPE-AWARE eval-AOT fragment specifically —
    /// it is the only fragment shape that carries a scope param, so it is the only one this test
    /// names. That is not a gap, because no fragment of either shape can reach here in the first
    /// place: both eval-AOT lowerers receive empty decision maps by construction
    /// (`ir_lower::function::eval_aot_decision_maps`), which is where fragments are actually
    /// excluded. What this line adds is a second statement of the same rule on the one shape whose
    /// context is visible from here — a fragment's spans are its own, so a key of the outer
    /// program's that matched one would be an accident.
    fn local_binding_may_force_mixed_storage(&self, name: &str) -> bool {
        let kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        if kind != LocalKind::PhpLocal {
            return false;
        }
        if self.uses_global_storage(name, kind) || self.extern_globals.contains_key(name) {
            return false;
        }
        if self.ref_cell_owner_locals.contains_key(name) || self.is_ref_bound_local(name) {
            return false;
        }
        if self.eval_scope_read_param.is_some() {
            return false;
        }
        true
    }

    /// Releases an owned hidden temp's value and clears the slot, so the frame's own cleanup
    /// finds nothing left to release.
    pub(crate) fn release_owned_hidden_temp(&mut self, name: &str, span: Option<Span>) {
        let Some(slot) = self.local_slots.get(name).copied() else {
            return;
        };
        if self.builder.local_kind(slot) != LocalKind::OwnedTemp {
            return;
        }
        self.emit_void(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::ReleaseLocalSlot.default_effects(),
            span,
        );
        self.clear_owned_hidden_temp(name, span);
    }

    /// Clears an owned hidden temp after its value has been loaded into SSA.
    pub(crate) fn clear_owned_hidden_temp(&mut self, name: &str, span: Option<Span>) {
        let Some(slot) = self.local_slots.get(name).copied() else {
            return;
        };
        if self.builder.local_kind(slot) != LocalKind::OwnedTemp {
            return;
        }
        self.emit_void(
            Op::UnsetLocal,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::UnsetLocal.default_effects(),
            span,
        );
    }

    /// Emits an idempotent promotion of an initialized local into an owned fallback ref-cell.
    pub(crate) fn promote_local_ref_cell(&mut self, name: &str, span: Option<Span>) {
        self.promote_local_ref_cell_owner(name, span);
    }

    /// Promotes like [`Self::promote_local_ref_cell`] and returns the hidden owner slot, which
    /// holds the cell only when this promotion created it at run time.
    pub(crate) fn promote_local_ref_cell_owner(
        &mut self,
        name: &str,
        span: Option<Span>,
    ) -> LocalSlotId {
        let slot = self.declare_local(name, self.local_type(name));
        let fallback_ty = self.builder.local_php_type(slot);
        let owner_slot = self.declare_ref_cell_owner(name, fallback_ty.clone());
        self.builder.emit_with_effects(
            Op::PromoteLocalRefCell,
            Vec::new(),
            Some(Immediate::LocalSlotPair {
                first: slot,
                second: owner_slot,
            }),
            IrType::Void,
            fallback_ty,
            Ownership::NonHeap,
            Op::PromoteLocalRefCell.default_effects(),
            span,
        );
        self.mark_ref_bound_local(name);
        self.initialized_slots.insert(slot);
        self.initialized_slots.insert(owner_slot);
        owner_slot
    }

    /// Widens a concrete local to boxed `Mixed` storage, then promotes that storage to a
    /// durable heap reference cell suitable for an escaping by-reference parameter.
    pub(crate) fn promote_local_mixed_ref_cell(&mut self, name: &str, span: Option<Span>) {
        if self.is_ref_bound_local(name) && self.local_type(name).codegen_repr() == PhpType::Mixed {
            return;
        }
        if !self.is_ref_bound_local(name) {
            // Frame layout applies the final slot representation to all earlier stores.
            // Widen the slot directly: reading it as a concrete value and boxing it again
            // would introduce an unnecessary conversion owner during reference promotion.
            let slot = self.declare_local(name, PhpType::Mixed);
            self.builder.widen_local_storage_type(slot, PhpType::Mixed);
            self.set_local_type(name, PhpType::Mixed);
            self.promote_local_ref_cell(name, span);
            return;
        }
        if self.local_type(name).codegen_repr() != PhpType::Mixed {
            let source = self.load_local(name, span);
            let source_ty = self.builder.value_php_type(source.value).codegen_repr();
            let boxed = self.emit_value(
                Op::MixedBox,
                vec![source.value],
                None,
                PhpType::Mixed,
                Op::MixedBox.default_effects(),
                span,
            );
            // Final Mixed storage makes this narrower load an owned unbox. The new
            // box has already retained its payload, so retire the detached source view.
            // Ownership finalization prunes this release if the load stays borrowed.
            crate::ir_lower::ownership::release_if_owned(self, source, span);
            self.store_local(name, boxed, PhpType::Mixed, span);
            // This promotion gives the slot boxed frame storage for the whole function, so the
            // `string` load above is an unboxing COPY, and boxing it copies it again. Nothing
            // else owns the first copy: `$x = 'a'; $a[0] = &$x;` leaked one string per call.
            // Same shape as the `(_, Mixed)` loop storage contract.
            if source_ty == PhpType::Str
                && self.builder.value_defining_op(source.value) == Some(Op::LoadLocal)
            {
                crate::ir_lower::ownership::release_if_owned(self, source, span);
            }
        }
        if !self.is_ref_bound_local(name) {
            self.promote_local_ref_cell(name, span);
        }
    }

    /// Rebinds a local at the executed global declaration, retaining both sides
    /// until the new target is installed so old-value destruction cannot see
    /// an intermediate dangling binding.
    pub(crate) fn bind_global_local(&mut self, name: &str, span: Option<Span>) {
        if self.in_main || crate::superglobals::is_superglobal(name) || self.extern_globals.contains_key(name) {
            let ty = self.global_alias_type(name);
            self.declare_local_with_kind(name, ty, LocalKind::GlobalAlias);
            return;
        }
        let previous = self.local_slots.get(name).copied()
            .filter(|slot| self.initialized_slots.contains(slot))
            .map(|_| {
                let value = self.load_local(name, span);
                crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
            });
        let global = self.intern_global_storage_name(name);
        let cell = self.emit_value(
            Op::GlobalRefCell, Vec::new(), Some(Immediate::GlobalName(global)),
            PhpType::Int, Op::GlobalRefCell.default_effects(), span,
        );
        self.bind_local_ref_cell_ptr(name, cell, PhpType::Mixed, span);
        let owner = self.declare_ref_cell_owner(name, PhpType::Mixed);
        self.emit_void(
            Op::BindRefCellPtr, vec![cell.value], Some(Immediate::LocalSlot(owner)),
            Op::BindRefCellPtr.default_effects(), span,
        );
        self.initialized_slots.insert(owner);
        if let Some(previous) = previous {
            crate::ir_lower::ownership::release_if_owned(self, previous, span);
        }
    }

    /// Binds the local `name` to the persistent reference cell of the global `storage`, as
    /// `global` does: the static-by-reference binding (`static $x` that some by-reference use
    /// names, see `crate::ir_lower::static_refs`) keeps its value in a hidden global of its own.
    pub(crate) fn bind_static_reference_local(&mut self, name: &str, storage: &str, span: Option<Span>) {
        let previous = self.local_slots.get(name).copied()
            .filter(|slot| self.initialized_slots.contains(slot))
            .map(|_| {
                let value = self.load_local(name, span);
                crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
            });
        let global = self.intern_global_name(storage);
        let cell = self.emit_value(
            Op::GlobalRefCell, Vec::new(), Some(Immediate::GlobalName(global)),
            PhpType::Int, Op::GlobalRefCell.default_effects(), span,
        );
        self.bind_local_ref_cell_ptr(name, cell, PhpType::Mixed, span);
        let owner = self.declare_ref_cell_owner(name, PhpType::Mixed);
        self.emit_void(
            Op::BindRefCellPtr, vec![cell.value], Some(Immediate::LocalSlot(owner)),
            Op::BindRefCellPtr.default_effects(), span,
        );
        self.initialized_slots.insert(owner);
        if let Some(previous) = previous {
            crate::ir_lower::ownership::release_if_owned(self, previous, span);
        }
    }

    /// Lowers `$target = &$_SESSION` (any request superglobal): binds `$target` to the
    /// superglobal's shared reference cell, holding one share of it.
    ///
    /// A superglobal has no local slot -- every read and write goes to its `_eir_global_*`
    /// storage -- so the local-to-local alias promoted a fresh, empty frame slot named
    /// `_SESSION` and bound `$target` to that instead. Writes through the alias never reached
    /// the superglobal, and reads saw `null`. Symfony's `NativeSessionStorage::loadSession()`
    /// does `$session = &$_SESSION` and then binds every session bag to
    /// `$session[$key]`, so the restored login lived in a stray array while the session data
    /// itself was decoded into a `$_SESSION` the bags never saw.
    ///
    /// The cell carries the superglobal's storage representation: the bare hash under `--web`,
    /// a boxed `Mixed` elsewhere (`global_alias_type`).
    pub(crate) fn bind_superglobal_ref_alias(&mut self, target: &str, name: &str, span: Option<Span>) {
        // A superglobal's shared cell holds a boxed Mixed in every mode (`lower_global_ref_cell`),
        // which is what lets a local, an array element and a property all share it.
        let value_type = PhpType::Mixed;
        // A binding executed again (a loop body) finds its previous share in the owner slot
        // without the name being ref-bound at lowering time; release it first.
        if !self.is_ref_bound_local(target) {
            self.release_ref_cell_owner(target, span);
        }
        let global = self.intern_global_storage_name(name);
        let cell = self.emit_value(
            Op::GlobalRefCell, Vec::new(), Some(Immediate::GlobalName(global)),
            PhpType::Int, Op::GlobalRefCell.default_effects(), span,
        );
        self.bind_local_ref_cell_ptr(target, cell, value_type.clone(), span);
        let owner = self.declare_ref_cell_owner(target, value_type);
        self.emit_void(
            Op::BindRefCellPtr, vec![cell.value], Some(Immediate::LocalSlot(owner)),
            Op::BindRefCellPtr.default_effects(), span,
        );
        self.initialized_slots.insert(owner);
    }

    /// Lowers `$obj->prop = &$_SERVER` (any request superglobal): binds the property to the
    /// superglobal's own shared reference cell, so the property, the superglobal and every other
    /// alias of it are one PHP reference.
    ///
    /// The parser spells the statement as a copy into the property followed by
    /// `$_SERVER = &$obj->prop`, the form that makes a LOCAL adopt the property's cell. A
    /// superglobal has no local slot, so that form bound a stray frame slot named `_SERVER` and
    /// the property kept a copy (Monolog's `WebProcessor` reads `$this->serverData`). The share
    /// `GlobalRefCell` returns is parked in a hidden owner slot released with the frame.
    pub(crate) fn bind_property_to_superglobal_cell(
        &mut self,
        object: LoweredValue,
        property: &str,
        name: &str,
        span: Option<Span>,
    ) {
        let owner_key = format!("__elephc_superglobal_property_ref_{name}_{property}");
        self.release_ref_cell_owner(&owner_key, span);
        let global = self.intern_global_storage_name(name);
        let cell = self.emit_value(
            Op::GlobalRefCell, Vec::new(), Some(Immediate::GlobalName(global)),
            PhpType::Int, Op::GlobalRefCell.default_effects(), span,
        );
        let owner = self.declare_ref_cell_owner(&owner_key, PhpType::Mixed);
        self.emit_void(
            Op::BindRefCellPtr, vec![cell.value], Some(Immediate::LocalSlot(owner)),
            Op::BindRefCellPtr.default_effects(), span,
        );
        self.initialized_slots.insert(owner);
        let property_data = self.intern_string(property);
        self.emit_void(
            Op::BindPropRefCell,
            vec![object.value, cell.value],
            Some(Immediate::Data(property_data)),
            Op::BindPropRefCell.default_effects(),
            span,
        );
    }

    /// Lowers `$obj->prop = &$param` for a by-reference PARAMETER whose cell outlives the call:
    /// the property is bound to the parameter's own cell, which is the caller's variable.
    ///
    /// The parser spells the statement as a copy into the property followed by
    /// `$param = &$obj->prop` (`parse_complex_ref_assignment`), the form that makes a local adopt
    /// the property's cell. For a by-reference parameter that re-pointed the parameter at a
    /// fresh cell and cut it off from the caller: Symfony's `ContainerConfigurator` and
    /// `ServicesConfigurator` constructors each bind `$this->instanceof = &$instanceof`, the
    /// configurators stopped sharing the loader's array, and a later read found `null`.
    ///
    /// The checker records every call site passing a local to such a parameter
    /// (`binds_property_to_ref_param`, `record_property_bound_promotion_sites`), and the caller
    /// promotes that local to a managed cell; an omitted argument's default already is one. The
    /// cell is leased first: an address with no cell owner -- a caller that could not know --
    /// raises php's catchable reference error instead of the property keeping a dangling frame
    /// address. Both references outlive the call by the cell's own refcount. The lease is parked in
    /// a hidden owner slot released with the frame. Returns `false`, leaving the generic lowering,
    /// when `param` is not a by-reference parameter.
    pub(crate) fn bind_property_to_param_ref_cell(
        &mut self,
        object: &crate::parser::ast::Expr,
        property: &str,
        param: &str,
        span: Option<Span>,
    ) -> bool {
        let Some(&slot) = self.local_slots.get(param) else {
            return false;
        };
        if !self.builder.local_is_by_ref_parameter(slot) {
            return false;
        }
        let object = crate::ir_lower::expr::lower_expr(self, object);
        if self.builder.insertion_block_is_terminated() {
            return true;
        }
        let marker = self.load_local(param, span);
        let owner_key = format!("__elephc_param_property_ref_{param}_{property}");
        let owner = self.declare_ref_cell_owner(&owner_key, PhpType::Mixed);
        let leased = self.emit_value(
            Op::AcquireRefCell,
            vec![marker.value],
            Some(Immediate::LocalSlot(owner)),
            PhpType::Pointer(None),
            Op::AcquireRefCell.default_effects(),
            span,
        );
        self.initialized_slots.insert(owner);
        let property_data = self.intern_string(property);
        self.emit_void(
            Op::BindPropRefCell,
            vec![object.value, leased.value],
            Some(Immediate::Data(property_data)),
            Op::BindPropRefCell.default_effects(),
            span,
        );
        true
    }

    /// Binds one local name to the same ref-cell pointer as another local.
    pub(crate) fn alias_local_ref_cell(&mut self, target: &str, source: &str, span: Option<Span>) {
        if target == source {
            return;
        }
        // An existing reference can point into an object property whose payload remains raw.
        // Reboxing that shared cell here would make property reads treat a box as an object.
        if !self.is_ref_bound_local(source)
            && matches!(self.local_type(source).codegen_repr(), PhpType::Object(_))
        {
            self.promote_local_mixed_ref_cell(source, span);
        }
        let source_ty = self.local_type(source);
        // `is_ref_bound_local` is intentionally conservative across lowered
        // branches, so a source marked by a conditional predecessor may still
        // be raw on another runtime path. An idempotent promotion here gives
        // every alias operation a cell on all incoming paths.
        self.promote_local_ref_cell(source, span);
        self.clear_static_callable_local(target);
        self.forget_static_closures_capturing(target);
        self.clear_reflection_class_local(target);
        self.clear_reflection_function_local(target);
        self.clear_reflection_property_local(target);
        self.clear_reflection_method_local(target);
        self.clear_reflection_arg_array_local(target);
        self.clear_fiber_start_sig(target);
        // Declare the owner before retirement so the first syntactic binding also
        // releases an alias left by a previous execution of this loop body.
        let owner_pair = self.ref_cell_owner_slot(source).map(|source_owner| {
            // The first syntactic binding of `target` decides its owner's payload type. An owner
            // slot pre-declared for a previous binding that never existed
            // (`root_previous_reference_binding`) only carries the checker's guess for the name,
            // and widening that guess with the real payload would turn a `Mixed` guess over a raw
            // `string` cell into a boxed owner, so a store through the alias wrote a box into it.
            if !self.is_ref_bound_local(target) {
                if let Some(existing) = self.ref_cell_owner_locals.get(target).copied() {
                    self.builder.set_local_storage_type(existing, source_ty.clone());
                }
            }
            let target_owner = self.declare_ref_cell_owner(target, source_ty.clone());
            (source_owner, target_owner)
        });
        self.release_replaced_local_before_ref_alias(target, span);
        let source_slot = self.declare_local(source, source_ty.clone());
        let target_slot = self.declare_local(target, source_ty.clone());
        self.set_local_type(target, source_ty.clone());
        self.builder.emit_with_effects(
            Op::AliasLocalRefCell,
            Vec::new(),
            Some(Immediate::LocalSlotPair {
                first: target_slot,
                second: source_slot,
            }),
            IrType::Void,
            source_ty.clone(),
            Ownership::NonHeap,
            Op::AliasLocalRefCell.default_effects(),
            span,
        );
        self.mark_ref_bound_local(target);
        // An alias of a borrowed interior address is itself borrowed; a managed source clears
        // any interior marker a previous binding of this name left behind.
        if self.is_borrowed_element_ref_local(source) {
            self.mark_borrowed_element_ref_local(target);
        } else {
            self.borrowed_element_ref_locals.remove(target);
        }
        self.initialized_slots.insert(target_slot);
        if let Some((source_owner, target_owner)) = owner_pair {
            self.emit_void(
                Op::RetainLocalRefCell, Vec::new(),
                Some(Immediate::LocalSlotPair { first: source_owner, second: target_owner }),
                Op::RetainLocalRefCell.default_effects(), span,
            );
            self.initialized_slots.insert(target_owner);
        }
    }

    /// Binds `target` to a borrowed ref-cell pointer without acquiring allocation ownership.
    /// `value_type` describes the cell payload for subsequent alias loads and stores.
    ///
    /// Unlike `alias_local_ref_cell`, no hidden owner slot is created and no `ReleaseLocalRefCell`
    /// is emitted for `target` at scope exit. Managed properties and resolved reference-returning
    /// calls instead use the owning or adopting bind helpers.
    pub(crate) fn bind_local_ref_cell_ptr(
        &mut self,
        target: &str,
        cell_ptr: LoweredValue,
        value_type: PhpType,
        span: Option<Span>,
    ) {
        self.bind_ref_cell_ptr_impl(target, cell_ptr, value_type, false, false, span);
    }

    /// Retains a property cell before replacing the target, which may own the source object.
    pub(crate) fn bind_owned_local_ref_cell_ptr(
        &mut self,
        target: &str,
        cell_ptr: LoweredValue,
        value_type: PhpType,
        span: Option<Span>,
    ) {
        let staged = self.declare_synthetic_php_local(value_type.clone());
        self.bind_ref_cell_ptr_impl(&staged, cell_ptr, value_type, true, false, span);
        self.alias_local_ref_cell(target, &staged, span);
        self.release_ref_cell_owner(&staged, span);
    }

    /// Reserves the staging a reference assignment publishes before lowering its source.
    ///
    /// Only the hidden OWNER slot is declared here, because the staging local's storage type is
    /// not known until the transferred cell's payload type is. The owner slot is zero-initialized
    /// with every other ref-cell owner in the frame prologue, so publishing it in the unwind
    /// chain before anything can be adopted into it is a no-op for cleanup.
    pub(crate) fn predeclare_returned_ref_cell_staging(&mut self) -> (String, LocalSlotId) {
        let staged = self.next_synthetic_place_local_name();
        let owner = self.declare_ref_cell_owner(&staged, PhpType::Mixed);
        (staged, owner)
    }

    /// Adopts a transferred cell into staging whose owner is already published for unwinding.
    pub(crate) fn adopt_returned_ref_cell_into(
        &mut self,
        staged: &str,
        cell_ptr: LoweredValue,
        value_type: PhpType,
        span: Option<Span>,
    ) {
        self.bind_ref_cell_ptr_impl(staged, cell_ptr, value_type, true, true, span);
    }

    /// Binds a borrowed or owned cell, retiring the previous binding before publishing the new one.
    fn bind_ref_cell_ptr_impl(
        &mut self,
        target: &str,
        cell_ptr: LoweredValue,
        value_type: PhpType,
        owns_cell: bool,
        adopts_cell: bool,
        span: Option<Span>,
    ) {
        self.clear_static_callable_local(target);
        self.forget_static_closures_capturing(target);
        self.clear_fiber_start_sig(target);
        self.release_replaced_local_before_ref_alias(target, span);
        self.abandon_slot_of_another_representation(target, &value_type);
        let target_slot = self.declare_local(target, value_type.clone());
        self.set_local_type(target, value_type.clone());
        let immediate = if owns_cell {
            let owner = self.declare_ref_cell_owner(target, value_type.clone());
            self.initialized_slots.insert(owner);
            Immediate::LocalSlotPair { first: target_slot, second: owner }
        } else {
            Immediate::LocalSlot(target_slot)
        };
        let op = if adopts_cell { Op::AdoptRefCellPtr } else { Op::BindRefCellPtr };
        self.builder.emit_with_effects(
            op,
            vec![cell_ptr.value],
            Some(immediate),
            IrType::Void,
            value_type,
            Ownership::NonHeap,
            op.default_effects(),
            span,
        );
        self.mark_ref_bound_local(target);
        // The caller marks an interior binding explicitly; a plain rebinding clears the marker
        // so a name reused for a managed cell is not treated as borrowed storage.
        self.borrowed_element_ref_locals.remove(target);
        self.initialized_slots.insert(target_slot);
    }

    /// Binds `target` to a reference cell it OWNS a share of, e.g. the cell an array element
    /// became (`Op::ElemRefCell`). The caller hands over one retained share; the hidden owner slot
    /// releases it when the binding ends -- a rebind, `unset`, or the scope exit -- so the cell
    /// outlives whatever container it came from, as a PHP reference does.
    pub(crate) fn bind_owned_ref_cell_ptr(
        &mut self,
        target: &str,
        cell_ptr: LoweredValue,
        span: Option<Span>,
    ) {
        // A binding lowered once but executed repeatedly -- a loop body -- finds its own previous
        // share in the owner slot without the name being ref-bound at lowering time, which is the
        // only case `bind_local_ref_cell_ptr` releases. Release it here; a zeroed owner is a no-op.
        if !self.is_ref_bound_local(target) {
            // The owner slot may not exist yet: the FIRST lowering of this bind is the one that
            // declares it, and `release_ref_cell_owner` does nothing for a name without one. A
            // loop re-executes that very bind, so declaring first (the prologue zeroes it, which
            // makes the first run a no-op) lets the next iteration retire the share the last
            // one left behind instead of dropping it.
            self.declare_ref_cell_owner(target, PhpType::Mixed);
            self.release_ref_cell_owner(target, span);
        }
        self.bind_local_ref_cell_ptr(target, cell_ptr, PhpType::Mixed, span);
        let owner = self.declare_ref_cell_owner(target, PhpType::Mixed);
        self.builder.emit_with_effects(
            Op::BindRefCellPtr,
            vec![cell_ptr.value],
            Some(Immediate::LocalSlot(owner)),
            IrType::Void,
            PhpType::Mixed,
            Ownership::NonHeap,
            Op::BindRefCellPtr.default_effects(),
            span,
        );
        self.initialized_slots.insert(owner);
    }

    /// Gives `name` a fresh slot when its current one stores a different representation than the
    /// cell it is about to alias.
    ///
    /// A cell holds its value in ONE representation, the one its owner writes: an object's
    /// reference property keeps its declared type's (a `Hash` for an `array` property). Binding a
    /// local whose slot was declared at another one -- a by-reference `array $x` parameter is an
    /// `Array` slot -- joined the two into boxed `Mixed` storage, so later accesses through the
    /// alias read and wrote the cell as a box while the property kept reading it raw.
    /// `$this->instanceof = &$instanceof; $instanceof = [];` (Symfony's `ServicesConfigurator`)
    /// released the property's hash as a box and then stored a box into the property, and every
    /// service definition built from it later carried a boxed array into an `array` parameter.
    ///
    /// Only the NAME moves: the old slot keeps its own storage type and cleanup, exactly as for
    /// an `unset`. A name eval can address, or one backed by global storage, keeps its slot.
    fn abandon_slot_of_another_representation(&mut self, name: &str, value_type: &PhpType) {
        let Some(slot) = self.local_slots.get(name).copied() else {
            return;
        };
        if self.builder.local_php_type(slot).codegen_repr() == value_type.codegen_repr() {
            return;
        }
        let Some(kind) = self.local_kinds.get(name).copied() else {
            return;
        };
        if kind != LocalKind::PhpLocal
            || self.uses_global_storage(name, kind)
            || self.extern_globals.contains_key(name)
            || self.eval_barrier_active
            || self.eval_scope_read_param.is_some()
        {
            return;
        }
        self.abandon_local_binding(name);
    }

    /// Releases storage currently owned by a local before rebinding it as a ref alias.
    fn release_replaced_local_before_ref_alias(&mut self, name: &str, span: Option<Span>) {
        // Hidden owners start at zero but may hold a cell after a loop back-edge,
        // even before this name has been marked ref-bound during AST lowering.
        self.release_ref_cell_owner(name, span);
        if self.is_ref_bound_local(name) {
            return;
        }
        let Some(slot) = self.local_slots.get(name).copied() else {
            return;
        };
        if !self.initialized_slots.contains(&slot) {
            return;
        }
        // Retire the slot itself, avoiding a cleanup load whose representation may later widen.
        // The new cell has already been staged, so this may safely destroy the source object.
        self.release_stored_local_value_before_overwrite(name, slot, &self.builder.local_php_type(slot), span);
    }

    /// Releases a promoted fallback ref-cell owner if the variable still owns one.
    pub(crate) fn release_ref_cell_owner(&mut self, name: &str, span: Option<Span>) {
        let Some(owner_slot) = self.ref_cell_owner_slot(name) else {
            return;
        };
        let owner_ty = self.builder.local_php_type(owner_slot);
        self.invalidate_callable_facts_after_user_code(
            Op::ReleaseLocalRefCell,
            &[],
            Some(&Immediate::LocalSlot(owner_slot)),
            Op::ReleaseLocalRefCell.default_effects(),
        );
        self.builder.emit_with_effects(
            Op::ReleaseLocalRefCell,
            Vec::new(),
            Some(Immediate::LocalSlot(owner_slot)),
            IrType::Void,
            owner_ty,
            Ownership::NonHeap,
            Op::ReleaseLocalRefCell.default_effects(),
            span,
        );
    }

    /// Returns whether a value producer owns storage duplicated by a retaining consumer.
    pub(crate) fn value_is_owning_temporary(&self, value: LoweredValue) -> bool {
        // While a container write BORROWS its value operand, nothing in that write may treat
        // the value as a temporary it owns — neither the release afterwards nor any coercion
        // along the way. The `??=` result temp is the case: the slot keeps the reference and
        // hands it to the expression's consumer, so a write that consumed it left the cell at
        // zero while the container still pointed at it.
        if self.write_operand_is_borrowed {
            return false;
        }
        let php_type = self.builder.value_php_type(value.value);
        if !value.ir_type.is_refcounted_storage()
            && !Ownership::php_type_needs_lifetime_tracking(&php_type)
        {
            return false;
        }
        if matches!(
            self.builder.value_defining_instruction(value.value)
                .and_then(|inst| inst.immediate.as_ref()),
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::MixedCellPromoteToHash(_)
                    | crate::ir::RuntimeCallTarget::MixedCellPromoteAttachedToHash(_),
            ))
        ) {
            // This helper installs the hash in an owned Mixed cell and returns only a borrow of
            // that cell's payload. Unlike ArrayFetchForWrite, the generic RuntimeCall fallback
            // must not turn that borrow into a release obligation at the builtin call boundary.
            return false;
        }
        if self.value_is_owning_builtin_temporary(value.value) {
            return true;
        }
        if self.builder.value_ownership(value.value) == Ownership::Owned
            && self.builder.value_defining_op(value.value) == Some(Op::MixedUnbox)
            && (php_type.codegen_repr() == PhpType::Callable || php_type.is_refcounted())
        {
            // Callable and object coercion extract and retain a payload from physical Mixed
            // storage. Only a producer that asked for `Owned` claims that lease; every other
            // `MixedUnbox` stays the borrowed projection its consumers expect.
            return true;
        }
        if self.builder.value_ownership(value.value) == Ownership::Owned
            && self.builder.value_defining_op(value.value) == Some(Op::NormalizeCallable)
        {
            // A producer that asked for `Owned` (a callable-array argument normalized into a
            // fresh bound descriptor) hands the consumer a release obligation; the gradual
            // coercions keep the default ownership and stay out of this rule.
            return true;
        }
        if self.builder.value_ownership(value.value) == Ownership::Owned
            && self.builder.value_defining_op(value.value) == Some(Op::ScopedConstantGet)
        {
            // A scoped read may retain a cached heap value without allocating it. Its owned
            // lease still has to retire after a destination or call operand acquires it.
            return true;
        }
        if self.value_is_owned_temp_load(value.value) {
            return true;
        }
        if self.value_is_owned_unboxed_local_load(value.value) {
            return true;
        }
        if self.value_is_owning_mixed_unbox(value.value) {
            return true;
        }
        if self.value_is_owning_mixed_string_cast(value.value) {
            return true;
        }
        if self.value_is_owning_array_cast(value.value) {
            return true;
        }
        if self.value_is_owning_container_read(value.value) {
            return true;
        }
        if self.value_is_owned_index_read_temp(value) {
            return true;
        }
        if let Some(source) = self.gradual_union_param_guard_source(value.value) {
            // The guard FORWARDS its operand, so it owns exactly what the operand owned.
            let source_php_type = self.builder.value_php_type(source);
            return self.value_is_owning_temporary(LoweredValue {
                value: source,
                ir_type: value_ir_type(&source_php_type),
            });
        }
        if self.value_is_borrowed_gradual_nominal_guard(value.value) {
            return false;
        }
        if self.value_is_borrowed_user_call_result(value.value) {
            return false;
        }
        if matches!(
            self.builder.value_defining_op(value.value),
            Some(Op::PropGet | Op::DynamicPropGet | Op::NullsafePropGet)
        ) && matches!(php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        {
            return true;
        }
        if self.builder.value_defining_op(value.value) == Some(Op::Cast)
            && matches!(php_type.codegen_repr(), PhpType::Object(_))
        {
            return true;
        }
        // `(array) $scalar` boxes the scalar and rebuilds a FRESH `array<mixed>` from the cell
        // (`__rt_array_from_mixed`), so the cast result is its only owner.
        if self.builder.value_defining_op(value.value) == Some(Op::Cast)
            && matches!(php_type.codegen_repr(), PhpType::Array(ref element) if element.codegen_repr() == PhpType::Mixed)
            && self
                .builder
                .value_defining_instruction(value.value)
                .and_then(|inst| inst.operands.first().copied())
                .is_some_and(|operand| {
                    self.builder.value_php_type(operand).codegen_repr() == PhpType::Mixed
                })
        {
            return true;
        }
        // By-value foreach binds either an owned current value or an owned boxed
        // Mixed key. Concrete `Str` values are the exception: like `ArrayGet`
        // string results they borrow the source container's payload, so treating
        // them as owning would free the array's string block out from under it.
        match self.builder.value_defining_op(value.value) {
            Some(Op::IterCurrentValue) => {
                return !matches!(php_type.codegen_repr(), PhpType::Str);
            }
            Some(Op::IterCurrentKey) => return true,
            _ => {}
        }
        matches!(
            self.builder.value_defining_op(value.value),
                Some(
                    Op::Acquire
                    | Op::EnsureOwned
                    | Op::IToStr
                    | Op::FToStr
                    | Op::BoolToStr
                    | Op::ResourceToStr
                    | Op::MixedBox
                    | Op::MixedClone
                    | Op::ArrayToMixed
                    | Op::ArrayToPointerWords
                    | Op::HashToMixed
                    | Op::InvokerRefArg
                    | Op::ArrayLocalRefCell
                    | Op::MixedNumericBinop
                    | Op::StrIncDec
                    | Op::ICheckedAdd
                    | Op::ICheckedSub
                    | Op::ICheckedMul
                    | Op::ICheckedPow
            | Op::ICheckedDiv
                    | Op::MixedCastString
                    | Op::StrConcat
                    | Op::StrAppend
                    | Op::StrBitAnd
                    | Op::StrBitOr
                    | Op::StrBitXor
                    | Op::StrSetOffset
                    | Op::StrPersist
                    | Op::StrInterpolate
                    | Op::ArrayNew
                    | Op::HashNew
                    | Op::ArrayCloneShallow
                    | Op::HashCloneShallow
                    | Op::ArrayUnion
                    | Op::HashUnion
                    | Op::ArrayHashUnion
                    | Op::HashArrayUnion
                    | Op::ArrayToHash
                    | Op::MixedToHash
                    | Op::MixedUnbox
                    | Op::ObjectNew
                    | Op::ObjectCloneShallow
                    | Op::DynamicObjectNew
                    | Op::DynamicObjectNewMixed
                    | Op::DynamicObjectNewWithoutConstructorMixed
                    | Op::ReflectionMaterialize
                    | Op::ClosureNew
                    // Binding creates a fresh descriptor that owns its receiver capture.
                    | Op::ClosureBind
                    | Op::FirstClassCallableNew
                    | Op::CallableArrayNew
                    | Op::BufferNew
                    | Op::GeneratorNew
                    | Op::CatchBind
                    // `yield`/`yield from` return owned Mixed cells (the sent
                    // value from `__rt_gen_suspend`, the delegated return from
                    // `__rt_gen_delegate`); a discarded result must be released.
                    | Op::GeneratorYield
                    | Op::GeneratorYieldFrom
                    | Op::Call
                    | Op::FunctionVariantCall
                    | Op::EvalLiteralCall
                    | Op::EvalFunctionCall
                    | Op::EvalFunctionCallArray
                    | Op::EvalConstantFetch
                    | Op::EvalStaticMethodCall
                    | Op::RuntimeCall
                    | Op::ExternCall
                    | Op::MethodCall
                    | Op::NullsafeMethodCall
                    | Op::StaticMethodCall
                    | Op::ClosureCall
                    | Op::CallableDescriptorInvoke
                    | Op::ExprCall
                    | Op::PipeCall
                    | Op::IteratorMethodCall
                    | Op::SplRuntimeCall
                    | Op::FiberRuntimeCall
            )
        )
    }

    /// Publishes lowering's owning-temporary proof into final EIR ownership metadata.
    ///
    /// Ordinary local loads stay conservative because their provisional ownership is
    /// repaired separately after final slot widening. One-shot `OwnedTemp` loads are
    /// exact and can be promoted along with non-load producers. String results stay
    /// conservative because `Owned` cannot distinguish heap strings from concat scratch
    /// storage; their Mixed-box transfer remains classified by the string-specific path.
    pub(crate) fn finalize_value_ownership_metadata(&mut self) {
        let owned_values = (0..self.builder.value_count())
            .filter_map(|raw| {
                let value = ValueId::from_raw(raw as u32);
                if self.builder.value_ownership(value) != Ownership::MaybeOwned {
                    return None;
                }
                if self.builder.value_php_type(value).codegen_repr() == PhpType::Str {
                    return None;
                }
                let op = self.builder.value_defining_op(value);
                if matches!(op, Some(Op::LoadLocal | Op::LoadStaticLocal | Op::LoadRefCell))
                    && !self.value_is_owned_temp_load(value)
                {
                    return None;
                }
                let lowered = LoweredValue {
                    value,
                    ir_type: self.builder.value_type(value),
                };
                self.value_is_owning_temporary(lowered).then_some(value)
            })
            .collect::<Vec<_>>();
        for value in owned_values {
            self.builder.set_value_ownership(value, Ownership::Owned);
        }
    }

    /// Returns whether a user-call result can alias a borrowed visible argument.
    ///
    /// The argument of `value`'s defining call that was read from `slot`, if any.
    ///
    /// This is the accumulator pattern `$acc = f(..., $acc, ...)` seen from the store, and it
    /// needs NO alias summary — which is what makes it work for an INTERFACE call, where the
    /// callee is unknown and no summary exists. Symfony's
    /// `Config\Definition\Processor::process()` is exactly this shape:
    /// `$currentConfig = $configTree->merge($currentConfig, $config);`.
    fn call_argument_read_from_slot(&self, value: ValueId, slot: LocalSlotId) -> Option<ValueId> {
        let inst = self.builder.value_defining_instruction(value)?;
        if !matches!(
            inst.op,
            Op::Call | Op::MethodCall | Op::NullsafeMethodCall | Op::StaticMethodCall
        ) {
            return None;
        }
        inst.operands
            .iter()
            .copied()
            .find(|operand| self.value_source_slot(*operand) == Some(slot))
    }

    /// The argument a user-call result MAY alias, when the callee's summary says it might hand
    /// that parameter back.
    ///
    /// `value_is_borrowed_user_call_result` deliberately demands a PROVEN alias, because it
    /// declares the result borrowed on every path. A callee that returns its parameter on only
    /// ONE branch — `if (!$r) { return $l; } return $l + $r;` — is neither: the result is owned
    /// on one path and borrowed on the other, so the release has to be decided at RUNTIME.
    pub(crate) fn user_call_result_may_alias_argument(&self, result: ValueId) -> Option<ValueId> {
        let inst = self.builder.value_defining_instruction(result)?;
        if inst.op != Op::Call {
            return None;
        }
        let Some(Immediate::Data(function_id)) = inst.immediate else {
            return None;
        };
        let function_name = self
            .data
            .function_names
            .get(function_id.as_raw() as usize)?;
        let return_alias = self.return_alias_summaries.function(function_name)?;
        inst.operands
            .iter()
            .enumerate()
            .find(|(parameter_index, argument)| {
                return_alias.may_alias_parameter(*parameter_index)
                    && !return_alias.proven_aliases_parameter(*parameter_index)
                    && self.call_result_may_alias_arg(**argument, result)
                    && !self.boxed_param_cannot_become_result(function_name, *parameter_index, result)
                    // An owned argument that comes back unchanged transfers its reference
                    // to the result: argument cleanup already skips that alias. Suppressing
                    // result cleanup after a retaining store as well would leak the owner.
                    && !self.value_is_owning_temporary(LoweredValue {
                        value: **argument,
                        ir_type: self.builder.value_type(**argument),
                    })
            })
            .map(|(_, argument)| *argument)
    }

    /// Callee-owned entry shadows and by-value reads from reference parameters return independent owners.
    /// Other refcounted parameters may forward a borrowed argument, or transfer
    /// the owner of a temporary argument through the call. Function calls are answered from the
    /// function's summary here; method, static-method and nullsafe results are recorded in
    /// `borrowed_call_results` when their arguments are released, where the summary is at hand.
    fn value_is_borrowed_user_call_result(&self, result: ValueId) -> bool {
        if self.borrowed_call_results.contains(&result) {
            return true;
        }
        let Some(inst) = self.builder.value_defining_instruction(result) else {
            return false;
        };
        if inst.op != Op::Call {
            return false;
        }
        let Some(Immediate::Data(function_id)) = inst.immediate else {
            return false;
        };
        let Some(function_name) = self.data.function_names.get(function_id.as_raw() as usize)
        else {
            return false;
        };
        if matches!(self.builder.value_php_type(result).codegen_repr(), PhpType::Object(_))
            && self.functions.get(function_name).is_some_and(|sig| !sig.by_ref_return)
        {
            return false;
        }
        let Some(return_alias) = self.return_alias_summaries.function(function_name) else {
            return false;
        };
        inst.operands
            .iter()
            .enumerate()
            .any(|(parameter_index, argument)| {
                if self.functions.get(function_name)
                    .is_some_and(|signature| signature.returned_parameter_has_independent_owner(parameter_index))
                    || !return_alias.proven_aliases_parameter(parameter_index)
                    || !self.call_result_may_alias_arg(*argument, result)
                    || self.boxed_param_cannot_become_result(function_name, parameter_index, result)
                {
                    return false;
                }
                !self.call_argument_is_caller_temporary(*argument)
            })
    }

    /// Returns whether a call argument is a temporary the caller owns, so a callee that hands
    /// it straight back transfers that owner through its result.
    ///
    /// A plain load of a PHP local reads as an owning temporary, but that answer is
    /// PROVISIONAL: it exists so a later store can widen the slot, and the argument's own
    /// release is repaired at builder finalization. The caller owes no release on the load
    /// itself — the local's lifetime covers it — so for the question here it is the wrong
    /// answer. Taking it made the caller release a class-typed value the callee handed back at
    /// `+0` (issue #1203). A freshly built operand is not a local load and still refuses, which
    /// is what #486 needs. A concrete string projected from final Mixed storage is the
    /// exception: its cast detached an owned string, whose lease flows through a passthrough.
    ///
    /// A string view a builtin returned into its argument (`dirname($path)`, `trim($s)`) owns
    /// nothing of its own: it is a caller temporary only when its source is. Counting the view
    /// itself made the caller release `same(dirname($path))`'s result, whose pointer is the
    /// base of `$path`'s block, and free the caller's string.
    pub(crate) fn call_argument_is_caller_temporary(&self, argument: ValueId) -> bool {
        if let Some(sources) = self.borrowed_string_view_sources(argument) {
            return sources
                .into_iter()
                .any(|source| self.call_argument_is_caller_temporary(source));
        }
        let lowered = LoweredValue {
            value: argument,
            ir_type: self.builder.value_type(argument),
        };
        self.value_is_owning_temporary(lowered)
            && (!self.value_is_owned_unboxed_local_load(argument)
                || self.builder.value_php_type(argument).codegen_repr() == PhpType::Str)
    }

    /// Returns the lifetime-tracked operands of a string a runtime builtin returned without
    /// owning it (a view into, or an alias of, its arguments), or `None` for any other value.
    fn borrowed_string_view_sources(&self, value: ValueId) -> Option<Vec<ValueId>> {
        if self.builder.value_php_type(value).codegen_repr() != PhpType::Str {
            return None;
        }
        let inst = self.builder.value_defining_instruction(value)?;
        if inst.op != Op::RuntimeCall {
            return None;
        }
        let target = match inst.immediate {
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target)))
            | Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ProfiledFunction {
                target,
                ..
            })) => target,
            _ => return None,
        };
        if !matches!(
            target.result_ownership(),
            crate::builtins::semantics::BuiltinResultOwnership::MayAliasArguments
                | crate::builtins::semantics::BuiltinResultOwnership::Borrowed
                | crate::builtins::semantics::BuiltinResultOwnership::Aliases(_)
        ) {
            return None;
        }
        Some(
            inst.operands
                .iter()
                .copied()
                .filter(|operand| {
                    Ownership::php_type_needs_lifetime_tracking(&self.builder.value_php_type(*operand))
                })
                .collect(),
        )
    }

    /// Returns whether user function `function_name` receives parameter `index` as a boxed cell
    /// that its STRING result cannot be.
    ///
    /// The callee sees a box -- one made at the call boundary when the argument is concrete -- and
    /// every way a body turns a box into a string hands back its own reference (see
    /// `arg_and_result_types_can_alias`). The summary cannot tell: it is built from the AST, where
    /// `return (string) $x;` returns `$x`. Believing it declared the result BORROWED from the
    /// argument, so the caller never released it: `function f($x) { return (string) $x; }`
    /// leaked the cast's copy on every call made with a variable or a literal.
    ///
    /// A parameter is boxed when it is declared `mixed`, and when it is an untyped by-value one:
    /// `ir_lower::program::metadata::dynamic_untyped_param_names` gives those the boxed ABI
    /// whatever type the checker inferred from the call sites.
    fn boxed_param_cannot_become_result(
        &self,
        function_name: &str,
        index: usize,
        result: ValueId,
    ) -> bool {
        if self.builder.value_php_type(result).codegen_repr() != PhpType::Str {
            return false;
        }
        let Some(signature) = self.functions.get(function_name) else {
            return false;
        };
        let Some((name, php_type)) = signature.params.get(index) else {
            return false;
        };
        if matches!(php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)) {
            return true;
        }
        let declared = signature.declared_params.get(index).copied().unwrap_or(false);
        let by_ref = signature.ref_params.get(index).copied().unwrap_or(false);
        let variadic = signature.variadic.as_deref() == Some(name.as_str());
        let callable = matches!(php_type.codegen_repr(), PhpType::Callable)
            || self
                .callable_param_sigs
                .contains_key(&(function_name.to_string(), name.clone()));
        !declared && !by_ref && !variadic && !callable
    }

    /// Returns whether a call result can legally reuse one argument's refcounted payload.
    ///
    /// This is the conservative check used on the *may-alias* path (a callee whose
    /// summary is `Unknown` or only possibly returns the parameter). A freshly boxed
    /// checked-arithmetic operand is excluded here: without a proof that the callee
    /// hands it back, the caller must still release that owning temporary after the
    /// call, or it leaks once per call (issue #486). When the callee is *proven* to
    /// return the parameter, callers use [`Self::arg_and_result_types_can_alias`]
    /// instead, which admits those fresh boxes (issue #604).
    pub(crate) fn call_result_may_alias_arg(&self, argument: ValueId, result: ValueId) -> bool {
        if matches!(
            self.builder.value_defining_op(argument),
            Some(
                Op::MixedNumericBinop
                    | Op::StrIncDec
                    | Op::ICheckedAdd
                    | Op::ICheckedSub
                    | Op::ICheckedMul
                    | Op::ICheckedPow
            | Op::ICheckedDiv
            )
        ) {
            return false;
        }
        self.arg_and_result_types_can_alias(argument, result)
    }

    /// Returns whether the argument and result runtime types permit a shared payload.
    ///
    /// This is the pure type-compatibility half of [`Self::call_result_may_alias_arg`],
    /// without the fresh-arithmetic-box exclusion. It is consulted on the proven-return
    /// path: a callee proven to return this parameter (`return $x;`) genuinely hands the
    /// same box straight back even when the argument was a freshly boxed `$i + 1`, so the
    /// caller must not also release it as an argument temporary (issue #604).
    pub(crate) fn arg_and_result_types_can_alias(
        &self,
        argument: ValueId,
        result: ValueId,
    ) -> bool {
        let argument_type = self.builder.value_php_type(argument);
        let result_type = self.builder.value_php_type(result);
        // A declared PHP array has boxed storage, but its container cannot be the
        // object, string or callable passed to the call. Array children own their
        // references separately, so they do not justify keeping that argument alive.
        let may_contain_array_payload = |ty: &PhpType| matches!(
            ty.codegen_repr(),
            PhpType::Array(_) | PhpType::AssocArray { .. } | PhpType::Iterable | PhpType::Mixed
        );
        if (argument_type.is_php_array() && !may_contain_array_payload(&result_type))
            || (result_type.is_php_array() && !may_contain_array_payload(&argument_type))
        {
            return false;
        }
        let argument_type = argument_type.codegen_repr();
        let result_type = result_type.codegen_repr();
        if !Ownership::php_type_needs_lifetime_tracking(&argument_type)
            || !Ownership::php_type_needs_lifetime_tracking(&result_type)
        {
            return false;
        }
        // A string result reaches the caller through `__rt_str_persist`, which adopts a block in
        // place only when it is a `.`-operator heap temporary; every other source is duplicated.
        // The copy `(string) $mixed` hands an argument is never such a temporary, so no callee
        // can return it: `function f(string $s) { return $s; }` given `f($untyped)` leaked it.
        if result_type == PhpType::Str && self.value_is_owning_mixed_string_cast(argument) {
            return false;
        }
        match (&argument_type, &result_type) {
            // A string result is never a boxed argument's storage: every way a body turns a
            // boxed value into a string hands back its OWN reference -- `__rt_mixed_cast_string`
            // persists a copy of a string payload and formats every other tag into scratch, and
            // `MixedUnbox` increfs the payload it returns. Answering "may alias" here made the
            // caller skip releasing the box it created at the call boundary, so
            // `function f(mixed $x) { return (string) $x; }` leaked the box and its payload on
            // every call.
            (PhpType::Mixed | PhpType::Union(_), PhpType::Str) => false,
            // The converse holds for the same reason: boxing a string always persists a copy the
            // box owns (`__rt_mixed_from_value_string`), so a boxed result never IS a string
            // argument's storage. A prelude helper returning its `string $subject` parameter in
            // a `mixed` result kept the caller from releasing the `(string)` copy it passed.
            (PhpType::Str, PhpType::Mixed | PhpType::Union(_)) => false,
            // Nor is a boxed result ever a callable's or an object's storage: a box is its own
            // cell and boxing RETAINS a descriptor or an object (`__rt_mixed_from_value`), so a
            // result that wraps the argument already holds its own reference. Answering "may
            // alias" kept every closure passed to a function that returns a callback's result
            // alive: `function f(callable $c) { return $c(1); } f(fn($k) => $k);` leaked the
            // closure object per call.
            (PhpType::Callable | PhpType::Object(_), PhpType::Mixed | PhpType::Union(_)) => false,
            (PhpType::Mixed | PhpType::Union(_), _)
            | (_, PhpType::Mixed | PhpType::Union(_)) => true,
            (PhpType::Object(_), PhpType::Object(_)) => true,
            (PhpType::Array(_), PhpType::Array(_)) => true,
            (
                PhpType::AssocArray { .. },
                PhpType::AssocArray { .. } | PhpType::Array(_) | PhpType::Iterable,
            ) => true,
            (
                PhpType::Iterable,
                PhpType::Iterable
                | PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_),
            ) => true,
            (PhpType::Array(_) | PhpType::Object(_), PhpType::Iterable) => true,
            (PhpType::Str, PhpType::Str) => true,
            (PhpType::Callable, PhpType::Callable) => true,
            (PhpType::Buffer(_), PhpType::Buffer(_)) => true,
            _ => argument_type == result_type,
        }
    }

    /// Returns whether the value is an owning read from a one-shot hidden expression temp.
    ///
    /// Argument-evaluation ledgers deliberately expose a borrowed read while the slot owns
    /// the retained lease. That explicit metadata is authoritative until the ledger takes
    /// the slot, so coercion and staging must not consume the borrowed read independently.
    pub(crate) fn value_is_owned_temp_load(&self, value: ValueId) -> bool {
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        if inst.op != Op::LoadLocal {
            return false;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return false;
        };
        self.builder.local_kind(slot) == LocalKind::OwnedTemp
            && self.builder.value_ownership(value) != Ownership::Borrowed
    }

    /// Returns whether a concrete local heap load may require an owned-unbox release.
    ///
    /// Later source-order stores can widen the final frame slot after this load has
    /// already been lowered. Array/hash/object/iterable loads are therefore treated as
    /// provisional owners; builder finalization removes their emitted releases if the
    /// slot stays concrete. String and callable loads use the eager answer because assignment has
    /// a separate move-vs-retain decision that cannot be repaired by pruning a release.
    ///
    /// Callers that *publish* the pointer without consuming the local's ownership
    /// (notably `throw $e`) must still retain: the slot remains an owner after the load.
    pub(crate) fn value_is_owned_unboxed_local_load(&self, value: ValueId) -> bool {
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        if !matches!(inst.op, Op::LoadLocal | Op::LoadStaticLocal) {
            return false;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return false;
        };
        // Parameters borrow their caller's refcounted payload. Returning one must therefore
        // pass through `acquire_borrowed_return_value()` instead of treating the load as a
        // movable local owner. Mutable array parameters first rebind to a non-parameter COW
        // shadow slot, so that independent owner remains eligible below.
        if self.builder.local_is_parameter(slot) {
            return false;
        }
        if !matches!(
            self.builder.local_kind(slot),
            LocalKind::PhpLocal | LocalKind::StaticLocal
        ) {
            return false;
        }
        let storage_type = self.builder.local_php_type(slot).codegen_repr();
        let result_type = self.builder.value_php_type(value).codegen_repr();
        if matches!(
            result_type,
            PhpType::Array(_)
                | PhpType::AssocArray { .. }
                | PhpType::Object(_)
                | PhpType::Iterable
        ) {
            return true;
        }
        matches!(storage_type, PhpType::Mixed | PhpType::Union(_))
            && matches!(result_type, PhpType::Str | PhpType::Callable)
    }

    /// Returns whether a `MixedUnbox` already handed back a reference of its own.
    ///
    /// `lower_mixed_unbox` ends in `emit_unbox_mixed_to_owned_refcounted_result`, so the payload
    /// it produces is OWNED, not a borrow of the box. Reading it as a borrow made the store that
    /// consumes it acquire a second reference, which the single slot release in the epilogue
    /// never balanced — the unboxed object, and the whole subtree it owns, stayed alive for the
    /// rest of the process.
    ///
    /// `coerce_typed_assign_value` is the only emitter, and it has two callers — a typed local
    /// (`lower_typed_assign`) and a typed property write (`lower_property_assign`) — so this says
    /// one thing about both: a typed target initialized from a Mixed source already owns the value
    /// it was handed. The property path has a separate leak of its own that predates this and is
    /// unchanged by it (#1115): measured at three blocks per assignment with the predicate both on
    /// and off.
    fn value_is_owning_mixed_unbox(&self, value: ValueId) -> bool {
        self.builder.value_defining_op(value) == Some(Op::MixedUnbox)
    }

    /// Returns the operand a gradual UNION parameter guard forwards, when `value` is one.
    ///
    /// `guard_gradual_union_param` emits a `RuntimeCall` carrying an `Immediate::TypeName` — the
    /// only producer of that immediate — for an argument whose declared parameter is a scalar/null
    /// union such as `?string`. Codegen (`lower_gradual_union_param_guard`) checks the boxed value's
    /// active tag against the declared members and, on the accepted path, hands BACK THE SAME CELL:
    /// it never acquires and never allocates. The result's PHP type is a union, so
    /// `Ownership::for_php_type` marks it `MaybeOwned` and the generic `Op::RuntimeCall` arm of
    /// `value_is_owning_temporary` then classified it as an owning temporary. The call-argument
    /// cleanup therefore released a cell the guard had only borrowed: for `new H($n)` with a
    /// `?string` parameter and a branch-retyped `$n`, the constructor's `prop_set` incref and that
    /// spurious release cancelled out, so the caller frame's own release at scope exit freed the
    /// cell while the object's property still pointed at it. The property then read recycled
    /// memory — `int(9223372036854775806)` once the free list overwrote the tag word.
    ///
    /// Because the guard is a pure forward, its ownership is exactly its operand's: an owning
    /// temporary source (a `mixed` call result) still transfers through it and must be released
    /// after the call, while a borrowed local load must not be.
    fn gradual_union_param_guard_source(&self, value: ValueId) -> Option<ValueId> {
        let inst = self.builder.value_defining_instruction(value)?;
        if inst.op != Op::RuntimeCall
            || !matches!(
                inst.immediate,
                Some(Immediate::TypeName(_) | Immediate::ParameterType { .. })
            )
            || inst.operands.len() != 1
        {
            return None;
        }
        inst.operands.first().copied()
    }

    /// Returns whether a nullable nominal guard forwards its input Mixed cell unchanged.
    ///
    /// A gradual object-or-null parameter guard validates the tag and class, then returns the
    /// original boxed cell when the declared ABI remains Mixed. That result borrows the caller's
    /// local; only a guard that narrows to a concrete object acquires an independent payload
    /// reference. Treating this forwarding `RuntimeCall` as owned makes parent-constructor calls
    /// release the callee parameter's backing cell while a typed property still stores it.
    fn value_is_borrowed_gradual_nominal_guard(&self, value: ValueId) -> bool {
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        if inst.op != Op::RuntimeCall
            || !matches!(inst.immediate, Some(Immediate::NominalObject { .. }))
            || !matches!(self.builder.value_php_type(value).codegen_repr(), PhpType::Mixed | PhpType::Union(_))
        {
            return false;
        }
        let Some(source) = inst.operands.first().copied() else {
            return false;
        };
        matches!(
            self.builder.value_php_type(source).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    }

    /// Returns the fresh Mixed box a borrowed nominal guard forwards, when this expression made it.
    ///
    /// `f(new C())` against a `?C` PARAMETER boxes the object for the Mixed ABI and then runs the
    /// nullable nominal guard over that box. The guard FORWARDS the same cell
    /// (`value_is_borrowed_gradual_nominal_guard`), so the value the call receives is BORROWED and
    /// the post-call cleanup — which asks only about the argument itself — releases nothing. The
    /// box, and the object inside it, stay one reference above zero forever: `__destruct` never
    /// runs. A non-nullable `C` parameter takes no guard and was always balanced.
    ///
    /// Only a box the ARGUMENT EXPRESSION itself minted is returned. A guard over a local load —
    /// a callee forwarding its own `?ParentLink $parent` to `parent::__construct` — borrows
    /// storage the frame still owns, and releasing that is exactly the use-after-free
    /// `test_nullable_nominal_parent_constructor_parameter_keeps_its_boxed_owner` pins.
    pub(crate) fn owning_box_behind_borrowed_nominal_guard(
        &self,
        value: LoweredValue,
    ) -> Option<LoweredValue> {
        if !self.value_is_borrowed_gradual_nominal_guard(value.value) {
            return None;
        }
        // Guards chain (a `?Throwable` parameter guards its argument twice), each forwarding the
        // same cell, so peel every forwarding guard down to the cell the argument produced.
        let mut source = self
            .builder
            .value_defining_instruction(value.value)
            .and_then(|inst| inst.operands.first().copied())?;
        while self.value_is_borrowed_gradual_nominal_guard(source) {
            source = self
                .builder
                .value_defining_instruction(source)
                .and_then(|inst| inst.operands.first().copied())?;
        }
        // The cell is the argument's own when the expression minted it: a fresh box, or the
        // owned Mixed result of a call (`f($e->getPrevious())` against `?Throwable $p` left the
        // previous exception one reference high, so it was never destroyed).
        if !matches!(
            self.builder.value_defining_op(source),
            Some(Op::MixedBox | Op::MethodCall | Op::Call | Op::StaticMethodCall | Op::PropGet)
        ) {
            return None;
        }
        let php_type = self.builder.value_php_type(source);
        let boxed = LoweredValue {
            value: source,
            ir_type: value_ir_type(&php_type),
        };
        self.value_is_owning_temporary(boxed).then_some(boxed)
    }

    /// Returns whether a generic cast owns a detached string copy of a Mixed operand.
    fn value_is_owning_mixed_string_cast(&self, value: ValueId) -> bool {
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        if inst.op != Op::Cast || inst.immediate != Some(Immediate::CastTarget(IrType::Str)) {
            return false;
        }
        let Some(source) = inst.operands.first().copied() else {
            return false;
        };
        // An object source converts through `__toString()`, whose result is the caller's own
        // string like any user call's: `"x" . $obj` dropped it unreleased after the concat.
        matches!(
            self.builder.value_php_type(source).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_) | PhpType::Object(_)
        )
    }

    /// Returns whether an array cast produced fresh or freshly boxed owning storage.
    fn value_is_owning_array_cast(&self, value: ValueId) -> bool {
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        inst.op == Op::Cast
            && matches!(
                inst.immediate,
                Some(Immediate::CastTarget(IrType::Heap(
                    crate::ir::IrHeapKind::Hash | crate::ir::IrHeapKind::Mixed
                )))
            )
    }

    /// Returns whether a retained local/global store should release its source value.
    pub(crate) fn value_needs_release_after_retaining_store(&self, value: LoweredValue) -> bool {
        self.value_needs_release_after_use(value)
    }

    /// Includes local-load conversions without claiming their ownership can move.
    ///
    /// A later store or eval barrier can widen a local to Mixed after a consumer is lowered.
    /// Reading it then allocates a detached string or retains its callable payload. Conversely,
    /// a concrete slot read as Mixed allocates a fresh box. Emit provisional cleanup and let
    /// finalization prune it when the final storage instead lends the value.
    pub(crate) fn value_needs_release_after_use(&self, value: LoweredValue) -> bool {
        if self.write_operand_is_borrowed {
            return false;
        }
        if self.value_is_owning_temporary(value) {
            return true;
        }
        // A `char*` extern return is copied into an owned PHP string by
        // `__rt_cstr_to_str`, so an extern string handed straight to another call must be
        // released after that call. String temporaries are excluded from the generic
        // owning-temporary proof (concat scratch shares the same representation), which
        // otherwise leaked one copy per bridge call.
        if self.builder.value_defining_op(value.value) == Some(Op::ExternCall)
            && self.builder.value_php_type(value.value).codegen_repr() == PhpType::Str
        {
            return true;
        }
        let result_type = self.builder.value_php_type(value.value).codegen_repr();
        if !matches!(result_type, PhpType::Str | PhpType::Callable | PhpType::Mixed) {
            return false;
        }
        let Some(inst) = self.builder.value_defining_instruction(value.value) else {
            return false;
        };
        // A byte view owns no allocation, but a later argument can replace its source.
        // Preserve source-evaluation pinning; backend release skips the borrowed view itself.
        if inst.op == Op::StrCharAt { return true; }
        if !matches!(inst.op, Op::LoadLocal | Op::LoadStaticLocal) {
            return false;
        }
        let Some(Immediate::LocalSlot(slot)) = inst.immediate else {
            return false;
        };
        matches!(self.builder.local_kind(slot), LocalKind::PhpLocal | LocalKind::StaticLocal)
            && (result_type != PhpType::Mixed
                || crate::ir::local_load_coercion_owns_result(
                    &self.builder.local_php_type(slot), &result_type,
                ))
    }

    /// Returns whether a container read now owns a caller reference.
    fn value_is_owning_container_read(&self, value: ValueId) -> bool {
        let php_type = self.builder.value_php_type(value);
        let php_type = php_type.codegen_repr();
        let op = self.builder.value_defining_op(value);
        // A callable descriptor is not `is_refcounted()`, but the array and hash getters retain
        // it all the same; read as borrowed, it was acquired again and the element read's own
        // reference leaked, so a closure read out of an array (`$fs[0]()`) outlived the array.
        (matches!(php_type, PhpType::Mixed | PhpType::Union(_) | PhpType::Callable)
            || (php_type.is_refcounted() && php_type != PhpType::Str))
            && matches!(
                op,
                Some(
                    Op::ArrayGet
                        | Op::ArrayGetForWrite
                        | Op::ArrayGetMixedKeyForWrite
                        | Op::HashGet
                        | Op::HashGetSilent
                        | Op::HashGetForWrite
                )
            )
    }

    /// Returns whether an index-read receiver is itself an owned intermediate
    /// produced by an index read, i.e. the inner step of a chained subscript
    /// read such as `$a[$i][$j]`.
    ///
    /// Container reads of refcounted or boxed-Mixed elements return a +1 caller
    /// reference (the `array_get`/`hash_get` emitters incref pointer payloads and
    /// box Mixed cells), so when that result is consumed directly as the receiver
    /// of another index read there is no local slot whose release machinery would
    /// ever drop the reference — the consuming read must release it explicitly.
    /// String results are excluded: they are borrowed pointers into the container
    /// payload and carry no reference of their own.
    pub(crate) fn value_is_owned_index_read_temp(&self, value: LoweredValue) -> bool {
        let php_type = self.builder.value_php_type(value.value).codegen_repr();
        if !(matches!(php_type, PhpType::Mixed | PhpType::Union(_))
            || (php_type.is_refcounted() && php_type != PhpType::Str))
        {
            return false;
        }
        matches!(
            self.builder.value_defining_op(value.value),
            Some(
                Op::ArrayGet
                    | Op::ArrayGetSilent
                    | Op::ArrayGetForWrite
                    | Op::HashGet
                    | Op::HashGetSilent
                    | Op::HashGetForWrite
                    | Op::ArrayGetMixedKey
                    | Op::ArrayGetMixedKeySilent
                    | Op::ArrayGetMixedKeyForWrite
            )
        )
    }

    /// Returns true for typed builtin calls whose result is newly allocated for the caller.
    fn value_is_owning_builtin_temporary(&self, value: ValueId) -> bool {
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        // The specialized `filter_var$*` filters box a FRESH result (a parsed int, bool or float,
        // or a copied string) and never hand an argument back. Not counted as owned, the return
        // path acquired it a second time: two leaked blocks per `filter_var($s, FILTER_VALIDATE_IP)`.
        if inst.op == Op::LanguageConstructCall {
            if let Some(Immediate::Data(data)) = inst.immediate {
                return self
                    .data
                    .function_names
                    .get(data.as_raw() as usize)
                    .is_some_and(|name| name.starts_with("filter_var$"));
            }
        }
        match inst.immediate {
            Some(Immediate::I64(selector)) if inst.op == Op::CoreBuiltin => {
                crate::ir::CoreBuiltinOp::from_i64(selector).is_some_and(|operation| {
                    matches!(
                        operation.result_ownership(),
                        crate::builtins::semantics::BuiltinResultOwnership::Fresh
                    )
                })
            }
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::ArrayFetchForWrite
                    | crate::ir::RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed
                    | crate::ir::RuntimeCallTarget::PendingAppendFetchForWrite
                    | crate::ir::RuntimeCallTarget::PendingAppendFetchForWriteAlreadyDiagnosed,
            )) => matches!(inst.result_php_type.codegen_repr(), PhpType::Mixed | PhpType::Union(_)),
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::DynamicInclude { .. },
            )) => true,
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))) => {
                matches!(
                    target.result_ownership(),
                    crate::builtins::semantics::BuiltinResultOwnership::Fresh
                )
            }
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Pcntl(target))) => {
                matches!(
                    target.result_ownership(),
                    crate::builtins::semantics::BuiltinResultOwnership::Fresh
                )
            }
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. },
            )) => matches!(
                target.result_ownership(),
                crate::builtins::semantics::BuiltinResultOwnership::Fresh
            ),
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::UnaryString(_)
                | crate::ir::RuntimeCallTarget::StringOffsetSet,
            )) => true,
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ArrayUnpackToHash)) => true,
            Some(Immediate::Data(name_id) | Immediate::ProfiledData { data: name_id, .. })
                if inst.op == Op::LanguageConstructCall => self
                .data
                .function_names
                .get(name_id.as_raw() as usize)
                .is_some_and(|name| php_symbol_key(name.trim_start_matches('\\')) == "eval"),
            _ => false,
        }
    }

    /// Returns true when straight-line callable binding metadata is safe for a local.
    pub(crate) fn can_track_static_callable_local(&self, name: &str) -> bool {
        let kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        !self.uses_global_storage(name, kind) && kind == LocalKind::PhpLocal
    }

    /// Records that a PHP local currently holds a compile-time-known callable.
    pub(crate) fn bind_static_callable_local(&mut self, name: &str, target: StaticCallableBinding) {
        let top_level_program_global = self.in_main
            && self.all_global_var_names.contains(name)
            && self
                .local_kinds
                .get(name)
                .copied()
                .unwrap_or(LocalKind::PhpLocal)
                == LocalKind::PhpLocal;
        if self.can_track_static_callable_local(name) || top_level_program_global {
            self.static_callable_local_epochs
                .entry(name.to_string())
                .or_default();
            self.static_callable_locals.insert(name.to_string(), target);
        }
    }

    /// Returns the compile-time callable currently associated with a local, if any.
    pub(crate) fn static_callable_local(&self, name: &str) -> Option<StaticCallableBinding> {
        self.static_callable_locals.get(name).cloned()
    }

    /// Records that a PHP local currently holds a statically-known `ReflectionClass` object.
    pub(crate) fn bind_reflection_class_local(&mut self, name: &str, reflected_class: String) {
        if self.can_track_static_callable_local(name) {
            self.reflection_class_locals
                .insert(name.to_string(), reflected_class);
        }
    }

    /// Returns the reflected class associated with a local `ReflectionClass`, if known.
    pub(crate) fn reflection_class_local(&self, name: &str) -> Option<String> {
        self.reflection_class_locals.get(name).cloned()
    }

    /// Records that a PHP local currently holds a statically-known `ReflectionFunction`.
    pub(crate) fn bind_reflection_function_local(
        &mut self,
        name: &str,
        reflected_function: String,
    ) {
        if self.can_track_static_callable_local(name) {
            self.reflection_function_locals
                .insert(name.to_string(), reflected_function);
        }
    }

    /// Returns the reflected function associated with a local `ReflectionFunction`.
    pub(crate) fn reflection_function_local(&self, name: &str) -> Option<String> {
        self.reflection_function_locals.get(name).cloned()
    }

    /// Records that a PHP local currently holds a statically-known `ReflectionProperty` object.
    pub(crate) fn bind_reflection_property_local(
        &mut self,
        name: &str,
        reflected_class: String,
        reflected_property: String,
    ) {
        if self.can_track_static_callable_local(name) {
            self.reflection_property_locals
                .insert(name.to_string(), (reflected_class, reflected_property));
        }
    }

    /// Returns the reflected class/property associated with a local `ReflectionProperty`.
    pub(crate) fn reflection_property_local(&self, name: &str) -> Option<(String, String)> {
        self.reflection_property_locals.get(name).cloned()
    }

    /// Records that a PHP local currently holds a statically-known `ReflectionMethod` object.
    pub(crate) fn bind_reflection_method_local(
        &mut self,
        name: &str,
        reflected_class: String,
        reflected_method: String,
    ) {
        if self.can_track_static_callable_local(name) {
            self.reflection_method_locals
                .insert(name.to_string(), (reflected_class, reflected_method));
        }
    }

    /// Returns the reflected class/method associated with a local `ReflectionMethod`.
    pub(crate) fn reflection_method_local(&self, name: &str) -> Option<(String, String)> {
        self.reflection_method_locals.get(name).cloned()
    }

    /// Records that a PHP local currently holds a safe static argument array for reflection.
    pub(crate) fn bind_reflection_arg_array_local(&mut self, name: &str, args: Vec<Expr>) {
        if self.can_track_static_callable_local(name) {
            self.reflection_arg_array_locals
                .insert(name.to_string(), args);
        }
    }

    /// Returns the static reflection argument array associated with a local.
    pub(crate) fn reflection_arg_array_local(&self, name: &str) -> Option<Vec<Expr>> {
        self.reflection_arg_array_locals.get(name).cloned()
    }

    /// Records that a PHP local currently holds a Fiber with a known callback signature.
    pub(crate) fn bind_fiber_start_sig(&mut self, name: &str, sig: FunctionSig) {
        if self.can_track_static_callable_local(name) {
            self.fiber_start_sigs.insert(name.to_string(), sig);
        }
    }

    /// Returns the Fiber callback start signature currently associated with a local.
    pub(crate) fn fiber_start_sig_for_local(&self, name: &str) -> Option<FunctionSig> {
        self.fiber_start_sigs.get(name).cloned()
    }

    /// Returns the known Fiber callback start signature returned by a function.
    pub(crate) fn fiber_return_sig(&self, name: &str) -> Option<FunctionSig> {
        self.fiber_return_sigs.get(name).cloned()
    }

    /// Returns the specialized signature inferred for a callable parameter in this scope.
    pub(crate) fn callable_param_signature(&self, name: &str) -> Option<&FunctionSig> {
        self.callable_param_sigs
            .get(&(self.owner_name.clone(), name.to_string()))
    }

    /// Invalidates one local's callable association after a known mutation.
    ///
    /// This advances the mutation generation. CFG joins that only discard flow-sensitive facts
    /// must use `clear_static_callable_locals` instead.
    pub(crate) fn clear_static_callable_local(&mut self, name: &str) {
        self.static_callable_locals.remove(name);
        let epoch = self
            .static_callable_local_epochs
            .entry(name.to_string())
            .or_default();
        *epoch = epoch.saturating_add(1);
    }

    /// Captures the straight-line callable facts at a control-flow split.
    pub(crate) fn static_callable_locals_snapshot(
        &self,
    ) -> HashMap<String, StaticCallableBinding> {
        self.static_callable_locals.clone()
    }

    /// Captures per-local mutation generations alongside callable flow facts.
    pub(crate) fn static_callable_local_epochs_snapshot(&self) -> HashMap<String, u64> {
        self.static_callable_local_epochs.clone()
    }

    /// Restores entry closure facts that no executable path in a lowered region mutated.
    ///
    /// Try/catch lowering clears all flow-sensitive callable facts at its handler and join. That
    /// conservative clear must not erase an unchanged closure's compile-time function identity,
    /// because a later explicit `Closure::bind` scope needs that identity to clone the function
    /// with the requested lexical access class. Mutation epochs distinguish those CFG clears from
    /// stores, unsets, and reference rebindings, all of which clear one named local directly.
    pub(crate) fn restore_unchanged_static_closure_locals(
        &mut self,
        mut snapshot: HashMap<String, StaticCallableBinding>,
        epochs: &HashMap<String, u64>,
    ) {
        snapshot.retain(|name, target| {
            matches!(target, StaticCallableBinding::Closure { .. })
                && self.static_callable_local_epochs.get(name).copied().unwrap_or(0)
                    == epochs.get(name).copied().unwrap_or(0)
        });
        self.static_callable_locals = snapshot;
    }

    /// Restores the callable facts for one control-flow arm or completed join.
    pub(crate) fn restore_static_callable_locals(
        &mut self,
        snapshot: HashMap<String, StaticCallableBinding>,
    ) {
        self.static_callable_locals = snapshot;
    }

    /// Clears the compile-time `ReflectionClass` association for one local.
    pub(crate) fn clear_reflection_class_local(&mut self, name: &str) {
        self.reflection_class_locals.remove(name);
    }

    /// Clears the compile-time `ReflectionFunction` association for one local.
    pub(crate) fn clear_reflection_function_local(&mut self, name: &str) {
        self.reflection_function_locals.remove(name);
    }

    /// Clears the compile-time `ReflectionProperty` association for one local.
    pub(crate) fn clear_reflection_property_local(&mut self, name: &str) {
        self.reflection_property_locals.remove(name);
    }

    /// Clears the compile-time `ReflectionMethod` association for one local.
    pub(crate) fn clear_reflection_method_local(&mut self, name: &str) {
        self.reflection_method_locals.remove(name);
    }

    /// Clears the compile-time reflection argument-array association for one local.
    pub(crate) fn clear_reflection_arg_array_local(&mut self, name: &str) {
        self.reflection_arg_array_locals.remove(name);
    }

    /// Clears the known Fiber callback association for one local.
    pub(crate) fn clear_fiber_start_sig(&mut self, name: &str) {
        self.fiber_start_sigs.remove(name);
    }

    /// Clears all compile-time callable associations after a control-flow join.
    ///
    /// This deliberately leaves mutation generations unchanged so an enclosing region can
    /// restore entry facts when no executable operation could have changed their local values.
    pub(crate) fn clear_static_callable_locals(&mut self) {
        self.static_callable_locals.clear();
        self.reflection_class_locals.clear();
        self.reflection_function_locals.clear();
        self.reflection_property_locals.clear();
        self.reflection_method_locals.clear();
        self.reflection_arg_array_locals.clear();
        self.fiber_start_sigs.clear();
    }

    /// Invalidates all compile-time callable associations after an unknown local mutation.
    ///
    /// Unlike a control-flow join, an eval or unresolved include can replace locals even when a
    /// preceding join already cleared their flow facts. Advance every tracked generation before
    /// clearing so an enclosing try region cannot restore an entry closure after the mutation.
    pub(crate) fn invalidate_static_callable_locals(&mut self) {
        for epoch in self.static_callable_local_epochs.values_mut() {
            *epoch = epoch.saturating_add(1);
        }
        self.clear_static_callable_locals();
    }

    /// Invalidates callable identity facts after an opaque opcode that can invoke PHP code.
    ///
    /// Statically resolved calls invalidate their known by-reference captures and arguments at
    /// their lowering sites. This fallback is reserved for invokers whose target cannot be
    /// recovered. Keeping known harmless calls out of it preserves unrelated closure identity.
    fn invalidate_callable_facts_after_user_code(
        &mut self,
        op: Op,
        operands: &[ValueId],
        immediate: Option<&Immediate>,
        effects: Effects,
    ) {
        let operand_types = operands
            .iter()
            .map(|operand| self.builder.value_php_type(*operand))
            .collect::<Vec<_>>();
        let local_type = match immediate {
            Some(Immediate::LocalSlot(slot)) => Some(self.builder.local_php_type(*slot)),
            _ => None,
        };
        // A collection safe point can re-enter PHP only through a destructor. Keep escaped
        // callable facts when the closed AOT class set has no destructor, so scalar container
        // cleanup does not erase a closure's explicit-scope specialization. Eval can add a
        // destructor at run time, so any preceding eval keeps the conservative boundary.
        let gc_can_invoke_user_code = op != Op::GcCollect
            || self.has_eval_barrier()
            || self.eval_executed()
            || self
                .classes
                .values()
                .any(|class| class.methods.contains_key("__destruct"));
        if gc_can_invoke_user_code
            && instruction_has_opaque_user_code_boundary(
            op,
            immediate,
            effects,
            &operand_types,
            local_type.as_ref(),
        )
        {
            self.invalidate_escaped_callable_locals();
        }
        if let Some(callback_index) = runtime_callback_operand_index(op, immediate) {
            let binding = operands
                .get(callback_index)
                .and_then(|operand| self.static_callable_binding_for_operand(*operand));
            match binding {
                Some(StaticCallableBinding::Closure { captures, .. }) => {
                    self.invalidate_callable_capture_locals(&captures);
                }
                Some(StaticCallableBinding::UserFunction(function)) => {
                    self.invalidate_callable_user_function(&function, None, &[]);
                }
                Some(
                    StaticCallableBinding::StaticMethod { .. }
                    | StaticCallableBinding::StaticMethodDescriptor { .. }
                    | StaticCallableBinding::InstanceMethod { .. },
                ) => self.invalidate_escaped_callable_locals(),
                Some(
                    StaticCallableBinding::ExternFunction(_)
                    | StaticCallableBinding::Builtin(_),
                ) => {}
                None => self.invalidate_escaped_callable_locals(),
            }
            return;
        }
    }

    /// Invalidates every tracked callable local whose cell escaped this activation.
    fn invalidate_escaped_callable_locals(&mut self) {
        let escaped = self
            .static_callable_local_epochs
            .keys()
            .filter(|name| {
                self.ref_bound_locals.contains(*name)
                    || (self.in_main && self.all_global_var_names.contains(*name))
            })
            .cloned()
            .collect::<Vec<_>>();
        for name in escaped {
            self.clear_static_callable_local(&name);
        }
    }

    /// Invalidates callable locals whose cells can be reached through any PHP reference alias.
    fn invalidate_ref_bound_callable_locals(&mut self) {
        let names = self.ref_bound_locals.iter().cloned().collect::<Vec<_>>();
        for name in names {
            self.clear_static_callable_local(&name);
        }
    }

    /// Invalidates callable facts for caller locals passed to declared by-reference parameters.
    ///
    /// Argument normalization leaves regular operands in parameter order. Reference operands
    /// retain their original local-load marker, so this can identify exactly which caller cells
    /// the callee may write without discarding unrelated closure facts.
    pub(crate) fn invalidate_callable_ref_argument_locals(
        &mut self,
        signature: Option<&FunctionSig>,
        operands: &[ValueId],
    ) {
        let Some(signature) = signature else {
            return;
        };
        let names = operands
            .iter()
            .enumerate()
            .filter(|(index, _)| {
                signature
                    .ref_params
                    .get(*index)
                    .copied()
                    .unwrap_or(false)
            })
            .filter_map(|(_, operand)| self.operand_local_name(*operand))
            .collect::<Vec<_>>();
        for name in names {
            self.clear_static_callable_local(&name);
        }
    }

    /// Invalidates caller cells directly writable by one statically resolved user function.
    pub(crate) fn invalidate_callable_user_function(
        &mut self,
        function: &str,
        signature: Option<&FunctionSig>,
        operands: &[ValueId],
    ) {
        self.invalidate_callable_ref_argument_locals(signature, operands);
        self.invalidate_ref_bound_callable_locals();
        // A declared by-reference container parameter lets the callee replace an element owned
        // by the caller. Releasing that element can run a destructor which writes program-global
        // callable storage, including through another statically resolved wrapper. The ordinary
        // global-name summary sees direct `global` declarations, but the destructor is reached
        // through the argument's contents rather than through a direct call edge.
        let ref_argument_cleanup_can_invoke_user_code = signature.is_some_and(|signature| {
            operands.iter().enumerate().any(|(index, operand)| {
                signature
                    .ref_params
                    .get(index)
                    .copied()
                    .unwrap_or(false)
                    && php_type_cleanup_may_invoke_user_code(
                        &self.builder.value_php_type(*operand),
                    )
            })
        });
        if ref_argument_cleanup_can_invoke_user_code {
            self.invalidate_escaped_callable_locals();
        }
        match self
            .function_global_names
            .get(&php_symbol_key(function.trim_start_matches('\\')))
            .cloned()
        {
            Some(Some(globals)) => {
                for name in globals {
                    self.clear_static_callable_local(&name);
                }
            }
            Some(None) | None => self.invalidate_top_level_global_callable_locals(),
        }
    }

    /// Invalidates callable facts backed by the top-level global storage namespace.
    fn invalidate_top_level_global_callable_locals(&mut self) {
        if !self.in_main {
            return;
        }
        let names = self
            .static_callable_local_epochs
            .keys()
            .filter(|name| self.all_global_var_names.contains(*name))
            .cloned()
            .collect::<Vec<_>>();
        for name in names {
            self.clear_static_callable_local(&name);
        }
    }

    /// Installs transitive direct-function global reachability for the top-level activation.
    pub(crate) fn set_function_global_names(
        &mut self,
        names: HashMap<String, Option<HashSet<String>>>,
    ) {
        self.function_global_names = names;
    }

    /// Invalidates exactly the caller locals exposed through one known closure's by-reference
    /// capture list. Direct static closure calls use this instead of treating every escaped local
    /// in the frame as reachable from that specific closure.
    pub(crate) fn invalidate_callable_capture_locals(&mut self, captures: &[ClosureCapture]) {
        self.invalidate_ref_bound_callable_locals();
        for name in captures
            .iter()
            .filter_map(|capture| capture.by_ref_local.as_deref())
        {
            self.clear_static_callable_local(name);
        }
    }

    /// Resolves a callable descriptor operand back to a tracked local binding when possible.
    fn static_callable_binding_for_operand(
        &self,
        operand: ValueId,
    ) -> Option<StaticCallableBinding> {
        let name = self.operand_local_name(operand)?;
        self.static_callable_local(&name)
    }

    /// Records callable identity for a compiler-owned callback temporary.
    pub(crate) fn bind_static_callable_temp(
        &mut self,
        name: &str,
        target: StaticCallableBinding,
    ) {
        let slot = self.local_slots.get(name).copied();
        if slot.is_some_and(|slot| self.builder.local_kind(slot) == LocalKind::HiddenTemp) {
            self.static_callable_local_epochs
                .entry(name.to_string())
                .or_default();
            self.static_callable_locals.insert(name.to_string(), target);
        }
    }

    /// Returns whether the named PHP variable should use program-global storage.
    ///
    /// Request superglobals (`$_SERVER`/`$_GET`/`$_POST`) route to the shared
    /// `_eir_global_*` symbol in EVERY scope — main and functions alike — so a
    /// function read targets the same storage the top-level `--web` prelude writes.
    /// Returns whether an entry-block array seed would land on storage that is not this scope's
    /// to create.
    ///
    /// Only the two families the checker's own seeding skips: a superglobal is pre-populated, and
    /// a `$GLOBALS` alias names storage shared with every other scope. A plain top-level local
    /// that some FUNCTION later declares `global` is still created here — main is where it is
    /// first written, so the seed belongs in main's entry exactly as for any other local.
    pub(crate) fn uses_global_storage_for_seeding(&self, name: &str) -> bool {
        crate::superglobals::is_superglobal(name) || crate::globals_array::is_alias(name)
    }

    /// Promotes local `name`'s indexed array to a hash, MOVING the slot's reference into the
    /// conversion.
    ///
    /// The promotion stores the hash back through `store_prepared_mutated_local`, which releases
    /// nothing: it expects the conversion to have consumed the reference the slot held. A plain
    /// `ArrayToHash` consumes only an operand the codegen can prove owned, and a slot load is
    /// not one, so the slot's old array leaked on every first string-key write --
    /// `$out = []; $out['a'] = 1;` lost the empty array each time it ran. The immediate says the
    /// operand IS the slot's reference. A global keeps the plain form: its store releases the
    /// previous value itself.
    pub(crate) fn emit_local_array_to_hash_move(
        &mut self,
        name: &str,
        array_value: LoweredValue,
        assoc_ty: PhpType,
        span: Span,
    ) -> LoweredValue {
        let kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        let moves_slot_reference = !self.uses_global_storage(name, kind);
        self.emit_value(
            Op::ArrayToHash,
            vec![array_value.value],
            moves_slot_reference.then_some(Immediate::Bool(true)),
            assoc_ty,
            Op::ArrayToHash.default_effects(),
            Some(span),
        )
    }

    /// Whether the variable `name` lives outside this frame's own slots: a `global` alias, a
    /// superglobal, a top-level variable some function aliases, or a function `static`.
    pub(crate) fn variable_storage_is_global_or_static(&self, name: &str) -> bool {
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        kind == LocalKind::StaticLocal || self.uses_global_storage(name, kind)
    }

    fn uses_global_storage(&self, name: &str, kind: LocalKind) -> bool {
        kind == LocalKind::GlobalAlias
            || crate::superglobals::is_superglobal(name)
            || crate::globals_array::is_alias(name)
            || (self.in_main && self.all_global_var_names.contains(name))
    }

    /// Emits a store to the program-global symbol for a global alias variable.
    fn store_global_name(
        &mut self,
        name: &str,
        slot: LocalSlotId,
        value: LoweredValue,
        span: Option<Span>,
    ) {
        // Under `--web` a superglobal's slot holds the raw hash every reader assumes
        // (`global_alias_type`), not a boxed cell. A boxed value -- `$_SERVER += $_ENV`, whose
        // array union is the gradual packed-or-hash type -- is converted to that hash here, and
        // the reference the caller handed over for the slot moves to the converted container.
        // A packed array -- the prelude's `$_ENV = []` -- is converted the same way: read as a
        // hash, its header crashed the first `$_SERVER += $_ENV`.
        let stored_repr = self.builder.value_php_type(value.value).codegen_repr();
        let conversion = if self.web && crate::superglobals::is_superglobal(name) {
            match stored_repr {
                PhpType::Mixed | PhpType::Union(_) => Some(Op::MixedToHash),
                PhpType::Array(_) => Some(Op::ArrayToHash),
                _ => None,
            }
        } else {
            None
        };
        let value = if let Some(op) = conversion {
            // `ArrayToHash` consumes the operand it is given, so it gets its own reference;
            // `MixedToHash` does not, and the caller's reference for the slot is released.
            let source = if op == Op::ArrayToHash {
                crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
            } else {
                value
            };
            let hash = self.emit_value(
                op,
                vec![source.value],
                None,
                crate::superglobals::superglobal_type(),
                op.default_effects(),
                span,
            );
            crate::ir_lower::ownership::release_if_owned(self, value, span);
            hash
        } else {
            value
        };
        let data = self.intern_global_storage_name(name);
        self.invalidate_callable_facts_after_user_code(
            Op::StoreGlobal,
            &[value.value],
            Some(&Immediate::GlobalName(data)),
            Op::StoreGlobal.default_effects(),
        );
        self.builder.emit_with_effects(
            Op::StoreGlobal,
            vec![value.value],
            Some(Immediate::GlobalName(data)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            Op::StoreGlobal.default_effects(),
            span,
        );
        self.initialized_slots.insert(slot);
    }

    /// Emits a store opcode to an already declared local or static-local slot.
    /// Reports whether a string value may still point into the shared concat scratch buffer.
    ///
    /// Two ways that happens: the value was produced by a scratch-writing opcode, or it was
    /// LOADED from a slot that previously received such a value. The second case is the one a
    /// check on `value_defining_op` alone cannot see — `$v .= ...` in a loop leaves `$v`'s slot
    /// holding scratch bytes, and `return $v` then hands the caller a pointer the next string
    /// operation is free to overwrite.
    pub(crate) fn value_is_scratch_backed(&self, value: LoweredValue) -> bool {
        if value.ir_type != IrType::Str {
            return false;
        }
        self.value_id_is_scratch_backed(value.value, 0)
    }

    /// Walks a value back through the retains a store inserts, so the scratch fact is not lost
    /// behind an `Acquire` the store emitted around the concat result.
    fn value_id_is_scratch_backed(&self, value: crate::ir::ValueId, depth: usize) -> bool {
        if depth > 4 {
            return false;
        }
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        if inst.op == Op::StrPersist || crate::ir_lower::runtime_call_string_is_fresh(inst) {
            return false;
        }
        if crate::ir_lower::expr::string_op_uses_scratch_storage(inst.op) {
            return true;
        }
        if matches!(inst.op, Op::Acquire) {
            return inst
                .operands
                .first()
                .is_some_and(|operand| self.value_id_is_scratch_backed(*operand, depth + 1));
        }
        self.value_source_slot(value)
            .is_some_and(|slot| self.scratch_backed_slots.contains(&slot))
    }

    /// The local slot a value was loaded from, when it is a plain slot load.
    fn value_source_slot(&self, value: crate::ir::ValueId) -> Option<LocalSlotId> {
        let inst = self.builder.value_defining_instruction(value)?;
        if !matches!(
            inst.op,
            Op::LoadLocal | Op::LoadStaticLocal | Op::LoadRefCell
        ) {
            return None;
        }
        match inst.immediate.as_ref()? {
            Immediate::LocalSlot(slot) => Some(*slot),
            _ => None,
        }
    }

    fn store_slot_with_op(
        &mut self,
        slot: LocalSlotId,
        value: LoweredValue,
        op: Op,
        span: Option<Span>,
    ) {
        // Flow-sensitive on purpose: a later persistent store CLEARS the mark. Keeping it as a
        // grow-only MAY set makes every subsequent `return $slot` persist, which duplicates
        // strings that already own their bytes and changes who is responsible for freeing them.
        if self.value_is_scratch_backed(value) {
            self.scratch_backed_slots.insert(slot);
        } else {
            self.scratch_backed_slots.remove(&slot);
        }
        self.invalidate_callable_facts_after_user_code(
            op,
            &[value.value],
            Some(&Immediate::LocalSlot(slot)),
            op.default_effects(),
        );
        self.builder.emit_with_effects(
            op,
            vec![value.value],
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            op.default_effects(),
            span,
        );
        self.initialized_slots.insert(slot);
    }

    /// Emits a ref-cell store that carries the alias target type for backend dereference.
    fn store_ref_cell_slot(
        &mut self,
        slot: LocalSlotId,
        value: LoweredValue,
        alias_ty: PhpType,
        span: Option<Span>,
    ) {
        self.invalidate_callable_facts_after_user_code(
            Op::StoreRefCell,
            &[value.value],
            Some(&Immediate::LocalSlot(slot)),
            Op::StoreRefCell.default_effects(),
        );
        self.builder.emit_with_effects(
            Op::StoreRefCell,
            vec![value.value],
            Some(Immediate::LocalSlot(slot)),
            IrType::Void,
            alias_ty,
            Ownership::NonHeap,
            Op::StoreRefCell.default_effects(),
            span,
        );
        self.initialized_slots.insert(slot);
    }

    /// Starts a call-local capture group without disturbing a surrounding argument evaluation.
    pub(crate) fn begin_argument_guard_scope(&mut self) {
        self.argument_guard_scopes.push(Vec::new());
    }

    /// Finishes capture registration while keeping emitted guards active through invocation.
    pub(crate) fn end_argument_guard_scope(&mut self) {
        self.argument_guard_scopes.pop().expect("balanced argument capture scope");
    }

    /// Protects a captured argument in PHP parameter order until normal release or unwinding.
    pub(crate) fn guard_call_argument(&mut self, value: LoweredValue, parameter: usize, span: Span) {
        let ty = self.builder.value_php_type(value.value);
        if !Ownership::php_type_needs_lifetime_tracking(&ty) { return; }
        let previous = self.argument_guard_scopes.last().expect("active call capture scope")
            .iter().filter(|(index, _)| *index < parameter).max_by_key(|(index, _)| *index)
            .map(|(_, token)| *token);
        let anchor = previous.unwrap_or_else(|| self.builder.emit_const_i64(0));
        let token = self.emit_value(Op::RuntimeCall, vec![value.value, anchor],
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ExceptionGuardOwned)),
            PhpType::Int, Effects::WRITES_GLOBAL, Some(span));
        self.argument_guards.insert(value.value, token.value);
        self.argument_guard_scopes.last_mut().expect("active call capture scope").push((parameter, token.value));
    }

    /// Reports whether construction already registered exceptional ownership for this call operand.
    pub(crate) fn has_call_argument_guard(&self, value: ValueId) -> bool {
        self.argument_guards.contains_key(&value)
    }

    /// Refreshes an active indexed or associative argument guard after mutation can replace its heap address.
    pub(crate) fn refresh_argument_array_guard(&mut self, array: LoweredValue, span: Span) {
        let Some(&token) = self.argument_guards.get(&array.value) else { return; };
        let target = if matches!(self.builder.value_php_type(array.value), PhpType::AssocArray { .. }) {
            crate::ir::RuntimeCallTarget::ExceptionUpdateHashGuard
        } else { crate::ir::RuntimeCallTarget::ExceptionUpdateArrayGuard };
        self.emit_void(Op::RuntimeCall, vec![array.value, token],
            Some(Immediate::RuntimeCall(target)),
            Effects::WRITES_GLOBAL, Some(span));
    }

    /// Ends exceptional ownership immediately before normal call cleanup consumes a captured value.
    pub(crate) fn unguard_call_argument(&mut self, value: ValueId, span: Span) {
        if let Some(token) = self.argument_guards.remove(&value) {
            self.emit_void(Op::RuntimeCall, vec![token],
                Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::ExceptionUnguardOwned)),
                Effects::WRITES_GLOBAL, Some(span));
        }
    }

    /// Emits a void opcode with optional operands and source span.
    pub(crate) fn emit_void(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        effects: Effects,
        span: Option<Span>,
    ) {
        self.invalidate_callable_facts_after_user_code(op, &operands, immediate.as_ref(), effects);
        self.builder.emit_with_effects(
            op,
            operands,
            immediate,
            IrType::Void,
            PhpType::Void,
            Ownership::NonHeap,
            effects,
            span,
        );
    }

    /// Boxes a value into a Mixed cell and releases the producer's reference when the
    /// operand is an owning temporary. `__rt_mixed_from_value` retains refcounted payloads
    /// (objects, arrays, hashes, callables, nested cells) and persists strings, so the
    /// boxed cell always carries its own reference or copy; keeping the producer's
    /// reference too leaked one payload per boxing (issue #484). Borrowed operands (e.g.
    /// a loaded local) are left untouched — the box's retain is their +1.
    pub(crate) fn box_value_as_mixed(
        &mut self,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        // An indexed array of compact nullable scalars (`[$nullableInt]`) stores tagged scalar
        // words that no Mixed-array reader understands: boxed as is, `implode(',', [$v])` through
        // a `mixed` parameter read every element as empty. Its elements are boxed first.
        let value = match self.builder.value_php_type(value.value).codegen_repr() {
            PhpType::Array(element) if element.codegen_repr() == PhpType::TaggedScalar => {
                // The conversion takes its operand's reference (copying a shared array first),
                // so a borrowed source -- a local read -- is retained for it.
                let reads_storage = matches!(
                    self.builder.value_defining_op(value.value),
                    Some(Op::LoadLocal | Op::LoadStaticLocal | Op::LoadGlobal | Op::PropGet)
                );
                let value = if self.value_is_owning_temporary(value) && !reads_storage {
                    value
                } else {
                    crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
                };
                self.emit_value(
                    Op::ArrayToMixed,
                    vec![value.value],
                    None,
                    PhpType::Array(Box::new(PhpType::Mixed)),
                    Op::ArrayToMixed.default_effects(),
                    span,
                )
            }
            _ => value,
        };
        let release_source = self.value_is_owning_temporary(value);
        let boxed = self.emit_value(
            Op::MixedBox,
            vec![value.value],
            None,
            php_type,
            Op::MixedBox.default_effects(),
            span,
        );
        if release_source {
            crate::ir_lower::ownership::release_if_owned(self, value, span);
        }
        boxed
    }

    /// Widens a pointer array or hash to boxed Mixed elements, leaving any other value as it is.
    ///
    /// Each null pointer word becomes PHP null and each live word its integer address, so a view
    /// of the container through `mixed` agrees with its `?ptr<T>` element reads. A borrowed
    /// source is copied first, so the pointer container itself keeps its storage.
    pub(crate) fn widen_pointer_container_to_mixed(
        &mut self,
        value: LoweredValue,
        span: Option<Span>,
    ) -> LoweredValue {
        let source_ty = self.builder.value_php_type(value.value).codegen_repr();
        let target_ty = match &source_ty {
            PhpType::Array(elem) if matches!(elem.codegen_repr(), PhpType::Pointer(_)) => {
                PhpType::Array(Box::new(PhpType::Mixed))
            }
            PhpType::AssocArray { key, value: elem }
                if matches!(elem.codegen_repr(), PhpType::Pointer(_)) =>
            {
                PhpType::AssocArray {
                    key: key.clone(),
                    value: Box::new(PhpType::Mixed),
                }
            }
            _ => return value,
        };
        crate::ir_lower::expr::coerce_container_to_mixed_payload(
            self,
            value,
            &source_ty,
            &target_ty,
            span,
        )
    }

    /// Emits a value-producing opcode with computed storage and ownership metadata.
    pub(crate) fn emit_value(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        php_type: PhpType,
        effects: Effects,
        span: Option<Span>,
    ) -> LoweredValue {
        let ir_type = value_ir_type(&php_type);
        let ownership = Ownership::for_php_type(&php_type);
        self.invalidate_callable_facts_after_user_code(op, &operands, immediate.as_ref(), effects);
        let value = self
            .builder
            .emit_with_effects(
                op, operands, immediate, ir_type, php_type, ownership, effects, span,
            )
            .expect("value opcode produces a value");
        LoweredValue { value, ir_type }
    }

    /// Retypes a packed record address only when a pointer sink consumes it.
    ///
    /// Packed producers keep their record-shaped metadata so field-access optimization still
    /// recognizes them and does not insert pointer null checks. `Move` is the existing pure
    /// forwarding operation, so this changes only the sink-facing PHP and EIR types while
    /// preserving the raw address payload. The sink's pointer type wins even when it names
    /// another class: the checker accepts any pointer for any pointer, and the payload is the
    /// same address either way.
    ///
    /// A PHP `null` reaching a pointer sink (including a `?ptr<T>` slot, stored as one pointer
    /// word) becomes the word 0: null and a null pointer are the same value there, and the
    /// ordinary null sentinel word would read as a live address.
    pub(crate) fn retype_packed_address_for_pointer_sink(
        &mut self,
        value: LoweredValue,
        expected: &PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        let source = self.builder.value_php_type(value.value).codegen_repr();
        let PhpType::Pointer(target) = expected.codegen_repr() else {
            return value;
        };
        if matches!(source, PhpType::Void | PhpType::Never) {
            return self.emit_value(
                Op::ConstI64,
                Vec::new(),
                Some(Immediate::I64(0)),
                PhpType::Pointer(target),
                Op::ConstI64.default_effects(),
                span,
            );
        }
        if !matches!(source, PhpType::Packed(_)) {
            return value;
        }
        self.emit_value(
            Op::Move,
            vec![value.value],
            None,
            PhpType::Pointer(target),
            Op::Move.default_effects(),
            span,
        )
    }

    /// Retypes a value written into an array or hash element for pointer storage.
    ///
    /// A packed record address is stored as the `ptr<T>` word the checker types it as, whatever
    /// the container's current element type: an empty local array takes its element type from
    /// the first write, and a `Packed` value there would widen it to boxed `mixed`. `null`
    /// becomes the word 0 only when the container already stores pointers; anywhere else it
    /// stays PHP null.
    pub(crate) fn retype_value_for_container_element(
        &mut self,
        container_ty: &PhpType,
        value: LoweredValue,
        span: Option<Span>,
    ) -> LoweredValue {
        let element = match container_ty.codegen_repr() {
            PhpType::Array(element) => Some(*element),
            PhpType::AssocArray { value, .. } => Some(*value),
            _ => None,
        };
        if let Some(element) =
            element.filter(|element| matches!(element.codegen_repr(), PhpType::Pointer(_)))
        {
            return self.retype_packed_address_for_pointer_sink(value, &element, span);
        }
        match self.builder.value_php_type(value.value).codegen_repr() {
            PhpType::Packed(class_name) => self.retype_packed_address_for_pointer_sink(
                value,
                &PhpType::Pointer(Some(class_name)),
                span,
            ),
            _ => value,
        }
    }

    /// Emits a value-producing opcode whose result is an unconditional owned reference.
    ///
    /// This is reserved for runtime transfers such as `CatchBind`, where the opcode clears
    /// the source runtime cell and hands its sole reference to the returned SSA value.
    pub(crate) fn emit_owned_value(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        php_type: PhpType,
        effects: Effects,
        span: Option<Span>,
    ) -> LoweredValue {
        let ir_type = value_ir_type(&php_type);
        self.invalidate_callable_facts_after_user_code(op, &operands, immediate.as_ref(), effects);
        let value = self
            .builder
            .emit_with_effects(
                op,
                operands,
                immediate,
                ir_type,
                php_type,
                Ownership::Owned,
                effects,
                span,
            )
            .expect("owned value opcode produces a value");
        LoweredValue { value, ir_type }
    }

    /// Emits an `is_truthy` conversion unless the value is an integer that is already its own
    /// truthiness.
    ///
    /// A raw PHP null is also an I64 (the null literal or an assignment expression whose
    /// right-hand side is null), and its sentinel is nonzero, so a `Void`/`Never` value must
    /// still go through `is_truthy`, whose null arm answers false (#1990).
    pub(crate) fn truthy(&mut self, input: LoweredValue, span: Option<Span>) -> LoweredValue {
        if let Some(value) = super::expr::object_operators::lower_object_truthy(self, input, span) {
            return value;
        }
        if input.ir_type == IrType::I64
            && !matches!(
                self.builder.value_php_type(input.value).codegen_repr(),
                PhpType::Void | PhpType::Never
            )
        {
            return input;
        }
        self.emit_value(
            Op::IsTruthy,
            vec![input.value],
            None,
            PhpType::Bool,
            Op::IsTruthy.default_effects(),
            span,
        )
    }

    /// Computes truthiness and releases `input` when this path owns it.
    ///
    /// `IsTruthy` reads its operand and yields a fresh, non-aliasing boolean, so
    /// a branch condition that consumes an owned temporary — e.g. the boxed
    /// `Mixed` result of `$x[0] ?? default` fed into an `if`/ternary/`&&`/`!` —
    /// must release that temporary here or it leaks on the truthiness path
    /// (issue #586). Use this at condition sites that discard the original
    /// value; callers that also forward it (short ternary `?:`) must keep using
    /// [`Self::truthy`], which never releases.
    pub(crate) fn truthy_consuming(
        &mut self,
        input: LoweredValue,
        span: Option<Span>,
    ) -> LoweredValue {
        let owns_input = self.value_is_owning_temporary(input);
        let result = self.truthy(input, span);
        // Only release when a distinct boolean was produced; the `I64` fast path
        // in `truthy` returns `input` itself (and non-heap values never own).
        if owns_input && result.value != input.value {
            crate::ir_lower::ownership::release_if_owned(self, input, span);
        }
        result
    }
}

impl crate::builtins::semantics::BuiltinLoweringContext for LoweringContext<'_, '_> {
    /// Returns the source file associated with the enclosing lowered function.
    fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
    }

    /// Reads the profile-resolved `E_ALL` literal installed during compiler prescan.
    fn error_reporting_mask(&self) -> i64 {
        self.constants
            .get("E_ALL")
            .and_then(|(value, _)| match value {
                ExprKind::IntLiteral(mask) => Some(*mask),
                _ => None,
            })
            .unwrap_or_else(|| {
                crate::php_version::PhpVersion::default().error_reporting_mask()
            })
    }

    /// Loads each PHP-visible parameter through its current name binding.
    fn current_frame_arguments(&mut self, span: Span) -> Vec<ValueId> {
        let names = self
            .builder
            .function()
            .signature
            .as_ref()
            .map(|signature| {
                signature
                    .params
                    .iter()
                    .map(|(name, _)| name.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        names
            .into_iter()
            .map(|name| self.load_local(&name, Some(span)).value)
            .collect()
    }

    /// Returns PHP metadata already attached to an EIR operand.
    fn value_php_type(&self, value: ValueId) -> PhpType {
        self.builder.value_php_type(value)
    }

    /// Reports whether one of the program's declared classes is the named internal class.
    fn has_class(&self, name: &str) -> bool {
        self.classes
            .keys()
            .any(|class| class.eq_ignore_ascii_case(name))
    }

    /// Interns a class-name literal through the module data pool.
    fn intern_class_name(&mut self, value: &str) -> DataId {
        LoweringContext::intern_class_name(self, value)
    }

    /// Interns an ordinary string through the module data pool.
    fn intern_string(&mut self, value: &str) -> DataId {
        LoweringContext::intern_string(self, value)
    }

    /// Emits a backend-neutral builtin operation through the ordinary EIR builder path.
    fn emit_value(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        php_type: PhpType,
        effects: Effects,
        span: Option<Span>,
    ) -> crate::builtins::semantics::LoweredBuiltinValue {
        let lowered = LoweringContext::emit_value(
            self,
            op,
            operands,
            immediate,
            php_type,
            effects,
            span,
        );
        crate::builtins::semantics::LoweredBuiltinValue {
            value: lowered.value,
        }
    }

    /// Emits a void backend-neutral builtin operation through the ordinary EIR builder path.
    fn emit_void(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        effects: Effects,
        span: Option<Span>,
    ) {
        LoweringContext::emit_void(self, op, operands, immediate, effects, span);
    }

    /// Emits a typed runtime call whose helper symbol and physical ABI remain backend-owned.
    fn emit_runtime_call(
        &mut self,
        target: crate::ir::RuntimeCallTarget,
        operands: Vec<ValueId>,
        php_type: PhpType,
        effects: Effects,
        span: Option<Span>,
    ) -> crate::builtins::semantics::LoweredBuiltinValue {
        let target = match target {
            crate::ir::RuntimeCallTarget::Function(target)
                if matches!(
                    target,
                    crate::ir::RuntimeFnId::FunctionExists
                        | crate::ir::RuntimeFnId::IsCallable
                        | crate::ir::RuntimeFnId::ObStart
                        | crate::ir::RuntimeFnId::StripTags
                ) || target.string_callback_operand_index().is_some()
                    || target.uses_mbstring_runtime() =>
            {
                crate::ir::RuntimeCallTarget::ProfiledFunction {
                    target,
                    arguments: crate::ir::RuntimeArgumentLayout::Values,
                    strict_php: crate::strict_php::is_enabled(),
                    strict_types: Some(crate::source::current_strict_types()),
                }
            }
            target => target,
        };
        let lowered = LoweringContext::emit_value(
            self,
            Op::RuntimeCall,
            operands,
            Some(Immediate::RuntimeCall(target)),
            php_type,
            effects,
            span,
        );
        crate::builtins::semantics::LoweredBuiltinValue {
            value: lowered.value,
        }
    }

    /// Emits a direct `Op::Call` to a declared PHP function through the ordinary user-call
    /// path, with the conservative user-call effect set.
    ///
    /// A prelude helper called from a builtin's `Eir` lowering never goes through the ordinary
    /// call-argument planner, so this boundary must append the ABI-only
    /// `crate::func_args` slots itself. They only exist when `eval()` is reachable
    /// (`capture_all_frames`) and the helper does not inspect them; an empty collector keeps
    /// the callee ABI complete without changing PHP-visible arguments.
    fn emit_user_call(
        &mut self,
        name: &str,
        operands: Vec<ValueId>,
        php_type: PhpType,
        span: Option<Span>,
    ) -> crate::builtins::semantics::LoweredBuiltinValue {
        let mut operands = operands;
        if let Some(signature) = self.functions.get(name) {
            if crate::func_args::sig_has_hidden_argc_param(signature) {
                let argc = LoweringContext::emit_value(
                    self,
                    Op::ConstI64,
                    Vec::new(),
                    Some(Immediate::I64(operands.len() as i64)),
                    PhpType::Int,
                    Op::ConstI64.default_effects(),
                    span,
                );
                operands.push(argc.value);
            }
            if crate::func_args::sig_collects_surplus_args(signature) {
                let collector = LoweringContext::emit_value(
                    self,
                    Op::ArrayNew,
                    Vec::new(),
                    Some(Immediate::Capacity(0)),
                    PhpType::Array(Box::new(PhpType::Mixed)),
                    Op::ArrayNew.default_effects(),
                    span,
                );
                operands.push(collector.value);
            }
        }
        let data = self.intern_function_name(name);
        let effects = crate::ir_lower::effects_lookup::user_call_effects(name);
        // A prelude helper can box operands and then throw before its post-call releases.
        // Park each owned temporary in the unwind chain for the call's duration so a caught
        // helper exception retires it exactly once.
        // Only the GMP prelude boxes its own operands inside the lowering hook; those boxes
        // are the ones the unwind chain must own. Other prelude helpers pass caller storage
        // through unchanged, and pinning an acquired local would release the caller's object.
        let pins = if name.starts_with("__elephc_gmp_")
            && effects.contains(crate::ir::Effects::MAY_THROW)
        {
            crate::ir_lower::expr::call_operand_owners::pin_in_flight_owners(
                self,
                &operands,
                span.unwrap_or_else(crate::span::Span::dummy),
            )
        } else {
            Vec::new()
        };
        let lowered = LoweringContext::emit_value(
            self,
            Op::Call,
            operands,
            Some(Immediate::Data(data)),
            php_type,
            effects,
            span,
        );
        crate::ir_lower::expr::call_operand_owners::unpin_in_flight_owners(
            self,
            pins,
            span.unwrap_or_else(crate::span::Span::dummy),
        );
        crate::builtins::semantics::LoweredBuiltinValue {
            value: lowered.value,
        }
    }

    /// Resolves the operand back to the variable it was loaded from and stores through
    /// `store_local`, which routes by storage kind (frame slot, `static`, `global`,
    /// reference cell, extern global, eval scope), so the variable is (re)declared at the
    /// output type and the previous occupant is released exactly like a source-level
    /// assignment.
    fn store_operand_local(
        &mut self,
        operand: ValueId,
        value: ValueId,
        php_type: PhpType,
        span: Option<Span>,
    ) -> bool {
        let Some(name) = self.operand_local_name(operand) else {
            return false;
        };
        let lowered = LoweredValue {
            value,
            ir_type: value_ir_type(&php_type),
        };
        self.store_local(&name, lowered, php_type, span);
        true
    }
}

impl LoweringContext<'_, '_> {
    /// Returns the PHP variable an operand was loaded from, for every load shape
    /// `load_local` emits, or `None` for any other value (a literal, a call result, a
    /// property read).
    ///
    /// Frame, `static` and reference-cell loads carry their `LocalSlot`, which maps back
    /// to the name through `local_slots`; `global`, extern-global and eval-scope loads
    /// carry the interned `GlobalName`, which maps back through the module data pool.
    fn operand_local_name(&self, operand: ValueId) -> Option<String> {
        let inst = self.builder.value_defining_instruction(operand)?;
        match (inst.op, &inst.immediate) {
            (
                Op::LoadLocal | Op::LoadStaticLocal | Op::LoadRefCell,
                Some(Immediate::LocalSlot(slot)),
            ) => self
                .local_slots
                .iter()
                .find(|(_, candidate)| *candidate == slot)
                .map(|(name, _)| name.clone()),
            (
                Op::LoadGlobal | Op::ExternGlobalLoad | Op::EvalScopeGet,
                Some(Immediate::GlobalName(data)),
            ) => self.data.global_names.get(data.as_raw() as usize).cloned(),
            _ => None,
        }
    }
}

/// Returns whether an abandoned slot still needs `ZeroLocalSlot` after its occupant's release.
///
/// The zero exists for the frame epilogue, which cleans every slot whose storage type is
/// refcounted, the abandoned one included. Two numeric storage shapes do not need it:
/// - raw `int`, `float` and `bool` storage, which no cleanup path ever releases; and
/// - a boxed `mixed` slot whose occupant `release_local_slot` has just retired
///   (`released_store_owner`, which also requires an ordinary store to own the slot), because
///   that retirement stores zero over the slot before it drops the reference, on every target,
///   for heap cells and numeric frame cells alike.
///
/// Every other storage keeps the zero it always had, including a `mixed` slot nothing released.
fn abandoned_slot_needs_zeroing(storage: &PhpType, released_store_owner: bool) -> bool {
    match storage.codegen_repr() {
        PhpType::Int | PhpType::Float | PhpType::Bool => false,
        PhpType::Mixed => !released_store_owner,
        _ => true,
    }
}

/// Returns true for addressable local kinds whose `StoreLocal` overwrites owned storage.
fn local_kind_uses_plain_store_cleanup(kind: LocalKind) -> bool {
    matches!(
        kind,
        LocalKind::PhpLocal
            | LocalKind::HiddenTemp
            | LocalKind::OwnedTemp
            | LocalKind::NamedArgTemp
    )
}

/// Returns the callback operand used by a runtime helper that invokes PHP code.
pub(super) fn runtime_callback_operand_index(op: Op, immediate: Option<&Immediate>) -> Option<usize> {
    let (
        Op::RuntimeCall,
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::Function(target)
            | crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. },
        )),
    ) = (op, immediate)
    else {
        return None;
    };
    target.string_callback_operand_index()
}

/// Returns whether one emitted operation invokes PHP through an unresolved target.
pub(super) fn opcode_has_opaque_user_code_target(op: Op, immediate: Option<&Immediate>) -> bool {
    if matches!(
        op,
        Op::FunctionVariantCall
            | Op::ClosureCall
            | Op::CallableDescriptorInvoke
            | Op::ExprCall
            | Op::PipeCall
            | Op::MethodCall
            | Op::NullsafeMethodCall
            | Op::StaticMethodCall
            | Op::EvalStaticMethodCall
            | Op::EvalLiteralCall
            | Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::ObjectNew
            | Op::EvalObjectNew
            | Op::IteratorMethodCall
            | Op::IterStart
            | Op::IterCurrentKey
            | Op::IterCurrentValue
            | Op::IterNext
            | Op::SplRuntimeCall
            | Op::FiberRuntimeCall
            | Op::DynamicObjectNew
            | Op::DynamicObjectNewMixed
            | Op::DynamicPdoStatementConstructorCall
            | Op::MixedArrayGetForWrite
            | Op::GeneratorYield
            | Op::GeneratorYieldFrom
            | Op::PropGet
            | Op::PropGetForWrite
            | Op::PropSet
            | Op::PropUnset
            | Op::DynamicPropGet
            | Op::DynamicPropSet
            | Op::DynamicPropUnset
            | Op::NullsafePropGet
            | Op::GcCollect
            | Op::Warn
    ) {
        return true;
    }
    match (op, immediate) {
        (
            Op::RuntimeCall,
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::Function(target)
                | crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. },
            )),
        ) => {
            matches!(
                target,
                crate::ir::RuntimeFnId::CloneWith
                    | crate::ir::RuntimeFnId::CurlEasyPerform
                    | crate::ir::RuntimeFnId::CurlMultiExec
                    | crate::ir::RuntimeFnId::Serialize
                    | crate::ir::RuntimeFnId::Unserialize
                    | crate::ir::RuntimeFnId::Implode
                    | crate::ir::RuntimeFnId::ArrayFlip
                    | crate::ir::RuntimeFnId::ArraySum
                    | crate::ir::RuntimeFnId::ArrayProduct
                    | crate::ir::RuntimeFnId::ObClean
                    | crate::ir::RuntimeFnId::ObEndClean
                    | crate::ir::RuntimeFnId::ObEndFlush
                    | crate::ir::RuntimeFnId::ObFlush
                    | crate::ir::RuntimeFnId::ObGetClean
                    | crate::ir::RuntimeFnId::ObGetFlush
            )
        }
        (
            Op::RuntimeCall,
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Pcntl(
                crate::ir::PcntlRuntime::SignalDispatch,
            ))),
        ) => true,
        (Op::CoreBuiltin, Some(Immediate::I64(selector))) => {
            matches!(crate::ir::CoreBuiltinOp::from_i64(*selector),
                Some(crate::ir::CoreBuiltinOp::TriggerError | crate::ir::CoreBuiltinOp::BuiltinWarning | crate::ir::CoreBuiltinOp::BuiltinDeprecated))
        }
        _ => false,
    }
}

/// Returns whether releasing a value of this storage type can reach a user destructor.
pub(super) fn php_type_cleanup_may_invoke_user_code(php_type: &PhpType) -> bool {
    // A union is asked member by member BEFORE its representation is consulted: `?string`
    // collapses to `Mixed` storage, but releasing a string-or-null slot can never reach a
    // destructor. Answering from the representation made `$text = null` inside a `try` an
    // opaque user-code boundary, which erased a global-backed closure fact nothing had touched.
    if let PhpType::Union(members) = php_type {
        return members.iter().any(php_type_cleanup_may_invoke_user_code);
    }
    match php_type.codegen_repr() {
        PhpType::Object(_) | PhpType::Mixed | PhpType::Iterable | PhpType::Callable => true,
        PhpType::Array(value) => php_type_cleanup_may_invoke_user_code(&value),
        PhpType::AssocArray { value, .. } => php_type_cleanup_may_invoke_user_code(&value),
        _ => false,
    }
}

/// Returns the value type an indexed or associative mutation may remove from its container.
pub(super) fn container_mutation_released_value_type(php_type: &PhpType) -> Option<PhpType> {
    match php_type.codegen_repr() {
        PhpType::Array(value) => Some(*value),
        PhpType::AssocArray { value, .. } => Some(*value),
        PhpType::Mixed => Some(PhpType::Mixed),
        _ => None,
    }
}

/// Returns whether one instruction can re-enter PHP outside a statically resolved direct call.
///
/// Direct `Call` instructions are closed through the user-function call graph instead. Treating
/// their deliberately broad default effects as an immediate opaque boundary would discard the
/// precise harmless-call summaries that closure binding relies on.
pub(super) fn instruction_has_opaque_user_code_boundary(
    op: Op,
    immediate: Option<&Immediate>,
    effects: Effects,
    operand_types: &[PhpType],
    local_type: Option<&PhpType>,
) -> bool {
    // HashUnset keeps conservative EIR effects, but this lowering fact only needs actual PHP
    // reentry. Scalar values and non-diagnostic keys cannot replace an unrelated global closure.
    if op == Op::HashUnset {
        let key_may_warn = operand_types.get(1).is_none_or(|key| {
            matches!(key.codegen_repr(), PhpType::Float | PhpType::Mixed)
        });
        let value_may_destruct = operand_types.first()
            .and_then(container_mutation_released_value_type)
            .is_none_or(|value| php_type_cleanup_may_invoke_user_code(&value));
        return key_may_warn || value_may_destruct;
    }
    // A release-family op carries the conservative `all()` default effects, so the effects test
    // below would call every one of them a boundary. Only the payload it retires decides: freeing
    // a string or an int slot cannot re-enter PHP, while a `Mixed`, object or object-bearing
    // container can run a destructor. Answering from the effects instead erased a global-backed
    // closure fact on `unset($number, $text)` and sent `Closure::bind` down the scope-less
    // runtime path for a closure nothing had touched.
    let releases_only = matches!(
        op,
        Op::Release
            | Op::ReleaseUnlessAliases
            | Op::ReleaseLocalSlot
            | Op::ReleaseLocalRefCell
            | Op::StoreStaticLocal
            | Op::StoreRefCell
    );
    if !releases_only
        && op != Op::Call
        && effects.intersects(Effects::MAY_WARN | Effects::OUTPUT)
    {
        return true;
    }

    let released_type = match op {
        Op::Release | Op::ReleaseUnlessAliases => operand_types.first().cloned(),
        Op::ReleaseLocalSlot
        | Op::ReleaseLocalRefCell
        | Op::StoreStaticLocal
        | Op::StoreRefCell => local_type.cloned(),
        Op::StoreGlobal | Op::StoreStaticProperty | Op::StoreReflectionStaticProperty => {
            return true;
        }
        Op::ArraySet | Op::HashSet | Op::HashSpread => operand_types
            .first()
            .and_then(container_mutation_released_value_type),
        Op::ArraySetMixedKey
        | Op::OffsetUnset
        | Op::AcquireRefCell
        | Op::MixedArrayAppend
        | Op::EvalScopeSet => return true,
        Op::CoreBuiltin => return core_builtin_registration_cleanup(immediate),
        // DescriptorArgSet writes only into a fresh internal argument container, after the
        // lowering-emitted duplicate-key guard. It cannot replace a live PHP value.
        _ => None,
    };
    if released_type
        .as_ref()
        .is_some_and(php_type_cleanup_may_invoke_user_code)
    {
        return true;
    }

    if opcode_has_opaque_user_code_target(op, immediate) {
        return true;
    }
    if matches!(op, Op::MixedCastString) {
        return true;
    }
    if op == Op::Cast
        && matches!(immediate, Some(Immediate::CastTarget(IrType::Str)))
        && operand_types
            .first()
            .is_some_and(php_type_may_reach_user_object)
    {
        return true;
    }
    runtime_call_may_invoke_implicit_user_code(immediate, operand_types)
}

/// Returns whether a type can directly or transitively contain an object with a PHP hook.
fn php_type_may_reach_user_object(php_type: &PhpType) -> bool {
    match php_type.codegen_repr() {
        PhpType::Object(_) | PhpType::Mixed | PhpType::Iterable => true,
        PhpType::Array(value) => php_type_may_reach_user_object(&value),
        PhpType::AssocArray { key, value } => {
            php_type_may_reach_user_object(&key) || php_type_may_reach_user_object(&value)
        }
        PhpType::Union(members) => members.iter().any(php_type_may_reach_user_object),
        _ => false,
    }
}

/// Returns whether a typed runtime operation can invoke an implicit PHP protocol method.
fn runtime_call_may_invoke_implicit_user_code(
    immediate: Option<&Immediate>,
    operand_types: &[PhpType],
) -> bool {
    let Some(Immediate::RuntimeCall(target)) = immediate else {
        return false;
    };
    match target {
        crate::ir::RuntimeCallTarget::ArrayFetchForWrite
            | crate::ir::RuntimeCallTarget::ArrayFetchForWriteAlreadyDiagnosed
            | crate::ir::RuntimeCallTarget::PendingAppendFetchForWrite
            | crate::ir::RuntimeCallTarget::PendingAppendFetchForWriteAlreadyDiagnosed => true,
        crate::ir::RuntimeCallTarget::Pcntl(
            crate::ir::PcntlRuntime::Signal | crate::ir::PcntlRuntime::SignalDispatch,
        ) => true,
        crate::ir::RuntimeCallTarget::Pcntl(crate::ir::PcntlRuntime::Exec) => {
            operand_types.iter().any(php_type_may_reach_user_object)
        }
        crate::ir::RuntimeCallTarget::Function(target)
        | crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. } => match target {
            crate::ir::RuntimeFnId::Count
            | crate::ir::RuntimeFnId::JsonEncode
            | crate::ir::RuntimeFnId::Printf
            | crate::ir::RuntimeFnId::Sprintf
            | crate::ir::RuntimeFnId::Vprintf
            | crate::ir::RuntimeFnId::Vsprintf
            | crate::ir::RuntimeFnId::Getenv => {
                operand_types.iter().any(php_type_may_reach_user_object)
            }
            _ => false,
        },
        _ => false,
    }
}

/// Returns whether a Core operation can retire a registered handler and its captured values.
pub(super) fn core_builtin_registration_cleanup(immediate: Option<&Immediate>) -> bool {
    let Some(Immediate::I64(selector)) = immediate else {
        return false;
    };
    matches!(
        crate::ir::CoreBuiltinOp::from_i64(*selector),
        Some(
            crate::ir::CoreBuiltinOp::RestoreErrorHandler
                | crate::ir::CoreBuiltinOp::RestoreExceptionHandler
                | crate::ir::CoreBuiltinOp::SetErrorHandler
                | crate::ir::CoreBuiltinOp::SetExceptionHandler
        )
    )
}

/// Returns true when eval can replace a local value with an arbitrary boxed cell.
fn eval_barrier_can_widen(php_type: &PhpType) -> bool {
    !matches!(
        php_type.codegen_repr(),
        PhpType::Never | PhpType::Pointer(_) | PhpType::Buffer(_) | PhpType::Packed(_)
    )
}

/// Converts an owner function name into a valid fragment for synthetic closure names.
fn closure_name_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
        .collect()
}

/// Returns the EIR return storage type for a function signature.
pub(crate) fn return_ir_type(php_type: &PhpType) -> IrType {
    let php_type = php_type.codegen_repr();
    match &php_type {
        PhpType::Void | PhpType::Never => IrType::Void,
        other => IrType::from_php(other),
    }
}

/// Returns the EIR storage type for an expression value.
pub(crate) fn value_ir_type(php_type: &PhpType) -> IrType {
    let php_type = php_type.codegen_repr();
    match &php_type {
        PhpType::Void | PhpType::Never => IrType::I64,
        other => IrType::from_php(other),
    }
}

/// Converts parsed type syntax into a conservative PHP type for fallback metadata.
pub(crate) fn type_expr_to_php_type(type_expr: &TypeExpr) -> PhpType {
    match type_expr {
        // Storage is a callable descriptor either way; the declared signature is checker-side.
        TypeExpr::CallableSig { .. } => PhpType::Callable,
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::False => PhpType::False,
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Iterable => PhpType::Iterable,
        TypeExpr::Array(inner) => PhpType::Array(Box::new(type_expr_to_php_type(inner))),
        TypeExpr::AssocArray { key, value } => PhpType::AssocArray {
            key: Box::new(type_expr_to_php_type(key)),
            value: Box::new(type_expr_to_php_type(value)),
        },
        TypeExpr::Ptr(name) => {
            PhpType::Pointer(name.as_ref().map(|name| {
                if name.is_unqualified() {
                    crate::names::resolved_pointer_target_tag(name.as_str())
                } else {
                    name.as_str().to_string()
                }
            }))
        }
        TypeExpr::Buffer(inner) => PhpType::Buffer(Box::new(
            crate::types::width_type_from_type_expr(inner)
                .unwrap_or_else(|| type_expr_to_php_type(inner)),
        )),
        TypeExpr::Named(name) => named_type_expr_to_php_type(name.as_str()),
        // Lowering only ever sees instantiated classes, which are `Named`. Should one
        // arrive uninstantiated, its head is the class it denotes — conservative metadata,
        // which is all this fallback promises.
        TypeExpr::GenericClass { name, .. } => named_type_expr_to_php_type(name.as_str()),
        TypeExpr::Nullable(inner) => {
            PhpType::Union(vec![PhpType::Void, type_expr_to_php_type(inner)])
        }
        TypeExpr::Union(members) => {
            PhpType::Union(members.iter().map(type_expr_to_php_type).collect())
        }
        // An intersection value is an object pointer; type it as its first member.
        TypeExpr::Intersection(members) => members
            .first()
            .map(type_expr_to_php_type)
            .unwrap_or(PhpType::Mixed),
    }
}

/// Converts parser-owned named type hints that represent PHP built-ins before falling back to objects.
fn named_type_expr_to_php_type(name: &str) -> PhpType {
    match name.trim_start_matches('\\').to_ascii_lowercase().as_str() {
        "array" => PhpType::php_array(),
        "callable" => PhpType::Callable,
        "closure" => PhpType::Callable,
        "mixed" => PhpType::Mixed,
        "null" | "void" => PhpType::Void,
        "object" => PhpType::Object(String::new()),
        _ => PhpType::Object(name.to_string()),
    }
}

/// Returns true when a typed-array source must be boxed to `Array(Mixed)` before being stored
/// through a reference cell.
///
/// The cell's element type is `Mixed` (the property is declared `array`) but the source array's
/// elements are a concrete non-`Mixed` type, so each element must be boxed for the property's
/// `Array(Mixed)` reads to interpret the slots correctly. Empty / `Never`-element sources have
/// no element descriptors to box and are excluded.
fn ref_cell_needs_mixed_array_conversion(cell_ty: &PhpType, value_ty: &PhpType) -> bool {
    ref_cell_array_element_type(cell_ty)
        .is_some_and(|elem| elem == PhpType::Mixed)
        && ref_cell_array_element_type(value_ty)
            .is_some_and(|elem| !matches!(elem, PhpType::Mixed | PhpType::Never))
}

/// Returns the element type of an array-shaped PHP type (indexed or associative), if any.
fn ref_cell_array_element_type(ty: &PhpType) -> Option<PhpType> {
    match ty.codegen_repr() {
        PhpType::Array(elem) => Some(elem.codegen_repr()),
        PhpType::AssocArray { value, .. } => Some(value.codegen_repr()),
        _ => None,
    }
}
