//! Purpose:
//! Resolves EIR-specific Fiber wrapper requirements for callable callbacks.
//! Keeps wrapper label selection shared between wrapper emission and `new Fiber` lowering.
//!
//! Called from:
//! - `crate::codegen_ir::block_emit` when emitting deferred wrapper functions.
//! - `crate::codegen_ir::lower_inst::objects` when lowering Fiber construction.
//!
//! Key details:
//! - Closure wrappers are per closure signature because they adapt boxed Fiber
//!   start arguments into the concrete callback ABI.
//! - Descriptor-backed callables share a generic wrapper that calls the descriptor
//!   invoker with an indexed Mixed argument array.
//! - Captures remain descriptor-owned; this phase only passes visible callback parameters.

use crate::ir::{Function, Immediate, Instruction, Module, Op, ValueDef};
use crate::names::php_symbol_key;
use crate::types::{FunctionSig, PhpType};

/// Static wrapper function required for an EIR Fiber callback ABI shape.
#[derive(Clone)]
pub(crate) struct FiberWrapper {
    pub(crate) label: String,
    pub(crate) sig: FunctionSig,
    pub(crate) visible_param_count: usize,
    pub(crate) hidden_arg_types: Vec<PhpType>,
    pub(crate) use_descriptor_invoker: bool,
}

/// Returns the wrapper needed when `new Fiber(...)` receives a supported callable operand.
pub(crate) fn wrapper_for_fiber_new(
    module: &Module,
    function: &Function,
    inst: &Instruction,
) -> Option<FiberWrapper> {
    if !is_fiber_object_new(module, inst) {
        return None;
    }
    let callable = inst.operands.first().copied()?;
    if let Some(closure) = closure_literal_operand(module, function, callable) {
        return Some(wrapper_for_closure(closure));
    }
    if callable_operand_is_descriptor_backed(function, callable) {
        return Some(descriptor_invoker_wrapper());
    }
    None
}

/// Returns true when an EIR object construction instruction targets PHP's built-in `Fiber`.
fn is_fiber_object_new(module: &Module, inst: &Instruction) -> bool {
    if !matches!(inst.op, Op::ObjectNew) {
        return false;
    }
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .is_some_and(|class_name| php_symbol_key(class_name.trim_start_matches('\\')) == "fiber")
}

/// Resolves a callable operand produced by `closure_new` to its EIR closure body.
fn closure_literal_operand<'a>(
    module: &'a Module,
    function: &Function,
    callable: crate::ir::ValueId,
) -> Option<&'a Function> {
    let value = function.value(callable)?;
    let ValueDef::Instruction {
        inst: callable_inst,
        ..
    } = value.def
    else {
        return None;
    };
    let callable_inst = function.instruction(callable_inst)?;
    if !matches!(callable_inst.op, Op::ClosureNew) {
        return None;
    }
    let Some(Immediate::Data(data)) = callable_inst.immediate else {
        return None;
    };
    let closure_name = module.data.strings.get(data.as_raw() as usize)?;
    module
        .closures
        .iter()
        .find(|closure| closure.name == *closure_name)
}

/// Returns true when a Fiber callable operand is already a runtime callable descriptor.
fn callable_operand_is_descriptor_backed(
    function: &Function,
    callable: crate::ir::ValueId,
) -> bool {
    function
        .value(callable)
        .is_some_and(|value| matches!(value.php_type.codegen_repr(), PhpType::Callable))
}

/// Builds a deferred Fiber wrapper description from the concrete EIR closure signature.
fn wrapper_for_closure(closure: &Function) -> FiberWrapper {
    FiberWrapper {
        label: fiber_wrapper_label(&closure.name),
        sig: signature_from_closure(closure),
        visible_param_count: closure.params.len(),
        hidden_arg_types: Vec::new(),
        use_descriptor_invoker: false,
    }
}

/// Builds the shared Fiber wrapper that delegates descriptor callables to their invoker slot.
fn descriptor_invoker_wrapper() -> FiberWrapper {
    FiberWrapper {
        label: "__eir_fiber_descriptor_invoker".to_string(),
        sig: descriptor_invoker_placeholder_sig(),
        visible_param_count: 0,
        hidden_arg_types: Vec::new(),
        use_descriptor_invoker: true,
    }
}

/// Returns an assembly-safe wrapper label derived from the EIR closure symbol.
fn fiber_wrapper_label(closure_name: &str) -> String {
    format!("{}_fiber_wrapper", crate::names::function_symbol(closure_name))
}

/// Builds a placeholder signature for the descriptor-invoker Fiber wrapper.
fn descriptor_invoker_placeholder_sig() -> FunctionSig {
    FunctionSig {
        params: Vec::new(),
        defaults: Vec::new(),
        return_type: PhpType::Mixed,
        declared_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    }
}

/// Reconstructs callable signature metadata from a lowered EIR closure function.
fn signature_from_closure(closure: &Function) -> FunctionSig {
    FunctionSig {
        params: closure
            .params
            .iter()
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        defaults: closure.params.iter().map(|_| None).collect(),
        return_type: closure.return_php_type.clone(),
        declared_return: !matches!(closure.return_php_type, PhpType::Mixed),
        ref_params: closure.params.iter().map(|param| param.by_ref).collect(),
        declared_params: closure
            .params
            .iter()
            .map(|param| !matches!(param.php_type, PhpType::Mixed))
            .collect(),
        variadic: closure
            .params
            .iter()
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
    }
}
