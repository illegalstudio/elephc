//! Purpose:
//! Injects Elephc's structured cooperative scheduler surface, implemented in
//! elephc-PHP on top of the existing native Fiber runtime.
//!
//! Called from:
//! - `crate::pipeline::compile()`, before name resolution and type checking.
//!
//! Key details:
//! - `run()` owns the root scope and `TaskGroup::spawn()` is the only public v1
//!   task-creation operation, so every child is joined structurally.
//! - `spawn()` starts a zero-argument runner that forwards unbounded positional
//!   and named task arguments through ordinary callable dispatch.
//! - The kernel schedules FIFO readiness, monotonic timers, descriptor waits, task
//!   waits, and cooperative cancellation points.
//! - Escaping failures request sibling cancellation; scope exit propagates the
//!   first task failure not observed through `Awaitable::await()`.
//! - PHP values and Fiber objects stay inside one `_rt_ctx`; no Rust bridge or
//!   cross-arena pointer is introduced by the Async executor.

use crate::parser::ast::{BinOp, CastType, Program, TypeExpr};
use crate::synthetic_class::*;

/// Builds the private probe for an actively executing Async task.
fn method_is_async_task_active() -> MethodBuilder {
    method("__isAsyncTaskActive")
        .static_()
        .returns(TypeExpr::Bool)
        .body(vec![s_return(e_self_static_prop("asyncTaskActive"))])
}

/// Builds the guard rejecting bare Fiber suspension while Async owns task wakeups.
fn method_assert_fiber_suspend_allowed_in_async_task() -> MethodBuilder {
    method("__assertFiberSuspendAllowed")
        .static_()
        .returns(TypeExpr::Void)
        .body(vec![s_if(
            e_static_call("__Scheduler", "__isAsyncTaskActive", vec![]),
            vec![s_throw(e_new_fq(
                "Error",
                vec![e_str(
                    "Fiber::suspend() is not a scheduler wakeup inside an Elephc Async task",
                )],
            ))],
            vec![],
            None,
        )])
}

/// Builds the indirect-call guard delegating to the active-task suspension check.
fn method_assert_fiber_suspend_callable_allowed() -> MethodBuilder {
    method("__assertFiberSuspendCallableAllowed")
        .private()
        .static_()
        .returns(TypeExpr::Void)
        .body(vec![s_expr(e_static_call(
            "__Scheduler",
            "__assertFiberSuspendAllowed",
            vec![],
        ))])
}

/// Builds the fail-closed placeholder that callable-guard lowering must replace.
fn method_assert_fiber_suspend_callable_guard() -> MethodBuilder {
    method("__assertFiberSuspendCallableGuard")
        .private()
        .static_()
        .param("callback", t_mixed())
        .returns(TypeExpr::Void)
        .body(vec![
            s_expr(e_call("unset", vec![e_var("callback")])),
            s_throw(e_new_fq(
                "Error",
                vec![e_str("Elephc Async callable guard was not lowered")],
            )),
        ])
}

/// Builds case-insensitive detection of Fiber::suspend callable pairs, including Fiber objects.
fn method_is_fiber_suspend_callable_array() -> MethodBuilder {
    let is_fiber_class_name = e_binop(
        e_binop(
            e_call(
                "strcasecmp",
                vec![e_index(e_var("callback"), e_int(0)), e_str("Fiber")],
            ),
            BinOp::StrictEq,
            e_int(0),
        ),
        BinOp::Or,
        e_binop(
            e_call(
                "strcasecmp",
                vec![e_index(e_var("callback"), e_int(0)), e_str("\\Fiber")],
            ),
            BinOp::StrictEq,
            e_int(0),
        ),
    );
    let is_fiber_class = e_binop(
        e_binop(
            e_call("is_string", vec![e_index(e_var("callback"), e_int(0))]),
            BinOp::And,
            is_fiber_class_name,
        ),
        BinOp::Or,
        e_instance_of(e_index(e_var("callback"), e_int(0)), "\\Fiber"),
    );
    let is_suspend_method = e_binop(
        e_call(
            "strcasecmp",
            vec![e_index(e_var("callback"), e_int(1)), e_str("suspend")],
        ),
        BinOp::StrictEq,
        e_int(0),
    );
    let is_callable_pair = e_binop(
        e_binop(
            e_binop(
                e_binop(
                    e_call("is_array", vec![e_var("callback")]),
                    BinOp::And,
                    e_binop(
                        e_call("count", vec![e_var("callback")]),
                        BinOp::StrictEq,
                        e_int(2),
                    ),
                ),
                BinOp::And,
                e_call("is_string", vec![e_index(e_var("callback"), e_int(1))]),
            ),
            BinOp::And,
            is_fiber_class,
        ),
        BinOp::And,
        is_suspend_method,
    );
    method("__isFiberSuspendCallableArray")
        .private()
        .static_()
        .param("callback", t_array())
        .returns(TypeExpr::Bool)
        .body(vec![s_return(is_callable_pair)])
}

