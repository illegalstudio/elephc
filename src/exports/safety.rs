//! Purpose:
//! Validates control-flow restrictions that make every cdylib export recoverable
//! at the native host boundary.
//!
//! Called from:
//! - `crate::pipeline::compile()` after AST-to-EIR lowering for cdylib output.
//!
//! Key details:
//! - Statically reachable functions, methods, constructors, implicit object hooks,
//!   and closures are traversed across every user-authored EIR body collection.
//! - Fatal terminators, fatal runtime subsets, `exit`/`die`, `eval`, and opaque
//!   invocation surfaces are rejected with a complete static call path.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::codegen::callable_reachability::CallableReachabilityAnalysis;
use crate::errors::CompileError;
use crate::ir::function_variants::{collect_dispatch_groups, resolve_variant_callee_name};
use crate::ir::{
    Function, Immediate, Module, Op, RuntimeCallTarget, RuntimeFnId, Terminator, ValueDef, ValueId,
};
use crate::names::php_symbol_key;
use crate::types::PhpType;

use super::ExportedFunction;

/// User bodies that the backend can invoke without emitting an EIR method-call opcode.
///
/// Construction and destruction are tied to object lifetime. The remaining hooks are
/// dispatched by string conversion, property, ArrayAccess, Countable, JSON, or iterator
/// runtime paths whose EIR instruction does not identify the concrete method body.
const IMPLICIT_OBJECT_METHODS: [&str; 16] = [
    "__construct",
    "__destruct",
    "__toString",
    "__get",
    "offsetGet",
    "offsetSet",
    "offsetExists",
    "offsetUnset",
    "count",
    "jsonSerialize",
    "getIterator",
    "rewind",
    "valid",
    "next",
    "key",
    "current",
];

