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
//! - Closure captures remain descriptor-owned and are passed as hidden wrapper
//!   arguments loaded from runtime descriptor capture slots.

use crate::ir::{Function, FunctionParam, Immediate, Instruction, Module, Op, ValueDef, ValueId};
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

/// Closure body plus capture count recovered from a `closure_new` Fiber operand.
struct ClosureFiberTarget<'a> {
    closure: &'a Function,
    capture_count: usize,
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
    if let Some(target) = closure_literal_operand(module, function, callable) {
        return wrapper_for_closure(target.closure, target.capture_count);
    }
    if callable_operand_uses_descriptor_invoker(module, function, callable) {
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
    callable: ValueId,
) -> Option<ClosureFiberTarget<'a>> {
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
    let closure = module
        .closures
        .iter()
        .find(|closure| closure.name == *closure_name)?;
    Some(ClosureFiberTarget {
        closure,
        capture_count: callable_inst.operands.len(),
    })
}

/// Returns true when a Fiber callable operand runs through the descriptor-invoker wrapper.
fn callable_operand_uses_descriptor_invoker(
    module: &Module,
    function: &Function,
    callable: ValueId,
) -> bool {
    function
        .value(callable)
        .is_some_and(|value| match value.php_type.codegen_repr() {
            PhpType::Callable | PhpType::Str => true,
            PhpType::Array(elem) => matches!(elem.codegen_repr(), PhpType::Mixed | PhpType::Str),
            PhpType::Object(class_name) => module
                .class_infos
                .get(class_name.trim_start_matches('\\'))
                .is_some_and(|class_info| class_info.methods.contains_key("__invoke")),
            _ => false,
        })
}

/// Builds a deferred Fiber wrapper description from the concrete EIR closure signature.
fn wrapper_for_closure(closure: &Function, capture_count: usize) -> Option<FiberWrapper> {
    if capture_count > closure.params.len() {
        return None;
    }
    let visible_abi_param_count = closure.params.len() - capture_count;
    let sig = signature_from_closure(closure, visible_abi_param_count);
    let visible_param_count = sig.params.len();
    Some(FiberWrapper {
        label: fiber_wrapper_label(&closure.name),
        sig,
        visible_param_count,
        hidden_arg_types: hidden_capture_arg_types_from_closure(closure, capture_count),
        use_descriptor_invoker: false,
    })
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
        by_ref_return: false,
        ref_params: Vec::new(),
        declared_params: Vec::new(),
        variadic: None,
        deprecation: None,
    }
}

/// Reconstructs caller-visible signature metadata from a lowered EIR closure function.
fn signature_from_closure(closure: &Function, visible_abi_param_count: usize) -> FunctionSig {
    if let Some(signature) = &closure.signature {
        let mut signature = signature.clone();
        let original_param_count = signature.params.len();
        ensure_variadic_param_slot(&mut signature);
        if original_param_count == visible_abi_param_count {
            return signature;
        }
    }

    FunctionSig {
        params: closure
            .params
            .iter()
            .take(visible_abi_param_count)
            .map(|param| (param.name.clone(), param.php_type.clone()))
            .collect(),
        defaults: closure
            .params
            .iter()
            .take(visible_abi_param_count)
            .map(|_| None)
            .collect(),
        return_type: closure.return_php_type.clone(),
        declared_return: !matches!(closure.return_php_type, PhpType::Mixed),
        by_ref_return: false,
        ref_params: closure
            .params
            .iter()
            .take(visible_abi_param_count)
            .map(|param| param.by_ref)
            .collect(),
        declared_params: closure
            .params
            .iter()
            .take(visible_abi_param_count)
            .map(|param| !matches!(param.php_type, PhpType::Mixed))
            .collect(),
        variadic: closure
            .params
            .iter()
            .take(visible_abi_param_count)
            .find(|param| param.variadic)
            .map(|param| param.name.clone()),
        deprecation: None,
    }
}

/// Adds the virtual variadic array slot when the EIR ABI stores it outside `params`.
fn ensure_variadic_param_slot(signature: &mut FunctionSig) {
    let Some(variadic) = signature.variadic.clone() else {
        return;
    };
    if signature.params.iter().any(|(name, _)| name == &variadic) {
        return;
    }
    signature
        .params
        .push((variadic, PhpType::Array(Box::new(PhpType::Mixed))));
    signature.defaults.push(None);
    signature.ref_params.push(false);
    signature.declared_params.push(false);
}

/// Returns hidden argument types for closure captures stored in descriptor capture slots.
fn hidden_capture_arg_types_from_closure(
    closure: &Function,
    capture_count: usize,
) -> Vec<PhpType> {
    closure
        .params
        .iter()
        .rev()
        .take(capture_count)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .map(hidden_capture_arg_type)
        .collect()
}

/// Maps by-reference captures to pointer-sized hidden arguments.
fn hidden_capture_arg_type(param: &FunctionParam) -> PhpType {
    if param.by_ref {
        PhpType::Int
    } else {
        param.php_type.clone()
    }
}
