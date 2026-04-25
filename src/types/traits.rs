use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{
    ClassMethod, ClassProperty, Program, StmtKind, TraitAdaptation, TraitUse, Visibility,
};
use crate::span::Span;

#[derive(Debug, Clone)]
pub struct FlattenedClass {
    pub name: String,
    pub extends: Option<String>,
    pub implements: Vec<String>,
    pub is_abstract: bool,
    pub is_final: bool,
    pub is_readonly_class: bool,
    pub properties: Vec<ClassProperty>,
    pub methods: Vec<ClassMethod>,
}

#[derive(Clone)]
struct TraitDeclInfo {
    trait_uses: Vec<TraitUse>,
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
    span: Span,
}

#[derive(Clone)]
struct ExpandedTrait {
    properties: Vec<ClassProperty>,
    methods: Vec<ClassMethod>,
}

#[derive(Clone)]
struct ImportedMethod {
    source_trait: String,
    decl: ClassMethod,
}

pub fn flatten_classes(program: &Program) -> (Vec<FlattenedClass>, Vec<CompileError>) {
    let mut trait_map = HashMap::new();
    let mut class_names = HashSet::new();
    let mut errors = Vec::new();

    for stmt in program {
        match &stmt.kind {
            StmtKind::TraitDecl {
                name,
                trait_uses,
                properties,
                methods,
            } => {
                if class_names.contains(name) || trait_map.contains_key(name) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate trait declaration: {}", name),
                    ));
                    continue;
                }
                trait_map.insert(
                    name.clone(),
                    TraitDeclInfo {
                        trait_uses: trait_uses.clone(),
                        properties: properties.clone(),
                        methods: methods.clone(),
                        span: stmt.span,
                    },
                );
            }
            StmtKind::ClassDecl { name, .. }
            | StmtKind::EnumDecl { name, .. }
            | StmtKind::InterfaceDecl { name, .. } => {
                if trait_map.contains_key(name) || !class_names.insert(name.clone()) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate class or interface declaration: {}", name),
                    ));
                    continue;
                }
            }
            _ => {}
        }
    }

    let mut cache = HashMap::new();
    let mut stack = Vec::new();
    let mut flattened = Vec::new();
    for stmt in program {
        if let StmtKind::ClassDecl {
            name,
            extends,
            implements,
            is_abstract,
            is_final,
            is_readonly_class,
            trait_uses,
            properties,
            methods,
        } = &stmt.kind
        {
            if let Err(error) = validate_direct_members(properties, methods, stmt.span, name) {
                errors.extend(error.flatten());
                continue;
            }
            let (imported_props, imported_methods) = match resolve_trait_uses(
                trait_uses,
                &trait_map,
                &mut cache,
                &mut stack,
                &format!("class {}", name),
                stmt.span,
            ) {
                Ok(result) => result,
                Err(error) => {
                    errors.extend(error.flatten());
                    continue;
                }
            };
            let merged_props = match merge_properties(
                &imported_props,
                properties,
                stmt.span,
                &format!("class {}", name),
                true,
            ) {
                Ok(props) => props,
                Err(error) => {
                    errors.extend(error.flatten());
                    continue;
                }
            };
            let merged_methods = match merge_methods(
                imported_methods,
                methods,
                stmt.span,
                &format!("class {}", name),
            ) {
                Ok(methods) => methods,
                Err(error) => {
                    errors.extend(error.flatten());
                    continue;
                }
            };
            flattened.push(FlattenedClass {
                name: name.clone(),
                extends: extends.as_ref().map(|name| name.as_str().to_string()),
                implements: implements.iter().map(|name| name.as_str().to_string()).collect(),
                is_abstract: *is_abstract,
                is_final: *is_final,
                is_readonly_class: *is_readonly_class,
                properties: merged_props,
                methods: merged_methods,
            });
        }
    }

    (flattened, errors)
}

