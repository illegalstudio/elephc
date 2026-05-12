//! Purpose:
//! Codegen entry point for generator functions: bodies containing `yield`
//! lower to a wrapper symbol that returns a fresh `GeneratorFrame` plus a
//! state-machine resume symbol that drives the body across yield points.
//!
//! Called from:
//!  - `crate::codegen::functions::emit_function()` when
//!    `yield_validation::body_contains_yield()` is true.
//!
//! Key details:
//!  - Emits two ARM64 symbols per generator: `_fn_<f>` (wrapper allocating a
//!    `GeneratorFrame`, copying params, zeroing locals, returning the frame
//!    pointer) and `_fn_<f>__resume` (per-yield resume label table, body
//!    statements, value boxing via `__rt_mixed_from_value`, refcount drops via
//!    `__rt_decref_mixed`, and `TERMINATED` flag on fall-off).
//!  - v1 body grammar accepts int locals + arithmetic, `$local = yield <expr>`
//!    for `send`, post-inc/dec, `if`/`while`/`do-while`/`for`/`switch`/`break`/
//!    `continue`, mixed int/string yield values, and `yield from` over
//!    int-array literals or generator-returning function calls.
//!  - Unsupported body shapes bail to the terminator silently — the wrapper
//!    still compiles, the generator just yields nothing past the gap.

mod build;
mod emit;
mod model;

use std::collections::HashMap;

use crate::codegen::data_section::DataSection;
use crate::codegen::emit::Emitter;
use crate::codegen::platform::Arch;
use crate::names::function_symbol;
use crate::parser::ast::Stmt;
use crate::types::{ClassInfo, FunctionSig, PhpType};

use build::{build_nodes, collect_locals};
use emit::{emit_resume, emit_wrapper};
use model::{SlotType, StateNumberer};

pub(crate) fn emit_generator_function(
    emitter: &mut Emitter,
    data: &mut DataSection,
    name: &str,
    sig: &FunctionSig,
    body: &[Stmt],
    classes: Option<&HashMap<String, ClassInfo>>,
) {
    let wrapper_label = function_symbol(name);
    emit_generator_with_label(emitter, data, &wrapper_label, sig, &[], body, classes);
}

pub(crate) fn emit_generator_closure(
    emitter: &mut Emitter,
    data: &mut DataSection,
    label: &str,
    sig: &FunctionSig,
    hidden_params: &[(String, PhpType)],
    body: &[Stmt],
    classes: Option<&HashMap<String, ClassInfo>>,
) {
    emit_generator_with_label(emitter, data, label, sig, hidden_params, body, classes);
}

fn emit_generator_with_label(
    emitter: &mut Emitter,
    data: &mut DataSection,
    wrapper_label: &str,
    sig: &FunctionSig,
    hidden_params: &[(String, PhpType)],
    body: &[Stmt],
    classes: Option<&HashMap<String, ClassInfo>>,
) {
    let resume_label = format!("{}__resume", wrapper_label);
    let generator_class_id = classes
        .and_then(|c| c.get("Generator"))
        .map(|info| info.class_id)
        .unwrap_or(0);

    let mut frame_params = sig.params.clone();
    frame_params.extend_from_slice(hidden_params);

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
    emit_resume(emitter, &resume_label, &nodes, highest_state, &mixed_slot_indices);
}
