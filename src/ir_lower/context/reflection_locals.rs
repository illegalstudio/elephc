//! Purpose:
//! Snapshots and intersects compile-time Reflection targets at conditional branches.
//!
//! Called from:
//! - Conditional statement lowering through `LoweringContext`.
//!
//! Key details:
//! - Only identical facts present on every reachable path survive a join.
//! - Each sibling starts with the facts after its own condition has executed.

use super::{Expr, HashMap, LoweringContext};

/// Reflection targets known for local values at one control-flow boundary.
#[derive(Clone, Default)]
pub(crate) struct ReflectionBindings {
    classes: HashMap<String, String>,
    functions: HashMap<String, String>,
    properties: HashMap<String, (String, String)>,
    methods: HashMap<String, (String, String)>,
    argument_arrays: HashMap<String, Vec<Expr>>,
}

impl ReflectionBindings {
    /// Keeps facts that agree with another reachable arm, discarding uncertain targets.
    pub(crate) fn intersect(&mut self, other: &Self) {
        retain_common(&mut self.classes, &other.classes);
        retain_common(&mut self.functions, &other.functions);
        retain_common(&mut self.properties, &other.properties);
        retain_common(&mut self.methods, &other.methods);
        retain_common(&mut self.argument_arrays, &other.argument_arrays);
    }
}

/// Intersects one target inventory without inventing a fact absent from either path.
fn retain_common<T: PartialEq>(facts: &mut HashMap<String, T>, other: &HashMap<String, T>) {
    facts.retain(|name, target| other.get(name) == Some(target));
}

impl LoweringContext<'_, '_> {
    /// Captures Reflection targets after a branch condition or reachable arm.
    pub(crate) fn reflection_bindings_snapshot(&self) -> ReflectionBindings {
        ReflectionBindings {
            classes: self.reflection_class_locals.clone(),
            functions: self.reflection_function_locals.clone(),
            properties: self.reflection_property_locals.clone(),
            methods: self.reflection_method_locals.clone(),
            argument_arrays: self.reflection_arg_array_locals.clone(),
        }
    }

    /// Restores the exact targets for a sibling arm or the intersection at a join.
    pub(crate) fn restore_reflection_bindings(&mut self, facts: ReflectionBindings) {
        self.reflection_class_locals = facts.classes;
        self.reflection_function_locals = facts.functions;
        self.reflection_property_locals = facts.properties;
        self.reflection_method_locals = facts.methods;
        self.reflection_arg_array_locals = facts.argument_arrays;
    }
}
