//! Purpose:
//! Literal eval AOT candidate discovery and internal function lowering.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Adds internal EIR functions for literal eval fragments accepted by the EIR AOT subset.
pub(super) fn lower_literal_eval_aot_functions(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    let candidates = collect_literal_eval_aot_function_candidates(module);
    let mut lowered_names = all_lowered_functions(module)
        .map(|function| function.name.clone())
        .collect::<HashSet<_>>();
    for (name, body) in candidates {
        match body {
            EvalAotFunctionCandidate::NoScope { body } => {
                if !lowered_names.insert(name.clone()) {
                    continue;
                }
                function::lower_eval_aot_function(
                    &name,
                    &body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            EvalAotFunctionCandidate::Scope {
                body,
                reads,
                direct_writes,
                flush_writes,
            } => {
                if !lowered_names.insert(name.clone()) {
                    continue;
                }
                function::lower_eval_aot_scope_function(
                    &name,
                    &body,
                    &reads,
                    &direct_writes,
                    &flush_writes,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
        }
    }
}

/// Candidate shape for an internal eval AOT EIR function.
pub(super) enum EvalAotFunctionCandidate {
    NoScope {
        body: Program,
    },
    Scope {
        body: Program,
        reads: BTreeSet<String>,
        direct_writes: BTreeSet<String>,
        flush_writes: BTreeSet<String>,
    },
}

/// Collects unique literal eval fragments that can be emitted as internal EIR functions.
pub(super) fn collect_literal_eval_aot_function_candidates(
    module: &Module,
) -> Vec<(String, EvalAotFunctionCandidate)> {
    let mut candidates = Vec::new();
    let mut seen = HashSet::new();
    for function in all_lowered_functions(module) {
        for inst in &function.instructions {
            let Some(fragment) = eval_literal_fragment_from_inst(module, inst) else {
                continue;
            };
            let mut plan =
                crate::eval_aot::plan_literal_fragment_with_source_path_and_static_and_method_calls(
                    &fragment,
                    module.source_path.as_deref(),
                    eval_literal_strict_profile(inst),
                    |name, args| {
                        eval_literal_static_function_supported_by_module(module, name, args)
                    },
                    |receiver, method, args| {
                        eval_literal_static_method_supported_by_module(
                            module, receiver, method, args,
                        )
                    },
                );
            if let Some(name) = plan.take_function_name() {
                if !seen.insert(name.clone()) {
                    continue;
                }
                let Some(program) = plan.take_eir_program() else {
                    continue;
                };
                candidates.push((name, EvalAotFunctionCandidate::NoScope { body: program }));
                continue;
            }
            let Some(name) = plan.take_scope_function_name() else {
                continue;
            };
            if !seen.insert(name.clone()) {
                continue;
            };
            let reads = plan.reads().clone();
            let direct_writes = plan.direct_writes().clone();
            let flush_writes = plan.flush_writes().clone();
            let Some(program) = plan.take_scope_eir_program() else {
                continue;
            };
            candidates.push((
                name,
                EvalAotFunctionCandidate::Scope {
                    body: program,
                    reads,
                    direct_writes,
                    flush_writes,
                },
            ));
        }
    }
    candidates
}

/// Returns the string payload from an `EvalLiteralCall` instruction.
pub(crate) fn eval_literal_fragment_from_inst(
    module: &Module,
    inst: &crate::ir::Instruction,
) -> Option<String> {
    if inst.op != Op::EvalLiteralCall {
        return None;
    }
    let data = match inst.immediate {
        Some(Immediate::Data(data)) | Some(Immediate::ProfiledData { data, .. }) => data,
        _ => return None,
    };
    module.data.strings.get(data.as_raw() as usize).cloned()
}

/// Returns the strict-PHP source profile attached to one literal eval instruction.
pub(super) fn eval_literal_strict_profile(inst: &crate::ir::Instruction) -> bool {
    matches!(
        inst.immediate,
        Some(Immediate::ProfiledData {
            strict_php: true,
            ..
        })
    )
}

/// Iterates every function-like body already materialized into the EIR module.
pub(in crate::ir_lower) fn all_lowered_functions(module: &Module) -> impl Iterator<Item = &Function> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
}

/// Returns the typed builtin target carried by a runtime-call instruction.
pub(super) fn typed_builtin_target(
    inst: &crate::ir::Instruction,
) -> Option<crate::ir::RuntimeFnId> {
    match inst.immediate {
        Some(Immediate::RuntimeCall(crate::ir::RuntimeCallTarget::Function(target))) => Some(target),
        Some(Immediate::RuntimeCall(
            crate::ir::RuntimeCallTarget::ProfiledFunction { target, .. },
        )) => Some(target),
        _ => None,
    }
}

/// Returns whether a typed builtin callback operand needs descriptor invocation support.
pub(super) fn typed_builtin_requires_descriptor_invoker(
    function: &Function,
    inst: &crate::ir::Instruction,
    target: crate::ir::RuntimeFnId,
) -> bool {
    let Some(callback_index) = target.string_callback_operand_index() else {
        return false;
    };
    let Some(callback) = inst.operands.get(callback_index).copied() else {
        return false;
    };
    function
        .value(callback)
        .is_some_and(|value| value.php_type.codegen_repr() == PhpType::Str)
}

/// Returns whether a compiler-resident call references the optional eval bridge.
pub(super) fn language_construct_call_requires_eval(
    module: &Module,
    inst: &crate::ir::Instruction,
) -> bool {
    let Some(Immediate::Data(data)) = inst.immediate else {
        return false;
    };
    module
        .data
        .function_names
        .get(data.as_raw() as usize)
        .is_some_and(|name| php_symbol_key(name.trim_start_matches('\\')) == "eval")
}

/// Returns whether a function belongs to a stream/archive helper class.
pub(super) fn function_belongs_to_phar_archive_helper_class(function: &Function) -> bool {
    let Some((class_name, _)) = function.name.split_once("::") else {
        return false;
    };
    is_phar_archive_helper_class_name(class_name)
}

/// Returns true when a class has generated methods that can route paths through PHAR helpers.
pub(super) fn is_phar_archive_helper_class_name(name: &str) -> bool {
    matches!(
        crate::names::php_symbol_key(name.trim_start_matches('\\')).as_str(),
        "phar" | "phardata" | "ziparchive" | "splfileobject" | "spltempfileobject"
    )
}

