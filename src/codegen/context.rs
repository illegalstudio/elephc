//! Purpose:
//! Carries mutable codegen state such as local slots, labels, class metadata, and ownership facts.
//! Provides the shared bookkeeping used while lowering expressions, statements, functions, and wrappers.
//!
//! Called from:
//! - `crate::codegen::generate()` and nested codegen emitters
//!
//! Key details:
//! - Ownership states must remain conservative across branches, temporaries, and cleanup paths.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parser::ast::{CallableTarget, ExprKind, Stmt};
use crate::span::Span;
use crate::types::{
    ClassInfo, EnumInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, InterfaceInfo,
    PackedClassInfo, PhpType,
};

static GLOBAL_LABEL_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub(crate) const TRY_HANDLER_SLOT_SIZE: usize = 224;
pub(crate) const TRY_HANDLER_DIAG_DEPTH_OFFSET: usize = 16;
pub(crate) const TRY_HANDLER_JMP_BUF_OFFSET: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeapOwnership {
    NonHeap,
    Owned,
    Borrowed,
    MaybeOwned,
}

impl HeapOwnership {
    pub fn for_type(ty: &PhpType) -> Self {
        if ty.is_refcounted() || matches!(ty, PhpType::Str) {
            HeapOwnership::MaybeOwned
        } else {
            HeapOwnership::NonHeap
        }
    }

    pub fn local_owner_for_type(ty: &PhpType) -> Self {
        if ty.is_refcounted() || matches!(ty, PhpType::Str) {
            HeapOwnership::Owned
        } else {
            HeapOwnership::NonHeap
        }
    }

    pub fn borrowed_alias_for_type(ty: &PhpType) -> Self {
        if ty.is_refcounted() || matches!(ty, PhpType::Str) {
            HeapOwnership::Borrowed
        } else {
            HeapOwnership::NonHeap
        }
    }

    pub fn merge(self, other: Self) -> Self {
        use HeapOwnership::*;
        match (self, other) {
            (NonHeap, NonHeap) => NonHeap,
            (Owned, Owned) => Owned,
            (Borrowed, Borrowed) => Borrowed,
            (MaybeOwned, _) | (_, MaybeOwned) => MaybeOwned,
            (Owned, Borrowed) | (Borrowed, Owned) => MaybeOwned,
            (NonHeap, x) | (x, NonHeap) => x,
        }
    }
}

/// A closure body to be emitted after the current function.
#[allow(dead_code)]
pub struct DeferredClosure {
    pub label: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub sig: FunctionSig,
    pub captures: Vec<(String, PhpType, bool)>,
    pub hidden_params: Vec<(String, PhpType, bool)>,
    pub current_class: Option<String>,
    /// `true` when the wrapper body must be emitted because the runtime can
    /// invoke it. Real closures default to `true` (the only way to call them is
    /// via the wrapper). First-class-callable wrappers are downgraded to `false`
    /// at the FCC variable assignment site and only flipped back to `true` if
    /// the variable's value is read in a context other than the short-circuit
    /// (see `emit_variable`). When `false`, the wrapper is replaced by a tiny
    /// `ret`-only stub that keeps the symbol resolvable for the address load.
    pub needed: bool,
}

/// A Fiber entry wrapper emitted next to deferred closure bodies.
pub struct DeferredFiberWrapper {
    pub label: String,
    pub sig: FunctionSig,
    pub visible_param_count: usize,
    pub hidden_arg_types: Vec<PhpType>,
}

/// A callback wrapper that adapts callback builtins to closures with hidden captures.
pub struct DeferredCallbackWrapper {
    pub label: String,
    pub visible_arg_types: Vec<PhpType>,
    pub capture_types: Vec<PhpType>,
}