/// The reviewable PHP form of the first scheduler slice.
#[cfg(test)]
const SOURCE: &str = include_str!("async_prelude/source.php");

/// Builds the scheduler declarations as compiler-owned AST.
pub(crate) fn async_declarations() -> Program {
    internal_declarations(|| vec![include!("async_prelude/build_expr.in")])
}

/// Prepends the Async declarations when the program references `run`, so ordinary
/// name resolution and EIR lowering compile their PHP implementation through the
/// same pipeline as user code.
///
/// The shared exhaustive function-reference walker detects direct calls, fully-qualified
/// callable strings, function and namespace import aliases, and namespace-relative calls.
/// The Parallel root also injects Async's cancellation surface. Namespace import tables
/// are scoped so an unrelated user `run()` does not inject the scheduler.
/// Strict PHP passes
/// `allow_extensions = false` and receives the original program unchanged.
pub fn inject_if_used(
    program: Program,
    allow_extensions: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    let uses_async_or_parallel_root = ["Elephc\\Async\\run", "Elephc\\Parallel\\run"]
        .into_iter()
        .any(|target| {
            crate::opcache_prelude::detect::program_references_function_or_import_alias(
                &program, target,
            )
        });
    if !allow_extensions || !uses_async_or_parallel_root {
        return program;
    }
    let mut combined = async_declarations();
    inventory.record_program("async", &combined);
    combined.extend(program);
    combined
}

#[cfg(test)]
mod tests {
    //! Purpose:
    //! Unit tests for the parsed Async scheduler surface and injection order.
    //!
    //! Called from:
    //! - `cargo test` through Rust's test harness.
    //!
    //! Key details:
    //! - The PHP source is the executable specification for the first scheduler
    //!   slice, so a syntax or declaration drift fails before codegen.

    use super::*;
    use crate::parser::ast::StmtKind;

    /// Parses a user fixture with ordinary, rather than compiler-internal, declaration rules.
    fn parse(source: &str) -> Program {
        let tokens = crate::lexer::tokenize(source).expect("fixture must tokenize");
        crate::parser::parse(&tokens).expect("fixture must parse")
    }

    /// Parses the reviewable PHP scheduler oracle with internal declaration access enabled.
    fn parsed_declarations() -> Program {
        let tokens = crate::lexer::tokenize(SOURCE).expect("Async prelude must tokenize");
        crate::parser::parse_internal(&tokens).expect("Async prelude must parse")
    }

    /// Removes source-position differences before comparing generated and parsed AST debug output.
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

    /// Pins every generated declaration and scheduler method to the PHP oracle, excluding spans.
    #[test]
    fn built_declarations_match_the_php_oracle() {
        let built = async_declarations();
        let parsed = parsed_declarations();
        let [built_namespace] = built.as_slice() else {
            panic!("expected one generated Async namespace");
        };
        let [parsed_namespace] = parsed.as_slice() else {
            panic!("expected one source Async namespace");
        };
        let (StmtKind::NamespaceBlock { body: built_body, .. }, StmtKind::NamespaceBlock { body: parsed_body, .. }) =
            (&built_namespace.kind, &parsed_namespace.kind)
        else {
            panic!("expected matching Async namespace declarations");
        };
        assert_eq!(built_body.len(), parsed_body.len());
        for (index, (built_stmt, parsed_stmt)) in built_body.iter().zip(parsed_body.iter()).enumerate() {
            if index == 6 {
                let (StmtKind::ClassDecl { properties: built_props, methods: built_methods, .. },
                    StmtKind::ClassDecl { properties: parsed_props, methods: parsed_methods, .. }) =
                    (&built_stmt.kind, &parsed_stmt.kind)
                else {
                    panic!("expected the Async scheduler class at declaration {index}");
                };
                assert_eq!(
                    strip_spans(&format!("{built_props:?}")),
                    strip_spans(&format!("{parsed_props:?}")),
                    "Async scheduler properties differ from the PHP oracle"
                );
                assert_eq!(built_methods.len(), parsed_methods.len());
                for (method_index, (built_method, parsed_method)) in
                    built_methods.iter().zip(parsed_methods.iter()).enumerate()
                {
                    if built_method.name == "sleepCurrent" {
                        assert_eq!(built_method.body.len(), parsed_method.body.len());
                        for (statement_index, (built_statement, parsed_statement)) in built_method
                            .body
                            .iter()
                            .zip(parsed_method.body.iter())
                            .enumerate()
                        {
                            assert_eq!(
                                strip_spans(&format!("{built_statement:?}")),
                                strip_spans(&format!("{parsed_statement:?}")),
                                "Async scheduler sleepCurrent statement {statement_index} differs"
                            );
                        }
                    }
                    assert_eq!(
                        strip_spans(&format!("{built_method:?}")),
                        strip_spans(&format!("{parsed_method:?}")),
                        "Async scheduler method {method_index} differs from the PHP oracle"
                    );
                }
            }
            assert_eq!(
                strip_spans(&format!("{built_stmt:?}")),
                strip_spans(&format!("{parsed_stmt:?}")),
                "Async prelude class or function {index} differs from its PHP oracle"
            );
        }
    }

