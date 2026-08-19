//! Purpose:
//! Validates control-flow restrictions that make string-returning cdylib
//! exports recoverable at the native host boundary.
//!
//! Called from:
//! - `crate::pipeline::compile()` after AST-to-EIR lowering for cdylib output.
//!
//! Key details:
//! - Statically reachable functions, methods, constructors, and closures are
//!   traversed transitively across every user-authored EIR body collection.
//! - `exit`/`die`, `eval`, and opaque invocation surfaces are rejected because
//!   they cannot be proven to return through the Stage B status/error boundary.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::errors::CompileError;
use crate::ir::{Function, Immediate, Module, Op};
use crate::names::php_symbol_key;

use super::{is_string_roundtrip_signature, ExportedFunction};

/// Rejects non-recoverable constructs reachable from a string-returning export.
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

    for export in exports
        .values()
        .filter(|export| is_string_roundtrip_signature(&export.sig))
    {
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
                                "string-returning export '{}' reaches {}() through {}; {} cannot return through the cdylib error boundary",
                                export.name,
                                construct,
                                path.join(" -> "),
                                if construct == "eval" { "eval" } else { "exit/die" }
                            ),
                        ));
                    }
                }

                if opaque_dynamic_dispatch(instruction.op) {
                    return Err(CompileError::new(
                        instruction.span.unwrap_or(export.span),
                        &format!(
                            "string-returning export '{}' reaches opaque invocation '{}' through {}; the cdylib boundary cannot prove process termination is unreachable",
                            export.name,
                            instruction.op.name(),
                            path.join(" -> ")
                        ),
                    ));
                }

                match instruction.op {
                    Op::Call | Op::FunctionVariantCall => {
                        let Some(callee) = immediate_function_name(
                            module,
                            instruction.immediate.as_ref(),
                        ) else {
                            continue;
                        };
                        enqueue_user_body(&functions, &mut queue, &path, callee);
                    }
                    Op::ObjectNew => {
                        if let Some(constructor) =
                            fixed_object_constructor(module, instruction.immediate.as_ref())
                        {
                            enqueue_user_body(&functions, &mut queue, &path, &constructor.name);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// Adds one statically resolved user body to the traversal queue when EIR emits it.
fn enqueue_user_body<'a>(
    functions: &HashMap<String, &'a Function>,
    queue: &mut VecDeque<(String, Vec<String>)>,
    path: &[String],
    callee: &str,
) {
    let callee_key = php_symbol_key(callee);
    let Some(function) = functions.get(&callee_key) else {
        return;
    };
    let mut callee_path = path.to_vec();
    callee_path.push(function.name.clone());
    queue.push_back((callee_key, callee_path));
}

/// Resolves a fixed-class allocation to the emitted constructor implementation body.
fn fixed_object_constructor<'a>(
    module: &'a Module,
    immediate: Option<&Immediate>,
) -> Option<&'a Function> {
    let class_name = immediate_class_name(module, immediate)?;
    let class_info = module.class_infos.get(class_name)?;
    let constructor_key = php_symbol_key("__construct");
    class_info.methods.get(&constructor_key)?;
    let implementation_class = class_info
        .method_impl_classes
        .get(&constructor_key)
        .map(String::as_str)
        .unwrap_or(class_name);
    module.class_methods.iter().find(|function| {
        function
            .name
            .rsplit_once("::")
            .is_some_and(|(candidate_class, candidate_method)| {
                php_symbol_key(candidate_class) == php_symbol_key(implementation_class)
                    && php_symbol_key(candidate_method) == constructor_key
            })
    })
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