pub struct Context {
    pub variables: HashMap<String, VarInfo>,
    pub stack_offset: usize,
    pub loop_stack: Vec<LoopLabels>,
    pub return_label: Option<String>,
    pub functions: HashMap<String, FunctionSig>,
    pub function_variant_groups: HashSet<String>,
    pub deferred_closures: Vec<DeferredClosure>,
    pub deferred_fiber_wrappers: Vec<DeferredFiberWrapper>,
    pub deferred_callback_wrappers: Vec<DeferredCallbackWrapper>,
    pub constants: HashMap<String, (ExprKind, PhpType)>,
    /// Variables declared with `global $var` in the current function scope.
    pub global_vars: HashSet<String>,
    /// Variables declared with `static $var` in functions — maps "func_var" to type.
    pub static_vars: HashSet<String>,
    /// Reference parameters in the current function — stores their address, not value.
    pub ref_params: HashSet<String>,
    /// Hidden flags for compiler-created local reference cells.
    /// A non-zero flag means the variable's reference slot owns a 16-byte heap cell
    /// instead of borrowing storage from a caller, global, or array element.
    pub local_ref_cell_flags: HashMap<String, LocalRefCellFlag>,
    /// Whether we're in the main scope (not inside a function).
    pub in_main: bool,
    /// Set of all variable names that are used globally across the program.
    pub all_global_var_names: HashSet<String>,
    /// Static variable declarations: (func_name, var_name) -> type
    pub all_static_vars: HashMap<(String, String), PhpType>,
    /// Closure signatures keyed by variable name, for resolving defaults at call sites.
    pub closure_sigs: HashMap<String, FunctionSig>,
    /// Captured variables per closure variable name: maps $fn -> [(capture_name, type, by_ref)].
    pub closure_captures: HashMap<String, Vec<(String, PhpType, bool)>>,
    /// First-class callable target stored in a variable, mirroring the Checker's
    /// `first_class_callable_targets` so call sites can short-circuit to a direct
    /// function/method/static-method call instead of going through the closure
    /// wrapper. Populated at assignment time; cleared on reassignment to a
    /// non-FCC value. See `emit_closure_call` for consumers.
    pub first_class_callable_targets: HashMap<String, CallableTarget>,
    /// For each variable currently bound to an FCC, the label of the deferred
    /// wrapper that materialises that FCC. Used by `emit_variable` to mark a
    /// wrapper as `needed = true` when the FCC value escapes to anything other
    /// than a short-circuited call — at which point the dead-wrapper stub
    /// optimisation must back off and emit the full body.
    pub variable_fcc_label: HashMap<String, String>,
    /// Class definitions for OOP support.
    pub classes: HashMap<String, ClassInfo>,
    /// Interface definitions for OOP support.
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Trait declarations preserved for AOT introspection builtins.
    pub traits: HashSet<String>,
    /// Enum definitions.
    pub enums: HashMap<String, EnumInfo>,
    /// Packed layout-only record definitions.
    pub packed_classes: HashMap<String, PackedClassInfo>,
    /// Name of the class currently being compiled (for $this resolution).
    pub current_class: Option<String>,
    /// Extern function declarations (FFI).
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    /// Extern class (C struct) declarations (FFI).
    pub extern_classes: HashMap<String, ExternClassInfo>,
    /// Extern global variable declarations (FFI).
    pub extern_globals: HashMap<String, PhpType>,
    /// Current function return type for return/finally control-flow handling.
    pub return_type: PhpType,
    /// Hidden activation-record slot offsets: prev frame / cleanup callback / frame base.
    pub activation_prev_offset: Option<usize>,
    pub activation_cleanup_offset: Option<usize>,
    pub activation_frame_base_offset: Option<usize>,
    /// Hidden control-flow continuation state used to route return/break/continue through finally blocks.
    pub pending_action_offset: Option<usize>,
    pub pending_target_offset: Option<usize>,
    pub nested_concat_offset_offset: Option<usize>,
    pub pending_return_value_offset: Option<usize>,
    /// Pre-allocated exception handler slots for try/catch lowering.
    pub try_slot_offsets: Vec<usize>,
    pub next_try_slot_idx: usize,
    /// Stack of active finally regions (innermost last).
    pub finally_stack: Vec<FinallyContext>,
}

pub struct VarInfo {
    pub ty: PhpType,
    pub static_ty: PhpType,
    pub stack_offset: usize,
    pub ownership: HeapOwnership,
    pub epilogue_cleanup_safe: bool,
}

pub struct LocalRefCellFlag {
    pub variable: String,
    pub offset: usize,
    pub value_ty: Option<PhpType>,
}

pub struct LoopLabels {
    pub continue_label: String,
    pub break_label: String,
    /// If true, this loop entry is a switch that pushed 16 bytes to the stack.
    /// Return statements inside need to pop this before jumping to epilogue.
    pub sp_adjust: usize,
}