fn expand_trait(
    trait_name: &str,
    trait_map: &HashMap<String, TraitDeclInfo>,
    cache: &mut HashMap<String, ExpandedTrait>,
    stack: &mut Vec<String>,
) -> Result<ExpandedTrait, CompileError> {
    if let Some(expanded) = cache.get(trait_name) {
        return Ok(expanded.clone());
    }
    if stack.iter().any(|name| name == trait_name) {
        let mut chain = stack.clone();
        chain.push(trait_name.to_string());
        return Err(CompileError::new(
            Span::dummy(),
            &format!("Circular trait composition detected: {}", chain.join(" -> ")),
        ));
    }
    let trait_info = trait_map.get(trait_name).ok_or_else(|| {
        CompileError::new(
            Span::dummy(),
            &format!("Unknown trait referenced during flattening: {}", trait_name),
        )
    })?;

    validate_direct_members(
        &trait_info.properties,
        &trait_info.methods,
        trait_info.span,
        trait_name,
    )?;

    stack.push(trait_name.to_string());
    let (imported_props, imported_methods) = resolve_trait_uses(
        &trait_info.trait_uses,
        trait_map,
        cache,
        stack,
        &format!("trait {}", trait_name),
        trait_info.span,
    )?;
    stack.pop();

    let properties = merge_properties(
        &imported_props,
        &trait_info.properties,
        trait_info.span,
        &format!("trait {}", trait_name),
        true,
    )?;
    let methods = merge_methods(
        imported_methods,
        &trait_info.methods,
        trait_info.span,
        &format!("trait {}", trait_name),
    )?;
    let expanded = ExpandedTrait { properties, methods };
    cache.insert(trait_name.to_string(), expanded.clone());
    Ok(expanded)
}

