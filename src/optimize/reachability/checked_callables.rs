//! Purpose:
//! Retains dependencies of checked callable guards introduced after source usage scans.
//!
//! Called from:
//! - Declaration graph construction before its behavioral fixed point.
//!
//! Key details:
//! - Guard dependencies belong to their checked body, so dead functions cannot widen roots.
//! - Original scopes also cover lexical closures through the shared scope containment helper.

use std::collections::HashSet;
use crate::{names::php_symbol_key, types::{CheckResult, PhpType}};
use super::{graph::DeclarationIndex, usage::Usage};

/// Adds late runtime-callable dependencies only when the owning body becomes reachable.
pub(super) fn include_guard_dependencies(index: &mut DeclarationIndex, executable: &mut Usage,
    checked: &CheckResult,
) {
    let scopes = checked.local_store_contracts.iter()
        .filter(|(_, contract)| contains_callable(&contract.ty)).map(|((scope, _, _), _)| php_symbol_key(scope))
        .chain(checked.local_store_contracts.copies.iter().filter(|(_, contract)| contains_callable(&contract.contract))
            .map(|((scope, _, _), _)| php_symbol_key(scope))).collect::<HashSet<_>>();
    let owns_guard = |scope: &str| scopes.iter().any(|candidate|
        crate::types::loop_storage_scope_is_within(candidate, scope));
    if owns_guard(crate::types::TOP_LEVEL_SCOPE) { executable.hazards.include_unknown_callable(); }
    for (name, usage) in &mut index.functions {
        if owns_guard(name) { usage.hazards.include_unknown_callable(); }
    }
    for (class, node) in &mut index.classes {
        for ((method, _), usage) in &mut node.methods {
            if owns_guard(&format!("{class}::{method}")) { usage.hazards.include_unknown_callable(); }
        }
    }
    for ((class, method, _), usage) in &mut index.checker_methods {
        if owns_guard(&format!("{class}::{method}")) { usage.hazards.include_unknown_callable(); }
    }
}

/// Finds callability checks nested within scalar unions or explicit structural containers.
fn contains_callable(ty: &PhpType) -> bool {
    match ty {
        PhpType::Callable => true,
        PhpType::Union(members) => members.iter().any(contains_callable),
        PhpType::Array(element) => contains_callable(element),
        PhpType::AssocArray { key, value } => contains_callable(key) || contains_callable(value),
        _ => false,
    }
}

/// Late guards retain catalog dependencies only for executable owning bodies.
#[cfg(test)]
#[test]
fn checked_callable_guard_dependencies_follow_body_reachability() {
    use super::{PreludeInventory, PruneOptions, prune_unreachable_declarations};
    use crate::{codegen_support::platform::Target, parser::ast::StmtKind};
    use elephc_semantics::{SemanticProfile, StrictFeature};
    let declarations = "<?php function checkedGuard(mixed $next): void { $value=static fn(): int=>1; $value=$next; } \
        class CheckedGuard { public static function apply(mixed $next): void { $value=static fn(): int=>1; $value=$next; } } ";
    for invocation in ["echo 1;", "checkedGuard(1);", "CheckedGuard::apply(1);"] {
        let source = format!("{declarations}{invocation}");
        let program = crate::parser::parse(&crate::lexer::tokenize(&source).unwrap()).unwrap();
        let profile = SemanticProfile::PERMISSIVE.with(StrictFeature::Locals, true);
        let mut inventory = PreludeInventory::new();
        let program = crate::object_cast_prelude::inject_with_semantics(program, &mut inventory, profile).unwrap();
        let mut checked = crate::types::checker::check_staged_semantics(&program, Target::detect_host(), profile).unwrap();
        assert!(!checked.local_store_contracts.is_empty());
        let roots = HashSet::new();
        let program = prune_unreachable_declarations(program, &mut checked, PruneOptions {
            inventory: &inventory, forced_groups: &roots, exported_functions: &roots, eval_forced: false,
        });
        let has_helper = program.iter().any(|statement| matches!(&statement.kind,
            StmtKind::FunctionDecl { name, .. } if name.as_str() == crate::object_cast_prelude::DYNAMIC_CAST_HELPER));
        assert_eq!(has_helper, invocation != "echo 1;", "{invocation}");
    }
}
