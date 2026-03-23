use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::parser::ast::Stmt;
use crate::types::{FunctionSig, PhpType};

static GLOBAL_LABEL_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A closure body to be emitted after the current function.
#[allow(dead_code)]
pub struct DeferredClosure {
    pub label: String,
    pub params: Vec<String>,
    pub body: Vec<Stmt>,
    pub sig: FunctionSig,
}

pub struct Context {
    pub variables: HashMap<String, VarInfo>,
    pub stack_offset: usize,
    pub loop_stack: Vec<LoopLabels>,
    pub return_label: Option<String>,
    pub functions: HashMap<String, FunctionSig>,
    pub deferred_closures: Vec<DeferredClosure>,
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
