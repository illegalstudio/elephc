//! Purpose:
//! Property initializer thunks and PHP-visible declaration metadata.
//!
//! Called from:
//! - `crate::ir_lower::program`.
//!
//! Key details:
//! - Keeps program metadata deterministic and EIR lowering behavior unchanged.

use super::*;

/// Lowers per-class property-default thunks referenced by `_class_propinit_ptrs`.
pub(super) fn lower_property_init_thunks(
    module: &mut Module,
    check_result: &CheckResult,
    constants: &std::collections::HashMap<String, (ExprKind, PhpType)>,
    fiber_return_sigs: &std::collections::HashMap<String, crate::types::FunctionSig>,
) {
    let mut classes = check_result.classes.iter().collect::<Vec<_>>();
    classes.sort_by_key(|(_, class_info)| class_info.class_id);
    for (class_name, class_info) in classes {
        if crate::ir_lower::reflection::canonical_builtin_reflection_class_name(class_name).is_some() {
            continue;
        }
        function::lower_property_init_thunk(
            class_name,
            class_info,
            module,
            check_result,
            constants,
            fiber_return_sigs,
        );
    }
}

/// Returns deterministic sorted keys for metadata placeholder tables.
pub(super) fn sorted_keys<T>(map: &std::collections::HashMap<String, T>) -> Vec<String> {
    let mut keys = map.keys().cloned().collect::<Vec<_>>();
    keys.sort();
    keys
}

/// Collects PHP-visible class and enum names in the order `get_declared_classes()` must expose.
pub(super) fn collect_declared_class_names(
    program: &Program,
    classes: &HashMap<String, ClassInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        classes,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::ClassDecl { name, .. } | StmtKind::EnumDecl { name, .. } => {
                Some(name.as_str())
            }
            _ => None,
        },
    );
    prepend_internal_names(classes.keys(), &user_names)
}

/// Collects PHP-visible interface names in the order `get_declared_interfaces()` must expose.
pub(super) fn collect_declared_interface_names(
    program: &Program,
    interfaces: &HashMap<String, InterfaceInfo>,
) -> Vec<String> {
    let mut user_names = Vec::new();
    collect_program_declared_names(
        program,
        interfaces,
        &mut HashSet::new(),
        &mut user_names,
        |stmt| match &stmt.kind {
            StmtKind::InterfaceDecl { name, .. } => Some(name.as_str()),
            _ => None,
        },
    );
    prepend_internal_names(interfaces.keys(), &user_names)
}

/// Collects user-declared trait names in source order, including namespace blocks.
pub(super) fn collect_declared_trait_names(program: &Program) -> Vec<String> {
    let mut names = Vec::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => names.push(name.clone()),
            StmtKind::NamespaceBlock { body, .. } => {
                names.extend(collect_declared_trait_names(body));
            }
            _ => {}
        }
    }
    names
}

/// Collects source line metadata for user-declared traits, keyed by trait name.
pub(super) fn collect_declared_trait_source_lines(program: &Program) -> HashMap<String, u32> {
    let mut lines = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl { name, .. } => {
                lines.insert(name.clone(), stmt.span.line);
            }
            StmtKind::NamespaceBlock { body, .. } => {
                lines.extend(collect_declared_trait_source_lines(body));
            }
            _ => {}
        }
    }
    lines
}

/// Collects direct trait-to-trait use declarations keyed by declaring trait name.
pub(super) fn collect_declared_trait_uses(program: &Program) -> HashMap<String, Vec<String>> {
    let mut uses = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name, trait_uses, ..
            } => {
                uses.insert(
                    name.clone(),
                    trait_uses
                        .iter()
                        .flat_map(|trait_use| trait_use.trait_names.iter())
                        .map(|trait_name| trait_name.as_str().to_string())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                uses.extend(collect_declared_trait_uses(body));
            }
            _ => {}
        }
    }
    uses
}

/// Collects direct PHP method names declared by each trait in source order.
pub(super) fn collect_declared_trait_method_names(program: &Program) -> HashMap<String, Vec<String>> {
    let mut methods = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                methods: trait_methods,
                ..
            } => {
                methods.insert(
                    name.clone(),
                    trait_methods
                        .iter()
                        .map(|method| method.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                methods.extend(collect_declared_trait_method_names(body));
            }
            _ => {}
        }
    }
    methods
}

/// Collects direct trait method metadata keyed by trait and PHP method key.
pub(super) fn collect_declared_trait_methods(
    program: &Program,
) -> HashMap<String, HashMap<String, TraitMethodInfo>> {
    let mut methods = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                methods: trait_methods,
                ..
            } => {
                methods.insert(
                    name.clone(),
                    trait_methods
                        .iter()
                        .map(|method| {
                            let method_key = php_symbol_key(&method.name);
                            let info = TraitMethodInfo {
                                name: method.name.clone(),
                                signature: function::method_signature_from_ast(method),
                                visibility: method.visibility.clone(),
                                is_static: method.is_static,
                                is_final: method.is_final,
                                is_abstract: method.is_abstract,
                            };
                            (method_key, info)
                        })
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                methods.extend(collect_declared_trait_methods(body));
            }
            _ => {}
        }
    }
    methods
}