/// Rejects non-recoverable constructs reachable from any public cdylib export.
pub fn validate_cdylib_call_graph(
    module: &Module,
    exports: &HashMap<String, ExportedFunction>,
) -> Result<(), CompileError> {
    let functions = module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .map(|function| (php_symbol_key(&function.name), function))
        .collect::<HashMap<_, _>>();
    let dispatch_groups = collect_dispatch_groups(module)
        .into_iter()
        .map(|group| (php_symbol_key(&group.name), group.variants))
        .collect::<HashMap<_, _>>();

    for export in exports.values() {
        let root = php_symbol_key(&export.name);
        let mut queue = VecDeque::from([(root.clone(), vec![export.name.clone()])]);
        let mut visited = HashSet::new();
        while let Some((function_key, path)) = queue.pop_front() {
            if !visited.insert(function_key.clone()) {
                continue;
            }
            let Some(function) = functions.get(&function_key) else {
                continue;
            };
            reject_fatal_terminators(module, function, export, &path)?;
            let callable_reachability = CallableReachabilityAnalysis::new(module, function);
            for instruction in &function.instructions {
                if instruction.op == Op::LanguageConstructCall {
                    let Some(name) = immediate_function_name(module, instruction.immediate.as_ref())
                    else {
                        continue;
                    };
                    let construct = php_symbol_key(name.trim_start_matches('\\'));
                    if matches!(construct.as_str(), "exit" | "die" | "eval") {
                        return Err(CompileError::new(
                            instruction.span.unwrap_or(export.span),
                            &format!(
                                "export '{}' reaches {}() through {}; {} cannot return through the cdylib error boundary",
                                export.name,
                                construct,
                                path.join(" -> "),
                                if construct == "eval" { "eval" } else { "exit/die" }
                            ),
                        ));
                    }
                }

                if instruction.op == Op::MethodCall {
                    if enqueue_preallocated_constructor(
                        module,
                        function,
                        instruction,
                        &functions,
                        &mut queue,
                        &path,
                    ) || enqueue_fixed_array_access_method(
                        module,
                        function,
                        instruction,
                        &functions,
                        &mut queue,
                        &path,
                    ) {
                        continue;
                    }
                }

                if opaque_dynamic_dispatch(instruction.op) {
                    return Err(CompileError::new(
                        instruction.span.unwrap_or(export.span),
                        &format!(
                            "export '{}' reaches opaque invocation '{}' through {}; the cdylib boundary cannot prove process termination is unreachable",
                            export.name,
                            instruction.op.name(),
                            path.join(" -> ")
                        ),
                    ));
                }

                match instruction.op {
                    Op::Call | Op::FunctionVariantCall => {
                        enqueue_call_immediate(
                            module,
                            &functions,
                            &dispatch_groups,
                            &mut queue,
                            &path,
                            instruction.immediate.as_ref(),
                        );
                    }
                    Op::ObjectNew => {
                        for method in IMPLICIT_OBJECT_METHODS {
                            if let Some(body) = fixed_object_method(
                                module,
                                instruction.immediate.as_ref(),
                                method,
                            ) {
                                enqueue_user_body(&functions, &mut queue, &path, &body.name);
                            }
                        }
                    }
                    Op::RuntimeCall => validate_runtime_call(
                        module,
                        function,
                        instruction,
                        export,
                        &functions,
                        &dispatch_groups,
                        &callable_reachability,
                        &mut queue,
                        &path,
                    )?,
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Rejects every fatal EIR terminator in a body reachable from an export.
fn reject_fatal_terminators(
    module: &Module,
    function: &Function,
    export: &ExportedFunction,
    path: &[String],
) -> Result<(), CompileError> {
    for block in &function.blocks {
        let Some(Terminator::Fatal { message }) = block.terminator.as_ref() else {
            continue;
        };
        let detail = module
            .data
            .strings
            .get(message.as_raw() as usize)
            .map(|message| message.trim())
            .filter(|message| !message.is_empty())
            .unwrap_or("fatal runtime path");
        return Err(CompileError::new(
            export.span,
            &format!(
                "export '{}' reaches fatal terminator '{}' through {}; a fatal terminator cannot return through the cdylib error boundary",
                export.name,
                detail,
                path.join(" -> ")
            ),
        ));
    }
    Ok(())
}

/// Resolves a direct or include-variant call and queues every possible user body.
fn enqueue_call_immediate<'a>(
    module: &Module,
    functions: &HashMap<String, &'a Function>,
    dispatch_groups: &HashMap<String, Vec<String>>,
    queue: &mut VecDeque<(String, Vec<String>)>,
    path: &[String],
    immediate: Option<&Immediate>,
) {
    match immediate {
        Some(Immediate::Data(_)) | Some(Immediate::ProfiledData { .. }) => {
            if let Some(callee) = immediate_function_name(module, immediate) {
                enqueue_named_target(functions, dispatch_groups, queue, path, callee);
            }
        }
        Some(Immediate::FunctionVariantRef { group, variant }) => {
            if let Some(callee) = resolve_variant_callee_name(module, *group, *variant) {
                enqueue_user_body(functions, queue, path, &callee);
            }
        }
        _ => {}
    }
}

/// Queues a concrete function or every body behind an include-variant dispatcher.
fn enqueue_named_target<'a>(
    functions: &HashMap<String, &'a Function>,
    dispatch_groups: &HashMap<String, Vec<String>>,
    queue: &mut VecDeque<(String, Vec<String>)>,
    path: &[String],
    callee: &str,
) -> bool {
    let key = php_symbol_key(callee);
    if let Some(variants) = dispatch_groups.get(&key) {
        let mut enqueued = false;
        for variant in variants {
            enqueued |= enqueue_user_body(functions, queue, path, variant);
        }
        return enqueued;
    }
    enqueue_user_body(functions, queue, path, callee)
}

/// Validates a typed runtime call and follows any statically known callback bodies.
#[allow(clippy::too_many_arguments)]
fn validate_runtime_call<'a>(
    module: &Module,
    function: &Function,
    instruction: &crate::ir::Instruction,
    export: &ExportedFunction,
    functions: &HashMap<String, &'a Function>,
    dispatch_groups: &HashMap<String, Vec<String>>,
    callable_reachability: &CallableReachabilityAnalysis,
    queue: &mut VecDeque<(String, Vec<String>)>,
    path: &[String],
) -> Result<(), CompileError> {
    let Some(target) = runtime_function_id(instruction.immediate.as_ref()) else {
        return Ok(());
    };
    if let Some(index) = target.string_callback_operand_index() {
        let Some(callback) = instruction.operands.get(index).copied() else {
            return reject_runtime_call(export, instruction, path, target, "has no statically inspectable callback operand");
        };
        let Some(candidates) = callable_reachability.candidates(callback) else {
            return reject_runtime_call(export, instruction, path, target, "uses an opaque runtime callback");
        };
        if candidates.is_empty()
            || candidates.iter().any(|candidate| {
                !enqueue_named_target(functions, dispatch_groups, queue, path, candidate)
            })
        {
            return reject_runtime_call(export, instruction, path, target, "uses a callback whose body is not fully present in EIR");
        }
    }
    if runtime_call_is_proven_safe(module, function, instruction, target) {
        return Ok(());
    }
    if runtime_function_requires_proof(target) {
        return reject_runtime_call(
            export,
            instruction,
            path,
            target,
            "can reach a process-fatal runtime path for these arguments",
        );
    }
    Ok(())
}

/// Returns the typed runtime function id carried by one `RuntimeCall` instruction.
fn runtime_function_id(immediate: Option<&Immediate>) -> Option<RuntimeFnId> {
    match immediate? {
        Immediate::RuntimeCall(RuntimeCallTarget::Function(target))
        | Immediate::RuntimeCall(RuntimeCallTarget::ProfiledFunction { target, .. }) => {
            Some(*target)
        }
        _ => None,
    }
}

/// Reports one runtime-call safety failure with export, builtin, and path context.
fn reject_runtime_call<T>(
    export: &ExportedFunction,
    instruction: &crate::ir::Instruction,
    path: &[String],
    target: RuntimeFnId,
    reason: &str,
) -> Result<T, CompileError> {
    Err(CompileError::new(
        instruction.span.unwrap_or(export.span),
        &format!(
            "export '{}' reaches builtin '{}' through {}; {} and cannot be proven to return through the cdylib error boundary",
            export.name,
            target.as_eir(),
            path.join(" -> "),
            reason
        ),
    ))
}

/// Identifies runtime functions with argument-dependent or lifecycle-driven fatal exits.
fn runtime_function_requires_proof(target: RuntimeFnId) -> bool {
    matches!(
        target,
        RuntimeFnId::Dirname
            | RuntimeFnId::ElephcPtrReadString
            | RuntimeFnId::GetObjectVars
            | RuntimeFnId::ObStart
            | RuntimeFnId::PhpUname
            | RuntimeFnId::Printf
            | RuntimeFnId::PtrReadString
            | RuntimeFnId::Sprintf
            | RuntimeFnId::StrRepeat
            | RuntimeFnId::Unserialize
            | RuntimeFnId::Vprintf
            | RuntimeFnId::Vsprintf
    )
}

/// Proves the safe argument subset for runtime functions that otherwise contain fatal exits.
fn runtime_call_is_proven_safe(
    module: &Module,
    function: &Function,
    instruction: &crate::ir::Instruction,
    target: RuntimeFnId,
) -> bool {
    match target {
        RuntimeFnId::StrRepeat => instruction
            .operands
            .get(1)
            .and_then(|value| const_int(function, *value))
            .is_some_and(|count| count >= 0),
        RuntimeFnId::Dirname => instruction
            .operands
            .get(1)
            .and_then(|value| const_int(function, *value))
            .is_some_and(|levels| levels > 0),
        RuntimeFnId::PhpUname => instruction
            .operands
            .first()
            .and_then(|value| const_str(module, function, *value))
            .is_some_and(|mode| matches!(mode, "a" | "s" | "n" | "r" | "v" | "m")),
        RuntimeFnId::Printf
        | RuntimeFnId::Sprintf
        | RuntimeFnId::Vprintf
        | RuntimeFnId::Vsprintf => instruction
            .operands
            .first()
            .and_then(|value| const_str(module, function, *value))
            .is_some_and(format_has_no_conversions),
        _ => false,
    }
}

/// Resolves a constant integer through ownership-preserving EIR forwarding operations.
fn const_int(function: &Function, value: ValueId) -> Option<i64> {
    let instruction = defining_instruction(function, value)?;
    match instruction.op {
        Op::ConstI64 => match instruction.immediate {
            Some(Immediate::I64(value)) => Some(value),
            _ => None,
        },
        Op::Acquire | Op::Borrow | Op::Move | Op::EnsureOwned => instruction
            .operands
            .first()
            .and_then(|value| const_int(function, *value)),
        _ => None,
    }
}

/// Resolves a constant string through ownership-preserving EIR forwarding operations.
fn const_str<'a>(module: &'a Module, function: &Function, value: ValueId) -> Option<&'a str> {
    let instruction = defining_instruction(function, value)?;
    match instruction.op {
        Op::ConstStr => match instruction.immediate {
            Some(Immediate::Data(data)) => module
                .data
                .strings
                .get(data.as_raw() as usize)
                .map(String::as_str),
            _ => None,
        },
        Op::Acquire | Op::Borrow | Op::Move | Op::EnsureOwned | Op::StrPersist => instruction
            .operands
            .first()
            .and_then(|value| const_str(module, function, *value)),
        _ => None,
    }
}

