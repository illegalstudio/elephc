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

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::ir::{
    BlockId, Builder, DataId, DataPool, Effects, Function, Immediate, IrType, LocalKind,
    LocalSlotId, Op, Ownership, ValueId,
};
use crate::names::{php_symbol_key, property_hook_get_method, property_hook_set_method};
use crate::parser::ast::{Expr, ExprKind, StaticReceiver, Stmt, TypeExpr};
use crate::span::Span;
use crate::types::{
    ClassInfo, EnumInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo, PhpType,
    ReturnAliasSummaries, ThrowAccessInfo, TypeEnv,
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
}

/// Compile-time callable target tracked for straight-line local FCC calls.
#[derive(Debug, Clone)]
pub(crate) enum StaticCallableBinding {
    UserFunction(String),
    ExternFunction(String),
    Builtin(String),
    Closure {
        name: String,
        signature: FunctionSig,
        captures: Vec<ClosureCapture>,
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
#[derive(Debug, Clone)]
pub(crate) struct ClosureCapture {
    pub value: ValueId,
}

const EVAL_CONTEXT_LOCAL_NAME: &str = "__eir_eval_context";
const EVAL_SCOPE_LOCAL_NAME: &str = "__eir_eval_scope";
const EVAL_GLOBAL_SCOPE_LOCAL_NAME: &str = "__eir_eval_global_scope";
const EVAL_ARGC_LOCAL_NAME: &str = "argc";
const EVAL_ARGV_LOCAL_NAME: &str = "argv";

/// Mutable state for one function body while it is lowered.
pub(crate) struct LoweringContext<'m, 'f> {
    pub builder: Builder<'f>,
    pub data: &'m mut DataPool,
    pub local_slots: HashMap<String, LocalSlotId>,
    pub local_kinds: HashMap<String, LocalKind>,
    pub local_types: TypeEnv,
    initialized_slots: HashSet<LocalSlotId>,
    pub functions: &'m HashMap<String, FunctionSig>,
    pub extern_functions: &'m HashMap<String, ExternFunctionSig>,
    pub extern_globals: &'m HashMap<String, PhpType>,
    pub callable_param_sigs: &'m HashMap<(String, String), FunctionSig>,
    pub(crate) return_alias_summaries: &'m ReturnAliasSummaries,
    pub(crate) fiber_return_sigs: &'m HashMap<String, FunctionSig>,
    pub classes: &'m HashMap<String, ClassInfo>,
    pub enums: &'m HashMap<String, EnumInfo>,
    pub interfaces: &'m HashMap<String, InterfaceInfo>,
    pub packed_classes: &'m HashMap<String, PackedClassInfo>,
    /// Statically-decided access violations lowered to runtime `Error` throws,
    /// keyed by the source span of the offending call/assignment.
    pub throw_access_sites: &'m HashMap<Span, ThrowAccessInfo>,
    /// Authoritative checker result types for builtin calls in this source module.
    pub builtin_call_types: &'m HashMap<Span, PhpType>,
    pub constants: HashMap<String, (ExprKind, PhpType)>,
    pub top_level_env: TypeEnv,
    pub current_class: Option<String>,
    pub loop_stack: Vec<LoopFrame>,
    pub finally_stack: Vec<FinallyFrame>,
    static_callable_locals: HashMap<String, StaticCallableBinding>,
    reflection_class_locals: HashMap<String, String>,
    reflection_function_locals: HashMap<String, String>,
    reflection_property_locals: HashMap<String, (String, String)>,
    reflection_method_locals: HashMap<String, (String, String)>,
    reflection_arg_array_locals: HashMap<String, Vec<Expr>>,
    fiber_start_sigs: HashMap<String, FunctionSig>,
    ref_bound_locals: HashSet<String>,
    ref_cell_owner_locals: HashMap<String, LocalSlotId>,
    /// foreach loop-key locals whose source is a concretely-indexed array
    /// (`Array` of a non-Mixed element type), so the runtime key is always an
    /// integer even though `Op::IterCurrentKey` lowers it as Mixed. Used by
    /// `lower_array_assign` to avoid promoting a `$dst[$key] = ...` write to the
    /// hash path (and coercing the key to int) for these int-valued keys, while
    /// still promoting for keys that may be strings (generic `Array(Mixed)`,
    /// `AssocArray`, `Mixed`, `Union` sources).
    foreach_int_key_locals: HashSet<String>,
    pub return_type: IrType,
    pub return_php_type: PhpType,
    /// `true` when the function/closure being lowered returns by reference (`function &f()`),
    /// so a `return $obj->prop` yields the property's ref-cell pointer instead of a value copy.
    pub by_ref_return: bool,
    pub in_main: bool,
    pub all_global_var_names: HashSet<String>,
    /// `true` when lowering for a `--web` compile. Gates whether a bare
    /// request-superglobal name (`$_SERVER`/`$_SESSION`/…) is trusted to
    /// resolve to the fixed `AssocArray{Str, Mixed}` type: only `--web`
    /// builds pre-initialize that shared global storage, so a CLI build must
    /// fall back to the ordinary local/top-level type lookup (typically
    /// `Mixed`) instead of assuming a live Hash pointer. See `global_alias_type`.
    pub web: bool,
    owner_name: String,
    closures: Vec<Function>,
    pending_static_callable_result: Option<StaticCallableBinding>,
    closure_counter: usize,
    hidden_temp_counter: usize,
    eval_barrier_active: bool,
    eval_executed: bool,
    eval_scope_read_param: Option<String>,
    eval_scope_read_names: HashSet<String>,
    eval_scope_write_names: HashSet<String>,
    eval_scope_flush_names: BTreeSet<String>,
    source_path: Option<String>,
}

impl<'m, 'f> LoweringContext<'m, 'f> {
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
        classes: &'m HashMap<String, ClassInfo>,
        enums: &'m HashMap<String, EnumInfo>,
        interfaces: &'m HashMap<String, InterfaceInfo>,
        packed_classes: &'m HashMap<String, PackedClassInfo>,
        throw_access_sites: &'m HashMap<Span, ThrowAccessInfo>,
        builtin_call_types: &'m HashMap<Span, PhpType>,
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
        let return_type = return_ir_type(&return_php_type);
        Self {
            builder,
            data,
            local_slots: HashMap::new(),
            local_kinds: HashMap::new(),
            local_types: env,
            initialized_slots: HashSet::new(),
            functions,
            extern_functions,
            extern_globals,
            callable_param_sigs,
            return_alias_summaries,
            fiber_return_sigs,
            classes,
            enums,
            interfaces,
            packed_classes,
            throw_access_sites,
            builtin_call_types,
            constants: constants.clone(),
            top_level_env,
            current_class,
            loop_stack: Vec::new(),
            finally_stack: Vec::new(),
            static_callable_locals: HashMap::new(),
            reflection_class_locals: HashMap::new(),
            reflection_function_locals: HashMap::new(),
            reflection_property_locals: HashMap::new(),
            reflection_method_locals: HashMap::new(),
            reflection_arg_array_locals: HashMap::new(),
            fiber_start_sigs: HashMap::new(),
            ref_bound_locals: HashSet::new(),
            ref_cell_owner_locals: HashMap::new(),
            foreach_int_key_locals: HashSet::new(),
            return_type,
            return_php_type,
            by_ref_return: false,
            in_main,
            all_global_var_names,
            web,
            owner_name,
            closures: Vec::new(),
            pending_static_callable_result: None,
            closure_counter: 0,
            hidden_temp_counter: 0,
            eval_barrier_active: false,
            eval_executed: false,
            eval_scope_read_param: None,
            eval_scope_read_names: HashSet::new(),
            eval_scope_write_names: HashSet::new(),
            eval_scope_flush_names: BTreeSet::new(),
            source_path,
        }
    }

    /// Returns the canonical PHP source path associated with this lowered body, if known.
    pub(crate) fn source_path(&self) -> Option<&str> {
        self.source_path.as_deref()
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
            TypeExpr::Buffer(inner) => {
                PhpType::Buffer(Box::new(self.type_expr_to_php_type_for_value(inner)))
            }
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
        if self.web && crate::superglobals::is_superglobal(name) {
            return crate::superglobals::superglobal_type();
        }
        PhpType::Mixed
    }

    /// Returns the prescanned value and PHP type for a global constant name.
    pub(crate) fn constant_value(&self, name: &str) -> Option<(ExprKind, PhpType)> {
        self.constants
            .get(name)
            .or_else(|| self.constants.get(name.trim_start_matches('\\')))
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
        self.constants.entry(name).or_insert((value, ty));
    }

    /// Updates the current known PHP type for a local.
    pub(crate) fn set_local_type(&mut self, name: &str, ty: PhpType) {
        if let Some(slot) = self.local_slots.get(name).copied() {
            self.builder.widen_local_storage_type(slot, ty.clone());
        }
        self.local_types.insert(name.to_string(), ty);
    }

    /// Updates only the flow-sensitive PHP type fact for a local.
    pub(crate) fn set_local_logical_type(&mut self, name: &str, ty: PhpType) {
        self.local_types.insert(name.to_string(), ty);
    }

    /// Returns `true` if a local slot has already been declared for `name`.
    pub(crate) fn has_local_slot(&self, name: &str) -> bool {
        self.local_slots.contains_key(name)
    }

    /// Declares a local slot if it does not already exist.
    pub(crate) fn declare_local(&mut self, name: &str, php_type: PhpType) -> LocalSlotId {
        self.declare_local_with_kind(name, php_type, LocalKind::PhpLocal)
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
        let ir_type = value_ir_type(&php_type);
        let slot = self
            .builder
            .add_local(Some(name.to_string()), ir_type, php_type.clone(), kind);
        self.local_slots.insert(name.to_string(), slot);
        self.local_kinds.insert(name.to_string(), kind);
        self.local_types.entry(name.to_string()).or_insert(php_type);
        slot
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

    /// Replaces the definitely-initialized local set after branch lowering or merge analysis.
    pub(crate) fn restore_initialized_slots(&mut self, initialized_slots: HashSet<LocalSlotId>) {
        self.initialized_slots = initialized_slots;
    }

    /// Records that a local currently aliases by-reference storage.
    pub(crate) fn mark_ref_bound_local(&mut self, name: &str) {
        self.ref_bound_locals.insert(name.to_string());
    }

    /// Clears the by-reference alias marker for a local after `unset()`.
    pub(crate) fn unmark_ref_bound_local(&mut self, name: &str) {
        self.ref_bound_locals.remove(name);
    }

    /// Returns true when a local is currently modeled as a by-reference alias.
    pub(crate) fn is_ref_bound_local(&self, name: &str) -> bool {
        self.ref_bound_locals.contains(name)
    }

    /// Declares a fresh hidden temporary slot and returns its synthetic name.
    pub(crate) fn declare_hidden_temp(&mut self, php_type: PhpType) -> String {
        let name = format!("__eir_tmp{}", self.hidden_temp_counter);
        self.hidden_temp_counter += 1;
        self.declare_local_with_kind(&name, php_type, LocalKind::HiddenTemp);
        name
    }

    /// Declares a one-shot hidden expression-result temporary.
    pub(crate) fn declare_owned_hidden_temp(&mut self, php_type: PhpType) -> String {
        let name = format!("__eir_tmp{}", self.hidden_temp_counter);
        self.hidden_temp_counter += 1;
        self.declare_local_with_kind(&name, php_type, LocalKind::OwnedTemp);
        name
    }

    /// Declares a parser-reserved hidden expression-result temporary.
    pub(crate) fn declare_owned_hidden_temp_with_name(
        &mut self,
        name: &str,
        php_type: PhpType,
    ) -> LocalSlotId {
        self.declare_local_with_kind(name, php_type, LocalKind::OwnedTemp)
    }

    /// Ensures this function has a persistent eval context handle slot.
    pub(crate) fn declare_eval_context_local(&mut self) -> LocalSlotId {
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
    pub(crate) fn apply_eval_barrier(&mut self) {
        self.eval_barrier_active = true;
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
                (kind == LocalKind::PhpLocal).then_some((name.clone(), *slot))
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
            if kind == LocalKind::PhpLocal && eval_barrier_can_widen(&ty) {
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
    fn declare_ref_cell_owner(&mut self, variable: &str, php_type: PhpType) -> LocalSlotId {
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
    fn ref_cell_owner_slot(&self, variable: &str) -> Option<LocalSlotId> {
        self.ref_cell_owner_locals.get(variable).copied()
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

    /// Clears stale callable-result metadata before lowering a new independent expression.
    pub(crate) fn clear_pending_static_callable_result(&mut self) {
        self.pending_static_callable_result = None;
    }

    /// Emits a load from a PHP local slot.
    pub(crate) fn load_local(&mut self, name: &str, span: Option<Span>) -> LoweredValue {
        if let Some(php_type) = self.extern_global_type(name) {
            return self.load_extern_global(name, php_type, span);
        }
        if self.should_load_from_eval_scope(name) {
            return self.load_eval_scope_name(name, span);
        }
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, kind);
        // Under `--web`, superglobals carry a fixed `AssocArray{Str, Mixed}` type
        // in every scope. Ordinary globals remain boxed Mixed cells.
        let php_type = if uses_global {
            self.global_alias_type(name)
        } else {
            self.local_type(name)
        };
        let slot = self.declare_local(name, php_type.clone());
        let ir_type = value_ir_type(&php_type);
        let ownership = Ownership::for_php_type(&php_type);
        let is_ref_bound = self.is_ref_bound_local(name) && !uses_global && kind == LocalKind::PhpLocal;
        let op = match (is_ref_bound, uses_global, kind) {
            (true, _, _) => Op::LoadRefCell,
            (false, true, _) => Op::LoadGlobal,
            (false, false, LocalKind::StaticLocal) => Op::LoadStaticLocal,
            _ => Op::LoadLocal,
        };
        let immediate = if uses_global {
            Some(Immediate::GlobalName(self.intern_global_name(name)))
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

    /// Returns true when a variable read should be sourced from the eval scope handle.
    fn should_load_from_eval_scope(&self, name: &str) -> bool {
        let Some(scope_param) = &self.eval_scope_read_param else {
            return false;
        };
        name != scope_param
            && !self.local_slots.contains_key(name)
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
        name != scope_param && self.eval_scope_write_names.contains(name)
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
            Some(Immediate::GlobalName(self.intern_global_name(name)))
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
        let storage_type = self.builder.local_php_type(slot);
        if !Ownership::php_type_needs_lifetime_tracking(&storage_type) {
            return;
        }
        let previous = self.load_local_storage(name, slot, storage_type, span);
        crate::ir_lower::ownership::release_if_owned(self, previous, span);
    }

    /// Releases the previous occupant immediately before a retaining store overwrites it.
    ///
    /// The caller must first retain the incoming value because borrowing operations
    /// can return storage that aliases the previous occupant (for example,
    /// `$value = trim($value)`). When the slot's storage type already needs lifetime
    /// tracking this emits the eager load+release pair. When it does not, the slot can
    /// STILL be widened to refcounted storage by a store lowered later that reaches
    /// this one through a loop back-edge (e.g. an inner `for` counter re-initialized
    /// by the outer body but widened Int→Mixed by its checked-add update). The storage
    /// type visible here is stale in that case, so inside loops a deferred
    /// `release_local_slot` is emitted instead: the backend releases the occupant
    /// using the final widened storage type, and `prune_untracked_release_local_slot_ops`
    /// erases the op when the slot never widens (issue #534: without this, the
    /// previous outer iteration's Mixed box leaked on every re-initialization).
    fn release_stored_local_value_before_overwrite(
        &mut self,
        name: &str,
        slot: LocalSlotId,
        span: Option<Span>,
    ) {
        let storage_type = self.builder.local_php_type(slot);
        if Ownership::php_type_needs_lifetime_tracking(&storage_type) {
            self.release_stored_local_value(name, slot, span);
            return;
        }
        if self.loop_stack.is_empty() {
            // Outside loops no back-edge can execute a later widening store before
            // this one, so the untracked storage type is final for this path.
            return;
        }
        // Ref-bound locals keep a cell pointer in the frame slot and are released
        // through the ref-cell owner machinery, never through a raw slot release.
        if self.is_ref_bound_local(name) {
            return;
        }
        self.emit_void(
            Op::ReleaseLocalSlot,
            Vec::new(),
            Some(Immediate::LocalSlot(slot)),
            Op::ReleaseLocalSlot.default_effects(),
            span,
        );
    }

    /// Emits a store to a PHP local slot, updates type facts, and returns the stored value.
    pub(crate) fn store_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
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
        } else {
            php_type
        };
        let slot = self.declare_local(name, php_type.clone());
        // Backend frame layout uses the final widened slot type for every load
        // and store, so cleanup loads must be typed after this store's widening.
        // For ref-bound locals, keep the existing slot type to avoid widening
        // Int→Mixed mid-function (which would break earlier loads that expect I64).
        // The codegen narrows Mixed→Int at the store point instead.
        let is_ref_bound = self.is_ref_bound_local(name);
        let widen_type = if is_ref_bound {
            previous_type.clone()
        } else {
            php_type.clone()
        };
        self.builder.widen_local_storage_type(slot, widen_type);
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
        // Retain before cleanup because a borrowed result can alias the old slot.
        let value = if (uses_global || previous_kind == LocalKind::PhpLocal)
            && !transfer_source_to_store
            && !self.is_ref_bound_local(name)
        {
            crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
        } else if (uses_global || previous_kind == LocalKind::PhpLocal)
            && !transfer_source_to_store
        {
            // For ref-bound locals, acquire only when NOT narrowing Mixed→Int.
            // When the source is Mixed and the ref cell's previous type is Int,
            // the ref cell store narrows via __rt_mixed_cast_int, consuming the
            // Mixed box. The release_if_owned at the end frees the original
            // without a paired incref, which is correct for narrowing.
            let source_is_mixed = matches!(
                self.builder.value_php_type(value.value).codegen_repr(),
                PhpType::Mixed
            );
            let target_is_int = matches!(previous_type.codegen_repr(), PhpType::Int);
            if !(source_is_mixed && target_is_int) {
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
            self.release_stored_local_value_before_overwrite(name, slot, span);
        }
        // A loop-carried slot can exist globally without being definitely initialized
        // on this CFG path. Release the runtime occupant before overwriting it.
        if !uses_global
            && local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_some_and(|slot| !self.initialized_slots.contains(&slot))
            && !self.loop_stack.is_empty()
        {
            self.release_stored_local_value_before_overwrite(name, slot, span);
        }
        // A first syntactic store inside a loop body (main or function) can still
        // overwrite a prior runtime iteration's value: the slot has no straight-line
        // predecessor store so it is not in `initialized_slots`, but the loop back-edge
        // makes it live on iterations 2+. Release the previous occupant so the old value
        // is freed on reassign. Function cleanup locals (including returned slots) are
        // zero-initialized in the prologue, so the first iteration safely releases a null
        // slot; subsequent iterations release the prior value.
        if !uses_global
            && local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_none()
            && !self.loop_stack.is_empty()
        {
            self.release_stored_local_value_before_overwrite(name, slot, span);
        }
        if uses_global {
            self.store_global_name(name, slot, value, span);
            self.set_local_type(name, php_type);
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
            && matches!(previous_type.codegen_repr(), PhpType::Int);
        if is_ref_bound {
            let value = self.box_typed_array_for_mixed_ref_cell(value, &previous_type, span);
            self.store_ref_cell_slot(slot, value, previous_type.clone(), span);
        } else {
            self.store_slot_with_op(slot, value, op, span);
        }
        if !is_ref_bound {
            self.set_local_type(name, php_type);
        }
        if release_source_after_store
            && !transfer_source_to_store
            && !ref_cell_narrowed_mixed_to_int
        {
            crate::ir_lower::ownership::release_if_owned(self, source, span);
        }
        value
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
            && !self.loop_stack.is_empty()
        {
            self.release_stored_local_value(name, slot, span);
        }
        if local_kind_uses_plain_store_cleanup(previous_kind)
            && previous_slot.is_none()
            && !self.loop_stack.is_empty()
        {
            self.release_stored_local_value(name, slot, span);
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
        let previous_kind = self
            .local_kinds
            .get(name)
            .copied()
            .unwrap_or(LocalKind::PhpLocal);
        if self.uses_global_storage(name, previous_kind) {
            return;
        }
        let slot = self.declare_local(name, replacement_type.clone());
        let is_ref_bound =
            self.is_ref_bound_local(name) && previous_kind == LocalKind::PhpLocal;
        let source_type = self.builder.value_php_type(source.value).codegen_repr();
        self.builder
            .widen_local_storage_type(slot, replacement_type);
        let storage_type = self.builder.local_php_type(slot).codegen_repr();
        if !is_ref_bound
            && matches!(storage_type, PhpType::Mixed | PhpType::Union(_))
            && !matches!(source_type, PhpType::Mixed | PhpType::Union(_))
        {
            self.release_stored_local_value(name, slot, span);
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
        self.store_mutated_local_impl(name, value, php_type, span, true)
    }

    /// Stores a mutation result whose previous boxed local owner was released beforehand.
    pub(crate) fn store_prepared_mutated_local(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
    ) -> LoweredValue {
        self.store_mutated_local_impl(name, value, php_type, span, false)
    }

    /// Implements consuming local storeback with caller-selected cleanup timing.
    fn store_mutated_local_impl(
        &mut self,
        name: &str,
        value: LoweredValue,
        php_type: PhpType,
        span: Option<Span>,
        release_previous: bool,
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
            self.set_local_type(name, php_type);
            return value;
        }
        let is_ref_bound = self.is_ref_bound_local(name) && previous_kind == LocalKind::PhpLocal;
        let value_type = self.builder.value_php_type(value.value).codegen_repr();
        self.set_local_type(name, php_type.clone());
        let storage_type = self.builder.local_php_type(slot).codegen_repr();
        if release_previous
            && !is_ref_bound
            && matches!(storage_type, PhpType::Mixed | PhpType::Union(_))
            && !matches!(value_type, PhpType::Mixed | PhpType::Union(_))
        {
            self.release_stored_local_value(name, slot, span);
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

    /// Emits `unset($local)`, breaking by-reference aliases without writing through them.
    pub(crate) fn unset_local(
        &mut self,
        name: &str,
        null: LoweredValue,
        span: Option<Span>,
    ) -> LoweredValue {
        if !self.is_ref_bound_local(name) {
            return self.store_local(name, null, PhpType::Void, span);
        }
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
        self.set_local_type(name, PhpType::Void);
        self.initialized_slots.insert(slot);
        null
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
    }

    /// Binds one local name to the same ref-cell pointer as another local.
    pub(crate) fn alias_local_ref_cell(&mut self, target: &str, source: &str, span: Option<Span>) {
        if target == source {
            return;
        }
        let source_ty = self.local_type(source);
        // `is_ref_bound_local` is intentionally conservative across lowered
        // branches, so a source marked by a conditional predecessor may still
        // be raw on another runtime path. An idempotent promotion here gives
        // every alias operation a cell on all incoming paths.
        self.promote_local_ref_cell(source, span);
        self.clear_static_callable_local(target);
        self.clear_reflection_class_local(target);
        self.clear_reflection_function_local(target);
        self.clear_reflection_property_local(target);
        self.clear_reflection_method_local(target);
        self.clear_reflection_arg_array_local(target);
        self.clear_fiber_start_sig(target);
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
            source_ty,
            Ownership::NonHeap,
            Op::AliasLocalRefCell.default_effects(),
            span,
        );
        self.mark_ref_bound_local(target);
        self.initialized_slots.insert(target_slot);
    }

    /// Binds `target` as a NON-owning reference alias to an already-materialized ref-cell
    /// pointer (`cell_ptr`), e.g. the cell behind an object reference property (`$x = &$obj->prop`)
    /// or returned by a by-reference call (`$x = &f()`). `value_type` is the PHP type the cell
    /// holds, used to type the target and to dereference it on later loads/stores.
    ///
    /// Unlike `alias_local_ref_cell`, no hidden owner slot is created and no `ReleaseLocalRefCell`
    /// is emitted for `target` at scope exit: the cell is owned by the source (the object), so the
    /// alias must not free it.
    pub(crate) fn bind_local_ref_cell_ptr(
        &mut self,
        target: &str,
        cell_ptr: LoweredValue,
        value_type: PhpType,
        span: Option<Span>,
    ) {
        self.clear_static_callable_local(target);
        self.clear_fiber_start_sig(target);
        self.release_replaced_local_before_ref_alias(target, span);
        let target_slot = self.declare_local(target, value_type.clone());
        self.set_local_type(target, value_type.clone());
        self.builder.emit_with_effects(
            Op::BindRefCellPtr,
            vec![cell_ptr.value],
            Some(Immediate::LocalSlot(target_slot)),
            IrType::Void,
            value_type,
            Ownership::NonHeap,
            Op::BindRefCellPtr.default_effects(),
            span,
        );
        self.mark_ref_bound_local(target);
        self.initialized_slots.insert(target_slot);
    }

    /// Releases storage currently owned by a local before rebinding it as a ref alias.
    fn release_replaced_local_before_ref_alias(&mut self, name: &str, span: Option<Span>) {
        if self.is_ref_bound_local(name) {
            self.release_ref_cell_owner(name, span);
            return;
        }
        let Some(slot) = self.local_slots.get(name).copied() else {
            return;
        };
        if !self.initialized_slots.contains(&slot) {
            return;
        }
        self.release_stored_local_value(name, slot, span);
    }

    /// Releases a promoted fallback ref-cell owner if the variable still owns one.
    pub(crate) fn release_ref_cell_owner(&mut self, name: &str, span: Option<Span>) {
        let Some(owner_slot) = self.ref_cell_owner_slot(name) else {
            return;
        };
        let owner_ty = self.builder.local_php_type(owner_slot);
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
        let php_type = self.builder.value_php_type(value.value);
        if !value.ir_type.is_refcounted_storage()
            && !Ownership::php_type_needs_lifetime_tracking(&php_type)
        {
            return false;
        }
        if self.value_is_owning_builtin_temporary(value.value) {
            return true;
        }
        if self.value_is_owned_temp_load(value.value) {
            return true;
        }
        if self.value_is_owned_unboxed_local_load(value.value) {
            return true;
        }
        if self.value_is_owning_mixed_string_cast(value.value) {
            return true;
        }
        if self.value_is_owning_container_read(value.value) {
            return true;
        }
        if self.value_is_owned_index_read_temp(value) {
            return true;
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
                    | Op::IToStr
                    | Op::FToStr
                    | Op::BoolToStr
                    | Op::ResourceToStr
                    | Op::MixedBox
                    | Op::ArrayToMixed
                    | Op::HashToMixed
                    | Op::InvokerRefArg
                    | Op::MixedNumericBinop
                    | Op::ICheckedAdd
                    | Op::ICheckedSub
                    | Op::ICheckedMul
                    | Op::MixedCastString
                    | Op::StrConcat
                    | Op::StrPersist
                    | Op::StrCharAt
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
                    | Op::ObjectNew
                    | Op::ObjectCloneShallow
                    | Op::DynamicObjectNew
                    | Op::DynamicObjectNewMixed
                    | Op::DynamicObjectNewWithoutConstructorMixed
                    | Op::ClosureNew
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

    /// Returns whether a user-call result can alias a borrowed visible argument.
    ///
    /// User functions currently return refcounted parameter storage without
    /// acquiring it for the caller. Such a result is borrowed when the matching
    /// argument is borrowed, but remains an owning temporary when an owning
    /// argument temporary transfers through the call.
    fn value_is_borrowed_user_call_result(&self, result: ValueId) -> bool {
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
        let Some(return_alias) = self.return_alias_summaries.function(function_name) else {
            return false;
        };
        inst.operands
            .iter()
            .enumerate()
            .any(|(parameter_index, argument)| {
                if !return_alias.proven_aliases_parameter(parameter_index)
                    || !self.call_result_may_alias_arg(*argument, result)
                {
                    return false;
                }
                let argument = LoweredValue {
                    value: *argument,
                    ir_type: self.builder.value_type(*argument),
                };
                !self.value_is_owning_temporary(argument)
            })
    }

    /// Returns whether a call result can legally reuse one argument's refcounted payload.
    pub(crate) fn call_result_may_alias_arg(&self, argument: ValueId, result: ValueId) -> bool {
        if matches!(
            self.builder.value_defining_op(argument),
            Some(Op::MixedNumericBinop | Op::ICheckedAdd | Op::ICheckedSub | Op::ICheckedMul)
        ) {
            return false;
        }
        let argument_type = self.builder.value_php_type(argument).codegen_repr();
        let result_type = self.builder.value_php_type(result).codegen_repr();
        if !Ownership::php_type_needs_lifetime_tracking(&argument_type)
            || !Ownership::php_type_needs_lifetime_tracking(&result_type)
        {
            return false;
        }
        match (&argument_type, &result_type) {
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

    /// Returns whether the value is a read from a one-shot hidden expression temp.
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
    }

    /// Returns whether a concrete local heap load may require an owned-unbox release.
    ///
    /// Later source-order stores can widen the final frame slot after this load has
    /// already been lowered. Array/hash/object/iterable loads are therefore treated as
    /// provisional owners; builder finalization removes their emitted releases if the
    /// slot stays concrete. Callable loads use the eager answer because assignment has
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
            && matches!(result_type, PhpType::Callable)
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
        matches!(
            self.builder.value_php_type(source).codegen_repr(),
            PhpType::Mixed | PhpType::Union(_)
        )
    }

    /// Returns whether a retained local/global store should release its source value.
    pub(crate) fn value_needs_release_after_retaining_store(&self, value: LoweredValue) -> bool {
        self.value_is_owning_temporary(value)
    }

    /// Returns whether a container read now owns a caller reference.
    fn value_is_owning_container_read(&self, value: ValueId) -> bool {
        let php_type = self.builder.value_php_type(value);
        let php_type = php_type.codegen_repr();
        let op = self.builder.value_defining_op(value);
        (matches!(php_type, PhpType::Mixed | PhpType::Union(_))
            || (php_type.is_refcounted() && php_type != PhpType::Str))
            && matches!(op, Some(Op::ArrayGet | Op::HashGet | Op::HashGetSilent))
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
                    | Op::HashGet
                    | Op::HashGetSilent
                    | Op::ArrayGetMixedKey
                    | Op::ArrayGetMixedKeySilent
            )
        )
    }

    /// Returns true for typed builtin calls whose result is newly allocated for the caller.
    fn value_is_owning_builtin_temporary(&self, value: ValueId) -> bool {
        let Some(inst) = self.builder.value_defining_instruction(value) else {
            return false;
        };
        match inst.immediate {
            Some(Immediate::RuntimeCall(
                crate::ir::RuntimeCallTarget::ArrayFetchForWrite,
            )) => false,
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))) => {
                matches!(
                    target.result_ownership(),
                    crate::builtins::semantics::BuiltinResultOwnership::Fresh
                )
            }
            Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::UnaryString(_))) => true,
            Some(Immediate::Data(name_id)) if inst.op == Op::LanguageConstructCall => self
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
        if self.can_track_static_callable_local(name) {
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

    /// Clears the compile-time callable association for one local.
    pub(crate) fn clear_static_callable_local(&mut self, name: &str) {
        self.static_callable_locals.remove(name);
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
    pub(crate) fn clear_static_callable_locals(&mut self) {
        self.static_callable_locals.clear();
        self.reflection_class_locals.clear();
        self.reflection_function_locals.clear();
        self.reflection_property_locals.clear();
        self.reflection_method_locals.clear();
        self.reflection_arg_array_locals.clear();
        self.fiber_start_sigs.clear();
    }

    /// Returns whether the named PHP variable should use program-global storage.
    ///
    /// Request superglobals (`$_SERVER`/`$_GET`/`$_POST`) route to the shared
    /// `_eir_global_*` symbol in EVERY scope — main and functions alike — so a
    /// function read targets the same storage the top-level `--web` prelude writes.
    fn uses_global_storage(&self, name: &str, kind: LocalKind) -> bool {
        kind == LocalKind::GlobalAlias
            || crate::superglobals::is_superglobal(name)
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
        let data = self.intern_global_name(name);
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
    fn store_slot_with_op(
        &mut self,
        slot: LocalSlotId,
        value: LoweredValue,
        op: Op,
        span: Option<Span>,
    ) {
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

    /// Emits a void opcode with optional operands and source span.
    pub(crate) fn emit_void(
        &mut self,
        op: Op,
        operands: Vec<ValueId>,
        immediate: Option<Immediate>,
        effects: Effects,
        span: Option<Span>,
    ) {
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
        let value = self
            .builder
            .emit_with_effects(
                op, operands, immediate, ir_type, php_type, ownership, effects, span,
            )
            .expect("value opcode produces a value");
        LoweredValue { value, ir_type }
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

    /// Emits an `is_truthy` conversion when a value is not already I64.
    pub(crate) fn truthy(&mut self, input: LoweredValue, span: Option<Span>) -> LoweredValue {
        if input.ir_type == IrType::I64 {
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
}

impl crate::builtins::semantics::BuiltinLoweringContext for LoweringContext<'_, '_> {
    /// Returns PHP metadata already attached to an EIR operand.
    fn value_php_type(&self, value: ValueId) -> PhpType {
        self.builder.value_php_type(value)
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

    /// Emits a typed runtime call whose helper symbol and physical ABI remain backend-owned.
    fn emit_runtime_call(
        &mut self,
        target: crate::ir::RuntimeCallTarget,
        operands: Vec<ValueId>,
        php_type: PhpType,
        effects: Effects,
        span: Option<Span>,
    ) -> crate::builtins::semantics::LoweredBuiltinValue {
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
        TypeExpr::Int => PhpType::Int,
        TypeExpr::Float => PhpType::Float,
        TypeExpr::Bool => PhpType::Bool,
        TypeExpr::False => PhpType::False,
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Iterable => PhpType::Iterable,
        TypeExpr::Array(inner) => PhpType::Array(Box::new(type_expr_to_php_type(inner))),
        TypeExpr::Ptr(name) => {
            PhpType::Pointer(name.as_ref().map(|name| name.as_str().to_string()))
        }
        TypeExpr::Buffer(inner) => PhpType::Buffer(Box::new(type_expr_to_php_type(inner))),
        TypeExpr::Named(name) => named_type_expr_to_php_type(name.as_str()),
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
        "array" => PhpType::Array(Box::new(PhpType::Mixed)),
        "callable" => PhpType::Callable,
        "closure" => PhpType::Callable,
        "mixed" => PhpType::Mixed,
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
