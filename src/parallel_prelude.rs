//! Purpose:
//! Injects Elephc's structured isolated-thread API as compiler-owned AST declarations.
//!
//! Called from:
//! - `crate::pipeline::compile()`, after the Async cancellation surface and before name resolution.
//!
//! Key details:
//! - The PHP source is retained only as a reviewable parse-parity oracle for generated builders.
//! - `run()` owns the structured scope; `TaskGroup::spawn()` is the only public task creation API.
//! - `TaskGroup::spawn()` reaches the dedicated Parallel EIR operation, while the generated PHP
//!   scope and Future methods own structured draining, cancellation, and failure reconstruction.
//! - No synchronous fallback may hide a missing Parallel lowering.

use crate::parser::ast::{
    BinOp, CType, ClassMethod, ClassProperty, Program, PropertyHooks, Stmt, StmtKind, TypeExpr,
    Visibility,
};
use crate::synthetic_class::*;

mod build;

#[cfg(test)]
const SOURCE: &str = include_str!("parallel_prelude/source.php");

use build::parallel_declarations;

/// Prepends the Parallel surface when a program references its root `run()` boundary,
/// including qualified callable strings, function/namespace import aliases, and
/// namespace-relative calls within the Parallel namespace.
pub fn inject_if_used(
    mut program: Program,
    allow_extensions: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !allow_extensions
        || !crate::opcache_prelude::detect::program_references_function_or_import_alias(
            &program,
            "Elephc\\Parallel\\run",
        )
    {
        return program;
    }
    patch_async_cancellation_for_parallel(&mut program);
    patch_async_run_for_parallel_worker(&mut program);
    let mut combined = parallel_declarations();
    inventory.record_program("parallel", &combined);
    combined.extend(program);
    combined
}

/// Extends Async's private cancellation state only in programs that actually use Parallel.
/// The public `Cancellation` type remains unchanged, while a worker-local instance can observe
/// the native job's cooperative cancellation flag through the Parallel bridge.
fn patch_async_cancellation_for_parallel(program: &mut Program) {
    let Some((properties, methods)) = program.iter_mut().find_map(|stmt| {
        let StmtKind::NamespaceBlock { name, body } = &mut stmt.kind else {
            return None;
        };
        if name.as_ref().map(|name| name.as_str()) != Some("Elephc\\Async") {
            return None;
        }
        body.iter_mut().find_map(|stmt| {
            let StmtKind::ClassDecl {
                name,
                properties,
                methods,
                ..
            } = &mut stmt.kind
            else {
                return None;
            };
            (name == "__CancellationState").then_some((properties, methods))
        })
    }) else {
        return;
    };

    properties.push(ClassProperty {
        name: "parallelJobId".to_string(),
        visibility: Visibility::Private,
        set_visibility: None,
        type_expr: Some(TypeExpr::Int),
        hooks: PropertyHooks::none(),
        readonly: false,
        is_final: false,
        is_static: false,
        is_abstract: false,
        by_ref: false,
        is_promoted: false,
        default: Some(e_int(0)),
        span: crate::span::Span::dummy(),
        attributes: Vec::new(),
    });
    let had_constructor = methods.iter().any(|method| method.name == "__construct");
    if !had_constructor {
        methods.push(ClassMethod {
            name: "__construct".to_string(),
            type_params: Vec::new(),
            visibility: Visibility::Public,
            is_static: false,
            is_abstract: false,
            is_final: false,
            has_body: true,
            params: vec![(
                "parallelJobId".to_string(),
                Some(TypeExpr::Int),
                Some(e_int(0)),
                false,
            )],
            param_attributes: vec![Vec::new()],
            variadic: None,
            variadic_by_ref: false,
            variadic_type: None,
            return_type: Some(TypeExpr::Void),
            by_ref_return: false,
            body: vec![s_prop_assign(
                e_this(),
                "parallelJobId",
                e_var("parallelJobId"),
            )],
            span: crate::span::Span::dummy(),
            attributes: Vec::new(),
        });
    }
    for method in methods {
        match method.name.as_str() {
            "__construct" if had_constructor => {
                method.params.push((
                    "parallelJobId".to_string(),
                    Some(TypeExpr::Int),
                    Some(e_int(0)),
                    false,
                ));
                method.param_attributes.push(Vec::new());
                method.body.push(s_prop_assign(
                    e_this(),
                    "parallelJobId",
                    e_var("parallelJobId"),
                ));
            }
            "isRequested" => {
                let native_requested = e_binop(
                    e_call(
                        "\\elephc_parallel_job_cancellation_requested",
                        vec![e_this_prop("parallelJobId")],
                    ),
                    BinOp::StrictEq,
                    e_int(1),
                );
                let has_native_job = e_binop(
                    e_this_prop("parallelJobId"),
                    BinOp::StrictNotEq,
                    e_int(0),
                );
                method.body = vec![s_return(e_binop(
                    e_this_prop("requested"),
                    BinOp::Or,
                    e_binop(has_native_job, BinOp::And, native_requested),
                ))];
            }
            _ => {}
        }
    }
}

