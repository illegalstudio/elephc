use std::collections::HashMap;

use crate::types::PhpType;

pub struct Context {
    pub variables: HashMap<String, VarInfo>,
    pub stack_offset: usize,
}

pub struct VarInfo {
    pub ty: PhpType,
    pub stack_offset: usize,
}

impl Context {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
            stack_offset: 0,
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
}
