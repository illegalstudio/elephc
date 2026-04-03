pub(crate) mod builtins;
mod builtin_types;
mod callables;
mod extern_decl;
mod functions;
mod inference;
mod schema;
mod stmt_check;
mod type_compat;

use std::collections::{HashMap, HashSet};

use crate::errors::CompileError;
use crate::parser::ast::{
    CallableTarget, Expr, Program, StmtKind, TypeExpr,
};
use crate::types::{
    ctype_stack_size, ctype_to_php_type, packed_type_size,
    traits::{flatten_classes, FlattenedClass},
    CheckResult, ClassInfo, EnumInfo, ExternClassInfo,
    ExternFieldInfo, ExternFunctionSig, FunctionSig, InterfaceInfo, PackedClassInfo,
    PackedFieldInfo, PhpType, TypeEnv,
};

pub use inference::{infer_expr_type_syntactic, infer_return_type_syntactic};
use builtin_types::{
    inject_builtin_throwables, patch_builtin_exception_signatures, patch_magic_method_signatures,
    validate_magic_method_contracts, InterfaceDeclInfo,
};
use schema::{
    build_class_info_recursive, build_enum_info, build_interface_info_recursive,
    propagate_abstract_return_types,
};

pub(crate) struct Checker {
    pub fn_decls: HashMap<String, FnDecl>,
    pub functions: HashMap<String, FunctionSig>,
    pub constants: HashMap<String, PhpType>,
    /// Tracks the return type of closures assigned to variables.
    pub closure_return_types: HashMap<String, PhpType>,
    /// Tracks known callable signatures assigned to variables.
    pub callable_sigs: HashMap<String, FunctionSig>,
    /// Tracks first-class callable targets assigned to variables.
    pub first_class_callable_targets: HashMap<String, CallableTarget>,
    /// Interface definitions collected during first pass.
    pub interfaces: HashMap<String, InterfaceInfo>,
    /// Class definitions collected during first pass.
    pub classes: HashMap<String, ClassInfo>,
    /// Canonical class names declared in the program, available for forward references.
    pub declared_classes: HashSet<String>,
    /// Enum definitions collected during first pass.
    pub enums: HashMap<String, EnumInfo>,
    /// Canonical interface names declared in the program, available for forward references.
    pub declared_interfaces: HashSet<String>,
    /// Name of the class currently being type-checked (for $this).
    pub current_class: Option<String>,
    /// Name of the current method, when type-checking a class method body.
    pub current_method: Option<String>,
    /// Whether the current class method is static.
    pub current_method_is_static: bool,
    /// Extern function declarations.
    pub extern_functions: HashMap<String, ExternFunctionSig>,
    /// Extern class (C struct) declarations.
    pub extern_classes: HashMap<String, ExternClassInfo>,
    /// Packed layout-only records.
    pub packed_classes: HashMap<String, PackedClassInfo>,
    /// Extern global variable declarations.
    pub extern_globals: HashMap<String, PhpType>,
    /// Libraries required by extern blocks.
    pub required_libraries: Vec<String>,
    /// Best-known top-level variable types visible to `global` statements.
    pub top_level_env: TypeEnv,
    /// Names that are by-ref parameters in the current local scope.
    pub active_ref_params: HashSet<String>,
    /// Names introduced via `global` in the current local scope.
    pub active_globals: HashSet<String>,
    /// Names introduced via `static` in the current local scope.
    pub active_statics: HashSet<String>,
}

#[derive(Clone)]
pub(crate) struct FnDecl {
    pub params: Vec<String>,
    pub param_types: Vec<Option<TypeExpr>>,
    pub defaults: Vec<Option<Expr>>,
    pub ref_params: Vec<bool>,
    pub variadic: Option<String>,
    pub return_type: Option<TypeExpr>,
    pub span: crate::span::Span,
    pub body: Vec<crate::parser::ast::Stmt>,
}

