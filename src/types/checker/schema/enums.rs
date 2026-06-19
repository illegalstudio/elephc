//! Purpose:
//! Validates schema enums declarations for the checker.
//! Turns parsed declarations into canonical metadata and rejects invalid contracts before code generation.
//!
//! Called from:
//! - `crate::types::checker::schema`
//!
//! Key details:
//! - Declaration metadata must align with name resolution, inheritance flattening, and runtime/codegen expectations.

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::names::php_symbol_key;
use crate::parser::ast::{ClassMethod, ExprKind, Visibility};
use crate::types::{ClassInfo, EnumCaseInfo, EnumCaseValue, EnumInfo, FunctionSig, PhpType};

use super::super::Checker;
use super::classes::{collect_attribute_args, collect_attribute_names};
use super::validation::build_method_sig;

/// Clones an enum method with `self`/`static` type hints rewritten to the enum itself. Enums
/// have no parent, so `parent` is left unresolved (and rejected later if it surfaces).
fn substitute_enum_relative_types(method: &ClassMethod, enum_name: &str) -> ClassMethod {
    let mut method = method.clone();
    for (_, type_ann, _, _) in method.params.iter_mut() {
        if let Some(ty) = type_ann.as_mut() {
            *ty = ty.substitute_relative_class_types(enum_name, None);
        }
    }
    if let Some(return_type) = method.return_type.as_mut() {
        *return_type = return_type.substitute_relative_class_types(enum_name, None);
    }
    method
}

/// Propagates concrete return types from overrides to their abstract parent declarations.
///
/// Iterates classes in reverse class-ID order so that subclasses override before their parents.
/// For each instance and static method in a class, walks up the inheritance chain until it finds
/// a parent that does NOT have an implementation for that method — the abstract declaration that
/// needs the return type filled in. Skips parents that already have explicit implementations.
///
/// Inputs:
/// - `checker.classes`: populated class metadata including methods, static_methods, parent chain
///
/// Side effects: Mutates `parent_sig.return_type` on abstract method signatures in checker.classes.
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

/// Validates and builds metadata for a single enum declaration.
///
/// Checks for duplicate class/enum/interface names, validates backing type (int or string only),
/// ensures pure enums have no case values and backed enums require values, rejects duplicate
/// case names and duplicate backing values, and constructs the `EnumInfo` plus a parallel `ClassInfo`
/// with synthesized `cases()`, `from()`, and `tryFrom()` static methods.
///
/// Inputs:
/// - `name`: enum identifier
/// - `backing_type`: optional `TypeExpr` for backed enums
/// - `cases`: parsed enum case declarations
/// - `span`: source location for error reporting
/// - `checker`: type checker state (classes, interfaces, enums, resolve_type_expr)
/// - `next_class_id`: incrementing class ID counter
///
/// Returns: `Ok(())` on success, `CompileError` for invalid declarations.
///
/// Side effects:
/// - Inserts `ClassInfo` into `checker.classes` with synthesized methods
/// - Inserts `EnumInfo` into `checker.enums`
/// - Increments `*next_class_id`
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_enum_info(
    name: &str,
    backing_type: Option<&crate::parser::ast::TypeExpr>,
    cases: &[crate::parser::ast::EnumCaseDecl],
    implements: &[crate::names::Name],
    user_methods: &[crate::parser::ast::ClassMethod],
    user_constants: &[crate::parser::ast::ClassConst],
    span: crate::span::Span,
    checker: &mut Checker,
    next_class_id: &mut u64,
) -> Result<(), CompileError> {
    let enum_key = php_symbol_key(name);
    if checker
        .classes
        .keys()
        .any(|existing| php_symbol_key(existing) == enum_key)
        || checker
            .interfaces
            .keys()
            .any(|existing| php_symbol_key(existing) == enum_key)
        || checker
            .enums
            .keys()
            .any(|existing| php_symbol_key(existing) == enum_key)
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
            attribute_names: collect_attribute_names(&case.attributes),
            attribute_args: collect_attribute_args(&case.attributes),
        });
    }

    insert_enum_metadata(
        name,
        resolved_backing,
        enum_cases,
        implements,
        user_methods,
        user_constants,
        checker,
        next_class_id,
    )
}

