use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parser::ast::{ExprKind, Stmt};
use crate::types::{FunctionSig, PhpType};

static GLOBAL_LABEL_COUNTER: AtomicUsize = AtomicUsize::new(0);

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
}

pub struct VarInfo {
    pub ty: PhpType,
    pub stack_offset: usize,
}

pub struct LoopLabels {
    pub continue_label: String,
    pub break_label: String,
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
        }
    }

    pub fn alloc_var(&mut self, name: &str, ty: PhpType) -> usize {
        self.stack_offset += ty.stack_size();
        let offset = self.stack_offset;
        self.variables.insert(
            name.to_string(),
            VarInfo {
                ty,
                stack_offset: offset,
            },
        );
        offset
    }

    pub fn next_label(&mut self, prefix: &str) -> String {
        let id = GLOBAL_LABEL_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("_{}_{}", prefix, id)
    }
}
