//! Purpose:
//! Checks application-prelude demand gating and injection from a small embedder thread.
//!
//! Called from:
//! - Focused library tests of the application prelude entry point.
//!
//! Key details:
//! - A supported-depth AST is constructed without recursive parsing on the small stack.
//! - Demand checks inspect request preparation separately from the complete bootstrap copy.

use super::*;

/// Collects only the generated request helper, excluding the adapter's bootstrap observers.
fn application_request_usage(
    source: &str,
    autoload_observes_environment: bool,
) -> crate::prelude_prune::usage::Usage {
    let tokens = crate::lexer::tokenize(source).expect("fixture tokenizes");
    let program = crate::parser::parse(&tokens).expect("fixture parses");
    let injected = inject_with_autoload_usage(
        program, PhpVersion::Php85, &[], &mut Default::default(), None,
        autoload_observes_environment,
    );
    let body = injected.iter().find_map(|statement| match &statement.kind {
        StmtKind::FunctionDecl { name, body, .. } if name == "__elephc_web_prepare_request" => Some(body),
        _ => None,
    }).expect("application request helper");
    crate::prelude_prune::usage::collect(body)
}

/// Unobserved request environments skip enumeration while retaining the empty session scratch array.
#[test]
fn application_environment_demand_omits_unobserved_values() {
    for source in [
        "<?php echo 'ok';",
        "<?php echo getenv('PATH');",
        "<?php echo $_SERVER['PATH'];",
        "<?php $fn = 'strlen'; echo $fn('ok');",
        "<?php $fn = fn() => 'ok'; echo $fn();",
        "<?php session_set_cookie_params(['path' => '/']); session_start();",
    ] {
        let usage = application_request_usage(source, false);
        assert!(!usage.references("elephc_web_env_count"), "{source}");
        assert!(usage.storage_variables.contains("_ENV"), "{source}");
    }
}

/// User observers retain population before the generated adapter can influence the demand scan.
#[test]
fn application_environment_demand_keeps_observable_values() {
    for source in [
        "<?php echo $_ENV['PATH'];",
        "<?php $_ENV['local'] = 'value';",
        "<?php echo $GLOBALS['_ENV']['PATH'];",
        "<?php $fn = fn() => $_ENV['PATH']; echo $fn();",
        "<?php echo count(get_defined_vars());",
        "<?php echo count(compact('_ENV'));",
        "<?php eval($code);",
        "<?php function invoke(string $fn) { $fn(); }",
    ] {
        assert!(application_request_usage(source, false).references("elephc_web_env_count"), "{source}");
    }
}

/// Autoload probe demand retains population even when the entry program has no observer.
#[test]
fn application_environment_demand_keeps_autoloaded_values() {
    assert!(application_request_usage("<?php echo 'ok';", true).references("elephc_web_env_count"));
}

/// A 2 MiB embedder thread can inject a deeply nested program and return it to its owner.
#[test]
fn application_injection_supports_small_embedder_stacks() {
    use crate::parser::ast::{Expr, ExprKind};
    let output = std::thread::Builder::new().stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let span = crate::span::Span::synthetic();
            let mut expression = crate::synthetic_class::e_call("session_set_save_handler", vec![]);
            for _ in 0..1000 {
                expression = Expr::new(ExprKind::ArrayLiteral(vec![expression]), span);
            }
            let program = vec![s_expr(expression)];
            let mut inventory = crate::optimize::reachability::PreludeInventory::new();
            let output = inject(program, PhpVersion::Php85, &[], &mut inventory, None);
            let count = output.len();
            crate::compiler_stack::with_compiler_stack(|| drop(output));
            count
        }).unwrap().join().unwrap();
    assert!(output > 0);
}
