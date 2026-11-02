//! Purpose:
//! Resolves PHP parameter passing modes before source-order call argument evaluation.
//!
//! Called from:
//! - Eval function, method, constructor, callable and mutating builtin dispatch.
//!
//! Key details:
//! - This is a passing-mode view only; existing binders still own validation and defaults.
//! - Native XML reference selections require write context at their source position.

use super::*;

/// A caller-visible signature snapshot that remains stable across argument side effects.
#[derive(Default)]
pub(in crate::interpreter) struct CallArgumentModes {
    names: Vec<String>,
    references: Vec<bool>,
    variadic: Option<usize>,
}

impl CallArgumentModes {
    /// Captures an eval-declared function's public signature.
    pub(in crate::interpreter) fn for_function(function: &EvalFunction) -> Self {
        Self { names: function.params().to_vec(), references: function.parameter_is_by_ref().to_vec(),
            variadic: function.parameter_is_variadic().iter().position(|flag| *flag) }
    }

    /// Captures a registered free function without the compiler's hidden collector.
    pub(in crate::interpreter) fn for_native_function(function: &NativeFunction) -> Self {
        Self { names: function.param_names().to_vec(),
            references: (0..function.param_count()).map(|index| function.param_by_ref(index)).collect(),
            variadic: function.source_variadic_index() }
    }

    /// Captures a generated method's visible physical slots and source variadic.
    fn for_native_signature(signature: crate::context::NativeCallableSignature) -> Self {
        let indexes = signature.visible_param_indexes();
        Self { names: indexes.iter().map(|index| signature.param_names().get(*index).cloned().unwrap_or_default()).collect(),
            references: indexes.iter().map(|index| signature.param_by_ref(*index)).collect(),
            variadic: signature.source_variadic_index().and_then(|physical| indexes.iter().position(|index| *index == physical)) }
    }

    /// Captures shared builtin reference flags, including named parameters.
    pub(in crate::interpreter) fn for_builtin(name: &str) -> Self {
        let Some(contract) = elephc_builtin_contract::lookup(name) else { return Self::default(); };
        Self { names: contract.params.iter().map(|parameter| parameter.name.to_string()).collect(),
            references: contract.params.iter().map(|parameter| parameter.by_ref).collect(), variadic: None }
    }

    /// Chooses the declared parameter or the source variadic without rebinding values.
    pub(in crate::interpreter) fn by_reference(&self, name: Option<&str>, position: usize) -> bool {
        let index = match name {
            Some(name) => self.names.iter().position(|candidate| candidate == name).or(self.variadic),
            None => Some(self.variadic.map_or(position, |index| index.min(position))),
        };
        index.and_then(|index| self.references.get(index)).copied().unwrap_or(false)
    }

    /// Resolves the method selected by the active lexical scope without executing it.
    pub(in crate::interpreter) fn for_method(
        object: RuntimeCellHandle, method: &str, context: &ElephcEvalContext,
        values: &mut impl RuntimeValueOps,
    ) -> Result<Self, EvalStatus> {
        let class = values.object_identity(object).ok().and_then(|identity| context.dynamic_object_class(identity))
            .map(|class| class.name().to_string());
        let class = match class { Some(class) => class, None => runtime_object_class_name(object, values)? };
        Ok(Self::for_class_method(&class, method, false, context))
    }

    /// Reads eval metadata first and generated method metadata for a native inherited slot.
    pub(in crate::interpreter) fn for_class_method(
        class: &str, method: &str, is_static: bool, context: &ElephcEvalContext,
    ) -> Self {
        let dynamic = if is_static { eval_dynamic_static_method_for_call(class, method, context) }
            else { eval_dynamic_method_for_call(class, method, context) };
        if let Some((owner, method)) = dynamic {
            if validate_eval_member_access(&owner, method.visibility(), context).is_ok() {
                return Self { names: method.params().to_vec(), references: method.parameter_is_by_ref().to_vec(),
                    variadic: method.parameter_is_variadic().iter().position(|flag| *flag) };
            }
            return Self::default();
        }
        let signature = if is_static { context.native_static_method_signature(class, method) }
            else { context.native_method_signature(class, method) };
        signature.map(Self::for_native_signature).unwrap_or_default()
    }

    /// Constructor lookup keeps the same public parameter flags as ordinary dispatch.
    pub(in crate::interpreter) fn for_constructor(class: &str, context: &ElephcEvalContext) -> Self {
        if context.class(class).is_some() { return Self::for_class_method(class, "__construct", false, context); }
        context.native_constructor_signature(class).map(Self::for_native_signature).unwrap_or_default()
    }

    /// Reads a resolved first-class callable's signature before evaluating its operands.
    pub(in crate::interpreter) fn for_callable(
        callable: &EvaluatedCallable, context: &ElephcEvalContext, values: &mut impl RuntimeValueOps,
    ) -> Result<Self, EvalStatus> {
        Ok(match callable {
            EvaluatedCallable::ForeignContext { .. } => Self::default(),
            EvaluatedCallable::NativeDescriptor { callback, .. } => {
                match values.native_callable_signature(*callback)? {
                    Some(signature) => Self { names: signature.parameters.iter().map(|parameter| parameter.name.clone()).collect(),
                        references: signature.parameters.iter().map(|parameter| parameter.by_ref).collect(), variadic: signature.variadic_index },
                    None => Self::default(),
                }
            }
            EvaluatedCallable::Named { name, .. } | EvaluatedCallable::BoundClosure { name, .. } => {
                if let Some(closure) = context.closure(name) { Self::for_function(closure.function()) }
                else if let Some(function) = context.function(name) { Self::for_function(function) }
                else if let Some(function) = context.native_function(name) { Self::for_native_function(&function) }
                else { Self::for_builtin(name) }
            }
            EvaluatedCallable::InvokableObject { object } => Self::for_method(*object, "__invoke", context, values)?,
            EvaluatedCallable::ObjectMethod { object, method, bridge_scope, .. } => {
                if let Some(class) = bridge_scope { Self::for_class_method(class, method, false, context) }
                else { Self::for_method(*object, method, context, values)? }
            }
            EvaluatedCallable::StaticMethod { class_name, method, bridge_scope, .. } =>
                Self::for_class_method(bridge_scope.as_deref().unwrap_or(class_name), method, true, context),
        })
    }
}
