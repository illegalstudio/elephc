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
    let mut program = crate::parser::parse_with_mode(&crate::lexer::tokenize(source).unwrap(), mode).unwrap();
    let mut inventory = crate::optimize::reachability::PreludeInventory::new();
    program = crate::core_prelude::inject_if_used(program, &mut inventory);
    program = crate::object_cast_prelude::inject_with_semantics(program, &mut inventory, profile).unwrap();
    program = crate::name_resolver::resolve(program).unwrap();
    let checked = crate::types::checker::check_staged_semantics(&program, target, profile).unwrap();
    let _profile = crate::optimize::scoped_semantic_profile(profile);
    if optimize {
        program = crate::optimize::fold_constants_for_target(program, target);
        program = crate::optimize::propagate_constants(program, checked.mixed_storage_local_names(), checked.buffer_read_sites.clone());
        program = crate::optimize::prune_constant_control_flow(program, checked.local_binding_decision_spans());
        program = crate::optimize::normalize_control_flow(program, checked.local_binding_decision_spans());
        program = crate::optimize::eliminate_dead_code(program, checked.local_binding_decision_spans());
    }
    let mut module = crate::ir_lower::lower_program(&program, &checked, target, false).unwrap();
    if optimize { crate::ir_passes::optimize_module(&mut module); }
    module
}