#[derive(Debug, Clone)]
pub struct FinallyContext {
    pub entry_label: String,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            stack_offset: 0,
            loop_stack: Vec::new(),
            return_label: None,
            functions: HashMap::new(),
            function_variant_groups: HashSet::new(),
            deferred_closures: Vec::new(),
            deferred_fiber_wrappers: Vec::new(),
            deferred_callback_wrappers: Vec::new(),
            constants: HashMap::new(),
            global_vars: HashSet::new(),
            static_vars: HashSet::new(),
            ref_params: HashSet::new(),
            local_ref_cell_flags: HashMap::new(),
            in_main: false,
            all_global_var_names: HashSet::new(),
            all_static_vars: HashMap::new(),
            closure_sigs: HashMap::new(),
            closure_captures: HashMap::new(),
            first_class_callable_targets: HashMap::new(),
            variable_fcc_label: HashMap::new(),
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            traits: HashSet::new(),
            enums: HashMap::new(),
            packed_classes: HashMap::new(),
            current_class: None,
            extern_functions: HashMap::new(),
            extern_classes: HashMap::new(),
            extern_globals: HashMap::new(),
            return_type: PhpType::Void,
            activation_prev_offset: None,
            activation_cleanup_offset: None,
            activation_frame_base_offset: None,
            pending_action_offset: None,
            pending_target_offset: None,
            nested_concat_offset_offset: None,
            pending_return_value_offset: None,
            try_slot_offsets: Vec::new(),
            next_try_slot_idx: 0,
            finally_stack: Vec::new(),
        }
    }

    pub fn alloc_var(&mut self, name: &str, ty: PhpType) -> usize {
        self.alloc_var_with_static_type(name, ty.clone(), ty)
    }

    pub fn alloc_var_with_static_type(
        &mut self,
        name: &str,
        ty: PhpType,
        static_ty: PhpType,
    ) -> usize {
        self.stack_offset += ty.stack_size();
        let offset = self.stack_offset;
        let ownership = HeapOwnership::for_type(&ty);
        self.variables.insert(
            name.to_string(),
            VarInfo {
                ty,
                static_ty,
                stack_offset: offset,
                ownership,
                epilogue_cleanup_safe: true,
            },
        );
        offset
    }

    pub fn alloc_hidden_slot(&mut self, size: usize) -> usize {
        self.stack_offset += size;
        self.stack_offset
    }

    pub fn foreach_local_ref_cell_flag_key(name: &str, span: Span) -> String {
        format!("{}:{}:{}", name, span.line, span.col)
    }

    pub fn ensure_local_ref_cell_flag(&mut self, key: String, name: &str) -> usize {
        if let Some(flag) = self.local_ref_cell_flags.get(&key) {
            return flag.offset;
        }
        let offset = self.alloc_hidden_slot(8);
        self.local_ref_cell_flags.insert(
            key,
            LocalRefCellFlag {
                variable: name.to_string(),
                offset,
                value_ty: None,
            },
        );
        offset
    }

    pub fn set_local_ref_cell_flag_type(&mut self, key: &str, value_ty: PhpType) {
        if let Some(flag) = self.local_ref_cell_flags.get_mut(key) {
            flag.value_ty = Some(value_ty);
        }
    }

    pub fn set_var_ownership(&mut self, name: &str, ownership: HeapOwnership) {
        if let Some(var) = self.variables.get_mut(name) {
            var.ownership = ownership;
        }
    }

    pub fn disable_epilogue_cleanup(&mut self, name: &str) {
        if let Some(var) = self.variables.get_mut(name) {
            var.epilogue_cleanup_safe = false;
        }
    }

    pub fn enable_epilogue_cleanup(&mut self, name: &str) {
        if let Some(var) = self.variables.get_mut(name) {
            var.epilogue_cleanup_safe = true;
        }
    }

    pub fn update_var_type_and_ownership(
        &mut self,
        name: &str,
        ty: PhpType,
        ownership: HeapOwnership,
    ) {
        self.update_var_type_static_and_ownership(name, ty.clone(), ty, ownership);
    }

    /// Marks the deferred FCC wrapper backing `var` as `needed = true`, so the
    /// emission loop emits its body instead of the dead-wrapper stub. Call this
    /// from any site that consumes an FCC variable's runtime value (loads its
    /// address for an indirect call, threads its captures through a callback
    /// builtin, materialises it into a Fiber, etc.). The short-circuit paths
    /// in `emit_closure_call` deliberately do NOT call this — that's the whole
    /// point of the optimisation.
    pub fn mark_fcc_used(&mut self, var: &str) {
        if let Some(label) = self.variable_fcc_label.get(var).cloned() {
            if let Some(deferred) =
                self.deferred_closures.iter_mut().find(|d| d.label == label)
            {
                deferred.needed = true;
            }
        }
    }

    pub fn update_var_type_static_and_ownership(
        &mut self,
        name: &str,
        ty: PhpType,
        static_ty: PhpType,
        ownership: HeapOwnership,
    ) {
        if let Some(var) = self.variables.get_mut(name) {
            var.ty = ty;
            var.static_ty = static_ty;
            var.ownership = ownership;
        }
    }

    pub fn common_object_type(&self, left: &str, right: &str) -> Option<PhpType> {
        if left == right {
            return Some(PhpType::Object(left.to_string()));
        }
        if self.is_subclass_of(left, right)
            || self.class_implements_interface(left, right)
            || self.interface_extends_interface(left, right)
        {
            return Some(PhpType::Object(right.to_string()));
        }
        if self.is_subclass_of(right, left)
            || self.class_implements_interface(right, left)
            || self.interface_extends_interface(right, left)
        {
            return Some(PhpType::Object(left.to_string()));
        }

        let mut left_ancestors = HashSet::new();
        let mut current = Some(left.to_string());
        while let Some(class_name) = current {
            left_ancestors.insert(class_name.clone());
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        let mut current = Some(right.to_string());
        while let Some(class_name) = current {
            if left_ancestors.contains(&class_name) {
                return Some(PhpType::Object(class_name));
            }
            current = self
                .classes
                .get(&class_name)
                .and_then(|class_info| class_info.parent.clone());
        }

        None
    }

    fn is_subclass_of(&self, class_name: &str, ancestor_name: &str) -> bool {
        let mut current = self
            .classes
            .get(class_name)
            .and_then(|class_info| class_info.parent.as_deref());
        while let Some(parent) = current {
            if parent == ancestor_name {
                return true;
            }
            current = self
                .classes
                .get(parent)
                .and_then(|class_info| class_info.parent.as_deref());
        }
        false
    }

    pub(crate) fn object_type_implements_interface(
        &self,
        type_name: &str,
        interface_name: &str,
    ) -> bool {
        if self.classes.contains_key(type_name) {
            return self.class_implements_interface(type_name, interface_name);
        }
        if self.interfaces.contains_key(type_name) {
            return type_name == interface_name
                || self.interface_extends_interface(type_name, interface_name);
        }
        false
    }

    fn class_implements_interface(&self, class_name: &str, interface_name: &str) -> bool {
        self.classes.get(class_name).is_some_and(|class_info| {
            class_info.interfaces.iter().any(|implemented| {
                implemented == interface_name
                    || self.interface_extends_interface(implemented, interface_name)
            })
        })
    }

    fn interface_extends_interface(&self, child_name: &str, ancestor_name: &str) -> bool {
        if child_name == ancestor_name {
            return true;
        }
        self.interfaces.get(child_name).is_some_and(|interface_info| {
            interface_info.parents.iter().any(|parent| {
                parent == ancestor_name || self.interface_extends_interface(parent, ancestor_name)
            })
        })
    }

    pub fn next_label(&mut self, prefix: &str) -> String {
        let id = GLOBAL_LABEL_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("_{}_{}", prefix, id)
    }

    pub fn next_try_slot(&mut self) -> usize {
        let offset = *self
            .try_slot_offsets
            .get(self.next_try_slot_idx)
            .expect("codegen bug: missing pre-allocated try handler slot");
        self.next_try_slot_idx += 1;
        offset
    }
}