/// Collects direct PHP property names declared by each trait in source order.
pub(super) fn collect_declared_trait_property_names(program: &Program) -> HashMap<String, Vec<String>> {
    let mut properties = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                properties: trait_properties,
                ..
            } => {
                properties.insert(
                    name.clone(),
                    trait_properties
                        .iter()
                        .map(|property| property.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                properties.extend(collect_declared_trait_property_names(body));
            }
            _ => {}
        }
    }
    properties
}

/// Collects direct PHP constant names declared by each trait in source order.
pub(super) fn collect_declared_trait_constant_names(program: &Program) -> HashMap<String, Vec<String>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| constant.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constant_names(body));
            }
            _ => {}
        }
    }
    constants
}

/// Collects direct PHP constant expressions declared by each trait.
pub(super) fn collect_declared_trait_constants(program: &Program) -> HashMap<String, HashMap<String, Expr>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| (constant.name.clone(), constant.value.clone()))
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constants(body));
            }
            _ => {}
        }
    }
    constants
}

/// Collects direct PHP declared constant types for each trait.
pub(super) fn collect_declared_trait_constant_types(
    program: &Program,
) -> HashMap<String, HashMap<String, crate::parser::ast::TypeExpr>> {
    let mut types = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                types.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .filter_map(|constant| {
                            constant
                                .type_expr
                                .clone()
                                .map(|type_expr| (constant.name.clone(), type_expr))
                        })
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                types.extend(collect_declared_trait_constant_types(body));
            }
            _ => {}
        }
    }
    types
}

/// Collects direct PHP constant visibilities declared by each trait.
pub(super) fn collect_declared_trait_constant_visibilities(
    program: &Program,
) -> HashMap<String, HashMap<String, Visibility>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .map(|constant| (constant.name.clone(), constant.visibility.clone()))
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_constant_visibilities(body));
            }
            _ => {}
        }
    }
    constants
}

/// Collects direct PHP final constant names declared by each trait.
pub(super) fn collect_declared_trait_final_constants(program: &Program) -> HashMap<String, HashSet<String>> {
    let mut constants = HashMap::new();
    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                constants: trait_constants,
                ..
            } => {
                constants.insert(
                    name.clone(),
                    trait_constants
                        .iter()
                        .filter(|constant| constant.is_final)
                        .map(|constant| constant.name.clone())
                        .collect(),
                );
            }
            StmtKind::NamespaceBlock { body, .. } => {
                constants.extend(collect_declared_trait_final_constants(body));
            }
            _ => {}
        }
    }
    constants
}

/// Recursively collects source-declared names that are present in checked metadata.
pub(super) fn collect_program_declared_names<T>(
    program: &Program,
    known: &HashMap<String, T>,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
    pick: impl Copy + Fn(&Stmt) -> Option<&str>,
) {
    for stmt in program {
        match &stmt.kind {
            StmtKind::NamespaceBlock { body, .. } => {
                collect_program_declared_names(body, known, seen, out, pick);
            }
            _ => {
                let Some(name) = pick(stmt) else {
                    continue;
                };
                let key = crate::names::php_symbol_key(name);
                let is_known = known.contains_key(name)
                    || known.keys().any(|candidate| {
                        crate::names::php_symbol_key(candidate.trim_start_matches('\\')) == key
                    });
                if is_known && seen.insert(key) {
                    out.push(name.to_string());
                }
            }
        }
    }
}

/// Prepends deterministic internal names before source-order user declarations.
pub(super) fn prepend_internal_names<'a>(
    known_names: impl Iterator<Item = &'a String>,
    user_names: &[String],
) -> Vec<String> {
    let user_keys: HashSet<String> = user_names
        .iter()
        .map(|name| crate::names::php_symbol_key(name))
        .collect();
    let mut names: Vec<String> = known_names
        .filter(|name| !is_internal_synthetic_class_name(name))
        .filter(|name| !user_keys.contains(&crate::names::php_symbol_key(name)))
        .cloned()
        .collect();
    names.sort();
    names.extend(user_names.iter().cloned());
    names
}

/// Returns true for compiler-internal helper classes hidden from PHP introspection.
pub(super) fn is_internal_synthetic_class_name(name: &str) -> bool {
    crate::names::php_symbol_key(name).starts_with("__elephc")
}

/// Converts a PHP type to EIR storage while preserving true void returns.
pub(super) fn value_or_void_ir_type(php_type: &PhpType) -> IrType {
    match php_type {
        PhpType::Void | PhpType::Never => IrType::Void,
        other => IrType::from_php(other),
    }
}
