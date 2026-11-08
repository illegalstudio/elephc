//! Purpose:
//! Injects the `__elephc_proc_*` helpers behind PHP's `proc_open()` family — php-src
//! 8.5's argument validation and diagnostics, the descriptor-spec walk, and the pipes
//! array — as synthetic AST declarations over the `elephc_proc` bridge externs.
//!
//! Called from:
//! - `crate::pipeline::compile()` after the GMP prelude and before name resolution.
//! - The codegen test harness through the same entry point.
//!
//! Key details:
//! - The public names are registry builtins (`crate::builtins::proc`) whose EIR lowering
//!   calls these helpers; reachability roots them from those calls, so an unused helper
//!   is pruned like any other prelude declaration.
//! - Injection is pay-for-use: a program that never names a `proc_*` function neither
//!   declares the helpers nor links `elephc_proc`. `--with-proc` forces both.
//! - Everything is built with `crate::synthetic_class` nodes; no PHP text is parsed.

mod build;
mod detect;

use crate::parser::ast::{CType, Program, Stmt};
use crate::synthetic_class::extern_fn;

/// Injects the proc helpers when the program names a `proc_*` function or `force` is set.
pub fn inject_if_used(
    mut program: Program,
    force: bool,
    inventory: &mut crate::optimize::reachability::PreludeInventory,
) -> Program {
    if !force && !detect::program_uses_proc(&program) {
        return program;
    }
    let mut declarations = declarations();
    inventory.record_program("proc", &declarations);
    declarations.append(&mut program);
    declarations
}

/// Builds every declaration the proc surface contributes.
pub(crate) fn declarations() -> Vec<Stmt> {
    let mut program = extern_declarations();
    program.extend(build::declarations());
    program
}

/// One bridge entry point: `(name, [(parameter, C type)], return C type)`.
type ExternSpec = (&'static str, &'static [(&'static str, CType)], CType);

/// The bridge's C ABI (`crates/elephc-proc/src/lib.rs`), in declaration order.
const EXTERNS: &[ExternSpec] = &[
    ("elephc_proc_builder_new", &[], CType::Int),
    ("elephc_proc_builder_free", &[("builder", CType::Int)], CType::Void),
    ("elephc_proc_builder_shell", &[("builder", CType::Int), ("command", CType::Str)], CType::Int),
    ("elephc_proc_builder_arg", &[("builder", CType::Int), ("argument", CType::Str)], CType::Int),
    ("elephc_proc_builder_cwd", &[("builder", CType::Int), ("cwd", CType::Str)], CType::Int),
    ("elephc_proc_builder_env_reset", &[("builder", CType::Int)], CType::Int),
    ("elephc_proc_builder_env", &[("builder", CType::Int), ("entry", CType::Str)], CType::Int),
    ("elephc_proc_builder_pipe", &[("builder", CType::Int), ("index", CType::Int), ("mode", CType::Str)], CType::Int),
    (
        "elephc_proc_builder_file",
        &[("builder", CType::Int), ("index", CType::Int), ("path", CType::Str), ("mode", CType::Str)],
        CType::Int,
    ),
    ("elephc_proc_builder_fd", &[("builder", CType::Int), ("index", CType::Int), ("fd", CType::Int)], CType::Int),
    ("elephc_proc_builder_null", &[("builder", CType::Int), ("index", CType::Int)], CType::Int),
    (
        "elephc_proc_builder_redirect",
        &[("builder", CType::Int), ("index", CType::Int), ("target", CType::Int)],
        CType::Int,
    ),
    ("elephc_proc_spawn", &[("builder", CType::Int)], CType::Int),
    ("elephc_proc_last_error", &[], CType::Str),
    ("elephc_proc_pipe_count", &[("process", CType::Int)], CType::Int),
    ("elephc_proc_pipe_index", &[("process", CType::Int), ("position", CType::Int)], CType::Int),
    ("elephc_proc_pipe_fd", &[("process", CType::Int), ("position", CType::Int)], CType::Int),
    ("elephc_proc_pid", &[("process", CType::Int)], CType::Int),
    ("elephc_proc_command", &[("process", CType::Int)], CType::Str),
    ("elephc_proc_poll", &[("process", CType::Int)], CType::Int),
    ("elephc_proc_status_field", &[("process", CType::Int), ("field", CType::Int)], CType::Int),
    ("elephc_proc_terminate", &[("process", CType::Int), ("signal", CType::Int)], CType::Int),
    ("elephc_proc_close", &[("process", CType::Int)], CType::Int),
    ("elephc_proc_nice", &[("increment", CType::Int)], CType::Int),
];

/// Declares every bridge entry point in one `extern "elephc_proc"` group.
fn extern_declarations() -> Vec<Stmt> {
    EXTERNS
        .iter()
        .map(|(name, params, returns)| {
            params
                .iter()
                .fold(extern_fn(name, "elephc_proc"), |builder, (param, ty)| {
                    builder.param(param, ty.clone())
                })
                .returns(returns.clone())
                .build()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::parser::ast::StmtKind;

    /// The helpers type-check on every supported target, and the bridge is declared once.
    #[test]
    fn proc_declarations_check_on_every_supported_target() {
        for target in ["macos-aarch64", "linux-aarch64", "linux-x86_64"] {
            let tokens = crate::lexer::tokenize("<?php proc_nice(0);").unwrap();
            let program = crate::parser::parse(&tokens).unwrap();
            let mut inventory = crate::optimize::reachability::PreludeInventory::new();
            let program = super::inject_if_used(program, false, &mut inventory);
            let externs = program
                .iter()
                .filter(|stmt| matches!(stmt.kind, StmtKind::ExternFunctionDecl { .. }))
                .count();
            assert_eq!(externs, super::EXTERNS.len());
            let program = crate::name_resolver::resolve(program).unwrap();
            let target = crate::codegen_support::platform::Target::parse(target).unwrap();
            crate::types::checker::check_types(&program, target)
                .unwrap_or_else(|error| panic!("proc declarations on {}: {error:?}", target.as_str()));
        }
    }

    /// A program that names no proc function is left untouched.
    #[test]
    fn programs_without_proc_calls_are_not_injected() {
        let tokens = crate::lexer::tokenize("<?php echo popen('true', 'r') ? 1 : 0;").unwrap();
        let program = crate::parser::parse(&tokens).unwrap();
        let before = program.len();
        let mut inventory = crate::optimize::reachability::PreludeInventory::new();
        assert_eq!(super::inject_if_used(program, false, &mut inventory).len(), before);
    }
}