fn resolve_trait_uses(
    trait_uses: &[TraitUse],
    trait_map: &HashMap<String, TraitDeclInfo>,
    cache: &mut HashMap<String, ExpandedTrait>,
    stack: &mut Vec<String>,
    owner_label: &str,
    owner_span: Span,
) -> Result<(Vec<ClassProperty>, Vec<ClassMethod>), CompileError> {
    let mut all_properties = Vec::new();
    let mut all_methods = Vec::new();

    for trait_use in trait_uses {
        let mut imported_properties = Vec::new();
        let mut candidates: HashMap<String, Vec<ImportedMethod>> = HashMap::new();
        let mut method_order = Vec::new();
        let listed_trait_names: HashSet<String> = trait_use
            .trait_names
            .iter()
            .map(|name| name.as_str().to_string())
            .collect();

        for trait_name in &trait_use.trait_names {
            let expanded = expand_trait(trait_name.as_str(), trait_map, cache, stack).map_err(|err| {
                CompileError::new(
                    trait_use.span,
                    &format!("{} references unknown or invalid trait '{}': {}", owner_label, trait_name, err.message),
                )
            })?;
            for property in expanded.properties {
                merge_property_into(
                    &mut imported_properties,
                    property,
                    trait_use.span,
                    owner_label,
                    false,
                )?;
            }
            for method in expanded.methods {
                if !candidates.contains_key(&method.name) {
                    method_order.push(method.name.clone());
                }
                candidates
                    .entry(method.name.clone())
                    .or_default()
                    .push(ImportedMethod {
                        source_trait: trait_name.to_string(),
                        decl: method,
                    });
            }
        }

        let mut suppressed: HashMap<String, HashSet<String>> = HashMap::new();
        let mut visibility_overrides: HashMap<(String, String), Visibility> = HashMap::new();
        let mut alias_methods: Vec<ImportedMethod> = Vec::new();

        for adaptation in &trait_use.adaptations {
            match adaptation {
                TraitAdaptation::InsteadOf {
                    trait_name,
                    method,
                    instead_of,
                } => {
                    let selected_trait = resolve_adaptation_source(
                        trait_name.as_ref().map(|name| name.as_str()),
                        method,
                        &candidates,
                        trait_use.span,
                    )?;
                    for loser in instead_of {
                        if !listed_trait_names.contains(loser.as_str()) {
                            return Err(CompileError::new(
                                trait_use.span,
                                &format!(
                                    "{} cannot use insteadof with unrelated trait '{}'",
                                    owner_label, loser
                                ),
                            ));
                        }
                        if loser.as_str() == selected_trait {
                            return Err(CompileError::new(
                                trait_use.span,
                                &format!(
                                    "{} cannot suppress the same trait '{}' with insteadof",
                                    owner_label, loser
                                ),
                            ));
                        }
                        suppressed
                            .entry(method.clone())
                            .or_default()
                            .insert(loser.to_string());
                    }
                }
                TraitAdaptation::Alias {
                    trait_name,
                    method,
                    alias,
                    visibility,
                } => {
                    let selected_trait = resolve_adaptation_source(
                        trait_name.as_ref().map(|name| name.as_str()),
                        method,
                        &candidates,
                        trait_use.span,
                    )?;
                    let imported = candidates
                        .get(method)
                        .and_then(|methods| {
                            methods
                                .iter()
                                .find(|candidate| candidate.source_trait == selected_trait)
                        })
                        .cloned()
                        .ok_or_else(|| {
                            CompileError::new(
                                trait_use.span,
                                &format!(
                                    "{} cannot alias undefined trait method {}::{}",
                                    owner_label, selected_trait, method
                                ),
                            )
                        })?;

                    if let Some(alias_name) = alias {
                        let mut alias_decl = imported.decl.clone();
                        alias_decl.name = alias_name.clone();
                        if let Some(visibility) = visibility {
                            alias_decl.visibility = visibility.clone();
                        }
                        alias_methods.push(ImportedMethod {
                            source_trait: selected_trait,
                            decl: alias_decl,
                        });
                    } else if let Some(visibility) = visibility {
                        visibility_overrides
                            .insert((selected_trait, method.clone()), visibility.clone());
                    }
                }
            }
        }

        let mut selected_methods = Vec::new();
        for method_name in method_order {
            let imports = candidates.remove(&method_name).unwrap_or_default();
            let remaining: Vec<ImportedMethod> = imports
                .into_iter()
                .filter(|candidate| {
                    !suppressed
                        .get(&method_name)
                        .is_some_and(|set| set.contains(&candidate.source_trait))
                })
                .collect();
            if remaining.len() > 1 {
                let trait_list = remaining
                    .iter()
                    .map(|candidate| candidate.source_trait.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(CompileError::new(
                    trait_use.span,
                    &format!(
                        "{} has ambiguous trait method '{}'; resolve with insteadof (candidates: {})",
                        owner_label, method_name, trait_list
                    ),
                ));
            }
            if let Some(mut selected) = remaining.into_iter().next() {
                if let Some(visibility) = visibility_overrides.get(&(
                    selected.source_trait.clone(),
                    method_name.clone(),
                )) {
                    selected.decl.visibility = visibility.clone();
                }
                selected_methods.push(selected.decl);
            }
        }

        for alias_method in alias_methods {
            selected_methods.push(alias_method.decl);
        }

        all_properties = merge_properties(
            &all_properties,
            &imported_properties,
            owner_span,
            owner_label,
            false,
        )?;
        merge_imported_method_set(&mut all_methods, selected_methods, owner_span, owner_label)?;
    }

    Ok((all_properties, all_methods))
}

fn merge_properties(
    imported: &[ClassProperty],
    local: &[ClassProperty],
    span: Span,
    owner_label: &str,
    replace_compatible_existing: bool,
) -> Result<Vec<ClassProperty>, CompileError> {
    let mut merged = imported.to_vec();
    for property in local {
        merge_property_into(
            &mut merged,
            property.clone(),
            span,
            owner_label,
            replace_compatible_existing,
        )?;
    }
    Ok(merged)
}

fn merge_property_into(
    merged: &mut Vec<ClassProperty>,
    property: ClassProperty,
    span: Span,
    owner_label: &str,
    replace_compatible_existing: bool,
) -> Result<(), CompileError> {
    if let Some(index) = merged
        .iter()
        .position(|existing| existing.name == property.name)
    {
        let existing = &merged[index];
        if properties_compatible(existing, &property) {
            if replace_compatible_existing {
                merged[index] = property;
            }
            return Ok(());
        }
        return Err(CompileError::new(
            span,
            &format!(
                "{} has incompatible duplicate property '{}'",
                owner_label, property.name
            ),
        ));
    }
    merged.push(property);
    Ok(())
}

fn merge_methods(
    imported: Vec<ClassMethod>,
    local: &[ClassMethod],
    span: Span,
    owner_label: &str,
) -> Result<Vec<ClassMethod>, CompileError> {
    validate_direct_method_duplicates(local, span, owner_label)?;

    let mut local_keys = HashSet::new();
    for method in local {
        local_keys.insert((method.name.clone(), method.is_static));
    }

    let mut merged = Vec::new();
    let mut seen_imported = HashSet::new();
    for imported_method in imported {
        let key = (imported_method.name.clone(), imported_method.is_static);
        if local_keys.contains(&key) {
            continue;
        }
        if !seen_imported.insert(key.clone()) {
            return Err(CompileError::new(
                span,
                &format!(
                    "{} imports duplicate trait method '{}'",
                    owner_label, imported_method.name
                ),
            ));
        }
        merged.push(imported_method);
    }

    merged.extend(local.iter().cloned());
    Ok(merged)
}

fn merge_imported_method_set(
    existing: &mut Vec<ClassMethod>,
    incoming: Vec<ClassMethod>,
    span: Span,
    owner_label: &str,
) -> Result<(), CompileError> {
    let mut seen: HashSet<(String, bool)> = existing
        .iter()
        .map(|method| (method.name.clone(), method.is_static))
        .collect();
    for method in incoming {
        let key = (method.name.clone(), method.is_static);
        if !seen.insert(key) {
            return Err(CompileError::new(
                span,
                &format!("{} imports duplicate trait method '{}'", owner_label, method.name),
            ));
        }
        existing.push(method);
    }
    Ok(())
}

fn validate_direct_members(
    properties: &[ClassProperty],
    methods: &[ClassMethod],
    span: Span,
    owner_name: &str,
) -> Result<(), CompileError> {
    let mut seen_props = HashSet::new();
    for property in properties {
        if !seen_props.insert(property.name.clone()) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate property declaration in {}: {}", owner_name, property.name),
            ));
        }
    }
    validate_direct_method_duplicates(methods, span, owner_name)
}