/// Adds the worker-domain half of the Async root-scope boundary when Parallel is linked.
///
/// The guard lives in this conditional patch rather than the base Async prelude because the
/// `elephc_parallel_worker_active` extern belongs to the Parallel bridge. A program that uses only
/// Async must not acquire that bridge or an unresolved native symbol. Any Parallel program has the
/// extern declaration below, including one whose worker reaches `Async::run()` through a runtime
/// callable dispatch that the static transfer analysis cannot resolve.
fn patch_async_run_for_parallel_worker(program: &mut Program) {
    let Some(body) = program.iter_mut().find_map(|stmt| {
        let StmtKind::NamespaceBlock { name, body } = &mut stmt.kind else {
            return None;
        };
        if name.as_ref().map(|name| name.as_str()) != Some("Elephc\\Async") {
            return None;
        }
        body.iter_mut().find_map(|stmt| match &mut stmt.kind {
            StmtKind::FunctionDecl { name, body, .. } if name == "run" => Some(body),
            _ => None,
        })
    }) else {
        return;
    };

    let worker_active = e_binop(
        e_call("\\elephc_parallel_worker_active", vec![]),
        BinOp::StrictEq,
        e_int(1),
    );
    body.insert(
        0,
        s_if(
            worker_active,
            vec![s_throw(e_new(
                "\\Error",
                vec![e_str(
                    "Elephc\\Async\\run() cannot be called from a Parallel worker in v1",
                )],
            ))],
            Vec::new(),
            None,
        ),
    );
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Parse-parity and public-surface tests for the generated Parallel prelude.
    //!
    //! Called from:
    //! - `cargo test --lib parallel_prelude`.

    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses user injection fixtures without compiler-internal declaration privileges.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Parses the reviewable Parallel PHP oracle with internal declarations enabled.
    fn parsed_declarations() -> Program {
        let tokens = crate::lexer::tokenize(SOURCE).expect("Parallel prelude must tokenize");
        crate::parser::parse_internal(&tokens).expect("Parallel prelude must parse")
    }

    /// Erases source positions before comparing the generated and oracle ASTs.
    fn strip_spans(rendered: &str) -> String {
        let mut cleaned = String::with_capacity(rendered.len());
        let mut rest = rendered;
        while let Some(at) = rest.find("Span {") {
            cleaned.push_str(&rest[..at]);
            cleaned.push_str("Span");
            let after = &rest[at..];
            let close = after.find('}').map(|end| end + 1).unwrap_or(after.len());
            rest = &after[close..];
        }
        cleaned.push_str(rest);
        cleaned
    }

    /// Pins generated Parallel declarations to the PHP oracle with local mismatch diagnostics.
    #[test]
    fn built_declarations_match_the_php_oracle() {
        let built = parallel_declarations();
        let parsed = parsed_declarations();
        assert_eq!(built.len(), parsed.len());
        for (built_stmt, parsed_stmt) in built.iter().zip(parsed.iter()) {
            let built_debug = strip_spans(&format!("{built_stmt:?}"));
            let parsed_debug = strip_spans(&format!("{parsed_stmt:?}"));
            if built_debug != parsed_debug {
                let mismatch = built_debug
                    .chars()
                    .zip(parsed_debug.chars())
                    .position(|(built, parsed)| built != parsed)
                    .unwrap_or_else(|| built_debug.chars().count().min(parsed_debug.chars().count()));
                let start = mismatch.saturating_sub(500);
                let built_context = built_debug.chars().skip(start).take(1000).collect::<String>();
                let parsed_context = parsed_debug.chars().skip(start).take(1000).collect::<String>();
                panic!(
                    "Parallel prelude AST diverges near character {mismatch}:\nbuilt:  {built_context}\nsource: {parsed_context}"
                );
            }
        }
    }

    /// Pins the public enum/class/function inventory and builtin-contract catalog parity.
    #[test]
    fn declarations_expose_the_locked_v1_surface() {
        let declarations = parallel_declarations();
        let namespace = declarations
            .iter()
            .find(|stmt| matches!(stmt.kind, StmtKind::NamespaceBlock { .. }))
            .expect("expected the Parallel namespace block");
        let StmtKind::NamespaceBlock { name, body } = &namespace.kind else {
            panic!("expected the Parallel namespace block");
        };
        assert_eq!(
            name.as_ref().map(|name| name.to_string()),
            Some("Elephc\\Parallel".to_string())
        );
        let enums = body
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::EnumDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let classes = body
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let functions = body
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } if !name.starts_with("__") => {
                    Some(name.as_str())
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(enums, vec!["TaskFailureKind"]);
        assert_eq!(
            classes,
            vec!["TaskFailure", "TaskGroupFailure", "Future", "TaskGroup"]
        );
        assert_eq!(functions, vec!["run"]);

        let declared: std::collections::BTreeSet<String> = enums
            .iter()
            .chain(classes.iter())
            .map(|name| format!("Elephc\\Parallel\\{name}"))
            .collect();
        let catalogued: std::collections::BTreeSet<String> = elephc_builtin_contract::classes()
            .iter()
            .filter(|contract| contract.name.starts_with("Elephc\\Parallel\\"))
            .map(|contract| contract.name.to_string())
            .collect();
        assert_eq!(declared, catalogued, "Parallel prelude class catalog drift");
    }

    /// Requires closed-scope and cancellation-admission guards on public TaskGroup operations.
    #[test]
    fn task_group_tracks_and_rejects_post_scope_use() {
        let declarations = parallel_declarations();
        let task_group = declarations
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::NamespaceBlock { body, .. } => body.iter().find_map(|stmt| match &stmt.kind {
                    StmtKind::ClassDecl {
                        name,
                        properties,
                        methods,
                        ..
                    } if name == "TaskGroup" => Some((properties, methods)),
                    _ => None,
                }),
                _ => None,
            })
            .expect("expected the Parallel TaskGroup declaration");
        assert!(task_group.0.iter().any(|property| property.name == "closed"));
        let assert_open = task_group
            .1
            .iter()
            .find(|method| method.name == "__assertOpen")
            .expect("TaskGroup must own its scope-lifetime guard");
        assert!(format!("{:?}", assert_open.body).contains("scope has closed"));
        let assert_spawnable = task_group
            .1
            .iter()
            .find(|method| method.name == "__assertSpawnable")
            .expect("TaskGroup must own its cancellation-admission guard");
        let rendered_spawnable = format!("{:?}", assert_spawnable.body);
        assert!(rendered_spawnable.contains("__assertOpen"));
        assert!(rendered_spawnable.contains("isRequested"));
        for public_method in ["cancellation", "cancel"] {
            let method = task_group
                .1
                .iter()
                .find(|method| method.name == public_method)
                .expect("expected public TaskGroup method");
            assert!(format!("{:?}", method.body).contains("__assertOpen"));
        }
        let spawn = task_group
            .1
            .iter()
            .find(|method| method.name == "spawn")
            .expect("expected public spawn method");
        assert!(format!("{:?}", spawn.body).contains("__assertSpawnable"));
    }

    /// Leaves Parallel root fixtures unchanged when extension injection is disabled.
    #[test]
    fn strict_php_does_not_inject_the_extension_surface() {
        let program = parse("<?php \\Elephc\\Parallel\\run(static fn ($tasks): int => 1);");
        let original_len = program.len();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(program, false, &mut inventory);
        assert_eq!(injected.len(), original_len);
    }

    /// Requires an aliased Parallel root to inject both Parallel and shared Async cancellation.
    #[test]
    fn aliased_parallel_run_import_injects_both_scheduler_surfaces() {
        let program = parse(
            r#"<?php
use function Elephc\Parallel\run as parallelRun;
parallelRun(static fn ($tasks): int => 1);
"#,
        );
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = crate::async_prelude::inject_if_used(program, true, &mut inventory);
        let injected = inject_if_used(program, true, &mut inventory);
        let namespaces: std::collections::BTreeSet<String> = injected
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::NamespaceBlock { name: Some(name), .. } => Some(name.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert!(namespaces.contains("Elephc\\Parallel"));
        assert!(namespaces.contains("Elephc\\Async"));
    }

    /// Prevents Async-only roots from acquiring Parallel declarations or bridge dependencies.
    #[test]
    fn async_run_import_does_not_inject_the_parallel_bridge() {
        let program = parse(
            r#"<?php
use function Elephc\Async\run;
run(static fn (): int => 1);
"#,
        );
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = crate::async_prelude::inject_if_used(program, true, &mut inventory);
        let injected = inject_if_used(program, true, &mut inventory);
        let namespaces: std::collections::BTreeSet<String> = injected
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::NamespaceBlock { name: Some(name), .. } => Some(name.as_str().to_string()),
                _ => None,
            })
            .collect();
        assert!(namespaces.contains("Elephc\\Async"));
        assert!(!namespaces.contains("Elephc\\Parallel"));
    }

    /// Checks the worker job identifier and native cancellation probe added to private Async state.
    #[test]
    fn parallel_injection_adds_worker_cancellation_probe_only_to_async_state() {
        let program = parse("<?php \\Elephc\\Parallel\\run(static fn ($tasks): int => 1);");
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = crate::async_prelude::inject_if_used(program, true, &mut inventory);
        let injected = inject_if_used(program, true, &mut inventory);
        let state = injected
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::NamespaceBlock { name, body }
                    if name.as_ref().map(|name| name.as_str()) == Some("Elephc\\Async") =>
                {
                    body.iter().find_map(|stmt| match &stmt.kind {
                        StmtKind::ClassDecl {
                            name,
                            properties,
                            methods,
                            ..
                        } if name == "__CancellationState" => Some((properties, methods)),
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("Async cancellation state must remain available");
        assert!(state.0.iter().any(|property| property.name == "parallelJobId"));
        let constructor = state
            .1
            .iter()
            .find(|method| method.name == "__construct")
            .expect("Parallel must add the worker-local constructor");
        assert_eq!(constructor.params.len(), 1);
        let is_requested = state
            .1
            .iter()
            .find(|method| method.name == "isRequested")
            .expect("Cancellation state must expose observation");
        assert!(format!("{:?}", is_requested.body)
            .contains("elephc_parallel_job_cancellation_requested"));
    }

    /// Requires the Parallel-worker refusal guard at the start of the Async root body.
    #[test]
    fn parallel_injection_adds_a_worker_guard_to_async_run() {
        let program = parse("<?php \\Elephc\\Parallel\\run(static fn ($tasks): int => 1);");
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let program = crate::async_prelude::inject_if_used(program, true, &mut inventory);
        let injected = inject_if_used(program, true, &mut inventory);
        let run_body = injected
            .iter()
            .find_map(|stmt| match &stmt.kind {
                StmtKind::NamespaceBlock { name, body }
                    if name.as_ref().map(|name| name.as_str()) == Some("Elephc\\Async") =>
                {
                    body.iter().find_map(|stmt| match &stmt.kind {
                        StmtKind::FunctionDecl { name, body, .. } if name == "run" => Some(body),
                        _ => None,
                    })
                }
                _ => None,
            })
            .expect("Parallel injection must retain Async::run()");
        let guard = run_body
            .first()
            .expect("Parallel injection must prepend the worker guard");
        let rendered = format!("{guard:?}");
        assert!(rendered.contains("elephc_parallel_worker_active"));
        assert!(rendered.contains("cannot be called from a Parallel worker"));
    }
}
