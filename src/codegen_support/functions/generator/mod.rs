//! Purpose:
//! Codegen entry point for generator functions: bodies containing `yield`
//! lower to a wrapper symbol that returns a fresh `GeneratorFrame` plus a
//! state-machine resume symbol that drives the body across yield points.
//!
//! Called from:
//!  - `crate::codegen_support::functions::emit_closure()` when a deferred
//!    closure wrapper body contains `yield`.
//!
//! Key details:
//!  - Emits two ARM64 symbols per generator: `_fn_<f>` (wrapper allocating a
//!    `GeneratorFrame`, copying params, zeroing locals, returning the frame
//!    pointer) and `_fn_<f>__resume` (per-yield resume label table, body
//!    statements, value boxing via `__rt_mixed_from_value`, refcount drops via
//!    `__rt_decref_mixed`, and `TERMINATED` flag on fall-off).
//!  - v1 body grammar accepts int locals + arithmetic, `$local = yield <expr>`
//!    for `send`, post-inc/dec, `if`/`while`/`do-while`/`for`/`switch`/`try`
//!    no-exception paths, `break`/`continue`, `echo`, mixed int/string yield
//!    values, and `yield from` over int-array literals or generator-returning
//!    function calls.
//!  - Unsupported body shapes bail to the terminator silently — the wrapper
//!    still compiles, the generator just yields nothing past the gap.

mod build;
mod emit;
mod model;

use std::collections::HashMap;

use crate::codegen_support::data_section::DataSection;
use crate::codegen_support::emit::Emitter;
use crate::codegen_support::platform::Arch;
use crate::parser::ast::Stmt;
use crate::types::{ClassInfo, FunctionSig, PhpType};

use build::{build_nodes, collect_locals};
use emit::{emit_resume, emit_wrapper};
use model::{SlotType, StateNumberer};

/// Emits the wrapper and resume symbols for a generator captured inside a closure.
///
/// Same shape as `emit_generator_function` but uses a caller-provided label
/// (typically the closure's inner symbol) instead of deriving it from a
/// function name. Also accepts additional hidden parameters that the closure
/// captures from the outer scope.
///
/// # Arguments
/// * `emitter` - Target instruction emitter
/// * `data` - Data section for constants and metadata
/// * `label` - Symbol name for the wrapper (closure inner symbol)
/// * `sig` - Function signature of the generator
/// * `hidden_params` - Captured variables from the enclosing closure scope
/// * `body` - Parsed AST statements (must contain `yield`)
/// * `classes` - Optional class info map (used to look up the Generator class id)
pub(crate) fn emit_generator_closure(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    sig: &FunctionSig,
    hidden_params: &[(String, PhpType, bool)],
    body: &[Stmt],
    classes: Option<&HashMap<String, ClassInfo>>,
) {
    emit_generator_with_label(emitter, data, label, sig, hidden_params, body, classes);
}

/// Common path for emitting both top-level generators and closure-captured generators.
///
/// Derives the resume label from `wrapper_label`, looks up the Generator class id,
/// builds the slot table from parameters and inferred locals, then emits the wrapper
/// and resume symbols.
///
/// # Arguments
/// * `emitter` - Target instruction emitter
/// * `data` - Data section for constants and metadata
/// * `wrapper_label` - Symbol name for the wrapper; resume label is `<wrapper_label>__resume`
/// * `sig` - Function signature including parameters
/// * `hidden_params` - Additional params from closures (may be empty)
/// * `body` - Parsed AST statements (must contain `yield`)
/// * `classes` - Optional class info map (used to look up the Generator class id)
pub(crate) fn emit_generator_with_label(
    emitter: &mut Emitter,
    data: &mut DataSection,
    wrapper_label: &str,
    sig: &FunctionSig,
    hidden_params: &[(String, PhpType, bool)],
    body: &[Stmt],
    classes: Option<&HashMap<String, ClassInfo>>,
) {
    let resume_label = format!("{}__resume", wrapper_label);
    let generator_class_id = classes
        .and_then(|c| c.get("Generator"))
        .map(|info| info.class_id)
        .unwrap_or(0);

    let mut frame_params = sig.params.clone();
    frame_params.extend(
        hidden_params
            .iter()
            .map(|(name, ty, _)| (name.clone(), ty.clone())),
    );

    // v1 only carries one-register scalar/object parameters into the generator frame.
    let int_param_limit = match emitter.target.arch {
        Arch::AArch64 => 8,
        Arch::X86_64 => usize::MAX,
    };
    let int_param_count = frame_params
        .iter()
        .take(int_param_limit)
        .take_while(|(_, ty)| {
            matches!(
                ty.codegen_repr(),
                PhpType::Int | PhpType::Bool | PhpType::Callable | PhpType::Object(_)
            )
        })
        .count();
    let int_param_names: Vec<String> = frame_params
        .iter()
        .take(int_param_count)
        .map(|(n, _)| n.clone())
        .collect();

    let local_typed = collect_locals(body, &int_param_names);

    // Build the unified params+locals slot table. Params are all
    // Int-typed; local types come from the build-phase inference.
    let mut all_slot_names: Vec<String> = int_param_names.clone();
    let mut all_slot_types: Vec<SlotType> = vec![SlotType::Int; int_param_count];
    for (name, ty) in &local_typed {
        all_slot_names.push(name.clone());
        all_slot_types.push(*ty);
    }

    let mut numberer = StateNumberer::new();
    let nodes = build_nodes(body, &all_slot_names, &all_slot_types, &mut numberer, data);
    let highest_state = numberer.next_state.saturating_sub(1);

    // Indices of Mixed-typed slots — needed by the terminator to decref
    // any cells the locals still own when the generator finishes.
    let mixed_slot_indices: Vec<usize> = all_slot_types
        .iter()
        .enumerate()
        .filter_map(|(idx, ty)| if *ty == SlotType::Mixed { Some(idx) } else { None })
        .collect();

    emit_wrapper(
        emitter,
        &wrapper_label,
        &resume_label,
        generator_class_id,
        int_param_count,
        local_typed.len(),
    );
    emit_resume(emitter, data, &resume_label, &nodes, highest_state, &mixed_slot_indices);
}