/// Inserts validated enum metadata and its parallel final readonly class metadata.
///
/// Used by parsed enum declarations and builtin enum injection after case/backing
/// validation has already happened. Synthesizes the static enum methods exposed
/// by PHP: all enums get `cases()`, while backed enums also get `from()` and
/// `tryFrom()`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_enum_metadata(
    name: &str,
    backing_type: Option<PhpType>,
    enum_cases: Vec<EnumCaseInfo>,
    implements: &[crate::names::Name],
    user_methods: &[ClassMethod],
    user_constants: &[crate::parser::ast::ClassConst],
    checker: &mut Checker,
    next_class_id: &mut u64,
) -> Result<(), CompileError> {
    let mut properties = Vec::new();
    let mut property_offsets = HashMap::new();
    let mut property_declaring_classes = HashMap::new();
    let mut defaults = Vec::new();
    let mut property_visibilities = HashMap::new();
    let mut declared_properties = HashSet::new();
    let mut property_declared_slots = Vec::new();
    let final_properties = HashSet::new();
    let mut readonly_properties = HashSet::new();
    let reference_properties = HashSet::new();
    let mut property_reference_slots = Vec::new();
    push_enum_readonly_property(
        "name",
        PhpType::Str,
        name,
        &mut properties,
        &mut property_offsets,
        &mut property_declaring_classes,
        &mut defaults,
        &mut property_visibilities,
        &mut declared_properties,
        &mut property_declared_slots,
        &mut readonly_properties,
        &mut property_reference_slots,
    );
    if let Some(backing_ty) = &backing_type {
        push_enum_readonly_property(
            "value",
            backing_ty.clone(),
            name,
            &mut properties,
            &mut property_offsets,
            &mut property_declaring_classes,
            &mut defaults,
            &mut property_visibilities,
            &mut declared_properties,
            &mut property_declared_slots,
            &mut readonly_properties,
            &mut property_reference_slots,
        );
    }

    let mut static_methods = HashMap::new();
    let mut static_method_visibilities = HashMap::new();
    let mut static_method_declaring_classes = HashMap::new();
    let mut static_method_impl_classes = HashMap::new();
    static_methods.insert(
        "cases".to_string(),
        FunctionSig {
            params: Vec::new(),
            param_type_exprs: Vec::new(),
            param_attributes: Vec::new(),
            defaults: Vec::new(),
            return_type: PhpType::Array(Box::new(PhpType::Object(name.to_string()))),
            declared_return: true,
            ref_params: Vec::new(),
            declared_params: Vec::new(),
            variadic: None,
            deprecation: None,
        },
    );
    static_method_visibilities.insert("cases".to_string(), Visibility::Public);
    static_method_declaring_classes.insert("cases".to_string(), name.to_string());
    static_method_impl_classes.insert("cases".to_string(), name.to_string());
    if let Some(backing_ty) = &backing_type {
        for method_name in ["from", "tryfrom"] {
            static_methods.insert(
                method_name.to_string(),
                FunctionSig {
                    params: vec![("value".to_string(), backing_ty.clone())],
                    param_type_exprs: vec![None],
                    param_attributes: Vec::new(),
                    defaults: vec![None],
                    return_type: if method_name == "from" {
                        PhpType::Object(name.to_string())
                    } else {
                        checker.normalize_union_type(vec![
                            PhpType::Object(name.to_string()),
                            PhpType::Void,
                        ])
                    },
                    declared_return: true,
                    ref_params: vec![false],
                    declared_params: vec![true],
                    variadic: None,
                    deprecation: None,
                },
            );
            static_method_visibilities.insert(method_name.to_string(), Visibility::Public);
            static_method_declaring_classes.insert(method_name.to_string(), name.to_string());
            static_method_impl_classes.insert(method_name.to_string(), name.to_string());
        }
    }

    // Register the enum as a known class name before building method signatures so that `self`
    // return/parameter types (rewritten to the enum name) resolve while its metadata is in flight.
    checker.declared_classes.insert(name.to_string());

    // User-declared enum methods. Enum cases are singleton objects, so instance methods dispatch
    // on the case like a class. `self`/`static` hints resolve to the enum.
    let mut methods = HashMap::new();
    let mut method_decls = Vec::new();
    let mut method_visibilities = HashMap::new();
    let mut method_declaring_classes = HashMap::new();
    let mut method_impl_classes = HashMap::new();
    for method in user_methods {
        let method = substitute_enum_relative_types(method, name);
        let sig = build_method_sig(checker, &method)?;
        let key = php_symbol_key(&method.name);
        if method.is_static {
            static_methods.insert(key.clone(), sig);
            static_method_visibilities.insert(key.clone(), method.visibility.clone());
            static_method_declaring_classes.insert(key.clone(), name.to_string());
            static_method_impl_classes.insert(key, name.to_string());
        } else {
            methods.insert(key.clone(), sig);
            method_visibilities.insert(key.clone(), method.visibility.clone());
            method_declaring_classes.insert(key.clone(), name.to_string());
            method_impl_classes.insert(key, name.to_string());
        }
        // Codegen emits both instance and static method bodies from `method_decls`.
        method_decls.push(method);
    }

    // User-declared enum constants. Values are kept as their parsed expressions, matching the
    // class-constant representation.
    let mut constants = HashMap::new();
    let mut final_constants = HashSet::new();
    let mut constant_attribute_names = HashMap::new();
    let mut constant_attribute_args = HashMap::new();
    for constant in user_constants {
        constants.insert(constant.name.clone(), constant.value.clone());
        if constant.is_final {
            final_constants.insert(constant.name.clone());
        }
        constant_attribute_names.insert(
            constant.name.clone(),
            collect_attribute_names(&constant.attributes),
        );
        constant_attribute_args.insert(
            constant.name.clone(),
            collect_attribute_args(&constant.attributes),
        );
    }
    for case in &enum_cases {
        constant_attribute_names.insert(case.name.clone(), case.attribute_names.clone());
        constant_attribute_args.insert(case.name.clone(), case.attribute_args.clone());
    }

    let interfaces: Vec<String> = implements
        .iter()
        .map(|interface| interface.as_str().to_string())
        .collect();

    checker.classes.insert(
        name.to_string(),
        ClassInfo {
            class_id: *next_class_id,
            parent: None,
            is_abstract: false,
            is_final: true,
            is_readonly_class: true,
            allow_dynamic_properties: false,
            constants,
            final_constants,
            attribute_names: Vec::new(),
            attribute_args: Vec::new(),
            method_attribute_names: HashMap::new(),
            method_attribute_args: HashMap::new(),
            property_attribute_names: HashMap::new(),
            property_attribute_args: HashMap::new(),
            constant_attribute_names,
            constant_attribute_args,
            used_traits: Vec::new(),
            properties,
            property_offsets,
            property_declaring_classes,
            defaults,
            property_visibilities,
            property_set_visibilities: HashMap::new(),
            declared_properties,
            property_declared_slots,
            final_properties,
            readonly_properties,
            reference_properties,
            property_reference_slots,
            abstract_properties: HashSet::new(),
            abstract_property_hooks: HashMap::new(),
            static_properties: Vec::new(),
            static_defaults: Vec::new(),
            static_property_declaring_classes: HashMap::new(),
            static_property_visibilities: HashMap::new(),
            declared_static_properties: HashSet::new(),
            final_static_properties: HashSet::new(),
            method_decls,
            methods,
            static_methods,
            callable_method_return_sigs: HashMap::new(),
            callable_array_method_return_sigs: HashMap::new(),
            method_visibilities,
            final_methods: HashSet::new(),
            method_declaring_classes,
            method_impl_classes,
            vtable_methods: Vec::new(),
            vtable_slots: HashMap::new(),
            static_method_visibilities,
            final_static_methods: HashSet::new(),
            static_method_declaring_classes,
            static_method_impl_classes,
            static_vtable_methods: Vec::new(),
            static_vtable_slots: HashMap::new(),
            interfaces,
            constructor_param_to_prop: Vec::new(),
        },
    );
    checker.enums.insert(
        name.to_string(),
        EnumInfo {
            backing_type,
            cases: enum_cases,
        },
    );
    *next_class_id += 1;
    Ok(())
}

/// Appends one synthetic public readonly enum case property to class metadata.
fn push_enum_readonly_property(
    property: &str,
    php_type: PhpType,
    enum_name: &str,
    properties: &mut Vec<(String, PhpType)>,
    property_offsets: &mut HashMap<String, usize>,
    property_declaring_classes: &mut HashMap<String, String>,
    defaults: &mut Vec<Option<crate::parser::ast::Expr>>,
    property_visibilities: &mut HashMap<String, Visibility>,
    declared_properties: &mut HashSet<String>,
    property_declared_slots: &mut Vec<bool>,
    readonly_properties: &mut HashSet<String>,
    property_reference_slots: &mut Vec<bool>,
) {
    let offset = 8 + properties.len() * 16;
    let property = property.to_string();
    properties.push((property.clone(), php_type));
    property_offsets.insert(property.clone(), offset);
    property_declaring_classes.insert(property.clone(), enum_name.to_string());
    defaults.push(None);
    property_visibilities.insert(property.clone(), Visibility::Public);
    declared_properties.insert(property.clone());
    property_declared_slots.push(true);
    readonly_properties.insert(property);
    property_reference_slots.push(false);
}
