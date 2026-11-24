//! Purpose:
//! Desugars PHP 8.6 partial function application into ordinary closures.
//!
//! Called from:
//! - `crate::pipeline::compile()` after argument-introspection desugaring and before the
//!   AST optimizer and type checker.
//!
//! Key details:
//! - The parser marks `?` and a trailing `...` with reserved sentinel variable names; this
//!   pass consumes every marker, so no later pass sees one.
//! - Bound argument expressions are evaluated at closure-creation time by passing them as
//!   arguments to an immediately-invoked closure, then captured by the returned closure.
//! - A known callee signature supplies parameter names, types, defaults, and reference modes;
//!   a method whose signature cannot be resolved is rejected instead of generating a closure
//!   whose receiver binding cannot be verified.
//! - `foo(...)` on its own stays first-class-callable syntax and never reaches this pass.

mod lower;
mod signatures;
mod walk;

pub(crate) use elephc_chopper::pfa::{PLACEHOLDER_VAR, VARIADIC_PLACEHOLDER_VAR};

/// Rewrites every PHP 8.6 partial-application call in `program`.
pub fn desugar(
    program: Vec<crate::parser::ast::Stmt>,
) -> Result<Vec<crate::parser::ast::Stmt>, crate::errors::CompileError> {
    desugar_for_version(program, crate::codegen_support::compile_php_version())
}

/// Rewrites partial application under an explicit PHP compatibility profile.
pub fn desugar_for_version(
    program: Vec<crate::parser::ast::Stmt>,
    php_version: elephc_builtin_contract::PhpVersion,
) -> Result<Vec<crate::parser::ast::Stmt>, crate::errors::CompileError> {
    // PHP 8.6 stream calls are rewritten after name resolution, before the checker. This is a
    // no-op for profiles before 8.6 or programs that do not reach a stream error wrapper.
    let program = crate::stream_errors::rewrite(program, php_version);
    // Almost every program has no partial application. The parser records whether it saw a
    // marker, so the recursive signature collection and AST walk can be skipped entirely.
    if !elephc_chopper::pfa::take_marker() {
        return Ok(program);
    }
    crate::compiler_stack::with_compiler_stack(|| desugar_on_stack(program, php_version))
}

/// The body of [`desugar_for_version`], run on the compiler's grown stack.
fn desugar_on_stack(
    program: Vec<crate::parser::ast::Stmt>,
    php_version: elephc_builtin_contract::PhpVersion,
) -> Result<Vec<crate::parser::ast::Stmt>, crate::errors::CompileError> {
    let signatures = signatures::Signatures::collect(program.clone());
    let enabled = php_version >= elephc_builtin_contract::PhpVersion::Php86;
    let mut pass = walk::Desugar::new(signatures, enabled);
    let rewritten = crate::magic_constants::walker::walk_program(program, &mut pass);
    match pass.into_error() {
        Some(error) => Err(error),
        None => Ok(rewritten),
    }
}
