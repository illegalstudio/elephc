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

use std::collections::{HashMap, HashSet};

use crate::ir::{
    BlockId, Builder, DataId, DataPool, Effects, Immediate, IrType, LocalKind, LocalSlotId, Op,
    Ownership, ValueId,
};
use crate::parser::ast::{ExprKind, TypeExpr};
use crate::span::Span;
use crate::types::{
    ClassInfo, EnumInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo, PhpType,
    TypeEnv,
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
}

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
    pub classes: &'m HashMap<String, ClassInfo>,
    pub enums: &'m HashMap<String, EnumInfo>,
    pub interfaces: &'m HashMap<String, InterfaceInfo>,
    pub packed_classes: &'m HashMap<String, PackedClassInfo>,
    pub constants: HashMap<String, (ExprKind, PhpType)>,
    pub top_level_env: TypeEnv,
    pub current_class: Option<String>,
    pub loop_stack: Vec<LoopFrame>,
    pub return_type: IrType,
    pub return_php_type: PhpType,
    pub in_main: bool,
    pub all_global_var_names: HashSet<String>,
    hidden_temp_counter: usize,
}

impl<'m, 'f> LoweringContext<'m, 'f> {
    /// Creates a lowering context over one function builder and shared module data.
    pub(crate) fn new(
        builder: Builder<'f>,
        data: &'m mut DataPool,
        env: TypeEnv,
        functions: &'m HashMap<String, FunctionSig>,
        extern_functions: &'m HashMap<String, ExternFunctionSig>,
        classes: &'m HashMap<String, ClassInfo>,
        enums: &'m HashMap<String, EnumInfo>,
        interfaces: &'m HashMap<String, InterfaceInfo>,
        packed_classes: &'m HashMap<String, PackedClassInfo>,
        constants: &'m HashMap<String, (ExprKind, PhpType)>,
        top_level_env: TypeEnv,
        current_class: Option<String>,
        return_php_type: PhpType,
        in_main: bool,
        all_global_var_names: HashSet<String>,
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
            classes,
            enums,
            interfaces,
            packed_classes,
            constants: constants.clone(),
            top_level_env,
            current_class,
            loop_stack: Vec::new(),
            return_type,
            return_php_type,
            in_main,
            all_global_var_names,
            hidden_temp_counter: 0,
        }
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
                if self.packed_classes.contains_key(name) {
                    PhpType::Packed(name.to_string())
                } else {
                    PhpType::Object(name.to_string())
                }
            }
            TypeExpr::Buffer(inner) => {
                PhpType::Buffer(Box::new(self.type_expr_to_php_type_for_value(inner)))
            }
            TypeExpr::Nullable(inner) => {
                PhpType::Union(vec![PhpType::Void, self.type_expr_to_php_type_for_value(inner)])
            }
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
        self.local_types.get(name).cloned().unwrap_or(PhpType::Mixed)
    }

    /// Returns the checker-known top-level type for a `global` alias name.
    pub(crate) fn global_alias_type(&self, name: &str) -> PhpType {
        self.top_level_env
            .get(name)
            .cloned()
            .unwrap_or_else(|| self.local_type(name))
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
        self.local_types.insert(name.to_string(), ty);
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
        let slot = self.builder.add_local(
            Some(name.to_string()),
            ir_type,
            php_type.clone(),
            kind,
        );
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

    /// Declares a fresh hidden temporary slot and returns its synthetic name.
    pub(crate) fn declare_hidden_temp(&mut self, php_type: PhpType) -> String {
        let name = format!("__eir_tmp{}", self.hidden_temp_counter);
        self.hidden_temp_counter += 1;
        self.declare_local_with_kind(&name, php_type, LocalKind::HiddenTemp);
        name
    }

    /// Emits a load from a PHP local slot.
    pub(crate) fn load_local(&mut self, name: &str, span: Option<Span>) -> LoweredValue {
        let php_type = self.local_type(name);
        let slot = self.declare_local(name, php_type.clone());
        let ir_type = value_ir_type(&php_type);
        let ownership = Ownership::for_php_type(&php_type);
        let kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, kind);
        let op = match (uses_global, kind) {
            (true, _) => Op::LoadGlobal,
            (false, LocalKind::StaticLocal) => Op::LoadStaticLocal,
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

    /// Emits a store to a PHP local slot and updates the local type fact.
    pub(crate) fn store_local(&mut self, name: &str, value: LoweredValue, php_type: PhpType, span: Option<Span>) {
        let previous_slot = self.local_slots.get(name).copied();
        let previous_type = self.local_type(name);
        let previous_kind = self.local_kinds.get(name).copied().unwrap_or(LocalKind::PhpLocal);
        let uses_global = self.uses_global_storage(name, previous_kind);
        let slot = self.declare_local(name, php_type.clone());
        if !uses_global
            && previous_kind == LocalKind::PhpLocal
            && previous_slot.is_some_and(|slot| self.initialized_slots.contains(&slot))
            && Ownership::php_type_needs_lifetime_tracking(&previous_type)
        {
            let previous = self.load_local(name, span);
            crate::ir_lower::ownership::release_if_owned(self, previous, span);
        }
        let value = if uses_global || previous_kind == LocalKind::PhpLocal {
            crate::ir_lower::ownership::acquire_if_refcounted(self, value, span)
        } else {
            value
        };
        if uses_global {
            self.store_global_name(name, slot, value, span);
            self.set_local_type(name, php_type);
            return;
        }
        let op = match previous_kind {
            LocalKind::StaticLocal => Op::StoreStaticLocal,
            _ => Op::StoreLocal,
        };
        self.store_slot_with_op(slot, value, op, span);
        self.set_local_type(name, php_type);
    }

    /// Returns whether the named PHP variable should use program-global storage.
    fn uses_global_storage(&self, name: &str, kind: LocalKind) -> bool {
        kind == LocalKind::GlobalAlias
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
            .emit_with_effects(op, operands, immediate, ir_type, php_type, ownership, effects, span)
            .expect("value opcode produces a value");
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

/// Returns the EIR return storage type for a function signature.
pub(crate) fn return_ir_type(php_type: &PhpType) -> IrType {
    match php_type {
        PhpType::Void | PhpType::Never => IrType::Void,
        other => IrType::from_php(other),
    }
}

/// Returns the EIR storage type for an expression value.
pub(crate) fn value_ir_type(php_type: &PhpType) -> IrType {
    match php_type {
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
        TypeExpr::Str => PhpType::Str,
        TypeExpr::Void => PhpType::Void,
        TypeExpr::Never => PhpType::Never,
        TypeExpr::Iterable => PhpType::Iterable,
        TypeExpr::Ptr(name) => PhpType::Pointer(name.as_ref().map(|name| name.as_str().to_string())),
        TypeExpr::Buffer(inner) => PhpType::Buffer(Box::new(type_expr_to_php_type(inner))),
        TypeExpr::Named(name) => PhpType::Object(name.as_str().to_string()),
        TypeExpr::Nullable(inner) => PhpType::Union(vec![PhpType::Void, type_expr_to_php_type(inner)]),
        TypeExpr::Union(members) => {
            PhpType::Union(members.iter().map(type_expr_to_php_type).collect())
        }
    }
}