#[cfg(test)]
mod tests {
    use super::HeapOwnership;
    use crate::types::PhpType;

    #[test]
    fn test_heap_ownership_type_classification() {
        assert_eq!(HeapOwnership::for_type(&PhpType::Int), HeapOwnership::NonHeap);
        assert_eq!(HeapOwnership::for_type(&PhpType::Str), HeapOwnership::MaybeOwned);
        assert_eq!(
            HeapOwnership::local_owner_for_type(&PhpType::AssocArray {
                key: Box::new(PhpType::Str),
                value: Box::new(PhpType::Int),
            }),
            HeapOwnership::Owned
        );
        assert_eq!(
            HeapOwnership::borrowed_alias_for_type(&PhpType::Object("Foo".to_string())),
            HeapOwnership::Borrowed
        );
    }

    #[test]
    fn test_heap_ownership_merge() {
        assert_eq!(
            HeapOwnership::Owned.merge(HeapOwnership::Owned),
            HeapOwnership::Owned
        );
        assert_eq!(
            HeapOwnership::Borrowed.merge(HeapOwnership::Borrowed),
            HeapOwnership::Borrowed
        );
        assert_eq!(
            HeapOwnership::Owned.merge(HeapOwnership::Borrowed),
            HeapOwnership::MaybeOwned
        );
        assert_eq!(
            HeapOwnership::NonHeap.merge(HeapOwnership::Borrowed),
            HeapOwnership::Borrowed
        );
    }
}