/// Finds the instruction that defines one SSA value.
fn defining_instruction(
    function: &Function,
    value: ValueId,
) -> Option<&crate::ir::Instruction> {
    let ValueDef::Instruction { inst, .. } = function.value(value)?.def else {
        return None;
    };
    function.instruction(inst)
}

/// Returns true only when a printf-style format contains literals or escaped percent signs.
fn format_has_no_conversions(format: &str) -> bool {
    let mut bytes = format.bytes();
    while let Some(byte) = bytes.next() {
        if byte == b'%' && bytes.next() != Some(b'%') {
            return false;
        }
    }
    true
}

/// Adds one statically resolved user body to the traversal queue when EIR emits it.
fn enqueue_user_body<'a>(
    functions: &HashMap<String, &'a Function>,
    queue: &mut VecDeque<(String, Vec<String>)>,
    path: &[String],
    callee: &str,
) -> bool {
    let callee_key = php_symbol_key(callee);
    let Some(function) = functions.get(&callee_key) else {
        return false;
    };
    let mut callee_path = path.to_vec();
    callee_path.push(function.name.clone());
    queue.push_back((callee_key, callee_path));
    true
}

/// Resolves the constructor call paired with a statically known preallocated receiver.
///
/// Receiver preallocation deliberately splits one source-level construction into
/// `ObjectNewWithoutConstructor` followed by `MethodCall("__construct")`. Only that
/// exact producer/consumer pair is devirtualized here; unrelated method calls remain
/// opaque and are rejected by the caller.
fn enqueue_preallocated_constructor<'a>(
    module: &'a Module,
    function: &Function,
    instruction: &crate::ir::Instruction,
    functions: &HashMap<String, &'a Function>,
    queue: &mut VecDeque<(String, Vec<String>)>,
    path: &[String],
) -> bool {
    let Some(method) = immediate_string(module, instruction.immediate.as_ref()) else {
        return false;
    };
    if php_symbol_key(method) != "__construct" {
        return false;
    }
    let Some(receiver) = instruction.operands.first().copied() else {
        return false;
    };
    let Some(allocation) = defining_instruction(function, receiver) else {
        return false;
    };
    if allocation.op != Op::ObjectNewWithoutConstructor {
        return false;
    }
    let Some(class_name) = immediate_class_name(module, allocation.immediate.as_ref()) else {
        return false;
    };
    let Some(PhpType::Object(receiver_class)) = function
        .value(receiver)
        .map(|value| value.php_type.codegen_repr())
    else {
        return false;
    };
    if php_symbol_key(&receiver_class) != php_symbol_key(class_name) {
        return false;
    }
    let Some(body) = fixed_class_method(module, class_name, method) else {
        return false;
    };
    enqueue_user_body(functions, queue, path, &body.name)
}

