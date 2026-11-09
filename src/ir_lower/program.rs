//! Purpose:
//! Orchestrates AST-to-EIR lowering for a complete checked program.
//!
//! Called from:
//! - `crate::ir_lower::lower_program()`.
//!
//! Key details:
//! - Declaration bodies are lowered before synthetic `main`; declaration
//!   statements themselves are no-ops inside `main`.
//! - The module is validated before it is returned to CLI/test callers.

use std::collections::BTreeSet;
use crate::fast_hash::{FastMap as HashMap, FastSet as HashSet};
use std::path::Path;

use crate::codegen::platform::Target;
use crate::codegen::RuntimeFeatures;
use crate::intrinsics::IntrinsicCall;
use crate::ir::{
    validate_function, validate_module, ExternDecl, ExternParamDecl, Function, Immediate, InstId,
    Instruction, IrType, LocalKind, Module, Op, TraitMethodInfo, ValidationError, ValueDef, ValueId,
};
use crate::ir_lower::{body_contains_eval_call, builtin_datetime, function, LoweringError};
use crate::names::php_symbol_key;
use crate::parser::ast::{
    ClassMethod, Expr, ExprKind, Program, StaticReceiver, Stmt, StmtKind, Visibility,
};
use crate::types::{CheckResult, ClassInfo, FunctionSig, InterfaceInfo, PhpType};

mod metadata;
mod runtime_features;
mod eval_aot;
mod eval_demand;
/// Read by `reflection` and `builtin_datetime`: which builtin surfaces the module's bridge-run
/// eval fragments can name, so those surfaces are lowered in proportion to the fragments.
pub(in crate::ir_lower) use eval_demand::{eval_demand, instruction_uses_eval};
#[allow(unused_imports)] // matched by the eval demand tests only
pub(in crate::ir_lower) use eval_demand::EvalDemand;
/// Read by `builtin_datetime`, which decides whether a module's eval fragments can reach the
/// date alias surface — a question only the fragment text can answer.
/// Read by `builtin_datetime`, which asks whether a module's eval fragments can reach the date
/// alias surface. `eval_literal_call_requires_bridge` is the authority on that: a fragment the
/// planner can compile ahead of time resolves its names through ordinary lowering, where the
/// reachability fixpoint already sees them; one that still needs the bridge resolves them at
/// runtime and can reach anything, including through an `include`.
pub(crate) use runtime_features::eval_literal_call_requires_bridge;
mod declaration_metadata;
mod runtime_bindings;
mod function_declarations;
mod class_methods;
mod spl_discovery;
mod spl_metadata;
mod spl_support;
mod spl_lowering;

use metadata::*;
use runtime_features::*;
use eval_aot::*;
use declaration_metadata::*;
use function_declarations::*;
use class_methods::*;
use spl_discovery::*;
use spl_metadata::*;
use spl_support::*;
use spl_lowering::*;

pub(super) use eval_aot::all_lowered_functions;
pub(super) use runtime_features::include_lowered_runtime_features;
#[allow(unused_imports)] // used by the binary's pipeline only
pub(crate) use runtime_features::module_runs_interpreted_code;
#[allow(unused_imports)] // used by the binary's pipeline only
pub(crate) use runtime_features::{closed_world_bridge_reasons, module_has_dynamic_include};
pub(super) use function_declarations::{
    close_function_global_names_over_calls, collect_function_global_names,
};
pub(super) use spl_discovery::{
    class_data_name, dynamic_object_new_metadata_names, php_method_key, string_data_name,
};
pub(super) use spl_lowering::class_method_already_lowered;

/// Lowers an optimized typed AST program into a validated EIR module.
///
/// `web` mirrors the CLI `--web` flag (the same source
/// `codegen_ir::block_emit::emit_module` receives) and is stored on the
/// returned module so every lowering entry point in `function.rs` can gate
/// request-superglobal type seeding on it; see `Module::web`.
pub(crate) fn lower(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: Option<&Path>,
    web: bool,
    source_catalog: Option<crate::ir::SourceCatalog>,
) -> Result<Module, LoweringError> {
    crate::compiler_stack::with_compiler_stack(|| {
        lower_on_compiler_stack(program, check_result, target, source_path, web, source_catalog)
    })
}