pub fn check_types(program: &Program) -> Result<CheckResult, CompileError> {
    let mut checker = Checker {
        fn_decls: HashMap::new(),
        functions: HashMap::new(),
        constants: HashMap::new(),
        closure_return_types: HashMap::new(),
        callable_sigs: HashMap::new(),
        first_class_callable_targets: HashMap::new(),
        interfaces: HashMap::new(),
        classes: HashMap::new(),
        declared_classes: HashSet::new(),
        enums: HashMap::new(),
        declared_interfaces: HashSet::new(),
        current_class: None,
        current_method: None,
        current_method_is_static: false,
        extern_functions: HashMap::new(),
        extern_classes: HashMap::new(),
        packed_classes: HashMap::new(),
        extern_globals: HashMap::new(),
        required_libraries: Vec::new(),
        top_level_env: HashMap::new(),
        active_ref_params: HashSet::new(),
        active_globals: HashSet::new(),
        active_statics: HashSet::new(),
    };
    let mut errors = Vec::new();

    for stmt in program {
        if let StmtKind::FunctionDecl {
            name,
            params,
            variadic,
            return_type,
            body,
            ..
        } = &stmt.kind
        {
            let param_names: Vec<String> = params.iter().map(|(n, _, _, _)| n.clone()).collect();
            let param_type_anns: Vec<Option<TypeExpr>> =
                params.iter().map(|(_, t, _, _)| t.clone()).collect();
            let defaults: Vec<Option<Expr>> = params.iter().map(|(_, _, d, _)| d.clone()).collect();
            let ref_flags: Vec<bool> = params.iter().map(|(_, _, _, r)| *r).collect();
            checker.fn_decls.insert(
                name.clone(),
                FnDecl {
                    params: param_names,
                    param_types: param_type_anns,
                    defaults,
                    ref_params: ref_flags,
                    variadic: variadic.clone(),
                    return_type: return_type.clone(),
                    span: stmt.span,
                    body: body.clone(),
                },
            );
        }
    }

    let (flattened_classes, flatten_errors) = flatten_classes(program);
    errors.extend(flatten_errors);
    let class_map: HashMap<String, FlattenedClass> = flattened_classes
        .iter()
        .cloned()
        .map(|class| (class.name.clone(), class))
        .collect();
    let mut class_map = class_map;
    let mut interface_map = HashMap::new();
    checker.declared_classes = class_map.keys().cloned().collect();
    for stmt in program {
        if let StmtKind::InterfaceDecl {
            name,
            extends,
            methods,
        } = &stmt.kind
        {
            if interface_map.contains_key(name) {
                errors.push(CompileError::new(
                    stmt.span,
                    &format!("Duplicate interface declaration: {}", name),
                ));
                continue;
            }
            interface_map.insert(
                name.clone(),
                InterfaceDeclInfo {
                    name: name.clone(),
                    extends: extends
                        .iter()
                        .map(|name| name.as_str().to_string())
                        .collect(),
                    methods: methods.clone(),
                    span: stmt.span,
                },
            );
        }
    }
    checker.declared_interfaces = interface_map.keys().cloned().collect();
    if let Err(error) = inject_builtin_throwables(&mut interface_map, &mut class_map) {
        errors.extend(error.flatten());
    }

    let mut next_interface_id = 0u64;
    let mut building_interfaces = HashSet::new();
    let interface_names: Vec<String> = interface_map.keys().cloned().collect();
    for interface_name in interface_names {
        if let Err(error) = build_interface_info_recursive(
            &interface_name,
            &interface_map,
            &class_map,
            &mut checker,
            &mut next_interface_id,
            &mut building_interfaces,
        ) {
            errors.extend(error.flatten());
        }
    }

    // First pass: collect flattened class declarations and build ClassInfo
    let mut next_class_id = 0u64;
    let mut building = HashSet::new();
    let class_names: Vec<String> = class_map.keys().cloned().collect();
    for class_name in class_names {
        if let Err(error) = build_class_info_recursive(
            &class_name,
            &class_map,
            &mut checker,
            &mut next_class_id,
            &mut building,
        ) {
            errors.extend(error.flatten());
        }
    }
    for stmt in program {
        if let StmtKind::EnumDecl {
            name,
            backing_type,
            cases,
        } = &stmt.kind
        {
            if let Err(error) = build_enum_info(
                name,
                backing_type.as_ref(),
                cases,
                stmt.span,
                &mut checker,
                &mut next_class_id,
            ) {
                errors.extend(error.flatten());
            }
        }
    }
    patch_builtin_exception_signatures(&mut checker);
    patch_magic_method_signatures(&mut checker);

    // Pre-scan: collect extern declarations
    for stmt in program {
        match &stmt.kind {
            StmtKind::ExternFunctionDecl {
                name,
                params,
                return_type,
                library,
            } => {
                if checker.extern_functions.contains_key(name)
                    || checker.fn_decls.contains_key(name)
                {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate function declaration: {}", name),
                    ));
                    continue;
                }
                let php_params: Vec<(String, PhpType)> = params
                    .iter()
                    .map(|p| (p.name.clone(), ctype_to_php_type(&p.c_type)))
                    .collect();
                let php_ret = ctype_to_php_type(return_type);
                if let Err(error) = checker.validate_extern_function_decl(
                    name,
                    params,
                    return_type,
                    &php_params,
                    &php_ret,
                    stmt.span,
                ) {
                    errors.extend(error.flatten());
                    continue;
                }
                // Register as a regular function sig so call-site type checking works
                let sig = FunctionSig {
                    params: php_params.clone(),
                    defaults: params.iter().map(|_| None).collect(),
                    return_type: php_ret.clone(),
                    ref_params: params.iter().map(|_| false).collect(),
                    declared_params: vec![true; php_params.len()],
                    variadic: None,
                };
                checker.functions.insert(name.clone(), sig);
                checker.extern_functions.insert(
                    name.clone(),
                    ExternFunctionSig {
                        name: name.clone(),
                        params: php_params,
                        return_type: php_ret,
                        library: library.clone(),
                    },
                );
                if let Some(lib) = library {
                    if !checker.required_libraries.contains(lib) {
                        checker.required_libraries.push(lib.clone());
                    }
                }
            }
            StmtKind::ExternClassDecl { name, fields } => {
                if checker.extern_classes.contains_key(name) || checker.classes.contains_key(name) {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate class declaration: {}", name),
                    ));
                    continue;
                }
                let mut extern_fields = Vec::new();
                let mut offset = 0usize;
                let mut seen_fields = std::collections::HashSet::new();
                let mut class_has_errors = false;
                for f in fields {
                    if let Err(error) = checker.validate_extern_field_decl(name, f, stmt.span) {
                        errors.extend(error.flatten());
                        class_has_errors = true;
                        continue;
                    }
                    if !seen_fields.insert(f.name.clone()) {
                        errors.push(CompileError::new(
                            stmt.span,
                            &format!("Duplicate extern field: {}::{}", name, f.name),
                        ));
                        class_has_errors = true;
                        continue;
                    }
                    let php_type = ctype_to_php_type(&f.c_type);
                    let size = ctype_stack_size(&f.c_type);
                    extern_fields.push(ExternFieldInfo {
                        name: f.name.clone(),
                        php_type,
                        offset,
                    });
                    offset += size;
                }
                if class_has_errors {
                    continue;
                }
                checker.extern_classes.insert(
                    name.clone(),
                    ExternClassInfo {
                        name: name.clone(),
                        total_size: offset,
                        fields: extern_fields,
                    },
                );
            }
            StmtKind::PackedClassDecl { name, fields } => {
                if checker.packed_classes.contains_key(name)
                    || checker.classes.contains_key(name)
                    || checker.extern_classes.contains_key(name)
                {
                    errors.push(CompileError::new(
                        stmt.span,
                        &format!("Duplicate packed class declaration: {}", name),
                    ));
                    continue;
                }
                let mut packed_fields = Vec::new();
                let mut offset = 0usize;
                let mut seen_fields = std::collections::HashSet::new();
                let mut class_has_errors = false;
                for field in fields {
                    if !seen_fields.insert(field.name.clone()) {
                        errors.push(CompileError::new(
                            field.span,
                            &format!("Duplicate packed field: {}::{}", name, field.name),
                        ));
                        class_has_errors = true;
                        continue;
                    }
                    let php_type = match checker.resolve_type_expr(&field.type_expr, field.span) {
                        Ok(php_type) => php_type,
                        Err(error) => {
                            errors.extend(error.flatten());
                            class_has_errors = true;
                            continue;
                        }
                    };
                    let Some(size) = packed_type_size(&php_type, &checker.packed_classes) else {
                        errors.push(CompileError::new(
                            field.span,
                            "Packed class fields must use POD scalars, pointers, or packed classes",
                        ));
                        class_has_errors = true;
                        continue;
                    };
                    packed_fields.push(PackedFieldInfo {
                        name: field.name.clone(),
                        php_type,
                        offset,
                    });
                    offset += size;
                }
                if class_has_errors {
                    continue;
                }
                checker.packed_classes.insert(
                    name.clone(),
                    PackedClassInfo {
                        fields: packed_fields,
                        total_size: offset,
                    },
                );
            }
            StmtKind::ExternGlobalDecl { name, c_type } => {
                if let Err(error) = checker.validate_extern_global_decl(name, c_type, stmt.span) {
                    errors.extend(error.flatten());
                    continue;
                }
                let php_type = ctype_to_php_type(c_type);
                checker.extern_globals.insert(name.clone(), php_type);
            }
            _ => {}
        }
    }

    let mut global_env: TypeEnv = HashMap::new();
    global_env.insert("argc".to_string(), PhpType::Int);
    global_env.insert("argv".to_string(), PhpType::Array(Box::new(PhpType::Str)));
    // Add extern globals to the global environment
    for (name, ty) in &checker.extern_globals {
        global_env.insert(name.clone(), ty.clone());
    }
    for stmt in program {
        checker.top_level_env = global_env.clone();
        if let Err(error) = checker.check_stmt(stmt, &mut global_env) {
            errors.extend(error.flatten());
        }
    }

    // Resolve signatures for functions that were declared but never called
    // directly so declared param/return types are still validated.
    let unchecked: Vec<String> = checker
        .fn_decls
        .keys()
        .filter(|name| !checker.functions.contains_key(*name))
        .cloned()
        .collect();
    for name in unchecked {
        if let Some(decl) = checker.fn_decls.get(&name).cloned() {
            match checker.initial_function_param_types(&name, &decl) {
                Ok(param_types) => {
                    if let Err(error) =
                        checker.resolve_function_signature(&name, &decl, param_types)
                    {
                        errors.extend(error.flatten());
                    }
                }
                Err(error) => errors.extend(error.flatten()),
            }
        }
    }

    // Post-pass: type-check class method bodies NOW that property types
    // have been updated from new ClassName(args) calls in the main scope.
    // Some methods also refine class property types that other methods depend on,
    // so iterate until ClassInfo stabilizes before surfacing method-body errors.
    let mut method_passes_remaining = (flattened_classes.len().max(1) * 2) + 1;
    loop {
        let classes_before_pass = checker.classes.clone();
        let mut pass_errors = Vec::new();

        for class in &flattened_classes {
            for method in &class.methods {
                if method.is_abstract {
                    continue;
                }
                let mut method_env: TypeEnv = global_env.clone();
                if !method.is_static {
                    method_env.insert("this".to_string(), PhpType::Object(class.name.clone()));
                }
                let sig_params = if method.is_static {
                    checker
                        .classes
                        .get(&class.name)
                        .and_then(|c| c.static_methods.get(&method.name))
                        .map(|s| s.params.clone())
                } else {
                    checker
                        .classes
                        .get(&class.name)
                        .and_then(|c| c.methods.get(&method.name))
                        .map(|s| s.params.clone())
                };
                for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
                    let ty = if let Some(type_ann) = type_ann {
                        checker.resolve_declared_param_type_hint(
                            type_ann,
                            method.span,
                            &format!("Method parameter ${}", pname),
                        )?
                    } else {
                        sig_params
                            .as_ref()
                            .and_then(|p| p.get(i))
                            .map(|(_, t)| t.clone())
                            .unwrap_or(PhpType::Int)
                    };
                    method_env.insert(pname.clone(), ty);
                }
                if let Some(variadic_name) = &method.variadic {
                    let ty = sig_params
                        .as_ref()
                        .and_then(|p| p.get(method.params.len()))
                        .map(|(_, t)| t.clone())
                        .unwrap_or(PhpType::Array(Box::new(PhpType::Int)));
                    method_env.insert(variadic_name.clone(), ty);
                }
                if method.name == "__construct" {
                    if let Some(ci) = checker.classes.get(&class.name).cloned() {
                        for (i, (pname, type_ann, _, _)) in method.params.iter().enumerate() {
                            if type_ann.is_some() {
                                continue;
                            }
                            if let Some(Some(prop_name)) = ci.constructor_param_to_prop.get(i) {
                                if let Some((_, ty)) = ci.properties.iter().find(|(n, _)| n == prop_name) {
                                    method_env.insert(pname.clone(), ty.clone());
                                    if let Some(ci_mut) = checker.classes.get_mut(&class.name) {
                                        if let Some(sig) = ci_mut.methods.get_mut("__construct") {
                                            if i < sig.params.len() {
                                                sig.params[i].1 = ty.clone();
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                checker.current_class = Some(class.name.clone());
                checker.current_method = Some(method.name.clone());
                checker.current_method_is_static = method.is_static;
                let method_ref_params: Vec<String> = method
                    .params
                    .iter()
                    .filter(|(_, _, _, is_ref)| *is_ref)
                    .map(|(name, _, _, _)| name.clone())
                    .collect();
                let mut method_errors = Vec::new();
                checker.with_local_storage_context(method_ref_params, |checker| {
                    for s in &method.body {
                        if let Err(error) = checker.check_stmt(s, &mut method_env) {
                            method_errors.extend(error.flatten());
                        }
                    }
                    Ok(())
                })?;
                let method_has_errors = !method_errors.is_empty();
                pass_errors.extend(method_errors);

                if !method_has_errors {
                    let inferred_return = checker
                        .find_return_type_in_body(&method.body, &method_env)
                        .unwrap_or(PhpType::Void);
                    let effective_return = if let Some(type_ann) = method.return_type.as_ref() {
                        match checker.resolve_declared_return_type_hint(
                            type_ann,
                            method.span,
                            &format!("Method '{}::{}'", class.name, method.name),
                        ) {
                            Ok(declared) => {
                                if let Err(error) = checker.require_compatible_arg_type(
                                    &declared,
                                    &inferred_return,
                                    method.span,
                                    &format!("Method '{}::{}' return type", class.name, method.name),
                                ) {
                                    pass_errors.extend(error.flatten());
                                    checker.current_class = None;
                                    checker.current_method = None;
                                    checker.current_method_is_static = false;
                                    continue;
                                }
                                declared
                            }
                            Err(error) => {
                                pass_errors.extend(error.flatten());
                                checker.current_class = None;
                                checker.current_method = None;
                                checker.current_method_is_static = false;
                                continue;
                            }
                        }
                    } else {
                        inferred_return
                    };
                    if !method.is_static {
                        if let Some(ci) = checker.classes.get_mut(&class.name) {
                            if let Some(sig) = ci.methods.get_mut(&method.name) {
                                sig.return_type = effective_return;
                            }
                        }
                    } else if let Some(ci) = checker.classes.get_mut(&class.name) {
                        if let Some(sig) = ci.static_methods.get_mut(&method.name) {
                            sig.return_type = effective_return;
                        }
                    }
                }
                checker.current_class = None;
                checker.current_method = None;
                checker.current_method_is_static = false;
            }
        }

        let stabilized = checker.classes == classes_before_pass;
        let out_of_passes = method_passes_remaining == 0;
        if stabilized || out_of_passes {
            errors.extend(pass_errors);
            break;
        }

        method_passes_remaining -= 1;
    }

    if !errors.is_empty() {
        return Err(CompileError::from_many(errors));
    }

    propagate_abstract_return_types(&mut checker);
    validate_magic_method_contracts(&checker)?;

    Ok(CheckResult {
        global_env,
        functions: checker.functions,
        interfaces: checker.interfaces,
        classes: checker.classes,
        enums: checker.enums,
        packed_classes: checker.packed_classes,
        extern_functions: checker.extern_functions,
        extern_classes: checker.extern_classes,
        extern_globals: checker.extern_globals,
        required_libraries: checker.required_libraries,
        warnings: crate::types::warnings::collect_warnings(program),
    })
}
