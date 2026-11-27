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

/// Identifies a physical declaration, independently of how many include guards contain it.
type DeclarationOrigin = (String, String, u32, u32, bool);

/// Returns physical source identity without merging synthetic or origin-free declarations.
fn declaration_origin(stmt: &Stmt, name: &str, module: &Module) -> Option<DeclarationOrigin> {
    let span = stmt.span;
    if span.line == 0 || span.col == 0 || span.line >= crate::span::SYNTHETIC_LINE_BASE {
        return None;
    }
    let path = module.source_paths.get(&span.source_id())?;
    Some((php_symbol_key(name.trim_start_matches('\\')), path.clone(), span.line, span.col,
        stmt.source_mode == crate::source::SourceMode::Internal))
}

/// Lowers each physical function body once while retaining every runtime include guard.
pub(super) fn lower_function_declarations(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::types::GlobalConstants,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    lower_function_declarations_inner(statements, module, check_result, constants,
        fiber_return_sigs, &mut HashSet::new());
}

/// Lowers every function declaration reachable in the statement tree.
fn lower_function_declarations_inner(
    statements: &[Stmt],
    module: &mut Module,
    check_result: &CheckResult,
    constants: &crate::types::GlobalConstants,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
    seen: &mut HashSet<DeclarationOrigin>,
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
            } => {
                if declaration_origin(stmt, name, module).is_some_and(|origin| !seen.insert(origin)) {
                    continue;
                }
                function::lower_user_function(
                    name,
                    stmt.source_mode == crate::source::SourceMode::Internal,
                    params,
                    return_type.as_ref(),
                    &stmt.attributes,
                    body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                );
            }
            StmtKind::NamespaceBlock { body, .. }
            | StmtKind::Synthetic(body)
            | StmtKind::IncludeOnceGuard { body, .. } => {
                lower_function_declarations_inner(
                    body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                    seen,
                );
            }
            StmtKind::If {
                then_body,
                elseif_clauses,
                else_body,
                ..
            } => {
                lower_function_declarations_inner(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                    seen,
                );
                for (_, body) in elseif_clauses {
                    lower_function_declarations_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                        seen,
                    );
                }
                if let Some(body) = else_body {
                    lower_function_declarations_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                        seen,
                    );
                }
            }
            StmtKind::IfDef {
                then_body,
                else_body,
                ..
            } => {
                lower_function_declarations_inner(
                    then_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                    seen,
                );
                if let Some(body) = else_body {
                    lower_function_declarations_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                        seen,
                    );
                }
            }
            StmtKind::While { body, .. }
            | StmtKind::DoWhile { body, .. }
            | StmtKind::For { body, .. }
            | StmtKind::Foreach { body, .. } => {
                lower_function_declarations_inner(
                    body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                    seen,
                );
            }
            StmtKind::Switch { cases, default, .. } => {
                for (_, body) in cases {
                    lower_function_declarations_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                        seen,
                    );
                }
                if let Some(body) = default {
                    lower_function_declarations_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                        seen,
                    );
                }
            }
            StmtKind::Try {
                try_body,
                catches,
                finally_body,
            } => {
                lower_function_declarations_inner(
                    try_body,
                    module,
                    check_result,
                    constants,
                    fiber_return_sigs,
                    seen,
                );
                for catch in catches {
                    lower_function_declarations_inner(
                        &catch.body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                        seen,
                    );
                }
                if let Some(body) = finally_body {
                    lower_function_declarations_inner(
                        body,
                        module,
                        check_result,
                        constants,
                        fiber_return_sigs,
                        seen,
                    );
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::codegen::platform::Target;
    use crate::span::Span;

    /// Builds one checked body while letting tests vary its include-site source identities.
    fn declaration(target: Target) -> (Stmt, CheckResult) {
        let tokens = crate::lexer::tokenize("<?php function physical_once(mixed $value): mixed { return $value; }").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let checked = crate::types::check_with_target(&program, target).unwrap();
        (program[0].clone(), checked)
    }

    /// Equal physical declarations share one body even when parsed under two source IDs.
    #[test]
    fn conditional_once_physical_declarations_are_lowered_once_on_every_target() {
        for name in ["linux-x86_64", "linux-aarch64", "macos-aarch64", "ios-arm64", "ios-sim-arm64"] {
            let target = Target::parse(name).unwrap();
            let (first, checked) = declaration(target);
            let mut second = first.clone();
            second.span = Span::new_in_source(first.span.line, first.span.col, 17);
            let mut module = Module::new(target);
            module.source_paths.insert(first.span.source_id(), "/fixture/library.php".into());
            module.source_paths.insert(17, "/fixture/library.php".into());
            let wrapped = Stmt::new(StmtKind::Synthetic(vec![second]), first.span);
            lower_function_declarations(&[first, wrapped], &mut module, &checked,
                &HashMap::new(), &HashMap::new());
            assert_eq!(module.functions.len(), 1, "{name}: one physical body");
            crate::ir::validate_function(&module.functions[0]).unwrap();
        }
    }

    /// Distinct physical locations and files are retained for the ordinary redeclaration checks.
    #[test]
    fn conditional_once_dedup_does_not_merge_distinct_declarations() {
        let target = Target::parse("linux-x86_64").unwrap();
        for same_file in [false, true] {
            let (first, checked) = declaration(target);
            let mut second = first.clone();
            second.span = Span::new_in_source(if same_file { first.span.line + 1 } else { first.span.line },
                first.span.col, 17);
            let mut module = Module::new(target);
            module.source_paths.insert(first.span.source_id(), "/fixture/first.php".into());
            module.source_paths.insert(17, if same_file { "/fixture/first.php" } else { "/fixture/second.php" }.into());
            lower_function_declarations(&[first, second], &mut module, &checked,
                &HashMap::new(), &HashMap::new());
            assert_eq!(module.functions.len(), 2, "distinct origins must not be silently discarded");
        }
    }

    /// Origin-free and synthetic nodes never acquire a guessed physical declaration identity.
    #[test]
    fn conditional_once_dedup_keeps_synthetic_and_unknown_origins() {
        let target = Target::parse("linux-x86_64").unwrap();
        for synthetic in [false, true] {
            let (mut first, checked) = declaration(target);
            let mut module = Module::new(target);
            if synthetic {
                first.span = Span::new(crate::span::SYNTHETIC_LINE_BASE, 1);
                module.source_paths.insert(0, "/fixture/synthetic.php".into());
            }
            lower_function_declarations(&[first.clone(), first], &mut module, &checked,
                &HashMap::new(), &HashMap::new());
            assert_eq!(module.functions.len(), 2);
        }
    }
}
