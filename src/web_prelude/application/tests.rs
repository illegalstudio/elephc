//! Purpose:
//! Checks standalone application-prelude injection from a small embedder thread.
//!
//! Called from:
//! - Focused library tests of the application prelude entry point.
//!
//! Key details:
//! - A supported-depth AST is constructed without recursive parsing on the small stack.

use super::*;

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
