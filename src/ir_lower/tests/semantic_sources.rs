//! Purpose:
//! Shares focused semantic-profile source lowering across regression fixtures.
//!
//! Called from:
//! - Key, unpack and constructor boundary tests.
//!
//! Key details:
//! - Uses core/object-cast declarations, the selected checker and optional AST/EIR passes.

use crate::{codegen_support::platform::Target, source::SourceMode};
use elephc_semantics::SemanticProfile;

/// Shares profiled lowering with conversion controls for the dynamic constructor boundary.
pub(super) fn lower(source: &str, target: Target, profile: SemanticProfile, optimize: bool, mode: SourceMode) -> crate::ir::Module {
    lower_with_capture(source, target, profile, optimize, mode, false)
}

/// Includes capture desugaring and conversion helpers required by unpruned builtin catalogs.
pub(super) fn lower_capturing_arguments(source: &str, target: Target, profile: SemanticProfile, optimize: bool, mode: SourceMode) -> crate::ir::Module {
    lower_with_capture(source, target, profile, optimize, mode, true)
}

/// Shares checking and lowering after selecting the fixture's required frontend preparation.
fn lower_with_capture(source: &str, target: Target, profile: SemanticProfile, optimize: bool, mode: SourceMode, capture: bool) -> crate::ir::Module {
    try_lower_with_capture(source, target, profile, optimize, mode, capture).unwrap()
}

/// Shares the staged frontend while preserving lowering refusals for diagnostic regressions.
pub(super) fn try_lower_capturing_arguments(source: &str, target: Target, profile: SemanticProfile,
    optimize: bool, mode: SourceMode,
) -> Result<crate::ir::Module, crate::ir_lower::LoweringError> {
    try_lower_with_capture(source, target, profile, optimize, mode, true)
}

/// Prepares one staged module and returns a source-located refusal before code generation.
fn try_lower_with_capture(source: &str, target: Target, profile: SemanticProfile, optimize: bool,
    mode: SourceMode, capture: bool,
) -> Result<crate::ir::Module, crate::ir_lower::LoweringError> {
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    program = crate::core_prelude::inject_if_used(program, false, false, &mut inventory);
    if capture {
        program = crate::name_resolver::resolve(program).unwrap();
        program = crate::func_args::desugar(program).unwrap();
        let mut helpers = crate::object_cast_prelude::object_cast_declarations();
        helpers.extend(program);
        program = helpers;
    } else {
        program = crate::object_cast_prelude::inject_with_semantics(program, &mut inventory, profile).unwrap();
    }
    program = crate::name_resolver::resolve(program).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    let _profile = crate::optimize::scoped_semantic_profile(profile);
    if optimize {
        program = crate::optimize::fold_constants_for_target(program, target);
        let optimizer = crate::optimize::PostTypecheckOptimizer::new_with_type_metadata(
            &program, &checked.functions, &checked.classes, &checked.interfaces,
            &checked.scalar_guard_arg_sites,
        );
        program = optimizer.propagate(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
        program = optimizer.prune(program, checked.local_binding_decision_spans());
        program = optimizer.normalize(program, checked.local_binding_decision_spans());
        program = optimizer.eliminate_dead_code(program, checked.local_binding_decision_spans());
    }
    let mut module = crate::ir_lower::lower_program(&program, &checked, target, false)?;
    if optimize { crate::ir_passes::optimize_module(&mut module); }
    Ok(module)
}