/// The lowering body, running on the stack [`lower`] established.
fn lower_on_compiler_stack(
    program: &Program,
    check_result: &CheckResult,
    target: Target,
    source_path: Option<&Path>,
    web: bool,
    source_catalog: Option<crate::ir::SourceCatalog>,
) -> Result<Module, LoweringError> {
    super::diagnostics::begin_collection();
    // A transparent literal `eval()` (`prelude_prune::usage::transparent_literal_eval`) loads no
    // code: it can neither add a function nor a class the closed world has not seen. Counting it anyway gave
    // `eval('return 1;')` the run-time fallbacks of a program that loads PHP (ArrayAccess probes
    // on every `mixed` subscript, eval-context dispatch) and so linked the interpreter.
    // A closed-world build links no interpreter: a non-literal eval() lowers to a runtime error and
    // a runtime include is answered from the compiled-source table, so neither counts as a program
    // that loads code at run time. Whatever still needs the bridge fails the build afterwards.
    let closed_world = crate::closed_world::is_enabled();
    let dynamic_code_load = !closed_world && crate::ast_usage::collect(program).dynamic_code_load;
    let contains_eval_call = !closed_world && body_contains_eval_call(program) && {
        let usage = crate::prelude_prune::usage::collect(program);
        usage.opaque_eval
            || usage.includes_runtime_php
            || usage.references(crate::names::DYNAMIC_INCLUDE_FUNCTION)
            || dynamic_code_load
    };
    if std::env::var_os("ELEPHC_EXPLAIN_EVAL_BRIDGE").is_some() {
        // Names which input makes the program "load code at run time", which keeps an eval
        // context in every web function that calls a method on a mixed receiver.
        eprintln!(
            "eval-source: eval_call={} dynamic_code_load={} class_files_deferred={}",
            contains_eval_call,
            dynamic_code_load,
            CLASS_FILES_DEFERRED.load(std::sync::atomic::Ordering::Relaxed)
        );
    }
    let _dynamic_function_resolution =
        super::context::enable_runtime_dynamic_function_resolution(contains_eval_call);
    let _program_loads_code = super::context::set_program_loads_code_at_run_time(
        !closed_world
            && (contains_eval_call
                || dynamic_code_load
                || CLASS_FILES_DEFERRED.load(std::sync::atomic::Ordering::Relaxed)),
    );
    let mut module = match source_catalog {
        Some(catalog) => Module::with_source_catalog(target, catalog),
        None => Module::new(target),
    };
    module.source_path = source_path.map(canonical_source_path);
    module.source_files = crate::span::Span::source_paths_snapshot();
    module.web = web;
    module.closed_world = closed_world;
    module.runtime_bound_functions = std::sync::Arc::new(
        runtime_bindings::collect_runtime_bound_functions(program),
    );
    module.ref_param_escaping_callees = check_result.ref_param_escaping_callees.clone();
    module.runtime_define_names = check_result.runtime_define_names.iter().cloned().collect();
    let _runtime_define_names =
        super::context::set_runtime_define_names(check_result.runtime_define_names.clone());
    super::expr::reset_method_declarer_index();
    super::builtin_datetime::reset_datetime_descendants();
    let _undefined_constant_reads = crate::types::predefined_constants::set_undefined_constant_reads(
        check_result.undefined_constant_reads.clone(),
    );
    let _ref_param_escaping_callees = super::context::set_ref_param_escaping_callees(
        check_result.ref_param_escaping_callees.clone(),
    );
    let _property_bound_promotion_sites = super::context::set_property_bound_promotion_sites(
        check_result.property_bound_promotion_sites.clone(),
    );
    let _by_ref_call_result_sites = super::context::set_by_ref_call_result_sites(
        check_result.by_ref_call_result_sites.clone(),
    );
    let _bcmath_operator_sites = crate::bcmath_number_prelude::set_operator_sites(
        check_result.bcmath_operator_sites.clone(),
    );
    let _stdclass_by_ref_walk_sites = crate::stdclass_by_ref_walk::set_sites(
        check_result.stdclass_by_ref_walk_sites.clone(),
    );
    let constants = crate::codegen::collect_constants(program, target);
    let builtin_constants = crate::codegen::collect_constants(&Vec::new(), target);
    module.user_defined_constants = constants
        .keys()
        .filter(|name| !builtin_constants.contains_key(*name))
        .filter(|name| !crate::dom_prelude::PRELUDE_CONSTANTS.contains(&name.as_str()))
        .cloned()
        .collect();
    module.user_defined_constants.sort();
    module.global_constants = constants.clone();
    let fiber_return_sigs = crate::ir_lower::fibers::collect_fiber_return_sigs(program);
    populate_metadata(&mut module, program, check_result);
    lower_function_declarations(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_class_like_methods(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_property_init_thunks(&mut module, check_result, &constants, &fiber_return_sigs);
    function::lower_main(
        program,
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    function::lower_eval_native_default_helpers(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_literal_eval_aot_functions(&mut module, check_result, &constants, &fiber_return_sigs);
    lower_dynamic_constructor_thunks(&mut module, check_result, &constants, &fiber_return_sigs);
    function::lower_optional_interface_adapters(&mut module);
    function::lower_object_comparison_dispatch(&mut module);
    include_lowered_runtime_features(&mut module);
    super::reflection::lower_referenced_builtin_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    lower_referenced_builtin_spl_methods(&mut module, check_result, &constants, &fiber_return_sigs);
    builtin_datetime::lower_referenced_builtin_datetime_methods(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    );
    include_lowered_runtime_features(&mut module);
    // Applicators are planned from the LOWERED module: their candidate classes and invocation
    // scopes come from the `RuntimeFnId::CloneWith` sites this program actually contains.
    super::clone_overrides::lower_clone_override_applicators(
        &mut module,
        check_result,
        &constants,
        &fiber_return_sigs,
    )?;
    super::boxed_array_appends::normalize_module(&mut module);
    super::closure_scopes::specialize_module(&mut module);
    super::effect_refinement::refine_module(&mut module);
    reserve_eval_subclass_property_storage(&mut module);
    // A refused shape is reported before validation: the placeholder EIR those sites emit
    // keeps the module well formed, but the program must not reach codegen regardless.
    if let Some(error) = super::diagnostics::take_first() {
        return Err(LoweringError::Unsupported(error));
    }
    if COLLECT_INVALID_BODIES.load(std::sync::atomic::Ordering::Relaxed) {
        let failures = invalid_bodies(&module);
        if let Some((_, first)) = failures.first() {
            let first = first.clone();
            return Err(LoweringError::InvalidBodies {
                first,
                failures: failures
                    .into_iter()
                    .map(|(name, error)| (name, format!("{error:?}")))
                    .collect(),
                module: Box::new(module),
            });
        }
    }
    validate_lowered_module(&module)?;
    Ok(module)
}

/// Whether some class file was handed back to the runtime autoloader. Its classes are declared by
/// the interpreter at run time, so the program loads code even with no `eval()` in its text; set
/// through `crate::ir_lower::set_class_files_deferred` before lowering.
pub(super) static CLASS_FILES_DEFERRED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Whether validation collects every failing body instead of stopping at the first; set through
/// `crate::ir_lower::collect_invalid_bodies` by a build that can hand class files to the runtime.
pub(super) static COLLECT_INVALID_BODIES: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Returns every body of `module` that fails EIR validation, with its first validator error.
fn invalid_bodies(module: &Module) -> Vec<(String, ValidationError)> {
    module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
        .filter_map(|function| {
            validate_function(function)
                .err()
                .map(|error| (function.name.clone(), error))
        })
        .collect()
}

/// Validates the lowered module, reporting every malformed body when backend inventory is active.
///
/// Ordinary builds preserve fail-fast validation. The inventory mode names every failing body and
/// its first validator error before returning the first error, extending the same scan-all contract
/// used by backend lowering to the EIR boundary that precedes it.
fn validate_lowered_module(module: &Module) -> Result<(), ValidationError> {
    if std::env::var("ELEPHC_BACKEND_INVENTORY").as_deref() != Ok("1") {
        return validate_module(module);
    }

    let mut first_error = None;
    for function in module
        .functions
        .iter()
        .chain(module.class_methods.iter())
        .chain(module.closures.iter())
        .chain(module.fiber_wrappers.iter())
        .chain(module.callback_wrappers.iter())
        .chain(module.extern_callback_trampolines.iter())
        .chain(module.runtime_callable_invokers.iter())
    {
        if let Err(error) = validate_function(function) {
            eprintln!("EIR validation failure in {}: {:?}", function.name, error);
            if let Some(context) = validation_error_context(function, &error) {
                eprintln!("  {context}");
            }
            // The whole refused body, for the refusals the one-line context cannot explain:
            // a dominance violation names two ids and says nothing about how the blocks relate.
            if std::env::var("ELEPHC_EIR_DUMP_INVALID").as_deref() == Ok("1") {
                eprintln!("{}", crate::ir::print_function(function));
            }
            first_error.get_or_insert(error);
        }
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// Formats the producer and consumer metadata relevant to one validation refusal.
fn validation_error_context(function: &Function, error: &ValidationError) -> Option<String> {
    match error {
        ValidationError::ValueDefMismatch(value)
        | ValidationError::VoidValueUsed(value)
        | ValidationError::ResultTypeMismatch(value)
        | ValidationError::PhpTypeMismatch(value)
        | ValidationError::OwnershipTypeMismatch(value) => {
            Some(validation_value_context(function, *value))
        }
        ValidationError::OperandTypeMismatch { inst, operand, .. } => Some(format!(
            "consumer={:?}; {}",
            function.instruction(*inst),
            validation_value_context(function, *operand)
        )),
        _ => None,
    }
}

/// Formats one SSA value together with the instruction that produced it when available.
fn validation_value_context(function: &Function, value_id: ValueId) -> String {
    let value = function.value(value_id);
    let producer = value.and_then(|value| match value.def {
        ValueDef::Instruction { inst, .. } => validation_instruction(function, inst),
        ValueDef::BlockParam { .. } => None,
    });
    format!("value={value_id:?} metadata={value:?}; producer={producer:?}")
}

/// Returns an instruction by id for compact validation diagnostics.
fn validation_instruction(function: &Function, inst: InstId) -> Option<&crate::ir::Instruction> {
    function.instruction(inst)
}
