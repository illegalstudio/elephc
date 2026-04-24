use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{ExprKind, Visibility};
use crate::types::{ClassInfo, EnumCaseInfo, EnumCaseValue, EnumInfo, FunctionSig, PhpType};

use super::super::Checker;

pub(crate) fn propagate_abstract_return_types(checker: &mut Checker) {
    let mut sorted_classes: Vec<(String, u64)> = checker
        .classes
        .iter()
        .map(|(name, info)| (name.clone(), info.class_id))
        .collect();
    sorted_classes.sort_by_key(|(_, class_id)| std::cmp::Reverse(*class_id));

    for (class_name, _) in sorted_classes {
        let Some(class_info) = checker.classes.get(&class_name).cloned() else {
            continue;
        };

        for (method_name, sig) in &class_info.methods {
            let mut parent_name = class_info.parent.clone();
            while let Some(name) = parent_name {
                let Some(parent_info) = checker.classes.get(&name).cloned() else {
                    break;
                };
                if !parent_info.methods.contains_key(method_name) {
                    break;
                }
                if parent_info.method_impl_classes.contains_key(method_name) {
                    break;
                }
                if let Some(parent_mut) = checker.classes.get_mut(&name) {
                    if let Some(parent_sig) = parent_mut.methods.get_mut(method_name) {
                        parent_sig.return_type = sig.return_type.clone();
                    }
                }
                parent_name = parent_info.parent.clone();
            }
        }

        for (method_name, sig) in &class_info.static_methods {
            let mut parent_name = class_info.parent.clone();
            while let Some(name) = parent_name {
                let Some(parent_info) = checker.classes.get(&name).cloned() else {
                    break;
                };
                if !parent_info.static_methods.contains_key(method_name) {
                    break;
                }
                if parent_info
                    .static_method_impl_classes
                    .contains_key(method_name)
                {
                    break;
                }
                if let Some(parent_mut) = checker.classes.get_mut(&name) {
                    if let Some(parent_sig) = parent_mut.static_methods.get_mut(method_name) {
                        parent_sig.return_type = sig.return_type.clone();
                    }
                }
                parent_name = parent_info.parent.clone();
            }
        }
    }
}