/// Resolves the synthetic ArrayAccess existence/removal calls that lowering emits as
/// `MethodCall`, while leaving every other virtual method call on the opaque-reject path.
fn enqueue_fixed_array_access_method<'a>(
    module: &'a Module,
    function: &Function,
    instruction: &crate::ir::Instruction,
    functions: &HashMap<String, &'a Function>,
    queue: &mut VecDeque<(String, Vec<String>)>,
    path: &[String],
) -> bool {
    let Some(method) = immediate_string(module, instruction.immediate.as_ref()) else {
        return false;
    };
    if !matches!(php_symbol_key(method).as_str(), "offsetexists" | "offsetunset") {
        return false;
    }
    let Some(receiver) = instruction.operands.first().copied() else {
        return false;
    };
    let Some(PhpType::Object(class_name)) = function
        .value(receiver)
        .map(|value| value.php_type.codegen_repr())
    else {
        return false;
    };
    let Some(body) = fixed_class_method(module, &class_name, method) else {
        return false;
    };
    enqueue_user_body(functions, queue, path, &body.name)
}

/// Resolves a fixed-class allocation to one emitted lifecycle method implementation body.
fn fixed_object_method<'a>(
    module: &'a Module,
    immediate: Option<&Immediate>,
    method: &str,
) -> Option<&'a Function> {
    let class_name = immediate_class_name(module, immediate)?;
    fixed_class_method(module, class_name, method)
}

