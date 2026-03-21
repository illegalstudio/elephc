use std::collections::HashMap;

use crate::types::PhpType;

pub struct Context {
    pub variables: HashMap<String, VarInfo>,
    pub stack_offset: usize,
    label_counter: usize,
    pub loop_stack: Vec<LoopLabels>,
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
            label_counter: 0,
            loop_stack: Vec::new(),
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
        let label = format!("_{}_{}", prefix, self.label_counter);
        self.label_counter += 1;
        label
    }
}