pub(crate) fn build_enum_info(
    name: &str,
    backing_type: Option<&crate::parser::ast::TypeExpr>,
    cases: &[crate::parser::ast::EnumCaseDecl],
    span: crate::span::Span,
    checker: &mut Checker,
    next_class_id: &mut u64,
) -> Result<(), CompileError> {
    if checker.classes.contains_key(name)
        || checker.interfaces.contains_key(name)
        || checker.enums.contains_key(name)
    {
        return Err(CompileError::new(
            span,
            &format!("Duplicate class or enum declaration: {}", name),
        ));
    }

    let resolved_backing = match backing_type {
        Some(backing_type) => {
            let resolved = checker.resolve_type_expr(backing_type, span)?;
            match resolved {
                PhpType::Int | PhpType::Str => Some(resolved),
                _ => {
                    return Err(CompileError::new(
                        span,
                        "Enum backing type must be int or string",
                    ))
                }
            }
        }
        None => None,
    };

    let mut seen_case_names = HashSet::new();
    let mut seen_int_values = HashSet::new();
    let mut seen_string_values = HashSet::new();
    let mut enum_cases = Vec::new();
    for case in cases {
        if !seen_case_names.insert(case.name.clone()) {
            return Err(CompileError::new(
                case.span,
                &format!("Duplicate enum case: {}::{}", name, case.name),
            ));
        }

        let value = match (&resolved_backing, &case.value) {
            (None, None) => None,
            (None, Some(_)) => {
                return Err(CompileError::new(
                    case.span,
                    "Pure enum cases cannot declare a backing value",
                ))
            }
            (Some(_), None) => {
                return Err(CompileError::new(
                    case.span,
                    "Backed enum cases must declare a value",
                ))
            }
            (Some(PhpType::Int), Some(expr)) => match &expr.kind {
                ExprKind::IntLiteral(value) => {
                    if !seen_int_values.insert(*value) {
                        return Err(CompileError::new(
                            case.span,
                            &format!("Duplicate enum backing value in {}: {}", name, value),
                        ));
                    }
                    Some(EnumCaseValue::Int(*value))
                }
                _ => {
                    return Err(CompileError::new(
                        case.span,
                        "Enum int backing values must be integer literals",
                    ))
                }
            },
            (Some(PhpType::Str), Some(expr)) => match &expr.kind {
                ExprKind::StringLiteral(value) => {
                    if !seen_string_values.insert(value.clone()) {
                        return Err(CompileError::new(
                            case.span,
                            &format!("Duplicate enum backing value in {}: {:?}", name, value),
                        ));
                    }
                    Some(EnumCaseValue::Str(value.clone()))
                }
                _ => {
                    return Err(CompileError::new(
                        case.span,
                        "Enum string backing values must be string literals",
                    ))
                }
            },
            _ => unreachable!("enum backing type already validated"),
        };

        enum_cases.push(EnumCaseInfo {
            name: case.name.clone(),
            value,
        });
    }

    let mut properties = Vec::new();
    let mut property_offsets = HashMap::new();
    let mut property_declaring_classes = HashMap::new();
    let mut defaults = Vec::new();
    let mut property_visibilities = HashMap::new();
    let final_properties = HashSet::new();
    let mut readonly_properties = HashSet::new();
    if let Some(backing_ty) = &resolved_backing {
        properties.push(("value".to_string(), backing_ty.clone()));
        property_offsets.insert("value".to_string(), 8);
        property_declaring_classes.insert("value".to_string(), name.to_string());
        defaults.push(None);
        property_visibilities.insert("value".to_string(), Visibility::Public);
        readonly_properties.insert("value".to_string());
    }

    let mut static_methods = HashMap::new();
    let mut static_method_visibilities = HashMap::new();
    let mut static_method_declaring_classes = HashMap::new();
    let mut static_method_impl_classes = HashMap::new();
    static_methods.insert(
        "cases".to_string(),
        FunctionSig {
            params: Vec::new(),
            defaults: Vec::new(),
            return_type: PhpType::Array(Box::new(PhpType::Object(name.to_string()))),
            ref_params: Vec::new(),
            declared_params: Vec::new(),
            variadic: None,
        },
    );
    static_method_visibilities.insert("cases".to_string(), Visibility::Public);
    static_method_declaring_classes.insert("cases".to_string(), name.to_string());
    static_method_impl_classes.insert("cases".to_string(), name.to_string());
    if let Some(backing_ty) = &resolved_backing {
        for method_name in ["from", "tryFrom"] {
            static_methods.insert(
                method_name.to_string(),
                FunctionSig {
                    params: vec![("value".to_string(), backing_ty.clone())],
                    defaults: vec![None],
                    return_type: if method_name == "from" {
                        PhpType::Object(name.to_string())
                    } else {
                        checker.normalize_union_type(vec![
                            PhpType::Object(name.to_string()),
                            PhpType::Void,
                        ])
                    },
                    ref_params: vec![false],
                    declared_params: vec![true],
                    variadic: None,
                },
            );
            static_method_visibilities.insert(method_name.to_string(), Visibility::Public);
            static_method_declaring_classes.insert(method_name.to_string(), name.to_string());
            static_method_impl_classes.insert(method_name.to_string(), name.to_string());
        }
    }

    checker.classes.insert(
        name.to_string(),
        ClassInfo {
            class_id: *next_class_id,
            parent: None,
            is_abstract: false,
            is_final: true,
            is_readonly_class: true,
            properties,
            property_offsets,
            property_declaring_classes,
            defaults,
            property_visibilities,
            final_properties,
            readonly_properties,
            method_decls: Vec::new(),
            methods: HashMap::new(),
            static_methods,
            method_visibilities: HashMap::new(),
            final_methods: HashSet::new(),
            method_declaring_classes: HashMap::new(),
            method_impl_classes: HashMap::new(),
            vtable_methods: Vec::new(),
            vtable_slots: HashMap::new(),
            static_method_visibilities,
            final_static_methods: HashSet::new(),
            static_method_declaring_classes,
            static_method_impl_classes,
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            interfaces: Vec::new(),
            constructor_param_to_prop: Vec::new(),
        },
    );
    checker.enums.insert(
        name.to_string(),
        EnumInfo {
            backing_type: resolved_backing,
            cases: enum_cases,
        },
    );
    *next_class_id += 1;
    Ok(())
}