/// Resolves one method implementation inherited or declared by a statically known class.
fn fixed_class_method<'a>(
    module: &'a Module,
    class_name: &str,
    method: &str,
) -> Option<&'a Function> {
    let class_info = module.class_infos.get(class_name)?;
    let method_key = php_symbol_key(method);
    class_info.methods.get(&method_key)?;
    let implementation_class = class_info
        .method_impl_classes
        .get(&method_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    module.class_methods.iter().find(|function| {
        function
            .name
            .rsplit_once("::")
            .is_some_and(|(candidate_class, candidate_method)| {
                php_symbol_key(candidate_class) == php_symbol_key(implementation_class)
                    && php_symbol_key(candidate_method) == method_key
            })
    })
}

/// Resolves a string-pool reference carried by a method-call instruction.
fn immediate_string<'a>(
    module: &'a Module,
    immediate: Option<&Immediate>,
) -> Option<&'a str> {
    let Immediate::Data(data) = immediate? else {
        return None;
    };
    module
        .data
        .strings
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Resolves a function-name data-pool reference carried by one EIR instruction.
fn immediate_function_name<'a>(
    module: &'a Module,
    immediate: Option<&Immediate>,
) -> Option<&'a str> {
    let data = match immediate? {
        Immediate::Data(data) | Immediate::ProfiledData { data, .. } => *data,
        _ => return None,
    };
    module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Resolves a class-name data-pool reference carried by one fixed `ObjectNew`.
fn immediate_class_name<'a>(
    module: &'a Module,
    immediate: Option<&Immediate>,
) -> Option<&'a str> {
    let Immediate::Data(data) = immediate? else {
        return None;
    };
    module
        .data
        .class_names
        .get(data.as_raw() as usize)
        .map(String::as_str)
}

/// Returns whether an invocation surface has a call graph opaque to AOT validation.
fn opaque_dynamic_dispatch(op: Op) -> bool {
    matches!(
        op,
        Op::DynamicObjectNew
            | Op::DynamicObjectNewMixed
            | Op::DynamicObjectNewWithoutConstructorMixed
            | Op::EvalObjectNew
            | Op::ExprCall
            | Op::ClosureCall
            | Op::CallableDescriptorInvoke
            | Op::MethodCall
            | Op::NullsafeMethodCall
            | Op::StaticMethodCall
            | Op::EvalStaticMethodCall
            | Op::EvalLiteralCall
            | Op::EvalFunctionCall
            | Op::EvalFunctionCallArray
            | Op::IteratorMethodCall
            | Op::SplRuntimeCall
            | Op::DynamicPdoStatementConstructorCall
            | Op::PipeCall
            | Op::FiberRuntimeCall
            | Op::ExternCall
    )
}
