//! Purpose:
//! Second consumer of EvalIR: a flat opcode slice with an explicit frame stack.
//! The tree walker remains the path for every named compile gap.
//!
//! Called from:
//! - `crate::interpreter::execute_program_outcome_with_context()`.
//! - Function declaration and the walker binder, for a body the slice admitted.
//!
//! Key details:
//! - `RuntimeValueOps` is the only value semantics. The slice does not invent a value model.
//! - The executable cache stores modules and gaps, never cells. Effects run on every execution.
//! - A frame's instruction index is the resume point. Line and exception tables are omitted
//!   until EvalIR carries the spans they would index.

mod cache;
mod compile;
mod vm;

pub(crate) use cache::with_executable_cache_key;
#[cfg(test)]
pub(in crate::interpreter) use cache::lookup_executable;
pub(in crate::interpreter) use compile::attach_simple_body;
#[cfg(test)]
pub(in crate::interpreter) use compile::compile_program;
pub(in crate::interpreter) use vm::{execute_bound_body, execute_module};
#[cfg(test)]
pub(in crate::interpreter) use vm::last_execution_used_vm;

use crate::context::ElephcEvalContext;
use crate::errors::EvalStatus;
use crate::eval_ir::EvalProgram;
use crate::interpreter::{EvalControl, RuntimeValueOps};
use crate::scope::ElephcEvalScope;

/// Runs the slice when the fragment compiles. `Ok(None)` means the tree walker must run.
pub(in crate::interpreter) fn try_execute(
    context: &mut ElephcEvalContext,
    program: &EvalProgram,
    scope: &mut ElephcEvalScope,
    values: &mut impl RuntimeValueOps,
) -> Result<Option<EvalControl>, EvalStatus> {
    let cache_key = cache::copy_current_key();
    if let Some(code) = cache_key.as_deref() {
        if let Some(cached) = cache::lookup_executable(code) {
            return match cached {
                Ok(module) => execute_module(&module, context, scope, values).map(|(control, _)| Some(control)),
                Err(_) => {
                    vm::mark_tree_walk();
                    Ok(None)
                }
            };
        }
    }
    match compile::compile_program(program) {
        Ok(module) => {
            if let Some(code) = cache_key.as_deref() {
                cache::store_executable(code, Ok(std::sync::Arc::clone(&module)));
            }
            execute_module(&module, context, scope, values).map(|(control, _)| Some(control))
        }
        Err(gap) => {
            if let Some(code) = cache_key.as_deref() {
                cache::store_executable(code, Err(gap));
            }
            vm::mark_tree_walk();
            Ok(None)
        }
    }
}