    /// Pins the v1 class/function inventory and its shared builtin-contract catalog entries.
    #[test]
    fn declarations_expose_only_the_v1_root_scope_surface() {
        let declarations = async_declarations();
        let [namespace] = declarations.as_slice() else {
            panic!("expected one namespace block");
        };
        let StmtKind::NamespaceBlock { name, body } = &namespace.kind else {
            panic!("expected the Async namespace block");
        };
        assert_eq!(
            name.as_ref().map(|name| name.to_string()),
            Some("Elephc\\Async".to_string())
        );

        let classes: Vec<&str> = body
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::ClassDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        let functions: Vec<&str> = body
            .iter()
            .filter_map(|stmt| match &stmt.kind {
                StmtKind::FunctionDecl { name, .. } => Some(name.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(
            classes,
            vec![
                "CancelledException",
                "__CancellationState",
                "Cancellation",
                "__TaskOutcome",
                "__RunResult",
                "__TaskStart",
                "__Scheduler",
                "__ScopeState",
                "TaskGroup",
                "Awaitable",
            ]
        );
        assert_eq!(functions, vec!["run"]);

        let declared: std::collections::BTreeSet<String> = classes
            .iter()
            .map(|name| format!("Elephc\\Async\\{name}"))
            .collect();
        let catalogued: std::collections::BTreeSet<String> = elephc_builtin_contract::classes()
            .iter()
            .filter(|contract| contract.name.starts_with("Elephc\\Async\\"))
            .map(|contract| contract.name.to_string())
            .collect();
        assert_eq!(declared, catalogued, "Async prelude class catalog drift");
    }

    /// Requires cancellation pairing to use the originating state's private token identity.
    #[test]
    fn cancellation_request_marker_requires_the_state_token() {
        assert!(SOURCE.contains("public function __markRequestPaired"));
        assert!(SOURCE.contains("public static function __isRequestPaired"));
        assert!(SOURCE.contains("public function __isRequestToken"));
        assert!(SOURCE.contains("$this->requestToken === $token"));
        assert!(!SOURCE.contains("private function __markRequestPaired"));
    }

    /// Keeps programs without scheduler roots free of Async prelude declarations.
    #[test]
    fn unrelated_programs_are_not_injected() {
        let program = parse("<?php echo 1;");
        let original_len = program.len();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(program, true, &mut inventory);
        assert_eq!(injected.len(), original_len);
    }

    /// Confirms an imported Async run call injects the scheduler namespace.
    #[test]
    fn run_call_injects_the_async_namespace() {
        let program = parse(
            r#"<?php
use function Elephc\Async\run;
run(function ($tasks): int { return 1; });
"#,
        );
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(program, true, &mut inventory);
        assert!(matches!(
            injected.first().map(|stmt| &stmt.kind),
            Some(StmtKind::NamespaceBlock { .. })
        ));
    }

    /// Confirms aliased function imports still activate Async root detection.
    #[test]
    fn aliased_run_import_injects_the_async_namespace() {
        let program = parse(
            r#"<?php
use function Elephc\Async\run as asyncRun;
asyncRun(static fn ($tasks): int => 1);
"#,
        );
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(program, true, &mut inventory);
        assert!(matches!(
            injected.first().map(|stmt| &stmt.kind),
            Some(StmtKind::NamespaceBlock { name: Some(name), .. })
                if name.as_str() == "Elephc\\Async"
        ));
    }

    /// Covers root detection through a qualified call_user_func callable string.
    #[test]
    fn fully_qualified_callable_string_injects_the_async_namespace() {
        let program = parse(
            r#"<?php call_user_func("Elephc\\Async\\run", static fn ($tasks): int => 1);"#,
        );
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(program, true, &mut inventory);
        assert!(matches!(
            injected.first().map(|stmt| &stmt.kind),
            Some(StmtKind::NamespaceBlock { name: Some(name), .. })
                if name.as_str() == "Elephc\\Async"
        ));
    }

    /// Leaves the source unchanged when strict PHP disables the Async extension surface.
    #[test]
    fn strict_php_does_not_inject_the_extension_surface() {
        let program = parse(
            r#"<?php
use function Elephc\Async\run;
run(function ($tasks): int { return 1; });
"#,
        );
        let original_len = program.len();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        let injected = inject_if_used(program, false, &mut inventory);
        assert_eq!(injected.len(), original_len);
    }
}