fn validate_direct_method_duplicates(
    methods: &[ClassMethod],
    span: Span,
    owner_name: &str,
) -> Result<(), CompileError> {
    let mut seen = HashSet::new();
    for method in methods {
        let key = (method.name.clone(), method.is_static);
        if !seen.insert(key) {
            return Err(CompileError::new(
                span,
                &format!("Duplicate method declaration in {}: {}", owner_name, method.name),
            ));
        }
    }
    Ok(())
}

fn resolve_adaptation_source(
    explicit_trait: Option<&str>,
    method: &str,
    candidates: &HashMap<String, Vec<ImportedMethod>>,
    span: Span,
) -> Result<String, CompileError> {
    let options = candidates.get(method).ok_or_else(|| {
        CompileError::new(
            span,
            &format!("Trait adaptation references undefined method '{}'", method),
        )
    })?;

    if let Some(trait_name) = explicit_trait {
        if options
            .iter()
            .any(|candidate| candidate.source_trait == trait_name)
        {
            return Ok(trait_name.to_string());
        }
        return Err(CompileError::new(
            span,
            &format!(
                "Trait adaptation references undefined method {}::{}",
                trait_name, method
            ),
        ));
    }

    if options.len() == 1 {
        Ok(options[0].source_trait.clone())
    } else {
        Err(CompileError::new(
            span,
            &format!(
                "Trait adaptation for '{}' is ambiguous without a qualifying trait name",
                method
            ),
        ))
    }
}

fn properties_compatible(left: &ClassProperty, right: &ClassProperty) -> bool {
    left.visibility == right.visibility
        && left.type_expr == right.type_expr
        && left.readonly == right.readonly
        && left.by_ref == right.by_ref
        && left.default == right.default
}
