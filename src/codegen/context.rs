use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parser::ast::{ExprKind, Stmt};
use crate::types::{ClassInfo, ExternClassInfo, ExternFunctionSig, FunctionSig, PhpType};

static GLOBAL_LABEL_COUNTER: AtomicUsize = AtomicUsize::new(0);

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
    pub captures: Vec<(String, PhpType)>,
}

pub struct Context {
    pub variables: HashMap<String, VarInfo>,
    pub stack_offset: usize,
    pub loop_stack: Vec<LoopLabels>,
    pub return_label: Option<String>,
    pub functions: HashMap<String, FunctionSig>,
    pub deferred_closures: Vec<DeferredClosure>,
    pub constants: HashMap<String, (ExprKind, PhpType)>,
    /// Variables declared with `global $var` in the current function scope.
    pub global_vars: HashSet<String>,
    /// Variables declared with `static $var` in functions — maps "func_var" to type.
    pub static_vars: HashSet<String>,
    /// Reference parameters in the current function — stores their address, not value.
    pub ref_params: HashSet<String>,
    /// Whether we're in the main scope (not inside a function).
    pub in_main: bool,
    /// Set of all variable names that are used globally across the program.
    pub all_global_var_names: HashSet<String>,
    /// Static variable declarations: (func_name, var_name) -> type
    pub all_static_vars: HashMap<(String, String), PhpType>,
    /// Closure signatures keyed by variable name, for resolving defaults at call sites.
    pub closure_sigs: HashMap<String, FunctionSig>,
    /// Captured variables per closure variable name: maps $fn -> [(capture_name, type)].
    pub closure_captures: HashMap<String, Vec<(String, PhpType)>>,
    /// Class definitions for OOP support.
    pub classes: HashMap<String, ClassInfo>,
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
    pub pending_return_value_offset: Option<usize>,
    /// Pre-allocated exception handler slots for try/catch lowering.
    pub try_slot_offsets: Vec<usize>,
    pub next_try_slot_idx: usize,
    /// Stack of active finally regions (innermost last).
    pub finally_stack: Vec<FinallyContext>,
}

pub struct VarInfo {
    pub ty: PhpType,
    pub stack_offset: usize,
    pub ownership: HeapOwnership,
    pub epilogue_cleanup_safe: bool,
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
            deferred_closures: Vec::new(),
            constants: HashMap::new(),
            global_vars: HashSet::new(),
            static_vars: HashSet::new(),
            ref_params: HashSet::new(),
            in_main: false,
            all_global_var_names: HashSet::new(),
            all_static_vars: HashMap::new(),
            closure_sigs: HashMap::new(),
            closure_captures: HashMap::new(),
            classes: HashMap::new(),
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
            pending_return_value_offset: None,
            try_slot_offsets: Vec::new(),
            next_try_slot_idx: 0,
            finally_stack: Vec::new(),
        }
    }

    pub fn alloc_var(&mut self, name: &str, ty: PhpType) -> usize {
        self.stack_offset += ty.stack_size();
        let offset = self.stack_offset;
        let ownership = HeapOwnership::for_type(&ty);
        self.variables.insert(
            name.to_string(),
            VarInfo {
                ty,
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
        if let Some(var) = self.variables.get_mut(name) {
            var.ty = ty;
            var.ownership = ownership;
        }
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
