//! Purpose:
//! Separates application bootstrap from the per-request web prelude and typed dispatch.
//!
//! Called from:
//! - `crate::pipeline::compile()` before and after name resolution.
//!
//! Key details:
//! - The original top level remains the bootstrap, preserving its PHP global scope.
//! - Portable contracts contain no native API; native calls live in the adapter.
//! - Exception boundaries surround generated callbacks before they enter Rust.

use crate::parser::ast::{CType, Program, Stmt, StmtKind};
use crate::synthetic_class::{e_call, e_static_call, e_var, extern_fn, function, s_expr};
use super::PhpVersion;

/// Names of the three generated no-argument C-compatible lifecycle callbacks.
pub(crate) const READY: &str = "__elephc_web_application_ready";
pub(crate) const REQUEST: &str = "__elephc_web_application_request";
pub(crate) const SHUTDOWN: &str = "__elephc_web_application_shutdown";
/// Roots the ordinary PHP options update called by the native four-argument adapter.
pub(crate) const CONTEXT_OPTION: &str = "__elephc_web_application_context_option";

/// Ordinary PHP contract shared with interpreter-specific entry points.
pub(crate) const CONTRACT_SOURCE: &str = include_str!("application/contract.php");
/// Native adapter source, also consumed directly by the prelude signature audit.
pub(crate) const ADAPTER_SOURCE: &str = include_str!("application/adapter.php");

#[cfg(test)]
mod tests;

/// Parses only repository-owned ordinary PHP contracts and adapter declarations.
pub(crate) fn declarations() -> Program {
    let mut program = Vec::new();
    for source in [CONTRACT_SOURCE, ADAPTER_SOURCE] {
        let tokens = crate::lexer::tokenize(source).expect("valid application prelude tokens");
        program.extend(crate::parser::parse(&tokens).expect("valid application prelude"));
    }
    program
}

/// Installs declarations and moves request preparation into its own ordinary PHP function.
pub fn inject(
    program: Program,
    version: PhpVersion,
    ini: &[(String, String)],
    inventory: &mut crate::optimize::reachability::PreludeInventory,
    preload: Option<&std::path::Path>,
) -> Program {
    crate::compiler_stack::with_compiler_stack(|| inject_on_compiler_stack(
        program, version, ini, inventory, preload,
    ))
}

/// Performs recursive usage scans with the same compiler stack budget as the request prelude.
fn inject_on_compiler_stack(
    program: Program,
    version: PhpVersion,
    ini: &[(String, String)],
    inventory: &mut crate::optimize::reachability::PreludeInventory,
    preload: Option<&std::path::Path>,
) -> Program {
    let program = super::lift_preload_to_startup(program, preload);
    let usage = super::usage::collect(&program);
    let needs_callable_session_handler = usage.references("session_set_save_handler")
        || usage.dynamic_function_call;
    let mut declarations_only = Vec::new();
    let mut prepare = Vec::new();
    for statement in super::build::web_declarations(version, ini) {
        if !needs_callable_session_handler && super::is_callable_session_handler_decl(&statement.kind) {
            continue;
        }
        if super::is_hoistable_decl(&statement.kind) { declarations_only.push(statement); }
        else { prepare.push(statement); }
    }
    inventory.record_program("web", &declarations_only);
    declarations_only.push(function("__elephc_web_prepare_request").body(prepare).build());
    declarations_only.extend(declarations());
    for name in [
        "elephc_web_application_cache_roots",
        "elephc_web_application_request_begin",
        "elephc_web_application_request_end",
        "elephc_web_application_shutdown_cleanup",
    ] {
        declarations_only.push(extern_fn(name, "elephc_web").returns(CType::Void).build());
    }
    declarations_only.push(extern_fn("elephc_web_application_phase", "elephc_web").returns(CType::Int).build());
    declarations_only.push(extern_fn("elephc_web_application_commit_bootstrap", "elephc_web").returns(CType::Void).build());
    declarations_only.extend(program);
    declarations_only
}

/// Adds a bootstrap commit and an exception boundary after namespace flattening.
pub fn wrap_bootstrap(program: Program) -> Program {
    let mut declarations = Vec::new();
    let mut executable = Vec::new();
    for statement in program {
        if super::is_hoistable_decl(&statement.kind) { declarations.push(statement); }
        else { executable.push(statement); }
    }
    executable.insert(0, s_expr(e_static_call("Elephc\\Web\\Worker", "beginBootstrap", vec![])));
    executable.insert(0, s_expr(e_call("__elephc_web_application_bootstrap_prepare", vec![])));
    executable.push(s_expr(e_call("elephc_web_application_commit_bootstrap", vec![])));
    executable.push(s_expr(e_static_call("Elephc\\Web\\Worker", "completeBootstrap", vec![])));
    executable.push(s_expr(e_call("elephc_web_application_cache_roots", vec![])));
    let mut wrapper: Stmt = crate::name_resolver::resolve(vec![super::web_wrap_stmt()])
        .expect("global application exception boundary").remove(0);
    let StmtKind::Try { try_body, catches, .. } = &mut wrapper.kind else { unreachable!() };
    catches[0].body = vec![s_expr(e_call("__elephc_web_application_bootstrap_failure", vec![e_var("__elephc_exc")]))];
    *try_body = executable;
    declarations.push(wrapper);
    declarations
}
