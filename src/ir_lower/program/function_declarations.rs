//! Purpose:
//! User function declaration discovery and lowering.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Collects the program-global names reachable from each declared function through direct calls.
pub(crate) fn collect_function_global_names(
    statements: &[Stmt],
) -> HashMap<String, Option<HashSet<String>>> {
    let mut names = HashMap::new();
    collect_function_global_names_into(statements, &mut names);
    names
}

/// Extends direct global declarations through the already-lowered user-function call graph.
pub(crate) fn close_function_global_names_over_calls(
    names: &mut HashMap<String, Option<HashSet<String>>>,
    functions: &[Function],
    function_names: &[String],
) {
    let callees = functions
        .iter()
        .filter_map(|function| {
            let caller = php_symbol_key(function.name.trim_start_matches('\\'));
            names.contains_key(&caller).then(|| {
                let direct = function
                    .instructions
                    .iter()
                    .filter(|instruction| instruction.op == Op::Call)
                    .filter_map(|instruction| match instruction.immediate {
                        Some(Immediate::Data(id)) => function_names.get(id.as_raw() as usize),
                        _ => None,
                    })
                    .map(|callee| php_symbol_key(callee.trim_start_matches('\\')))
                    .collect::<HashSet<_>>();
                let opaque = function.instructions.iter().any(|instruction| {
                    function_instruction_has_opaque_user_code(function, instruction)
                        || (instruction.op != Op::Call
                            && crate::ir_lower::context::runtime_callback_operand_index(
                                instruction.op,
                                instruction.immediate.as_ref(),
                            )
                            .is_some())
                });
                (caller, (direct, opaque))
            })
        })
        .collect::<HashMap<_, _>>();

    for (caller, (_, opaque)) in &callees {
        if *opaque {
            names.insert(caller.clone(), None);
        }
    }

    loop {
        let mut changed = false;
        for (caller, (direct_callees, _)) in &callees {
            let Some(current) = names.get(caller).cloned() else {
                continue;
            };
            let Some(mut reachable) = current else {
                continue;
            };
            for callee in direct_callees {
                match names.get(callee) {
                    Some(Some(callee_names)) => reachable.extend(callee_names.iter().cloned()),
                    Some(None) | None => {
                        if names.insert(caller.clone(), None).is_some() {
                            changed = true;
                        }
                        reachable.clear();
                        break;
                    }
                }
            }
            if names.get(caller).is_some_and(Option::is_none) {
                continue;
            }
            if names.get(caller).is_some_and(|known| known.as_ref() != Some(&reachable)) {
                names.insert(caller.clone(), Some(reachable));
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}

/// Applies the shared opaque-user-code inventory to an already-lowered function instruction.
fn function_instruction_has_opaque_user_code(
    function: &Function,
    instruction: &Instruction,
) -> bool {
    let operand_types = instruction
        .operands
        .iter()
        .map(|operand| {
            function
                .value(*operand)
                .map(|value| value.php_type.clone())
                .unwrap_or(PhpType::Mixed)
        })
        .collect::<Vec<_>>();
    let local_type = match instruction.immediate {
        Some(Immediate::LocalSlot(slot)) => function
            .locals
            .get(slot.as_raw() as usize)
            .map(|local| &local.php_type),
        _ => None,
    };
    crate::ir_lower::context::instruction_has_opaque_user_code_boundary(
        instruction.op,
        instruction.immediate.as_ref(),
        instruction.effects,
        &operand_types,
        local_type,
    )
}

/// Visits the same declaration-bearing statement containers as function lowering.
fn collect_function_global_names_into(
    statements: &[Stmt],
    names: &mut HashMap<String, Option<HashSet<String>>>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } => {
                names.insert(
                    php_symbol_key(name.trim_start_matches('\\')),
                    Some(crate::global_decls::collect_global_var_names(body)),
                );
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. }
            | StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                collect_function_global_names_into(body, names);
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                collect_function_global_names_into(then_body, names);
                for (_, body) in elseif_clauses {
                    collect_function_global_names_into(body, names);
                }
                if let Some(body) = else_body {
                    collect_function_global_names_into(body, names);
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                collect_function_global_names_into(then_body, names);
                if let Some(body) = else_body {
                    collect_function_global_names_into(body, names);
                }
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    collect_function_global_names_into(body, names);
                }
                if let Some(body) = default {
                    collect_function_global_names_into(body, names);
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                collect_function_global_names_into(try_body, names);
                for catch in catches {
                    collect_function_global_names_into(&catch.body, names);
                }
                if let Some(body) = finally_body {
                    collect_function_global_names_into(body, names);
                }
            }
            _ => {}
        }
    }
}

/// Lowers every function declaration reachable in the statement tree.
pub(super) fn lower_function_declarations(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    for stmt in statements {
        match &stmt.kind {
            StmtKind::FunctionDecl {
                by_ref_return: _,
                name,
                params,
                variadic: _,
                variadic_by_ref: _,
                variadic_type: _,
                return_type,
                body,
                ..
            } => function::lower_user_function(
                name,
                params,
                return_type.as_ref(),
                &stmt.attributes,
                body,
                module,
                check_result,
                constants,
                fiber_return_sigs,
            ),
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_function_declarations(
                    body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_function_declarations(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for (_, body) in elseif_clauses {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = else_body {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_function_declarations(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                if let Some(body) = else_body {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_function_declarations(
                    body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = default {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_function_declarations(
                    try_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
                for catch in catches {
                    lower_function_declarations(
                        &catch.body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
                if let Some(body) = finally_body {
                    lower_function_declarations(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                    );
                }
            }
            _ => {}
        }
    }
}
